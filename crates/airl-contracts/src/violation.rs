use airl_syntax::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractKind {
    Requires,
    Ensures,
    Invariant,
}

#[derive(Debug, Clone)]
pub struct ContractViolation {
    pub function: String,
    pub contract_kind: ContractKind,
    pub clause_source: String,
    pub bindings: Vec<(String, String)>, // (name, value_display)
    pub evaluated: String,
    pub span: Span,
}

impl std::fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.contract_kind {
            ContractKind::Requires => "Requires",
            ContractKind::Ensures => "Ensures",
            ContractKind::Invariant => "Invariant",
        };
        write!(f, "{} contract violated in `{}`: {} evaluated to {}",
            kind, self.function, self.clause_source, self.evaluated)?;
        if !self.bindings.is_empty() {
            write!(f, "\n  with")?;
            for (name, val) in &self.bindings {
                write!(f, " {} = {},", name, val)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_violation(kind: ContractKind) -> ContractViolation {
        ContractViolation {
            function: "my_fn".to_string(),
            contract_kind: kind,
            clause_source: "x > 0".to_string(),
            bindings: vec![("x".to_string(), "-1".to_string())],
            evaluated: "false".to_string(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn contract_kind_eq() {
        assert_eq!(ContractKind::Requires, ContractKind::Requires);
        assert_ne!(ContractKind::Requires, ContractKind::Ensures);
        assert_ne!(ContractKind::Ensures, ContractKind::Invariant);
    }

    #[test]
    fn contract_kind_clone() {
        let k = ContractKind::Invariant;
        let k2 = k.clone();
        assert_eq!(k, k2);
    }

    #[test]
    fn violation_display_requires() {
        let v = make_violation(ContractKind::Requires);
        let s = format!("{}", v);
        assert!(s.contains("my_fn"));
        assert!(s.contains("Requires"));
        assert!(s.contains("x > 0"));
        assert!(s.contains("false"));
    }

    #[test]
    fn violation_display_ensures() {
        let v = make_violation(ContractKind::Ensures);
        let s = format!("{}", v);
        assert!(s.contains("Ensures"));
    }

    #[test]
    fn violation_display_invariant() {
        let v = make_violation(ContractKind::Invariant);
        let s = format!("{}", v);
        assert!(s.contains("Invariant"));
    }

    #[test]
    fn violation_clone() {
        let v = make_violation(ContractKind::Requires);
        let v2 = v.clone();
        assert_eq!(v2.function, v.function);
        assert_eq!(v2.contract_kind, v.contract_kind);
        assert_eq!(v2.clause_source, v.clause_source);
        assert_eq!(v2.bindings, v.bindings);
        assert_eq!(v2.evaluated, v.evaluated);
    }

    #[test]
    fn violation_debug() {
        let v = make_violation(ContractKind::Requires);
        let s = format!("{:?}", v);
        assert!(s.contains("ContractViolation"));
    }

    #[test]
    fn violation_bindings_recorded() {
        let v = make_violation(ContractKind::Requires);
        assert_eq!(v.bindings.len(), 1);
        assert_eq!(v.bindings[0], ("x".to_string(), "-1".to_string()));
    }
}
