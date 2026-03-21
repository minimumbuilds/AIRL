use std::collections::{BTreeMap, HashMap};
use std::fmt;

use airl_syntax::ast::{FnDef, Param, Expr};

/// Runtime function value — a named function with its definition.
#[derive(Debug, Clone)]
pub struct FnValue {
    pub name: String,
    pub def: FnDef,
}

/// Runtime lambda (closure) value with captured environment.
#[derive(Debug, Clone)]
pub struct LambdaValue {
    pub params: Vec<Param>,
    pub body: Expr,
    pub captures: Vec<(String, Value)>,
}

/// The runtime representation of all AIRL values.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    Unit,
    Tensor(Box<crate::tensor::TensorValue>),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Variant(String, Box<Value>),
    Struct(BTreeMap<String, Value>),
    Map(HashMap<String, Value>),
    Function(FnValue),
    Lambda(LambdaValue),
    BuiltinFn(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::UInt(n) => write!(f, "{}", n),
            Value::Float(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    write!(f, "{}.0", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Nil => write!(f, "nil"),
            Value::Unit => write!(f, "()"),
            Value::Tensor(t) => write!(f, "tensor<{:?}, {:?}>", t.dtype, t.shape),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Value::Variant(name, inner) => {
                match inner.as_ref() {
                    Value::Unit => write!(f, "({})", name),
                    _ => write!(f, "({} {})", name, inner),
                }
            }
            Value::Struct(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                write!(f, "{{")?;
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, m[*k])?;
                }
                write!(f, "}}")
            }
            Value::Function(fv) => write!(f, "<fn {}>", fv.name),
            Value::Lambda(_) => write!(f, "<lambda>"),
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::UInt(a), Value::UInt(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Unit, Value::Unit) => true,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Variant(na, va), Value::Variant(nb, vb)) => na == nb && va == vb,
            (Value::Struct(a), Value::Struct(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter().all(|(k, v)| b.get(k).map_or(false, |bv| v == bv))
            }
            (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
            (Value::Tensor(a), Value::Tensor(b)) => {
                a.dtype == b.dtype && a.shape == b.shape && a.data == b.data
            }
            // Functions and lambdas are never equal
            (Value::Function(_), Value::Function(_)) => false,
            (Value::Lambda(_), Value::Lambda(_)) => false,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_display_and_eq() {
        let v = Value::Int(42);
        assert_eq!(format!("{}", v), "42");
        assert_eq!(v, Value::Int(42));
        assert_ne!(v, Value::Int(43));
    }

    #[test]
    fn uint_display_and_eq() {
        let v = Value::UInt(100);
        assert_eq!(format!("{}", v), "100");
        assert_eq!(v, Value::UInt(100));
    }

    #[test]
    fn float_display() {
        assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
        assert_eq!(format!("{}", Value::Float(1.0)), "1.0");
    }

    #[test]
    fn float_eq_bitwise() {
        assert_eq!(Value::Float(0.0), Value::Float(0.0));
        // NaN == NaN when comparing bits
        assert_eq!(Value::Float(f64::NAN), Value::Float(f64::NAN));
        // -0.0 != 0.0 in bit comparison
        assert_ne!(Value::Float(-0.0), Value::Float(0.0));
    }

    #[test]
    fn bool_display_and_eq() {
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(Value::Bool(false), Value::Bool(false));
    }

    #[test]
    fn str_display_and_eq() {
        let v = Value::Str("hello".into());
        assert_eq!(format!("{}", v), "\"hello\"");
        assert_eq!(v, Value::Str("hello".into()));
    }

    #[test]
    fn nil_and_unit() {
        assert_eq!(format!("{}", Value::Nil), "nil");
        assert_eq!(format!("{}", Value::Unit), "()");
        assert_eq!(Value::Nil, Value::Nil);
        assert_eq!(Value::Unit, Value::Unit);
        assert_ne!(Value::Nil, Value::Unit);
    }

    #[test]
    fn list_display_and_eq() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(format!("{}", v), "[1 2 3]");
        assert_eq!(v, Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
    }

    #[test]
    fn tuple_display() {
        let v = Value::Tuple(vec![Value::Int(1), Value::Str("hi".into())]);
        assert_eq!(format!("{}", v), "(1 \"hi\")");
    }

    #[test]
    fn variant_display() {
        let v = Value::Variant("Ok".into(), Box::new(Value::Int(42)));
        assert_eq!(format!("{}", v), "(Ok 42)");
        let v2 = Value::Variant("None".into(), Box::new(Value::Unit));
        assert_eq!(format!("{}", v2), "(None)");
    }

    #[test]
    fn variant_eq() {
        let a = Value::Variant("Ok".into(), Box::new(Value::Int(1)));
        let b = Value::Variant("Ok".into(), Box::new(Value::Int(1)));
        let c = Value::Variant("Err".into(), Box::new(Value::Int(1)));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn struct_display() {
        let mut m = BTreeMap::new();
        m.insert("x".into(), Value::Int(1));
        m.insert("y".into(), Value::Int(2));
        let v = Value::Struct(m);
        assert_eq!(format!("{}", v), "{x: 1, y: 2}");
    }

    #[test]
    fn builtin_fn_display_and_eq() {
        let v = Value::BuiltinFn("print".into());
        assert_eq!(format!("{}", v), "<builtin print>");
        assert_eq!(v, Value::BuiltinFn("print".into()));
    }

    #[test]
    fn function_display() {
        let v = Value::Function(FnValue {
            name: "add".into(),
            def: airl_syntax::ast::FnDef {
                name: "add".into(),
                params: vec![],
                return_type: airl_syntax::ast::AstType {
                    kind: airl_syntax::ast::AstTypeKind::Named("i32".into()),
                    span: airl_syntax::Span::dummy(),
                },
                intent: None,
                requires: vec![],
                ensures: vec![],
                invariants: vec![],
                body: airl_syntax::ast::Expr {
                    kind: airl_syntax::ast::ExprKind::IntLit(0),
                    span: airl_syntax::Span::dummy(),
                },
                execute_on: None,
                priority: None,
                span: airl_syntax::Span::dummy(),
            },
        });
        assert_eq!(format!("{}", v), "<fn add>");
    }

    #[test]
    fn functions_never_equal() {
        let def = airl_syntax::ast::FnDef {
            name: "f".into(),
            params: vec![],
            return_type: airl_syntax::ast::AstType {
                kind: airl_syntax::ast::AstTypeKind::Named("i32".into()),
                span: airl_syntax::Span::dummy(),
            },
            intent: None,
            requires: vec![],
            ensures: vec![],
            invariants: vec![],
            body: airl_syntax::ast::Expr {
                kind: airl_syntax::ast::ExprKind::IntLit(0),
                span: airl_syntax::Span::dummy(),
            },
            execute_on: None,
            priority: None,
            span: airl_syntax::Span::dummy(),
        };
        let a = Value::Function(FnValue { name: "f".into(), def: def.clone() });
        let b = Value::Function(FnValue { name: "f".into(), def });
        assert_ne!(a, b);
    }

    #[test]
    fn map_display_and_eq() {
        let mut m1 = HashMap::new();
        m1.insert("b".into(), Value::Int(2));
        m1.insert("a".into(), Value::Int(1));
        let v1 = Value::Map(m1);
        // Display should sort keys
        assert_eq!(format!("{}", v1), "{a: 1, b: 2}");

        let mut m2 = HashMap::new();
        m2.insert("a".into(), Value::Int(1));
        m2.insert("b".into(), Value::Int(2));
        let v2 = Value::Map(m2);
        assert_eq!(v1, v2);

        let mut m3 = HashMap::new();
        m3.insert("a".into(), Value::Int(1));
        let v3 = Value::Map(m3);
        assert_ne!(v1, v3);

        let empty = Value::Map(HashMap::new());
        assert_eq!(format!("{}", empty), "{}");
    }

    #[test]
    fn cross_type_not_equal() {
        assert_ne!(Value::Int(1), Value::UInt(1));
        assert_ne!(Value::Int(0), Value::Bool(false));
        assert_ne!(Value::Nil, Value::Bool(false));
    }

    #[test]
    fn value_is_clone_and_debug() {
        let v = Value::Int(42);
        let v2 = v.clone();
        let _ = format!("{:?}", v2);
    }
}
