use crate::value::Value;
use crate::error::RuntimeError;
use crate::tensor::TensorValue;
use airl_types::ty::PrimTy;
use std::collections::HashMap;

pub type BuiltinFnPtr = fn(&[Value]) -> Result<Value, RuntimeError>;

pub struct Builtins {
    fns: HashMap<String, BuiltinFnPtr>,
}

impl Builtins {
    pub fn new() -> Self {
        let mut b = Builtins {
            fns: HashMap::new(),
        };
        b.register_arithmetic();
        b.register_comparison();
        b.register_logic();
        b.register_tensor();
        b.register_collections();
        b.register_utility();
        b
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinFnPtr> {
        self.fns.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    fn register(&mut self, name: &str, f: BuiltinFnPtr) {
        self.fns.insert(name.to_string(), f);
    }

    // ── Arithmetic ──────────────────────────────────────

    fn register_arithmetic(&mut self) {
        self.register("+", builtin_add);
        self.register("-", builtin_sub);
        self.register("*", builtin_mul);
        self.register("/", builtin_div);
        self.register("%", builtin_rem);
    }

    // ── Comparison ──────────────────────────────────────

    fn register_comparison(&mut self) {
        self.register("=", builtin_eq);
        self.register("!=", builtin_neq);
        self.register("<", builtin_lt);
        self.register(">", builtin_gt);
        self.register("<=", builtin_le);
        self.register(">=", builtin_ge);
    }

    // ── Logic ───────────────────────────────────────────

    fn register_logic(&mut self) {
        self.register("and", builtin_and);
        self.register("or", builtin_or);
        self.register("not", builtin_not);
        self.register("xor", builtin_xor);
    }

    // ── Tensor ──────────────────────────────────────────

    fn register_tensor(&mut self) {
        self.register("tensor.zeros", builtin_tensor_zeros);
        self.register("tensor.ones", builtin_tensor_ones);
        self.register("tensor.rand", builtin_tensor_rand);
        self.register("tensor.identity", builtin_tensor_identity);
        self.register("tensor.add", builtin_tensor_add);
        self.register("tensor.mul", builtin_tensor_mul);
        self.register("tensor.matmul", builtin_tensor_matmul);
        self.register("tensor.reshape", builtin_tensor_reshape);
        self.register("tensor.transpose", builtin_tensor_transpose);
        self.register("tensor.softmax", builtin_tensor_softmax);
        self.register("tensor.sum", builtin_tensor_sum);
        self.register("tensor.max", builtin_tensor_max);
        self.register("tensor.slice", builtin_tensor_slice);
    }

    // ── Collections ─────────────────────────────────────

    fn register_collections(&mut self) {
        self.register("length", builtin_length);
        self.register("at", builtin_at);
        self.register("append", builtin_append);
    }

    // ── Utility ─────────────────────────────────────────

    fn register_utility(&mut self) {
        self.register("print", builtin_print);
        self.register("type-of", builtin_type_of);
        self.register("shape", builtin_shape);
        self.register("valid", builtin_valid);
    }
}

// ── Arithmetic implementations ──────────────────────────

fn expect_arity(name: &str, args: &[Value], n: usize) -> Result<(), RuntimeError> {
    if args.len() != n {
        return Err(RuntimeError::TypeError(format!(
            "`{}` expects {} argument(s), got {}",
            name,
            n,
            args.len()
        )));
    }
    Ok(())
}

fn builtin_add(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("+", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(*b))),
        (Value::UInt(a), Value::UInt(b)) => Ok(Value::UInt(a.wrapping_add(*b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
        _ => Err(RuntimeError::TypeError(format!(
            "`+` type mismatch: {} and {}",
            type_name(&args[0]),
            type_name(&args[1])
        ))),
    }
}

fn builtin_sub(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("-", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_sub(*b))),
        (Value::UInt(a), Value::UInt(b)) => Ok(Value::UInt(a.wrapping_sub(*b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        _ => Err(RuntimeError::TypeError(format!(
            "`-` type mismatch: {} and {}",
            type_name(&args[0]),
            type_name(&args[1])
        ))),
    }
}

fn builtin_mul(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("*", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_mul(*b))),
        (Value::UInt(a), Value::UInt(b)) => Ok(Value::UInt(a.wrapping_mul(*b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        _ => Err(RuntimeError::TypeError(format!(
            "`*` type mismatch: {} and {}",
            type_name(&args[0]),
            type_name(&args[1])
        ))),
    }
}

fn builtin_div(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("/", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::Int(a / b))
            }
        }
        (Value::UInt(a), Value::UInt(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::UInt(a / b))
            }
        }
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        _ => Err(RuntimeError::TypeError(format!(
            "`/` type mismatch: {} and {}",
            type_name(&args[0]),
            type_name(&args[1])
        ))),
    }
}

fn builtin_rem(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("%", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::Int(a % b))
            }
        }
        (Value::UInt(a), Value::UInt(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::UInt(a % b))
            }
        }
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
        _ => Err(RuntimeError::TypeError(format!(
            "`%` type mismatch: {} and {}",
            type_name(&args[0]),
            type_name(&args[1])
        ))),
    }
}

