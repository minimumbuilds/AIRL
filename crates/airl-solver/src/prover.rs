use std::sync::Mutex;
use z3::{Config, Context, SatResult, Solver};
use z3::ast::Ast;
use airl_syntax::ast::{Expr, ExprKind, FnDef, AstTypeKind};
use crate::translate::{Translator, VarSort, SeedVal};
use crate::{VerifyResult, FunctionVerification};

// ── Inductive verification ────────────────────────────────────────────────────
//
// Strategy (standard "assume-guarantee" / Hoare-style induction):
//
//   To prove :ensures P(params, result) of a recursive function f:
//     1. Assert :requires (base assumptions)
//     2. For every recursive call f(args) that appears in the body, inject an
//        axiom "the postcondition holds for those args" — this is the
//        *inductive hypothesis*: assume P(args, f(args)).
//     3. Translate the body and bind `result`.
//     4. Ask Z3: can (NOT P) be satisfied? If UNSAT → Proven.
//
//   This is sound for total functions (where the recursion terminates).  For
//   partial functions Z3 may report Proven even when the postcondition is
//   wrong on non-terminating inputs, but that is the same guarantee Dafny and
//   other Hoare-logic tools provide.  Termination proofs are out of scope here.
//
// The mechanism:
//   `inject_recursive_call_axioms` walks the body AST, finds every call whose
//   callee matches the function name, translates the arguments into fresh Z3
//   Int/Bool/Real vars (one per arg, named `__ind_{fn}_{i}_{arg}`), asserts the
//   instantiated postcondition for those fresh vars, and returns.  Then normal
//   body translation + negation check follows.

/// Z3's C library uses non-thread-safe global state during context creation.
/// Concurrent `Context::new()` calls SIGSEGV in CI. Serialize all Z3 access.
static Z3_LOCK: Mutex<()> = Mutex::new(());

// KNOWN LIMITATION: Linear ownership constraints are currently not encoded in Z3.
// Contracts on owned/borrowed parameters are verified structurally but not for linearity.
// The `.ownership` field on parameters is intentionally unused here — it requires
// a separate ownership checker, not Z3 SMT encoding.

/// Returns true if the expression (or any sub-expression) references the symbol "result".
/// Used to detect ensures clauses that cannot be verified without body translation.
fn clause_references_result(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::SymbolRef(name) => name == "result",
        ExprKind::FnCall(callee, args) => {
            clause_references_result(callee)
                || args.iter().any(clause_references_result)
        }
        ExprKind::If(cond, then_e, else_e) => {
            clause_references_result(cond)
                || clause_references_result(then_e)
                || clause_references_result(else_e)
        }
        ExprKind::Let(bindings, body) => {
            bindings.iter().any(|b| clause_references_result(&b.value))
                || clause_references_result(body)
        }
        ExprKind::Do(exprs) => exprs.iter().any(clause_references_result),
        ExprKind::Match(scrutinee, arms) => {
            clause_references_result(scrutinee)
                || arms.iter().any(|arm| clause_references_result(&arm.body))
        }
        ExprKind::Lambda(_, body) => clause_references_result(body),
        ExprKind::VariantCtor(_, args) => args.iter().any(clause_references_result),
        ExprKind::StructLit(_, fields) => fields.iter().any(|(_, e)| clause_references_result(e)),
        ExprKind::ListLit(elems) => elems.iter().any(clause_references_result),
        ExprKind::Try(inner) => clause_references_result(inner),
        ExprKind::Forall(_, guard, body) | ExprKind::Exists(_, guard, body) => {
            guard.as_deref().map_or(false, clause_references_result)
                || clause_references_result(body)
        }
        // Atoms: IntLit, FloatLit, StrLit, BoolLit, NilLit, KeywordLit — never reference result
        _ => false,
    }
}

pub struct Z3Prover;

impl Z3Prover {
    pub fn new() -> Self {
        Self
    }

