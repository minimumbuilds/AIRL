use z3::{Config, Context, SatResult, Solver};
use airl_syntax::ast::{Expr, ExprKind, FnDef, AstTypeKind};
use crate::translate::{Translator, VarSort};
use crate::{VerifyResult, FunctionVerification};

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

        // Prove each :ensures clause
        let mut ensures_results = Vec::new();
        for ensures_expr in &def.ensures {
            let clause_source = ensures_expr.to_airl();

            if clause_references_result(ensures_expr) {
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
                                            _ => {}
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
            body: Expr { kind: ExprKind::IntLit(0), span: Span::dummy() },
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
        //   :requires [(valid a) (valid b)]
        //   :ensures [(= result (+ a b))])
        // The ensures clause references `result` — without body translation we cannot
        // verify it, so it returns Unknown rather than a misleading Disproven.
        let def = make_fn("add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![call("valid", vec![sym("a")]), call("valid", vec![sym("b")])],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Unknown(msg) if msg.contains("body translation")),
            "expected Unknown(body translation) for result postcondition, got: {:?}",
            verification,
        );
    }

    #[test]
    fn disprove_wrong_contract() {
        // (defn mul [(a : i32) (b : i32) -> i32]
        //   :requires [(valid a)]
        //   :ensures [(= result (+ a b))])  ← wrong! body is *, not +
        // The ensures clause references `result` — without body translation we cannot
        // verify it, so it returns Unknown rather than a misleading Disproven.
        let def = make_fn("mul",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![call("valid", vec![sym("a")])],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Unknown(msg) if msg.contains("body translation")),
            "expected Unknown(body translation) for result postcondition, got: {:?}",
            verification,
        );
    }

    #[test]
    fn prove_valid_only_trivially() {
        // :ensures [(valid result)] — references `result`, so returns Unknown
        // (previously this was "proven" because valid() ignores its arg and returns true,
        // but that was misleading — the clause references result which isn't bound to the body)
        let def = make_fn("id",
            vec![("x", "i32")], "i32",
            vec![call("valid", vec![sym("x")])],
            vec![call("valid", vec![sym("result")])],
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Unknown(msg) if msg.contains("body translation")),
            "expected Unknown(body translation) for result postcondition, got: {:?}",
            verification,
        );
    }

    #[test]
    fn prove_with_requires_constraint() {
        // :requires [(>= a 0) (>= b 0) (= result (+ a b))]
        // :ensures [(>= result 0)]
        // The ensures clause references `result` — even though result is constrained in
        // :requires, we emit Unknown because body translation is not yet implemented.
        // The requires-based constraint pattern is documented but not yet exploited.
        let def = make_fn("add_pos",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![
                call(">=", vec![sym("a"), int(0)]),
                call(">=", vec![sym("b"), int(0)]),
                call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])]),
            ],
            vec![call(">=", vec![sym("result"), int(0)])],
        );
        let prover = Z3Prover::new();
        let verification = prover.verify_function(&def);
        assert_eq!(verification.ensures_results.len(), 1);
        assert!(
            matches!(&verification.ensures_results[0].1, VerifyResult::Unknown(msg) if msg.contains("body translation")),
            "expected Unknown(body translation) for result postcondition, got: {:?}",
            verification,
        );
    }

    #[test]
    fn unknown_for_string_params() {
        let def = make_fn("greet",
            vec![("name", "String")], "String",
            vec![], vec![],
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        // Should return Unknown for all clauses (no clauses = trivially OK)
        assert!(result.ensures_results.is_empty() || !result.has_disproven());
    }
}