// ── Comparison implementations ──────────────────────────

fn builtin_eq(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("=", args, 2)?;
    Ok(Value::Bool(args[0] == args[1]))
}

fn builtin_neq(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("!=", args, 2)?;
    Ok(Value::Bool(args[0] != args[1]))
}

macro_rules! cmp_builtin {
    ($name:ident, $op_name:literal, $op:tt) => {
        fn $name(args: &[Value]) -> Result<Value, RuntimeError> {
            expect_arity($op_name, args, 2)?;
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a $op b)),
                (Value::UInt(a), Value::UInt(b)) => Ok(Value::Bool(a $op b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a $op b)),
                (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a $op b)),
                _ => Err(RuntimeError::TypeError(format!(
                    "`{}` type mismatch: {} and {}",
                    $op_name,
                    type_name(&args[0]),
                    type_name(&args[1])
                ))),
            }
        }
    };
}

cmp_builtin!(builtin_lt, "<", <);
cmp_builtin!(builtin_gt, ">", >);
cmp_builtin!(builtin_le, "<=", <=);
cmp_builtin!(builtin_ge, ">=", >=);

// ── Logic implementations ───────────────────────────────

fn builtin_and(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("and", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        _ => Err(RuntimeError::TypeError(
            "`and` expects two Bool arguments".into(),
        )),
    }
}

fn builtin_or(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("or", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(RuntimeError::TypeError(
            "`or` expects two Bool arguments".into(),
        )),
    }
}

fn builtin_not(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("not", args, 1)?;
    match &args[0] {
        Value::Bool(a) => Ok(Value::Bool(!a)),
        _ => Err(RuntimeError::TypeError(
            "`not` expects one Bool argument".into(),
        )),
    }
}

fn builtin_xor(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("xor", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a ^ *b)),
        _ => Err(RuntimeError::TypeError(
            "`xor` expects two Bool arguments".into(),
        )),
    }
}

// ── Tensor implementations ──────────────────────────────

fn extract_shape(args: &[Value]) -> Result<Vec<usize>, RuntimeError> {
    match args.first() {
        Some(Value::List(items)) => {
            let mut shape = Vec::new();
            for item in items {
                match item {
                    Value::Int(n) if *n >= 0 => shape.push(*n as usize),
                    Value::UInt(n) => shape.push(*n as usize),
                    _ => {
                        return Err(RuntimeError::TypeError(
                            "shape elements must be non-negative integers".into(),
                        ))
                    }
                }
            }
            Ok(shape)
        }
        _ => Err(RuntimeError::TypeError(
            "expected a list for shape".into(),
        )),
    }
}

fn extract_tensor<'a>(val: &'a Value, name: &str) -> Result<&'a TensorValue, RuntimeError> {
    match val {
        Value::Tensor(t) => Ok(t),
        _ => Err(RuntimeError::TypeError(format!(
            "`{}` expects a tensor argument, got {}",
            name,
            type_name(val)
        ))),
    }
}

fn builtin_tensor_zeros(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.zeros", args, 1)?;
    let shape = extract_shape(args)?;
    Ok(Value::Tensor(Box::new(TensorValue::zeros(PrimTy::F32, shape))))
}

fn builtin_tensor_ones(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.ones", args, 1)?;
    let shape = extract_shape(args)?;
    Ok(Value::Tensor(Box::new(TensorValue::ones(PrimTy::F32, shape))))
}

fn builtin_tensor_rand(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.rand", args, 2)?;
    let shape = extract_shape(&args[0..1])?;
    let seed = match &args[1] {
        Value::Int(n) => *n as u64,
        Value::UInt(n) => *n,
        _ => {
            return Err(RuntimeError::TypeError(
                "tensor.rand expects an integer seed".into(),
            ))
        }
    };
    Ok(Value::Tensor(Box::new(TensorValue::rand(PrimTy::F32, shape, seed))))
}