    /// Verify all :ensures clauses of a function given its :requires.
    pub fn verify_function(&self, def: &FnDef) -> FunctionVerification {
        let _guard = Z3_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let mut translator = Translator::new(&ctx);

        // Declare variables for params
        let mut can_translate = true;
        for param in &def.params {
            if let AstTypeKind::Named(type_name) = &param.ty.kind {
                match Translator::sort_from_type_name(type_name) {
                    Some(VarSort::Int) => translator.declare_int(&param.name),
                    Some(VarSort::Bool) => translator.declare_bool(&param.name),
                    Some(VarSort::Real) => translator.declare_real(&param.name),
                    Some(VarSort::Str) => translator.declare_string(&param.name),
                    Some(VarSort::Seq) => translator.declare_seq(&param.name),
                    None => { can_translate = false; break; }
                }
            } else {
                can_translate = false;
                break;
            }
        }

        // Declare result variable
        if can_translate {
            if let AstTypeKind::Named(type_name) = &def.return_type.kind {
                match Translator::sort_from_type_name(type_name) {
                    Some(VarSort::Int) => translator.declare_int("result"),
                    Some(VarSort::Bool) => translator.declare_bool("result"),
                    Some(VarSort::Real) => translator.declare_real("result"),
                    Some(VarSort::Str) => translator.declare_string("result"),
                    Some(VarSort::Seq) => translator.declare_seq("result"),
                    None => can_translate = false,
                }
            } else {
                can_translate = false;
            }
        }

        if !can_translate {
            return FunctionVerification {
                function_name: def.name.clone(),
                ensures_results: def.ensures.iter().map(|e| {
                    (e.to_airl(), VerifyResult::Unknown("unsupported parameter types".into()))
                }).collect(),
                invariants_results: def.invariants.iter().map(|e| {
                    (e.to_airl(), VerifyResult::Unknown("unsupported parameter types".into()))
                }).collect(),
            };
        }

        // Assert :requires as assumptions
        for req in &def.requires {
            match translator.translate_bool(req) {
                Ok(z3_bool) => solver.assert(&z3_bool),
                Err(_) => {
                    // Can't translate requires — skip Z3 for this function
                    return FunctionVerification {
                        function_name: def.name.clone(),
                        ensures_results: def.ensures.iter().map(|e| {
                            (e.to_airl(), VerifyResult::Unknown("cannot translate requires".into()))
                        }).collect(),
                        invariants_results: def.invariants.iter().map(|e| {
                            (e.to_airl(), VerifyResult::Unknown("cannot translate requires".into()))
                        }).collect(),
                    };
                }
            }
        }

        // Translate the function body and bind it to `result`.
        // If translation fails (unsupported constructs), fall back to Unknown for
        // any ensures clause that references result.
        let body_translated = match &def.return_type.kind {
            AstTypeKind::Named(type_name) => match Translator::sort_from_type_name(type_name) {
                Some(VarSort::Int) => match translator.translate_int(&def.body) {
                    Ok(body_z3) => match translator.get_int_var("result") {
                        Some(result_var) => {
                            solver.assert(&result_var.clone()._eq(&body_z3));
                            true
                        }
                        None => false, // "result" was not declared; skip body binding
                    },
                    Err(_) => false,
                },
                Some(VarSort::Bool) => match translator.translate_bool(&def.body) {
                    Ok(body_z3) => match translator.get_bool_var("result") {
                        Some(result_var) => {
                            solver.assert(&result_var.clone()._eq(&body_z3));
                            true
                        }
                        None => false,
                    },
                    Err(_) => false,
                },
                Some(VarSort::Real) => match translator.translate_real(&def.body) {
                    Ok(body_z3) => match translator.get_real_var("result") {
                        Some(result_var) => {
                            solver.assert(&result_var.clone()._eq(&body_z3));
                            true
                        }
                        None => false,
                    },
                    Err(_) => false,
                },
                Some(VarSort::Str) => match translator.translate_string(&def.body) {
                    Ok(body_z3) => match translator.get_string_var("result") {
                        Some(result_var) => {
                            solver.assert(&result_var.clone()._eq(&body_z3));
                            true
                        }
                        None => false,
                    },
                    Err(_) => false,
                },
                // Seq(Int) return: body translation not yet supported — list-constructing
                // expressions can't be translated to Z3. Contracts on list params (length,
                // list-contains?) still work without body binding.
                Some(VarSort::Seq) => false,
                None => false,
            },
            _ => false,
        };

        // Prove each :ensures clause
        let mut ensures_results = Vec::new();
        for ensures_expr in &def.ensures {
            let clause_source = ensures_expr.to_airl();

            if clause_references_result(ensures_expr) && !body_translated {
                ensures_results.push((
                    clause_source,
                    VerifyResult::Unknown(
                        "result postconditions require body translation (not yet implemented)".into(),
                    ),
                ));
                continue;
            }

            let result = match translator.translate_bool(ensures_expr) {
                Ok(z3_bool) => {
                    // Negate the clause and check
                    solver.push();
                    solver.assert(&z3_bool.not());

                    let result = match solver.check() {
                        SatResult::Unsat => VerifyResult::Proven,
                        SatResult::Sat => {
                            // Extract counterexample
                            let mut counterexample = Vec::new();
                            if let Some(model) = solver.get_model() {
                                for param in &def.params {
                                    if let AstTypeKind::Named(type_name) = &param.ty.kind {
                                        match Translator::sort_from_type_name(type_name) {
                                            Some(VarSort::Int) => {
                                                if let Some(var) = translator.get_int_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Real) => {
                                                if let Some(var) = translator.get_real_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Bool) => {
                                                if let Some(var) = translator.get_bool_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        if let Some(b) = val.as_bool() {
                                                            counterexample.push((param.name.clone(), b.to_string()));
                                                        }
                                                    }
                                                }
                                            }
                                            Some(VarSort::Str) => {
                                                if let Some(var) = translator.get_string_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Seq) => {
                                                // Seq counterexamples: use Dynamic eval + to_string
                                                if let Some(var) = translator.get_seq_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            None => {}
                                        }
                                    }
                                }
                            }
                            VerifyResult::Disproven { counterexample }
                        }
                        SatResult::Unknown => {
                            VerifyResult::Unknown(
                                solver.get_reason_unknown().unwrap_or_else(|| "unknown".into())
                            )
                        }
                    };

                    solver.pop(1);
                    result
                }
                Err(e) => VerifyResult::TranslationError(e.to_string()),
            };

            ensures_results.push((clause_source, result));
        }

