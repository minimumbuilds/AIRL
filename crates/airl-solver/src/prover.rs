use std::sync::Mutex;
use z3::{Config, Context, SatResult, Solver};
use z3::ast::Ast;
use airl_syntax::ast::{Expr, ExprKind, FnDef, AstTypeKind};
use crate::translate::{Translator, VarSort};
use crate::{VerifyResult, FunctionVerification};

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

        FunctionVerification {
            function_name: def.name.clone(),
            ensures_results,
        }
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
    fn unknown_for_string_params() {
        let def = make_fn("greet",
            vec![("name", "String")], "String",
            vec![], vec![],
            sym("name"),
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        // Should return Unknown for all clauses (no clauses = trivially OK)
        assert!(result.ensures_results.is_empty() || !result.has_disproven());
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
}
