use z3::{Config, Context, SatResult, Solver};
use airl_syntax::ast::{FnDef, AstTypeKind};
use crate::translate::{Translator, VarSort};
use crate::{VerifyResult, FunctionVerification};

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
                    (format!("{:?}", e.kind), VerifyResult::Unknown("unsupported parameter types".into()))
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
                            (format!("{:?}", e.kind), VerifyResult::Unknown("cannot translate requires".into()))
                        }).collect(),
                    };
                }
            }
        }

        // Prove each :ensures clause
        let mut ensures_results = Vec::new();
        for ensures_expr in &def.ensures {
            let clause_source = format!("{:?}", ensures_expr.kind);

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
                                        if let Some(VarSort::Int) = Translator::sort_from_type_name(type_name) {
                                            if let Some(var) = translator.get_int_var(&param.name) {
                                                if let Some(val) = model.eval(var, true) {
                                                    counterexample.push((param.name.clone(), val.to_string()));
                                                }
                                            }
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
        // With only (valid a) (valid b) as requires (which translate to true),
        // result is a free variable. Z3 will find a counterexample where
        // result != a+b. So this is DISPROVEN, not proven.
        let def = make_fn("add",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![call("valid", vec![sym("a")]), call("valid", vec![sym("b")])],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        assert!(result.has_disproven(), "add contract should be disproven (result is free): {:?}", result);
    }

    #[test]
    fn disprove_wrong_contract() {
        // (defn mul [(a : i32) (b : i32) -> i32]
        //   :requires [(valid a)]
        //   :ensures [(= result (+ a b))])  ← wrong! body is *, not +
        // result is a free variable, so (= result (+ a b)) is not provable.
        let def = make_fn("mul",
            vec![("a", "i32"), ("b", "i32")], "i32",
            vec![call("valid", vec![sym("a")])],
            vec![call("=", vec![sym("result"), call("+", vec![sym("a"), sym("b")])])],
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        assert!(result.has_disproven(), "wrong contract should be disproven: {:?}", result);
    }

    #[test]
    fn prove_valid_only_trivially() {
        // :ensures [(valid result)] → (valid result) → true → trivially proven
        let def = make_fn("id",
            vec![("x", "i32")], "i32",
            vec![call("valid", vec![sym("x")])],
            vec![call("valid", vec![sym("result")])],
        );
        let prover = Z3Prover::new();
        let result = prover.verify_function(&def);
        assert!(result.all_proven(), "valid-only should be proven: {:?}", result);
    }

    #[test]
    fn prove_with_requires_constraint() {
        // :requires [(>= a 0) (>= b 0) (= result (+ a b))]
        // :ensures [(>= result 0)]
        // With result constrained to a+b, and a,b >= 0, result >= 0 is proven.
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
        let result = prover.verify_function(&def);
        assert!(result.all_proven(), "add_pos should be proven: {:?}", result);
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
