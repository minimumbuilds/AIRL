use airl_syntax::ast::Expr;
use crate::violation::{ContractViolation, ContractKind};

/// Trait for evaluating contract expressions to booleans.
pub trait ContractEvaluator {
    fn eval_to_bool(&self, expr: &Expr) -> Result<bool, String>;
    fn expr_to_string(&self, expr: &Expr) -> String;
    fn binding_values(&self) -> Vec<(String, String)>;
}

pub struct CheckedVerifier;

impl CheckedVerifier {
    pub fn check_requires(
        &self,
        contracts: &[Expr],
        eval: &dyn ContractEvaluator,
        fn_name: &str,
    ) -> Result<(), ContractViolation> {
        for contract in contracts {
            match eval.eval_to_bool(contract) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(ContractViolation {
                        function: fn_name.to_string(),
                        contract_kind: ContractKind::Requires,
                        clause_source: eval.expr_to_string(contract),
                        bindings: eval.binding_values(),
                        evaluated: "false".to_string(),
                        span: contract.span,
                    });
                }
                Err(e) => {
                    return Err(ContractViolation {
                        function: fn_name.to_string(),
                        contract_kind: ContractKind::Requires,
                        clause_source: eval.expr_to_string(contract),
                        bindings: eval.binding_values(),
                        evaluated: format!("error: {}", e),
                        span: contract.span,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn check_ensures(
        &self,
        contracts: &[Expr],
        eval: &dyn ContractEvaluator,
        fn_name: &str,
    ) -> Result<(), ContractViolation> {
        for contract in contracts {
            match eval.eval_to_bool(contract) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(ContractViolation {
                        function: fn_name.to_string(),
                        contract_kind: ContractKind::Ensures,
                        clause_source: eval.expr_to_string(contract),
                        bindings: eval.binding_values(),
                        evaluated: "false".to_string(),
                        span: contract.span,
                    });
                }
                Err(e) => {
                    return Err(ContractViolation {
                        function: fn_name.to_string(),
                        contract_kind: ContractKind::Ensures,
                        clause_source: eval.expr_to_string(contract),
                        bindings: eval.binding_values(),
                        evaluated: format!("error: {}", e),
                        span: contract.span,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::ast::ExprKind;
    use airl_syntax::Span;

    struct MockEval {
        result: bool,
    }

    impl ContractEvaluator for MockEval {
        fn eval_to_bool(&self, _: &Expr) -> Result<bool, String> {
            Ok(self.result)
        }
        fn expr_to_string(&self, _: &Expr) -> String {
            "mock".into()
        }
        fn binding_values(&self) -> Vec<(String, String)> {
            vec![("x".into(), "42".into())]
        }
    }

    struct ErrorEval;

    impl ContractEvaluator for ErrorEval {
        fn eval_to_bool(&self, _: &Expr) -> Result<bool, String> {
            Err("eval error".into())
        }
        fn expr_to_string(&self, _: &Expr) -> String {
            "bad_expr".into()
        }
        fn binding_values(&self) -> Vec<(String, String)> {
            vec![]
        }
    }

    fn dummy_expr() -> Expr {
        Expr {
            kind: ExprKind::BoolLit(true),
            span: Span::dummy(),
        }
    }

    #[test]
    fn check_requires_passes_when_true() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: true };
        let contracts = vec![dummy_expr()];
        assert!(verifier.check_requires(&contracts, &eval, "my_fn").is_ok());
    }

    #[test]
    fn check_requires_fails_when_false() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        let contracts = vec![dummy_expr()];
        let err = verifier.check_requires(&contracts, &eval, "my_fn").unwrap_err();
        assert_eq!(err.contract_kind, ContractKind::Requires);
        assert_eq!(err.function, "my_fn");
        assert_eq!(err.evaluated, "false");
        assert_eq!(err.clause_source, "mock");
        assert_eq!(err.bindings, vec![("x".to_string(), "42".to_string())]);
    }

    #[test]
    fn check_requires_fails_on_eval_error() {
        let verifier = CheckedVerifier;
        let eval = ErrorEval;
        let contracts = vec![dummy_expr()];
        let err = verifier.check_requires(&contracts, &eval, "my_fn").unwrap_err();
        assert_eq!(err.contract_kind, ContractKind::Requires);
        assert!(err.evaluated.starts_with("error:"));
    }

    #[test]
    fn check_requires_empty_contracts_ok() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        assert!(verifier.check_requires(&[], &eval, "my_fn").is_ok());
    }

    #[test]
    fn check_ensures_passes_when_true() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: true };
        let contracts = vec![dummy_expr()];
        assert!(verifier.check_ensures(&contracts, &eval, "my_fn").is_ok());
    }

    #[test]
    fn check_ensures_fails_when_false() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        let contracts = vec![dummy_expr()];
        let err = verifier.check_ensures(&contracts, &eval, "my_fn").unwrap_err();
        assert_eq!(err.contract_kind, ContractKind::Ensures);
        assert_eq!(err.function, "my_fn");
        assert_eq!(err.evaluated, "false");
    }

    #[test]
    fn check_ensures_fails_on_eval_error() {
        let verifier = CheckedVerifier;
        let eval = ErrorEval;
        let contracts = vec![dummy_expr()];
        let err = verifier.check_ensures(&contracts, &eval, "my_fn").unwrap_err();
        assert_eq!(err.contract_kind, ContractKind::Ensures);
        assert!(err.evaluated.starts_with("error:"));
    }

    #[test]
    fn check_ensures_empty_contracts_ok() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        assert!(verifier.check_ensures(&[], &eval, "my_fn").is_ok());
    }

    #[test]
    fn check_requires_stops_at_first_failure() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        let contracts = vec![dummy_expr(), dummy_expr(), dummy_expr()];
        let err = verifier.check_requires(&contracts, &eval, "fn_a").unwrap_err();
        // Should fail on first clause
        assert_eq!(err.function, "fn_a");
    }

    #[test]
    fn check_ensures_stops_at_first_failure() {
        let verifier = CheckedVerifier;
        let eval = MockEval { result: false };
        let contracts = vec![dummy_expr(), dummy_expr()];
        let err = verifier.check_ensures(&contracts, &eval, "fn_b").unwrap_err();
        assert_eq!(err.function, "fn_b");
    }
}