        // Prove each :invariant clause (same strategy as :ensures —
        // invariants must hold given :requires and body binding)
        let mut invariants_results = Vec::new();
        for inv_expr in &def.invariants {
            let clause_source = inv_expr.to_airl();

            if clause_references_result(inv_expr) && !body_translated {
                invariants_results.push((
                    clause_source,
                    VerifyResult::Unknown(
                        "invariant references result but body translation failed".into(),
                    ),
                ));
                continue;
            }

            let result = match translator.translate_bool(inv_expr) {
                Ok(z3_bool) => {
                    solver.push();
                    solver.assert(&z3_bool.not());

                    let result = match solver.check() {
                        SatResult::Unsat => VerifyResult::Proven,
                        SatResult::Sat => {
                            let mut counterexample = Vec::new();
                            if let Some(model) = solver.get_model() {
                                for param in &def.params {
                                    if let AstTypeKind::Named(type_name) = &param.ty.kind {
                                        match Translator::sort_from_type_name(type_name) {
                                            Some(VarSort::Int) => {
                                                if let Some(var) = translator.get_int_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Real) => {
                                                if let Some(var) = translator.get_real_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Bool) => {
                                                if let Some(var) = translator.get_bool_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        if let Some(b) = val.as_bool() {
                                                            counterexample.push((param.name.clone(), b.to_string()));
                                                        }
                                                    }
                                                }
                                            }
                                            Some(VarSort::Str) => {
                                                if let Some(var) = translator.get_string_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Seq) => {
                                                // Seq counterexamples: use Dynamic eval + to_string
                                                if let Some(var) = translator.get_seq_var(&param.name) {
                                                    if let Some(val) = model.eval(var, true) {
                                                        counterexample.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            None => {}
                                        }
                                    }
                                }
                            }
                            VerifyResult::Disproven { counterexample }
                        }
                        SatResult::Unknown => {
                            VerifyResult::Unknown(
                                solver.get_reason_unknown().unwrap_or_else(|| "unknown".into())
                            )
                        }
                    };

                    solver.pop(1);
                    result
                }
                Err(e) => VerifyResult::TranslationError(e.to_string()),
            };

            invariants_results.push((clause_source, result));
        }

        FunctionVerification {
            function_name: def.name.clone(),
            ensures_results,
            invariants_results,
        }
    }