fn builtin_tensor_identity(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.identity", args, 1)?;
    let n = match &args[0] {
        Value::Int(n) if *n >= 0 => *n as usize,
        Value::UInt(n) => *n as usize,
        _ => {
            return Err(RuntimeError::TypeError(
                "tensor.identity expects a non-negative integer".into(),
            ))
        }
    };
    Ok(Value::Tensor(Box::new(TensorValue::identity(PrimTy::F32, n))))
}

fn builtin_tensor_add(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.add", args, 2)?;
    let a = extract_tensor(&args[0], "tensor.add")?;
    let b = extract_tensor(&args[1], "tensor.add")?;
    Ok(Value::Tensor(Box::new(a.add(b)?)))
}

fn builtin_tensor_mul(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.mul", args, 2)?;
    let a = extract_tensor(&args[0], "tensor.mul")?;
    let b = extract_tensor(&args[1], "tensor.mul")?;
    Ok(Value::Tensor(Box::new(a.mul(b)?)))
}

fn builtin_tensor_matmul(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.matmul", args, 2)?;
    let a = extract_tensor(&args[0], "tensor.matmul")?;
    let b = extract_tensor(&args[1], "tensor.matmul")?;
    Ok(Value::Tensor(Box::new(a.matmul(b)?)))
}

fn builtin_tensor_reshape(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.reshape", args, 2)?;
    let t = extract_tensor(&args[0], "tensor.reshape")?;
    let shape = extract_shape(&args[1..2])?;
    Ok(Value::Tensor(Box::new(t.reshape(shape)?)))
}

fn builtin_tensor_transpose(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.transpose", args, 1)?;
    let t = extract_tensor(&args[0], "tensor.transpose")?;
    Ok(Value::Tensor(Box::new(t.transpose()?)))
}

fn builtin_tensor_softmax(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.softmax", args, 1)?;
    let t = extract_tensor(&args[0], "tensor.softmax")?;
    Ok(Value::Tensor(Box::new(t.softmax())))
}

fn builtin_tensor_sum(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.sum", args, 1)?;
    let t = extract_tensor(&args[0], "tensor.sum")?;
    Ok(Value::Float(t.sum()))
}

fn builtin_tensor_max(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.max", args, 1)?;
    let t = extract_tensor(&args[0], "tensor.max")?;
    Ok(Value::Float(t.max()))
}

fn builtin_tensor_slice(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tensor.slice", args, 3)?;
    let t = extract_tensor(&args[0], "tensor.slice")?;
    let start = match &args[1] {
        Value::Int(n) if *n >= 0 => *n as usize,
        Value::UInt(n) => *n as usize,
        _ => {
            return Err(RuntimeError::TypeError(
                "tensor.slice start must be a non-negative integer".into(),
            ))
        }
    };
    let end = match &args[2] {
        Value::Int(n) if *n >= 0 => *n as usize,
        Value::UInt(n) => *n as usize,
        _ => {
            return Err(RuntimeError::TypeError(
                "tensor.slice end must be a non-negative integer".into(),
            ))
        }
    };
    Ok(Value::Tensor(Box::new(t.slice(start, end)?)))
}

// ── Collection implementations ──────────────────────────

fn builtin_length(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("length", args, 1)?;
    match &args[0] {
        Value::List(items) => Ok(Value::Int(items.len() as i64)),
        Value::Str(s) => Ok(Value::Int(s.len() as i64)),
        _ => Err(RuntimeError::TypeError(format!(
            "`length` expects List or Str, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_at(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("at", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= items.len() {
                Err(RuntimeError::IndexOutOfBounds {
                    index: i,
                    len: items.len(),
                })
            } else {
                Ok(items[i].clone())
            }
        }
        _ => Err(RuntimeError::TypeError(
            "`at` expects (List, Int)".into(),
        )),
    }
}

fn builtin_append(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("append", args, 2)?;
    match &args[0] {
        Value::List(items) => {
            let mut new_items = items.clone();
            new_items.push(args[1].clone());
            Ok(Value::List(new_items))
        }
        _ => Err(RuntimeError::TypeError(
            "`append` expects a List as first argument".into(),
        )),
    }
}

// ── Utility implementations ─────────────────────────────

fn builtin_print(args: &[Value]) -> Result<Value, RuntimeError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg);
    }
    println!();
    Ok(Value::Unit)
}

