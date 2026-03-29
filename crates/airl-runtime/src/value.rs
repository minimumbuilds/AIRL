use std::collections::HashMap;
use std::fmt;

/// Value enum — used as the bytecode constant representation and for
/// marshalling between RtValue and the bytecode compiler/VM.
///
/// v0.6.0: Runtime-only variants (Tensor, Struct, UInt, IRClosure, Function,
/// Lambda) have been removed. Runtime values are now RtValue from airl-rt.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    Unit,
    List(Vec<Value>),
    IntList(Vec<i64>),  // Specialized homogeneous integer list (cache-friendly, no boxing)
    Tuple(Vec<Value>),
    Variant(String, Box<Value>),
    Map(HashMap<String, Value>),
    BuiltinFn(String),
    IRFuncRef(String),
    BytecodeClosure(crate::bytecode::BytecodeClosureValue),
    Bytes(Vec<u8>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
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
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::IntList(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{}", x)?;
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
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
            Value::IRFuncRef(name) => write!(f, "<ir-fn:{}>", name),
            Value::BytecodeClosure(_) => write!(f, "<bytecode-closure>"),
            Value::Bytes(v) => write!(f, "<Bytes len={}>", v.len()),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Unit, Value::Unit) => true,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::IntList(a), Value::IntList(b)) => a == b,
            (Value::IntList(xs), Value::List(ys)) | (Value::List(ys), Value::IntList(xs)) => {
                xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(x, y)| {
                    matches!(y, Value::Int(n) if *n == *x)
                })
            }
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Variant(na, va), Value::Variant(nb, vb)) => na == nb && va == vb,
            (Value::Map(a), Value::Map(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter().all(|(k, v)| b.get(k).map_or(false, |bv| v == bv))
            }
            (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
            (Value::IRFuncRef(a), Value::IRFuncRef(b)) => a == b,
            (Value::IRFuncRef(_), _) => false,
            (Value::BytecodeClosure(_), _) => false,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    /// Convert any list-like value into a Vec<Value>.
    /// IntList is transparently promoted to a boxed list.
    pub fn into_list(self) -> Vec<Value> {
        match self {
            Value::List(items) => items,
            Value::IntList(items) => items.into_iter().map(Value::Int).collect(),
            Value::Bytes(v) => v.into_iter().map(|b| Value::Int(b as i64)).collect(),
            other => vec![other],
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
    fn float_display() {
        assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
        assert_eq!(format!("{}", Value::Float(1.0)), "1.0");
    }

    #[test]
    fn float_eq_bitwise() {
        assert_eq!(Value::Float(0.0), Value::Float(0.0));
        assert_eq!(Value::Float(f64::NAN), Value::Float(f64::NAN));
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
    fn builtin_fn_display_and_eq() {
        let v = Value::BuiltinFn("print".into());
        assert_eq!(format!("{}", v), "<builtin print>");
        assert_eq!(v, Value::BuiltinFn("print".into()));
    }

    #[test]
    fn map_display_and_eq() {
        let mut m1 = HashMap::new();
        m1.insert("b".into(), Value::Int(2));
        m1.insert("a".into(), Value::Int(1));
        let v1 = Value::Map(m1);
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
        assert_ne!(Value::Int(0), Value::Bool(false));
        assert_ne!(Value::Nil, Value::Bool(false));
    }

    #[test]
    fn value_is_clone_and_debug() {
        let v = Value::Int(42);
        let v2 = v.clone();
        let _ = format!("{:?}", v2);
    }

    #[test]
    fn intlist_display() {
        let v = Value::IntList(vec![1, 2, 3]);
        assert_eq!(format!("{}", v), "[1 2 3]");
    }

    #[test]
    fn intlist_display_empty() {
        let v = Value::IntList(vec![]);
        assert_eq!(format!("{}", v), "[]");
    }

    #[test]
    fn intlist_eq_intlist() {
        assert_eq!(Value::IntList(vec![1, 2, 3]), Value::IntList(vec![1, 2, 3]));
        assert_ne!(Value::IntList(vec![1, 2]), Value::IntList(vec![1, 2, 3]));
    }

    #[test]
    fn intlist_eq_list_cross() {
        let il = Value::IntList(vec![1, 2, 3]);
        let l = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(il, l);
        assert_eq!(l, il);
    }

    #[test]
    fn intlist_neq_list_mixed() {
        let il = Value::IntList(vec![1, 2, 3]);
        let l = Value::List(vec![Value::Int(1), Value::Str("two".into()), Value::Int(3)]);
        assert_ne!(il, l);
    }

    #[test]
    fn intlist_into_list() {
        let il = Value::IntList(vec![10, 20]);
        assert_eq!(il.into_list(), vec![Value::Int(10), Value::Int(20)]);
    }
}