    /// Perform inductive verification for a recursive function.
    ///
    /// Identical to `verify_function` except that, before asserting the body
    /// binding, we inject Z3 axioms for the inductive hypothesis:
    /// "every recursive call to `def.name` satisfies each :ensures clause".
    ///
    /// Returns `None` when no recursive calls are found (caller falls back to
    /// `verify_function`).  Returns `Some(FunctionVerification)` otherwise.
    pub fn inductive_verify(&self, def: &FnDef) -> Option<FunctionVerification> {
        if !body_contains_recursive_call(&def.body, &def.name) {
            return None;
        }

        let _guard = Z3_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let mut translator = Translator::new(&ctx);

        // Declare parameters
        let mut can_translate = true;
        for param in &def.params {
            if let AstTypeKind::Named(type_name) = &param.ty.kind {
                match Translator::sort_from_type_name(type_name) {
                    Some(VarSort::Int)  => translator.declare_int(&param.name),
                    Some(VarSort::Bool) => translator.declare_bool(&param.name),
                    Some(VarSort::Real) => translator.declare_real(&param.name),
                    Some(VarSort::Str)  => translator.declare_string(&param.name),
                    Some(VarSort::Seq)  => translator.declare_seq(&param.name),
                    None => { can_translate = false; break; }
                }
            } else {
                can_translate = false;
                break;
            }
        }

        // Declare result variable
        if can_translate {
            if let AstTypeKind::Named(type_name) = &def.return_type.kind {
                match Translator::sort_from_type_name(type_name) {
                    Some(VarSort::Int)  => translator.declare_int("result"),
                    Some(VarSort::Bool) => translator.declare_bool("result"),
                    Some(VarSort::Real) => translator.declare_real("result"),
                    Some(VarSort::Str)  => translator.declare_string("result"),
                    Some(VarSort::Seq)  => translator.declare_seq("result"),
                    None => can_translate = false,
                }
            } else {
                can_translate = false;
            }
        }

        if !can_translate {
            return Some(FunctionVerification {
                function_name: def.name.clone(),
                ensures_results: def.ensures.iter().map(|e| {
                    (e.to_airl(), VerifyResult::Unknown("unsupported parameter types".into()))
                }).collect(),
                invariants_results: def.invariants.iter().map(|e| {
                    (e.to_airl(), VerifyResult::Unknown("unsupported parameter types".into()))
                }).collect(),
            });
        }

        // Assert :requires as assumptions
        for req in &def.requires {
            match translator.translate_bool(req) {
                Ok(z3_bool) => solver.assert(&z3_bool),
                Err(_) => {
                    return Some(FunctionVerification {
                        function_name: def.name.clone(),
                        ensures_results: def.ensures.iter().map(|e| {
                            (e.to_airl(), VerifyResult::Unknown("cannot translate requires".into()))
                        }).collect(),
                        invariants_results: def.invariants.iter().map(|e| {
                            (e.to_airl(), VerifyResult::Unknown("cannot translate requires".into()))
                        }).collect(),
                    });
                }
            }
        }

        // ── Inductive hypothesis injection ────────────────────────────────────
        //
        // For each recursive call site f(a0, a1, ...) found in the body:
        //   1. Translate each arg using the main translator.
        //   2. Build the uninterpreted function application f_interp(args_i) using the
        //      SAME uninterpreted function symbol that the body translation will use.
        //      This ensures Z3 sees the inductive hypothesis as a fact about f_interp.
        //   3. Build a call-site Translator with params bound to arg values and
        //      "result" bound to f_interp(args_i).
        //   4. Translate each :ensures clause in the call-site context and assert it.
        //
        // The key: "result" in the axiom IS the uninterpreted app f_interp(args_i),
        // so when the body translation later produces f_interp(args_i) as a sub-term,
        // Z3 can connect the axiom to the body term by symbol identity.
        let recursive_calls = collect_recursive_calls(&def.body, &def.name);
        for (site_idx, call_args) in recursive_calls.iter().enumerate() {
            let _ = site_idx; // only used for diagnostics
            if call_args.len() != def.params.len() { continue; }

            // Step 1: translate args and build the uninterpreted app for result.
            // We do this with the main translator so the func_decl is shared.
            let app_z3_result_opt = translator.apply_recursive_fn_as_result(
                &def.name, call_args, &def.return_type, &def.params
            );
            let (arg_vals, result_seed) = match app_z3_result_opt {
                Some(x) => x,
                None => continue,
            };

            // Step 2: build call-site translator with param and result bindings.
            let mut site_translator = Translator::new(&ctx);
            for (param, val) in def.params.iter().zip(arg_vals.into_iter()) {
                match val {
                    SeedVal::Int(v)  => site_translator.seed_int(&param.name, v),
                    SeedVal::Bool(v) => site_translator.seed_bool(&param.name, v),
                    SeedVal::Real(v) => site_translator.seed_real(&param.name, v),
                }
            }
            match result_seed {
                SeedVal::Int(v)  => site_translator.seed_int("result", v),
                SeedVal::Bool(v) => site_translator.seed_bool("result", v),
                SeedVal::Real(v) => site_translator.seed_real("result", v),
            }

            // Step 3: translate each ensures clause and assert it as the IH.
            for ensures_expr in &def.ensures {
                if let Ok(z3_axiom) = site_translator.translate_bool(ensures_expr) {
                    solver.assert(&z3_axiom);
                }
            }
        }
        // ── End inductive hypothesis injection ──────────────────────────────

        // Translate body and bind `result`
        let body_translated = match &def.return_type.kind {
            AstTypeKind::Named(type_name) => match Translator::sort_from_type_name(type_name) {
                Some(VarSort::Int) => match translator.translate_int(&def.body) {
                    Ok(body_z3) => match translator.get_int_var("result") {
                        Some(r) => { solver.assert(&r.clone()._eq(&body_z3)); true }
                        None => false,
                    },
                    Err(_) => false,
                },
                Some(VarSort::Bool) => match translator.translate_bool(&def.body) {
                    Ok(body_z3) => match translator.get_bool_var("result") {
                        Some(r) => { solver.assert(&r.clone()._eq(&body_z3)); true }
                        None => false,
                    },
                    Err(_) => false,
                },
                Some(VarSort::Real) => match translator.translate_real(&def.body) {
                    Ok(body_z3) => match translator.get_real_var("result") {
                        Some(r) => { solver.assert(&r.clone()._eq(&body_z3)); true }
                        None => false,
                    },
                    Err(_) => false,
                },
                _ => false,
            },
            _ => false,
        };

        // Prove each :ensures clause
        let mut ensures_results = Vec::new();
        for ensures_expr in &def.ensures {
            let clause_source = ensures_expr.to_airl();
            if clause_references_result(ensures_expr) && !body_translated {
                ensures_results.push((
                    clause_source,
                    VerifyResult::Unknown(
                        "result postconditions require body translation (recursive body unsupported)".into(),
                    ),
                ));
                continue;
            }
            let result = match translator.translate_bool(ensures_expr) {
                Ok(z3_bool) => {
                    solver.push();
                    solver.assert(&z3_bool.not());
                    let r = match solver.check() {
                        SatResult::Unsat => VerifyResult::Proven,
                        SatResult::Sat => {
                            let mut cex = Vec::new();
                            if let Some(model) = solver.get_model() {
                                for param in &def.params {
                                    if let AstTypeKind::Named(tn) = &param.ty.kind {
                                        match Translator::sort_from_type_name(tn) {
                                            Some(VarSort::Int) => {
                                                if let Some(v) = translator.get_int_var(&param.name) {
                                                    if let Some(val) = model.eval(v, true) {
                                                        cex.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            Some(VarSort::Bool) => {
                                                if let Some(v) = translator.get_bool_var(&param.name) {
                                                    if let Some(val) = model.eval(v, true) {
                                                        if let Some(b) = val.as_bool() {
                                                            cex.push((param.name.clone(), b.to_string()));
                                                        }
                                                    }
                                                }
                                            }
                                            Some(VarSort::Real) => {
                                                if let Some(v) = translator.get_real_var(&param.name) {
                                                    if let Some(val) = model.eval(v, true) {
                                                        cex.push((param.name.clone(), val.to_string()));
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            VerifyResult::Disproven { counterexample: cex }
                        }
                        SatResult::Unknown => VerifyResult::Unknown(
                            solver.get_reason_unknown().unwrap_or_else(|| "unknown".into())
                        ),
                    };
                    solver.pop(1);
                    r
                }
                Err(e) => VerifyResult::TranslationError(e.to_string()),
            };
            ensures_results.push((clause_source, result));
        }

        // Prove each :invariant clause
        let mut invariants_results = Vec::new();
        for inv_expr in &def.invariants {
            let clause_source = inv_expr.to_airl();
            if clause_references_result(inv_expr) && !body_translated {
                invariants_results.push((
                    clause_source,
                    VerifyResult::Unknown("invariant references result but body translation failed".into()),
                ));
                continue;
            }
            let result = match translator.translate_bool(inv_expr) {
                Ok(z3_bool) => {
                    solver.push();
                    solver.assert(&z3_bool.not());
                    let r = match solver.check() {
                        SatResult::Unsat => VerifyResult::Proven,
                        SatResult::Sat => VerifyResult::Disproven { counterexample: vec![] },
                        SatResult::Unknown => VerifyResult::Unknown(
                            solver.get_reason_unknown().unwrap_or_else(|| "unknown".into())
                        ),
                    };
                    solver.pop(1);
                    r
                }
                Err(e) => VerifyResult::TranslationError(e.to_string()),
            };
            invariants_results.push((clause_source, result));
        }

        Some(FunctionVerification {
            function_name: def.name.clone(),
            ensures_results,
            invariants_results,
        })
    }
}

// ── Body traversal helpers ────────────────────────────────────────────────────

/// Returns true if the expression tree contains a direct call to `fn_name`.
fn body_contains_recursive_call(expr: &Expr, fn_name: &str) -> bool {
    match &expr.kind {
        ExprKind::FnCall(callee, args) => {
            if let ExprKind::SymbolRef(name) = &callee.kind {
                if name == fn_name { return true; }
            }
            body_contains_recursive_call(callee, fn_name)
                || args.iter().any(|a| body_contains_recursive_call(a, fn_name))
        }
        ExprKind::If(cond, t, e) =>
            body_contains_recursive_call(cond, fn_name)
                || body_contains_recursive_call(t, fn_name)
                || body_contains_recursive_call(e, fn_name),
        ExprKind::Let(bindings, body) =>
            bindings.iter().any(|b| body_contains_recursive_call(&b.value, fn_name))
                || body_contains_recursive_call(body, fn_name),
        ExprKind::Do(exprs) => exprs.iter().any(|e| body_contains_recursive_call(e, fn_name)),
        ExprKind::Match(scrutinee, arms) =>
            body_contains_recursive_call(scrutinee, fn_name)
                || arms.iter().any(|a| body_contains_recursive_call(&a.body, fn_name)),
        ExprKind::Lambda(_, body) => body_contains_recursive_call(body, fn_name),
        ExprKind::VariantCtor(_, args) => args.iter().any(|a| body_contains_recursive_call(a, fn_name)),
        ExprKind::StructLit(_, fields) => fields.iter().any(|(_, e)| body_contains_recursive_call(e, fn_name)),
        ExprKind::ListLit(elems) => elems.iter().any(|e| body_contains_recursive_call(e, fn_name)),
        ExprKind::Try(inner) => body_contains_recursive_call(inner, fn_name),
        ExprKind::Forall(_, guard, body) | ExprKind::Exists(_, guard, body) =>
            guard.as_deref().map_or(false, |g| body_contains_recursive_call(g, fn_name))
                || body_contains_recursive_call(body, fn_name),
        _ => false,
    }
}

/// Collect the argument lists for every direct call to `fn_name` in `expr`.
fn collect_recursive_calls(expr: &Expr, fn_name: &str) -> Vec<Vec<Expr>> {
    let mut out = Vec::new();
    collect_recursive_calls_inner(expr, fn_name, &mut out);
    out
}

fn collect_recursive_calls_inner(expr: &Expr, fn_name: &str, out: &mut Vec<Vec<Expr>>) {
    match &expr.kind {
        ExprKind::FnCall(callee, args) => {
            if let ExprKind::SymbolRef(name) = &callee.kind {
                if name == fn_name { out.push(args.clone()); }
            }
            collect_recursive_calls_inner(callee, fn_name, out);
            for a in args { collect_recursive_calls_inner(a, fn_name, out); }
        }
        ExprKind::If(cond, t, e) => {
            collect_recursive_calls_inner(cond, fn_name, out);
            collect_recursive_calls_inner(t, fn_name, out);
            collect_recursive_calls_inner(e, fn_name, out);
        }
        ExprKind::Let(bindings, body) => {
            for b in bindings { collect_recursive_calls_inner(&b.value, fn_name, out); }
            collect_recursive_calls_inner(body, fn_name, out);
        }
        ExprKind::Do(exprs) => { for e in exprs { collect_recursive_calls_inner(e, fn_name, out); } }
        ExprKind::Match(scrutinee, arms) => {
            collect_recursive_calls_inner(scrutinee, fn_name, out);
            for arm in arms { collect_recursive_calls_inner(&arm.body, fn_name, out); }
        }
        ExprKind::Lambda(_, body) => collect_recursive_calls_inner(body, fn_name, out),
        ExprKind::VariantCtor(_, args) => { for a in args { collect_recursive_calls_inner(a, fn_name, out); } }
        ExprKind::StructLit(_, fields) => { for (_, e) in fields { collect_recursive_calls_inner(e, fn_name, out); } }
        ExprKind::ListLit(elems) => { for e in elems { collect_recursive_calls_inner(e, fn_name, out); } }
        ExprKind::Try(inner) => collect_recursive_calls_inner(inner, fn_name, out),
        ExprKind::Forall(_, guard, body) | ExprKind::Exists(_, guard, body) => {
            if let Some(g) = guard.as_deref() { collect_recursive_calls_inner(g, fn_name, out); }
            collect_recursive_calls_inner(body, fn_name, out);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::Span;
    use airl_syntax::ast::*;

    fn make_fn(
        name: &str,
        params: Vec<(&str, &str)>,
        ret_type: &str,
        requires: Vec<Expr>,
        ensures: Vec<Expr>,
        body: Expr,
    ) -> FnDef {
        FnDef {
            name: name.into(),
            params: params.iter().map(|(pname, ptype)| Param {
                ownership: Ownership::Default,
                name: pname.to_string(),
                ty: AstType { kind: AstTypeKind::Named(ptype.to_string()), span: Span::dummy() },
                default: None,
                span: Span::dummy(),
            }).collect(),
            return_type: AstType { kind: AstTypeKind::Named(ret_type.into()), span: Span::dummy() },
            intent: None,
            requires,
            ensures,
            invariants: vec![],
            is_pure: false,
            is_total: false,
            body,
            execute_on: None,
            priority: None,
            is_public: false,
            span: Span::dummy(),
        }
    }

    fn sym(name: &str) -> Expr { Expr { kind: ExprKind::SymbolRef(name.into()), span: Span::dummy() } }
    fn int(v: i64) -> Expr { Expr { kind: ExprKind::IntLit(v), span: Span::dummy() } }
    fn call(op: &str, args: Vec<Expr>) -> Expr {
        Expr {
            kind: ExprKind::FnCall(Box::new(sym(op)), args),
            span: Span::dummy(),
        }
    }

    #[test]
    fn prove_addition_contract() {
        // (defn add [(a : i32) (b : i32) -> i32]
        //   :requires []          ← no valid() so Z3 can run
        //   :ensures [(= result (+ a b))])
        // Body is (+ a b) — body translation binds result == a+b, so ensures is Proven.
        let def = make_fn("add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![], // valid() in requires would make Z3 bail with "cannot translate requires"
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for (= result (+ a b)) with body (+ a b), got: {:?}",
            verification,
        );
    }

    #[test]
    fn disprove_wrong_contract() {
        // (defn mul [(a : i32) (b : i32) -> i32]
        //   :requires []          ← no valid() requires so Z3 has no untranslatable clauses
        //   :ensures [(= result (+ a b))])  ← wrong! body is *, not +
        // Body is (* a b) — body translation binds result == a*b, ensures says result == a+b.
        // Z3 finds a counterexample (e.g. a=2, b=3: 6 != 5).
        let def = make_fn("mul",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![], // no requires — valid() would prevent Z3 from running
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
            call("*", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Disproven { .. }),
            "expected Disproven for (= result (+ a b)) with body (* a b), got: {:?}",
            verification,
        );
    }

    #[test]
    fn valid_ensures_yields_translation_error() {
        // valid() no longer translates to Z3 true — it returns Unsupported.
        // Ensures clauses using valid() should produce TranslationError, not a
        // spurious Proven.
        let def = make_fn("id",
            vec![("x", "i32")], "i32",
            vec![],
            vec![call("valid", vec![sym("result")])],
            sym("x"),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::TranslationError(_)),
            "expected TranslationError for (valid result) ensures — valid() is unsupported, got: {:?}",
            verification,
        );
    }

    #[test]
    fn prove_with_requires_constraint() {
        // Body is (+ a b). :requires [(>= a 0) (>= b 0)].
        // :ensures [(>= result 0)] — proven because result == a+b >= 0 given preconditions.
        let def = make_fn("add_pos",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![
                call(">=", vec![sym("a"), int(0)]),
                call(">=", vec![sym("b"), int(0)]),
            ],
            vec![call(">=", vec![sym("result"), int(0)])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for (>= result 0) with nonneg args and body (+ a b), got: {:?}",
            verification,
        );
    }

    #[test]
    fn prove_parameter_relationship() {
        // :ensures [(>= (* a a) 0)] — no `result` reference, exercises real Z3 SAT/UNSAT.
        // a² >= 0 is always true for integers, so Z3 should prove it.
        let def = make_fn("square_nonneg",
            vec![("a", "i32")], "i32",
            vec![],
            vec![call(">=", vec![call("*", vec![sym("a"), sym("a")]), int(0)])],
            call("*", vec![sym("a"), sym("a")]),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for a²≥0, got: {:?}",
            verification,
        );
    }

    #[test]
    fn disprove_false_parameter_relationship() {
        // :ensures [(> a 0)] — no preconditions, so a could be negative. Z3 should disprove.
        let def = make_fn("always_positive",
            vec![("a", "i32")], "i32",
            vec![],
            vec![call(">", vec![sym("a"), int(0)])],
            sym("a"),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Disproven { .. }),
            "expected Disproven for unconstrained a>0, got: {:?}",
            verification,
        );
    }

    #[test]
    fn prove_requires_implies_ensures() {
        // :requires [(>= a 0)]
        // :ensures [(>= a 0)] — tautology given the precondition, no `result` reference.
        let def = make_fn("passthrough",
            vec![("a", "i32")], "i32",
            vec![call(">=", vec![sym("a"), int(0)])],
            vec![call(">=", vec![sym("a"), int(0)])],
            sym("a"),
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for requires-implies-ensures tautology, got: {:?}",
            verification,
        );
    }

    #[test]
    fn string_params_now_supported() {
        // String params are now translatable — body (name) is bound to result,
        // and the function has no ensures, so the result is trivially OK.
        let def = make_fn("greet",
            vec![("name", "String")], "String",
            vec![], vec![],
            sym("name"),
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        assert!(result.ensures_results.is_empty());
    }

    #[test]
    fn prove_string_length_nonneg() {
        // (defn slen [(s : String) -> i32]
        //   :ensures [(>= result 0)]
        //   :body (string-length s))
        // string-length always returns >= 0.
        let def = make_fn("slen",
            vec![("s", "String")], "i32",
            vec![],
            vec![call(">=", vec![sym("result"), int(0)])],
            call("string-length", vec![sym("s")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for string-length >= 0, got: {:?}", v,
        );
    }

    #[test]
    fn prove_string_contains_self() {
        // (defn self_contains [(s : String) -> bool]
        //   :ensures [(= result true)]
        //   :body (string-contains? s s))
        // A string always contains itself.
        let def = make_fn("self_contains",
            vec![("s", "String")], "bool",
            vec![],
            vec![call("=", vec![sym("result"), Expr { kind: ExprKind::BoolLit(true), span: Span::dummy() }])],
            call("string-contains?", vec![sym("s"), sym("s")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for string-contains? self, got: {:?}", v,
        );
    }

    #[test]
    fn prove_string_concat_result() {
        // (defn greet [(name : String) -> String]
        //   :ensures [(= result (string-concat "hello " name))]
        //   :body (string-concat "hello " name))
        let hello = Expr { kind: ExprKind::StrLit("hello ".into()), span: Span::dummy() };
        let hello2 = Expr { kind: ExprKind::StrLit("hello ".into()), span: Span::dummy() };
        let def = make_fn("greet",
            vec![("name", "String")], "String",
            vec![],
            vec![call("=", vec![sym("result"), call("string-concat", vec![hello.clone(), sym("name")])])],
            call("string-concat", vec![hello2, sym("name")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for string-concat result, got: {:?}", v,
        );
    }

    // ── New tests for body translation feature ──────────────────────────────

    #[test]
    fn body_translation_proves_result_equals_sum() {
        // (defn add [(a : i32) (b : i32) -> i32] (+ a b))
        // :ensures [(= result (+ a b))]
        // Body (+ a b) is translated; result == a+b is asserted → Proven.
        let def = make_fn("add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven, got: {:?}", v,
        );
    }

    #[test]
    fn body_translation_proves_nonneg_result_from_nonneg_args() {
        // (defn add_pos [(a : i32) (b : i32) -> i32]
        //   :requires [(>= a 0) (>= b 0)]
        //   :ensures [(>= result 0)])
        // Body (+ a b) — Z3 proves result>=0 given a>=0, b>=0.
        let def = make_fn("add_pos",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![
                call(">=", vec![sym("a"), int(0)]),
                call(">=", vec![sym("b"), int(0)]),
            ],
            vec![call(">=", vec![sym("result"), int(0)])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven, got: {:?}", v,
        );
    }

    #[test]
    fn body_translation_disproves_false_postcondition() {
        // (defn wrong_add [(a : i32) (b : i32) -> i32]
        //   :ensures [(= result (* a b))])  ← wrong, body is +
        // Body (+ a b), ensures (* a b) — Z3 finds counterexample.
        let def = make_fn("wrong_add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![],
            vec![call("=", vec![sym("result"), call("*", vec![sym("a"), sym("b")])])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Disproven { .. }),
            "expected Disproven, got: {:?}", v,
        );
    }

    #[test]
    fn body_translation_handles_if_ite() {
        // (defn abs [(a : i32) -> i32]
        //   (if (> a 0) a (- 0 a)))
        // :ensures [(>= result 0)]
        // Body uses if — translates to ite(a>0, a, -a).
        let def = make_fn("abs",
            vec![("a", "i32")], "i32",
            vec![],
            vec![call(">=", vec![sym("result"), int(0)])],
            Expr {
                kind: ExprKind::If(
                    Box::new(call(">", vec![sym("a"), int(0)])),
                    Box::new(sym("a")),
                    Box::new(call("-", vec![int(0), sym("a")])),
                ),
                span: Span::dummy(),
            },
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for abs result >= 0, got: {:?}", v,
        );
    }

    #[test]
    fn body_translation_fallback_when_unsupported() {
        // Body uses a construct that can't be translated (string literal in int context).
        // result-referencing ensures should fall back to Unknown, not panic.
        let unsupported_body = Expr {
            kind: ExprKind::StrLit("hello".into()),
            span: Span::dummy(),
        };
        let def = make_fn("opaque",
            vec![("a", "i32")], "i32",
            vec![],
            vec![call("=", vec![sym("result"), int(42)])],
            unsupported_body,
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Unknown(_)),
            "expected Unknown fallback for untranslatable body, got: {:?}", v,
        );
    }

    #[test]
    fn prove_invariant_clause() {
        // (defn add_pos [(a : i32) (b : i32) -> i32]
        //   :requires [(>= a 0) (>= b 0)]
        //   :invariant [(>= result 0)]
        //   :body (+ a b))
        // Z3 proves the invariant: result == a+b >= 0 given a>=0, b>=0.
        let mut def = make_fn("add_pos",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![
                call(">=", vec![sym("a"), int(0)]),
                call(">=", vec![sym("b"), int(0)]),
            ],
            vec![], // no ensures
            call("+", vec![sym("a"), sym("b")]),
        );
        def.invariants = vec![call(">=", vec![sym("result"), int(0)])];
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert!(v.ensures_results.is_empty());
        assert_eq!(v.invariants_results.len(), 1);
        assert!(
            matches!(&v.invariants_results[0].1, VerifyResult::Proven),
            "expected invariant Proven, got: {:?}", v,
        );
    }

    #[test]
    fn disprove_invariant_clause() {
        // (defn sub [(a : i32) (b : i32) -> i32]
        //   :invariant [(>= result 0)]
        //   :body (- a b))
        // No requires — a=0, b=1 gives result=-1, violating the invariant.
        let mut def = make_fn("sub",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![], vec![],
            call("-", vec![sym("a"), sym("b")]),
        );
        def.invariants = vec![call(">=", vec![sym("result"), int(0)])];
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.invariants_results.len(), 1);
        assert!(
            matches!(&v.invariants_results[0].1, VerifyResult::Disproven { .. }),
            "expected invariant Disproven, got: {:?}", v,
        );
    }

    #[test]
    fn disprove_bool_param_false_ensures() {
        // (defn always_true [(b : bool) -> bool]
        //   :ensures [(= b true)])  -- b could be false, so disproven
        let def = make_fn("always_true",
            vec![("b", "bool")], "bool",
            vec![],
            vec![call("=", vec![sym("b"), Expr { kind: ExprKind::BoolLit(true), span: Span::dummy() }])],
            sym("b"),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Disproven { counterexample } if !counterexample.is_empty()),
            "expected Disproven with Bool counterexample, got: {:?}", v,
        );
    }

    #[test]
    fn prove_list_length_nonneg() {
        // (defn list_len [(xs : List) -> i32]
        //   :ensures [(>= result 0)]
        //   :body (length xs))
        // Seq length is always >= 0, so this should be Proven.
        let def = make_fn("list_len",
            vec![("xs", "List")], "i32",
            vec![],
            vec![call(">=", vec![sym("result"), int(0)])],
            call("length", vec![sym("xs")]),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for list length >= 0, got: {:?}", v,
        );
    }

    #[test]
    fn prove_list_length_sum() {
        // (defn concat_len [(a : List) (b : List) -> i32]
        //   :ensures [(= result (+ (length a) (length b)))]
        //   :body (+ (length a) (length b)))
        // Body is (+ (length a) (length b)), result == that sum. Should be Proven.
        let body = call("+", vec![
            call("length", vec![sym("a")]),
            call("length", vec![sym("b")]),
        ]);
        let ensures = call("=", vec![
            sym("result"),
            call("+", vec![
                call("length", vec![sym("a")]),
                call("length", vec![sym("b")]),
            ]),
        ]);
        let def = make_fn("concat_len",
            vec![("a", "List"), ("b", "List")], "i32",
            vec![],
            vec![ensures],
            body,
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for result == length(a) + length(b), got: {:?}", v,
        );
    }

    #[test]
    fn list_param_with_int_return() {
        // (defn first_or_zero [(xs : List) -> i32]
        //   :ensures [(>= (length xs) 0)]
        //   :body 0)
        // Ensures doesn't reference result, just checks length >= 0. Should be Proven.
        let def = make_fn("first_or_zero",
            vec![("xs", "List")], "i32",
            vec![],
            vec![call(">=", vec![call("length", vec![sym("xs")]), int(0)])],
            int(0),
        );
        let prover = Z3Prover::new();
        let v = prover.verify_function(&def);
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for length(xs) >= 0, got: {:?}", v,
        );
    }

    // ── Inductive verification tests ──────────────────────────────────────────

    #[test]
    fn inductive_verify_non_recursive_returns_none() {
        // A non-recursive function should return None from inductive_verify.
        let def = make_fn("add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
            call("+", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        assert!(
            prover.inductive_verify(&def).is_none(),
            "non-recursive function should return None from inductive_verify",
        );
    }

    #[test]
    fn inductive_verify_recursive_sum_nonneg() {
        // (defn sum [(n : i32) -> i32]
        //   :requires [(>= n 0)]
        //   :ensures [(>= result 0)]
        //   :body (if (= n 0) 0 (+ n (sum (- n 1)))))
        //
        // Inductive proof:
        //   - Base: n=0 → result=0 ≥ 0  ✓
        //   - Inductive step: assume sum(n-1) ≥ 0, then result = n + sum(n-1) ≥ 0
        //     given n ≥ 0 and IH: sum(n-1) ≥ 0.
        let body = Expr {
            kind: ExprKind::If(
                Box::new(call("=", vec![sym("n"), int(0)])),
                Box::new(int(0)),
                Box::new(call("+", vec![sym("n"), call("sum", vec![call("-", vec![sym("n"), int(1)])])])),
            ),
            span: Span::dummy(),
        };
        let def = make_fn("sum",
            vec![("n", "i32")], "i32",
            vec![call(">=", vec![sym("n"), int(0)])],
            vec![call(">=", vec![sym("result"), int(0)])],
            body,
        );
        let prover = Z3Prover::new();
        let v = prover.inductive_verify(&def).expect("should detect recursive call");
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Proven),
            "expected Proven for recursive sum ≥ 0 with inductive hypothesis, got: {:?}", v,
        );
    }

    #[test]
    fn inductive_verify_preserves_regular_verify_for_non_recursive() {
        // verify_function and inductive_verify should agree on non-recursive bodies.
        let def = make_fn("mul",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![],
            vec![call("=", vec![sym("result"), call("*", vec![sym("a"), sym("b")])])],
            call("*", vec![sym("a"), sym("b")]),
        );
        let prover = Z3Prover::new();
        // inductive_verify returns None for non-recursive
        assert!(prover.inductive_verify(&def).is_none());
        // verify_function proves it
        let v = prover.verify_function(&def);
        assert!(matches!(&v.ensures_results[0].1, VerifyResult::Proven));
    }

    #[test]
    fn inductive_verify_detects_wrong_postcondition() {
        // A recursive function with a WRONG postcondition should be Disproven
        // (the inductive hypothesis is not strong enough to save a false claim).
        //
        // (defn sum_wrong [(n : i32) -> i32]
        //   :requires [(>= n 0)]
        //   :ensures [(> result n)]   <-- WRONG: sum(0)=0, not > 0
        //   :body (if (= n 0) 0 (+ n (sum_wrong (- n 1)))))
        let body = Expr {
            kind: ExprKind::If(
                Box::new(call("=", vec![sym("n"), int(0)])),
                Box::new(int(0)),
                Box::new(call("+", vec![sym("n"), call("sum_wrong", vec![call("-", vec![sym("n"), int(1)])])])),
            ),
            span: Span::dummy(),
        };
        let def = make_fn("sum_wrong",
            vec![("n", "i32")], "i32",
            vec![call(">=", vec![sym("n"), int(0)])],
            vec![call(">", vec![sym("result"), sym("n")])],
            body,
        );
        let prover = Z3Prover::new();
        let v = prover.inductive_verify(&def).expect("should detect recursive call");
        assert_eq!(v.ensures_results.len(), 1);
        assert!(
            matches!(&v.ensures_results[0].1, VerifyResult::Disproven { .. }),
            "expected Disproven for wrong postcondition (> result n), got: {:?}", v,
        );
    }
}
