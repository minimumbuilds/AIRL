use airl_syntax::ast::{Expr, ExprKind};

#[derive(Debug, Clone, PartialEq)]
pub enum ProofResult {
    Proven,
    Disproven(String),
    Unknown(String),
}

pub struct StubProver {
    assumptions: Vec<Expr>,
}

impl StubProver {
    pub fn new(assumptions: Vec<Expr>) -> Self {
        Self { assumptions }
    }

    pub fn prove(&self, claim: &Expr) -> ProofResult {
        // Try strategies in order
        if let Some(result) = self.try_constant_eval(claim) {
            return if result {
                ProofResult::Proven
            } else {
                ProofResult::Disproven("constant evaluation".into())
            };
        }
        if let Some(true) = self.try_identity(claim) {
            return ProofResult::Proven;
        }
        if let Some(true) = self.try_tautology(claim) {
            return ProofResult::Proven;
        }
        ProofResult::Unknown("cannot determine truth of claim".into())
    }

    fn try_constant_eval(&self, expr: &Expr) -> Option<bool> {
        // Handle (= (+ literal literal) literal) → evaluate and compare
        // For now, just check if it's a BoolLit
        match &expr.kind {
            ExprKind::BoolLit(v) => Some(*v),
            _ => None,
        }
    }

    fn try_identity(&self, expr: &Expr) -> Option<bool> {
        // (= x x) → true
        match &expr.kind {
            ExprKind::FnCall(callee, args) if args.len() == 2 => {
                if let ExprKind::SymbolRef(op) = &callee.kind {
                    if op == "=" {
                        // Compare the two args structurally
                        if args[0] == args[1] {
                            return Some(true);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn try_tautology(&self, _expr: &Expr) -> Option<bool> {
        // (or a (not a)) → true — hard to detect without eval
        // For stub, just return None
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::ast::ExprKind;
    use airl_syntax::Span;

    fn bool_expr(v: bool) -> Expr {
        Expr {
            kind: ExprKind::BoolLit(v),
            span: Span::dummy(),
        }
    }

    fn symbol_expr(name: &str) -> Expr {
        Expr {
            kind: ExprKind::SymbolRef(name.to_string()),
            span: Span::dummy(),
        }
    }

    fn int_expr(n: i64) -> Expr {
        Expr {
            kind: ExprKind::IntLit(n),
            span: Span::dummy(),
        }
    }

    fn eq_call(lhs: Expr, rhs: Expr) -> Expr {
        let callee = Expr {
            kind: ExprKind::SymbolRef("=".to_string()),
            span: Span::dummy(),
        };
        Expr {
            kind: ExprKind::FnCall(Box::new(callee), vec![lhs, rhs]),
            span: Span::dummy(),
        }
    }

    fn str_expr(s: &str) -> Expr {
        Expr {
            kind: ExprKind::StrLit(s.to_string()),
            span: Span::dummy(),
        }
    }

    #[test]
    fn prove_bool_true_literal() {
        let prover = StubProver::new(vec![]);
        assert_eq!(prover.prove(&bool_expr(true)), ProofResult::Proven);
    }

    #[test]
    fn prove_bool_false_literal() {
        let prover = StubProver::new(vec![]);
        let result = prover.prove(&bool_expr(false));
        assert_eq!(result, ProofResult::Disproven("constant evaluation".into()));
    }

    #[test]
    fn prove_identity_symbol() {
        // (= x x) → Proven
        let prover = StubProver::new(vec![]);
        let claim = eq_call(symbol_expr("x"), symbol_expr("x"));
        assert_eq!(prover.prove(&claim), ProofResult::Proven);
    }

    #[test]
    fn prove_identity_int() {
        // (= 5 5) → Proven
        let prover = StubProver::new(vec![]);
        let claim = eq_call(int_expr(5), int_expr(5));
        assert_eq!(prover.prove(&claim), ProofResult::Proven);
    }

    #[test]
    fn prove_unknown_complex() {
        // (= x 42) where x != 42 → Unknown
        let prover = StubProver::new(vec![]);
        let claim = eq_call(symbol_expr("x"), int_expr(42));
        assert!(matches!(prover.prove(&claim), ProofResult::Unknown(_)));
    }

    #[test]
    fn prove_unknown_symbol_ref() {
        let prover = StubProver::new(vec![]);
        let claim = symbol_expr("some_condition");
        assert!(matches!(prover.prove(&claim), ProofResult::Unknown(_)));
    }

    #[test]
    fn prove_unknown_int_lit() {
        let prover = StubProver::new(vec![]);
        let claim = int_expr(42);
        assert!(matches!(prover.prove(&claim), ProofResult::Unknown(_)));
    }

    #[test]
    fn prove_unknown_string_lit() {
        let prover = StubProver::new(vec![]);
        let claim = str_expr("hello");
        assert!(matches!(prover.prove(&claim), ProofResult::Unknown(_)));
    }

    #[test]
    fn proof_result_clone_and_debug() {
        let p = ProofResult::Proven;
        let _ = p.clone();
        let _ = format!("{:?}", p);

        let d = ProofResult::Disproven("reason".into());
        let _ = d.clone();
        let _ = format!("{:?}", d);

        let u = ProofResult::Unknown("reason".into());
        let _ = u.clone();
        let _ = format!("{:?}", u);
    }

    #[test]
    fn proof_result_eq() {
        assert_eq!(ProofResult::Proven, ProofResult::Proven);
        assert_ne!(ProofResult::Proven, ProofResult::Disproven("x".into()));
        assert_eq!(
            ProofResult::Unknown("a".into()),
            ProofResult::Unknown("a".into())
        );
    }

    #[test]
    fn prover_with_assumptions_still_works() {
        let assumption = bool_expr(true);
        let prover = StubProver::new(vec![assumption]);
        // Prove something simple still works
        assert_eq!(prover.prove(&bool_expr(true)), ProofResult::Proven);
    }

    #[test]
    fn prove_disproven_message() {
        let prover = StubProver::new(vec![]);
        if let ProofResult::Disproven(msg) = prover.prove(&bool_expr(false)) {
            assert_eq!(msg, "constant evaluation");
        } else {
            panic!("expected Disproven");
        }
    }

    #[test]
    fn prove_unknown_message() {
        let prover = StubProver::new(vec![]);
        if let ProofResult::Unknown(msg) = prover.prove(&symbol_expr("x")) {
            assert_eq!(msg, "cannot determine truth of claim");
        } else {
            panic!("expected Unknown");
        }
    }
}