fn builtin_type_of(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("type-of", args, 1)?;
    Ok(Value::Str(type_name(&args[0]).to_string()))
}

fn builtin_shape(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("shape", args, 1)?;
    match &args[0] {
        Value::Tensor(t) => {
            let dims: Vec<Value> = t.shape.iter().map(|&d| Value::Int(d as i64)).collect();
            Ok(Value::List(dims))
        }
        _ => Err(RuntimeError::TypeError(format!(
            "`shape` expects a Tensor, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_valid(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("valid", args, 1)?;
    Ok(Value::Bool(true))
}

// ── Helper ──────────────────────────────────────────────

fn type_name(val: &Value) -> &'static str {
    match val {
        Value::Int(_) => "Int",
        Value::UInt(_) => "UInt",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "Str",
        Value::Nil => "Nil",
        Value::Unit => "Unit",
        Value::Tensor(_) => "Tensor",
        Value::List(_) => "List",
        Value::Tuple(_) => "Tuple",
        Value::Variant(_, _) => "Variant",
        Value::Struct(_) => "Struct",
        Value::Function(_) => "Function",
        Value::Lambda(_) => "Lambda",
        Value::BuiltinFn(_) => "BuiltinFn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> Builtins {
        Builtins::new()
    }

    fn call(b: &Builtins, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        let f = b.get(name).expect(&format!("builtin `{}` not found", name));
        f(args)
    }

    // ── Arithmetic ──────────────────────────────────────

    #[test]
    fn add_ints() {
        let b = builtins();
        assert_eq!(call(&b, "+", &[Value::Int(2), Value::Int(3)]).unwrap(), Value::Int(5));
    }

    #[test]
    fn add_floats() {
        let b = builtins();
        assert_eq!(
            call(&b, "+", &[Value::Float(1.5), Value::Float(2.5)]).unwrap(),
            Value::Float(4.0)
        );
    }

    #[test]
    fn sub_ints() {
        let b = builtins();
        assert_eq!(call(&b, "-", &[Value::Int(10), Value::Int(3)]).unwrap(), Value::Int(7));
    }

    #[test]
    fn mul_ints() {
        let b = builtins();
        assert_eq!(call(&b, "*", &[Value::Int(4), Value::Int(5)]).unwrap(), Value::Int(20));
    }

    #[test]
    fn div_ints() {
        let b = builtins();
        assert_eq!(call(&b, "/", &[Value::Int(10), Value::Int(3)]).unwrap(), Value::Int(3));
    }

    #[test]
    fn div_by_zero() {
        let b = builtins();
        let r = call(&b, "/", &[Value::Int(1), Value::Int(0)]);
        assert!(matches!(r, Err(RuntimeError::DivisionByZero)));
    }

    #[test]
    fn rem_ints() {
        let b = builtins();
        assert_eq!(call(&b, "%", &[Value::Int(10), Value::Int(3)]).unwrap(), Value::Int(1));
    }

    #[test]
    fn add_type_mismatch() {
        let b = builtins();
        let r = call(&b, "+", &[Value::Int(1), Value::Float(2.0)]);
        assert!(matches!(r, Err(RuntimeError::TypeError(_))));
    }

    // ── Comparison ──────────────────────────────────────

    #[test]
    fn eq_ints() {
        let b = builtins();
        assert_eq!(call(&b, "=", &[Value::Int(1), Value::Int(1)]).unwrap(), Value::Bool(true));
        assert_eq!(call(&b, "=", &[Value::Int(1), Value::Int(2)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn neq_ints() {
        let b = builtins();
        assert_eq!(call(&b, "!=", &[Value::Int(1), Value::Int(2)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn lt_ints() {
        let b = builtins();
        assert_eq!(call(&b, "<", &[Value::Int(1), Value::Int(2)]).unwrap(), Value::Bool(true));
        assert_eq!(call(&b, "<", &[Value::Int(2), Value::Int(1)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn gt_ints() {
        let b = builtins();
        assert_eq!(call(&b, ">", &[Value::Int(5), Value::Int(3)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn le_ge_ints() {
        let b = builtins();
        assert_eq!(call(&b, "<=", &[Value::Int(3), Value::Int(3)]).unwrap(), Value::Bool(true));
        assert_eq!(call(&b, ">=", &[Value::Int(3), Value::Int(3)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn compare_strings() {
        let b = builtins();
        assert_eq!(
            call(&b, "<", &[Value::Str("a".into()), Value::Str("b".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    // ── Logic ───────────────────────────────────────────

    #[test]
    fn and_bools() {
        let b = builtins();
        assert_eq!(
            call(&b, "and", &[Value::Bool(true), Value::Bool(false)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            call(&b, "and", &[Value::Bool(true), Value::Bool(true)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn or_bools() {
        let b = builtins();
        assert_eq!(
            call(&b, "or", &[Value::Bool(false), Value::Bool(true)]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn not_bool() {
        let b = builtins();
        assert_eq!(call(&b, "not", &[Value::Bool(true)]).unwrap(), Value::Bool(false));
        assert_eq!(call(&b, "not", &[Value::Bool(false)]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn xor_bools() {
        let b = builtins();
        assert_eq!(
            call(&b, "xor", &[Value::Bool(true), Value::Bool(false)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "xor", &[Value::Bool(true), Value::Bool(true)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Collections ─────────────────────────────────────

    #[test]
    fn length_list() {
        let b = builtins();
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(call(&b, "length", &[list]).unwrap(), Value::Int(3));
    }

    #[test]
    fn length_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "length", &[Value::Str("hello".into())]).unwrap(),
            Value::Int(5)
        );
    }

    #[test]
    fn at_list() {
        let b = builtins();
        let list = Value::List(vec![Value::Int(10), Value::Int(20), Value::Int(30)]);
        assert_eq!(call(&b, "at", &[list, Value::Int(1)]).unwrap(), Value::Int(20));
    }

    #[test]
    fn at_out_of_bounds() {
        let b = builtins();
        let list = Value::List(vec![Value::Int(1)]);
        let r = call(&b, "at", &[list, Value::Int(5)]);
        assert!(matches!(r, Err(RuntimeError::IndexOutOfBounds { .. })));
    }

    #[test]
    fn append_list() {
        let b = builtins();
        let list = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let result = call(&b, "append", &[list, Value::Int(3)]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    // ── Utility ─────────────────────────────────────────

    #[test]
    fn type_of_values() {
        let b = builtins();
        assert_eq!(call(&b, "type-of", &[Value::Int(1)]).unwrap(), Value::Str("Int".into()));
        assert_eq!(call(&b, "type-of", &[Value::Bool(true)]).unwrap(), Value::Str("Bool".into()));
        assert_eq!(
            call(&b, "type-of", &[Value::Str("hi".into())]).unwrap(),
            Value::Str("Str".into())
        );
    }

    #[test]
    fn valid_always_true() {
        let b = builtins();
        assert_eq!(call(&b, "valid", &[Value::Int(42)]).unwrap(), Value::Bool(true));
        assert_eq!(call(&b, "valid", &[Value::Nil]).unwrap(), Value::Bool(true));
    }

    // ── Tensor ──────────────────────────────────────────

    #[test]
    fn tensor_zeros_and_shape() {
        let b = builtins();
        let shape = Value::List(vec![Value::Int(2), Value::Int(3)]);
        let t = call(&b, "tensor.zeros", &[shape]).unwrap();
        let s = call(&b, "shape", &[t]).unwrap();
        assert_eq!(s, Value::List(vec![Value::Int(2), Value::Int(3)]));
    }

    #[test]
    fn tensor_ones() {
        let b = builtins();
        let shape = Value::List(vec![Value::Int(3)]);
        let t = call(&b, "tensor.ones", &[shape]).unwrap();
        let sum = call(&b, "tensor.sum", &[t]).unwrap();
        assert_eq!(sum, Value::Float(3.0));
    }

    #[test]
    fn tensor_add_and_sum() {
        let b = builtins();
        let shape = Value::List(vec![Value::Int(2)]);
        let a = call(&b, "tensor.ones", &[shape.clone()]).unwrap();
        let c = call(&b, "tensor.add", &[a.clone(), a]).unwrap();
        let sum = call(&b, "tensor.sum", &[c]).unwrap();
        assert_eq!(sum, Value::Float(4.0));
    }

    #[test]
    fn has_builtin() {
        let b = builtins();
        assert!(b.has("+"));
        assert!(b.has("tensor.matmul"));
        assert!(!b.has("nonexistent"));
    }
}
