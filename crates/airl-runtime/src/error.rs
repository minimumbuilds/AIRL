use airl_syntax::Span;
use airl_contracts::violation::ContractViolation;
use std::fmt;

#[derive(Debug)]
pub enum RuntimeError {
    TypeError(String),
    UseAfterMove { name: String, span: Span },
    ContractViolation(ContractViolation),
    DivisionByZero,
    IndexOutOfBounds { index: usize, len: usize },
    ShapeMismatch { expected: Vec<usize>, got: Vec<usize> },
    UndefinedSymbol(String),
    NotCallable(String),
    TryOnNonResult(String),
    Custom(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::TypeError(msg) => write!(f, "TypeError: {}", msg),
            RuntimeError::UseAfterMove { name, span } => {
                write!(f, "UseAfterMove: `{}` was moved at {}:{}", name, span.line, span.col)
            }
            RuntimeError::ContractViolation(cv) => write!(f, "{}", cv),
            RuntimeError::DivisionByZero => write!(f, "DivisionByZero"),
            RuntimeError::IndexOutOfBounds { index, len } => {
                write!(f, "IndexOutOfBounds: index {} but length is {}", index, len)
            }
            RuntimeError::ShapeMismatch { expected, got } => {
                write!(f, "ShapeMismatch: expected {:?}, got {:?}", expected, got)
            }
            RuntimeError::UndefinedSymbol(name) => {
                write!(f, "UndefinedSymbol: `{}`", name)
            }
            RuntimeError::NotCallable(desc) => {
                write!(f, "NotCallable: {}", desc)
            }
            RuntimeError::TryOnNonResult(desc) => {
                write!(f, "TryOnNonResult: {}", desc)
            }
            RuntimeError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_error_display() {
        let e = RuntimeError::TypeError("expected i32, got bool".into());
        assert!(format!("{}", e).contains("expected i32"));
    }

    #[test]
    fn use_after_move_display() {
        let e = RuntimeError::UseAfterMove {
            name: "x".into(),
            span: Span::new(0, 1, 3, 5),
        };
        let s = format!("{}", e);
        assert!(s.contains("x"));
        assert!(s.contains("3:5"));
    }

    #[test]
    fn division_by_zero_display() {
        let e = RuntimeError::DivisionByZero;
        assert_eq!(format!("{}", e), "DivisionByZero");
    }

    #[test]
    fn index_out_of_bounds_display() {
        let e = RuntimeError::IndexOutOfBounds { index: 10, len: 5 };
        let s = format!("{}", e);
        assert!(s.contains("10"));
        assert!(s.contains("5"));
    }

    #[test]
    fn shape_mismatch_display() {
        let e = RuntimeError::ShapeMismatch {
            expected: vec![2, 3],
            got: vec![3, 2],
        };
        let s = format!("{}", e);
        assert!(s.contains("[2, 3]"));
        assert!(s.contains("[3, 2]"));
    }

    #[test]
    fn undefined_symbol_display() {
        let e = RuntimeError::UndefinedSymbol("foo".into());
        assert!(format!("{}", e).contains("foo"));
    }

    #[test]
    fn not_callable_display() {
        let e = RuntimeError::NotCallable("integer".into());
        assert!(format!("{}", e).contains("integer"));
    }

    #[test]
    fn try_on_non_result_display() {
        let e = RuntimeError::TryOnNonResult("i32".into());
        assert!(format!("{}", e).contains("i32"));
    }

    #[test]
    fn custom_display() {
        let e = RuntimeError::Custom("something went wrong".into());
        assert!(format!("{}", e).contains("something went wrong"));
    }

    #[test]
    fn is_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(RuntimeError::DivisionByZero);
        let _ = format!("{}", e);
    }

    #[test]
    fn contract_violation_display() {
        let cv = ContractViolation {
            function: "add".into(),
            contract_kind: airl_contracts::violation::ContractKind::Requires,
            clause_source: "x > 0".into(),
            bindings: vec![],
            evaluated: "false".into(),
            span: Span::dummy(),
        };
        let e = RuntimeError::ContractViolation(cv);
        let s = format!("{}", e);
        assert!(s.contains("add"));
    }
}
