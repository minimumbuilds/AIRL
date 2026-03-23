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
        b.register_string();
        b.register_map();
        b.register_file_io();
        b.register_ir();
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
        self.register("head", builtin_head);
        self.register("tail", builtin_tail);
        self.register("empty?", builtin_empty);
        self.register("cons", builtin_cons);
    }

    // ── String ──────────────────────────────────────────

    fn register_string(&mut self) {
        self.register("char-at", builtin_char_at);
        self.register("substring", builtin_substring);
        self.register("split", builtin_split);
        self.register("join", builtin_join);
        self.register("contains", builtin_contains);
        self.register("starts-with", builtin_starts_with);
        self.register("ends-with", builtin_ends_with);
        self.register("trim", builtin_trim);
        self.register("to-upper", builtin_to_upper);
        self.register("to-lower", builtin_to_lower);
        self.register("replace", builtin_replace);
        self.register("index-of", builtin_index_of);
        self.register("chars", builtin_chars);
    }

    // ── Utility ─────────────────────────────────────────

    fn register_utility(&mut self) {
        self.register("print", builtin_print);
        self.register("type-of", builtin_type_of);
        self.register("shape", builtin_shape);
        self.register("valid", builtin_valid);
    }

    // ── Map ─────────────────────────────────────────────

    fn register_map(&mut self) {
        self.register("map-new", builtin_map_new);
        self.register("map-from", builtin_map_from);
        self.register("map-get", builtin_map_get);
        self.register("map-get-or", builtin_map_get_or);
        self.register("map-set", builtin_map_set);
        self.register("map-has", builtin_map_has);
        self.register("map-remove", builtin_map_remove);
        self.register("map-keys", builtin_map_keys);
        self.register("map-values", builtin_map_values);
        self.register("map-size", builtin_map_size);
    }

    // ── File I/O ────────────────────────────────────────

    fn register_file_io(&mut self) {
        self.register("read-file", builtin_read_file);
        self.register("write-file", builtin_write_file);
        self.register("file-exists?", builtin_file_exists);
    }

    // ── IR VM ────────────────────────────────────────────

    fn register_ir(&mut self) {
        self.register("run-ir", builtin_run_ir);
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
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
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

fn builtin_head(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("head", args, 1)?;
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(RuntimeError::TypeError("head: empty list".into()))
            } else {
                Ok(items[0].clone())
            }
        }
        _ => Err(RuntimeError::TypeError(format!(
            "`head` expects a List, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_tail(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tail", args, 1)?;
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(RuntimeError::TypeError("tail: empty list".into()))
            } else {
                Ok(Value::List(items[1..].to_vec()))
            }
        }
        _ => Err(RuntimeError::TypeError(format!(
            "`tail` expects a List, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_empty(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("empty?", args, 1)?;
    match &args[0] {
        Value::List(items) => Ok(Value::Bool(items.is_empty())),
        _ => Err(RuntimeError::TypeError(format!(
            "`empty?` expects a List, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_cons(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("cons", args, 2)?;
    match &args[1] {
        Value::List(items) => {
            let mut new_items = vec![args[0].clone()];
            new_items.extend(items.iter().cloned());
            Ok(Value::List(new_items))
        }
        _ => Err(RuntimeError::TypeError(format!(
            "`cons` expects a List as second argument, got {}",
            type_name(&args[1])
        ))),
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

// ── String implementations ──────────────────────────────

fn builtin_char_at(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("char-at", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Int(idx)) => {
            let i = *idx as usize;
            match s.chars().nth(i) {
                Some(c) => Ok(Value::Str(c.to_string())),
                None => Err(RuntimeError::Custom(format!(
                    "`char-at` index {} out of bounds for string of length {}",
                    idx,
                    s.chars().count()
                ))),
            }
        }
        _ => Err(RuntimeError::TypeError(
            "`char-at` expects (Str, Int)".into(),
        )),
    }
}

fn builtin_substring(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("substring", args, 3)?;
    match (&args[0], &args[1], &args[2]) {
        (Value::Str(s), Value::Int(start), Value::Int(end)) => {
            let start = *start as usize;
            let end = *end as usize;
            if end < start {
                return Err(RuntimeError::Custom(format!(
                    "`substring` end ({}) < start ({})",
                    end, start
                )));
            }
            let result: String = s.chars().skip(start).take(end - start).collect();
            Ok(Value::Str(result))
        }
        _ => Err(RuntimeError::TypeError(
            "`substring` expects (Str, Int, Int)".into(),
        )),
    }
}

fn builtin_split(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("split", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(delim)) => {
            let parts: Vec<Value> = s
                .split(delim.as_str())
                .map(|p| Value::Str(p.to_string()))
                .collect();
            Ok(Value::List(parts))
        }
        _ => Err(RuntimeError::TypeError(
            "`split` expects (Str, Str)".into(),
        )),
    }
}

fn builtin_join(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("join", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Str(sep)) => {
            let mut parts = Vec::new();
            for item in items {
                match item {
                    Value::Str(s) => parts.push(s.clone()),
                    other => parts.push(format!("{}", other)),
                }
            }
            Ok(Value::Str(parts.join(sep.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "`join` expects (List, Str)".into(),
        )),
    }
}

fn builtin_contains(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("contains", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(sub)) => Ok(Value::Bool(s.contains(sub.as_str()))),
        _ => Err(RuntimeError::TypeError(
            "`contains` expects (Str, Str)".into(),
        )),
    }
}

fn builtin_starts_with(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("starts-with", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(prefix)) => {
            Ok(Value::Bool(s.starts_with(prefix.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "`starts-with` expects (Str, Str)".into(),
        )),
    }
}

fn builtin_ends_with(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("ends-with", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(suffix)) => {
            Ok(Value::Bool(s.ends_with(suffix.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "`ends-with` expects (Str, Str)".into(),
        )),
    }
}

fn builtin_trim(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("trim", args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.trim().to_string())),
        _ => Err(RuntimeError::TypeError(
            "`trim` expects a Str argument".into(),
        )),
    }
}

fn builtin_to_upper(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("to-upper", args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.to_uppercase())),
        _ => Err(RuntimeError::TypeError(
            "`to-upper` expects a Str argument".into(),
        )),
    }
}

fn builtin_to_lower(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("to-lower", args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(s.to_lowercase())),
        _ => Err(RuntimeError::TypeError(
            "`to-lower` expects a Str argument".into(),
        )),
    }
}

fn builtin_replace(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("replace", args, 3)?;
    match (&args[0], &args[1], &args[2]) {
        (Value::Str(s), Value::Str(old), Value::Str(new)) => {
            Ok(Value::Str(s.replace(old.as_str(), new.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "`replace` expects (Str, Str, Str)".into(),
        )),
    }
}

fn builtin_index_of(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("index-of", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(sub)) => {
            // Find the byte offset, then convert to char index
            match s.find(sub.as_str()) {
                Some(byte_offset) => {
                    let char_index = s[..byte_offset].chars().count() as i64;
                    Ok(Value::Int(char_index))
                }
                None => Ok(Value::Int(-1)),
            }
        }
        _ => Err(RuntimeError::TypeError(
            "`index-of` expects (Str, Str)".into(),
        )),
    }
}

fn builtin_chars(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("chars", args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let char_list: Vec<Value> = s
                .chars()
                .map(|c| Value::Str(c.to_string()))
                .collect();
            Ok(Value::List(char_list))
        }
        _ => Err(RuntimeError::TypeError(
            "`chars` expects a Str argument".into(),
        )),
    }
}

// ── Map implementations ─────────────────────────────────

fn builtin_map_new(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-new", args, 0)?;
    Ok(Value::Map(std::collections::HashMap::new()))
}

fn builtin_map_from(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-from", args, 1)?;
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return Err(RuntimeError::TypeError(
            "`map-from` expects a List argument".into(),
        )),
    };
    if items.len() % 2 != 0 {
        return Err(RuntimeError::TypeError(
            "`map-from` expects an even-length list of [key value ...] pairs".into(),
        ));
    }
    let mut m = std::collections::HashMap::new();
    for chunk in items.chunks(2) {
        let key = match &chunk[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(RuntimeError::TypeError(
                "`map-from`: keys must be strings".into(),
            )),
        };
        m.insert(key, chunk[1].clone());
    }
    Ok(Value::Map(m))
}

fn builtin_map_get(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-get", args, 2)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-get`: first argument must be a Map".into(),
        )),
    };
    let key = match &args[1] {
        Value::Str(s) => s,
        _ => return Err(RuntimeError::TypeError(
            "`map-get`: key must be a String".into(),
        )),
    };
    Ok(m.get(key.as_str()).cloned().unwrap_or(Value::Nil))
}

fn builtin_map_get_or(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-get-or", args, 3)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-get-or`: first argument must be a Map".into(),
        )),
    };
    let key = match &args[1] {
        Value::Str(s) => s,
        _ => return Err(RuntimeError::TypeError(
            "`map-get-or`: key must be a String".into(),
        )),
    };
    Ok(m.get(key.as_str()).cloned().unwrap_or_else(|| args[2].clone()))
}

fn builtin_map_set(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-set", args, 3)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-set`: first argument must be a Map".into(),
        )),
    };
    let key = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`map-set`: key must be a String".into(),
        )),
    };
    let mut new_map = m.clone();
    new_map.insert(key, args[2].clone());
    Ok(Value::Map(new_map))
}

fn builtin_map_has(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-has", args, 2)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-has`: first argument must be a Map".into(),
        )),
    };
    let key = match &args[1] {
        Value::Str(s) => s,
        _ => return Err(RuntimeError::TypeError(
            "`map-has`: key must be a String".into(),
        )),
    };
    Ok(Value::Bool(m.contains_key(key.as_str())))
}

fn builtin_map_remove(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-remove", args, 2)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-remove`: first argument must be a Map".into(),
        )),
    };
    let key = match &args[1] {
        Value::Str(s) => s,
        _ => return Err(RuntimeError::TypeError(
            "`map-remove`: key must be a String".into(),
        )),
    };
    let mut new_map = m.clone();
    new_map.remove(key.as_str());
    Ok(Value::Map(new_map))
}

fn builtin_map_keys(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-keys", args, 1)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-keys`: argument must be a Map".into(),
        )),
    };
    let mut keys: Vec<String> = m.keys().cloned().collect();
    keys.sort();
    Ok(Value::List(keys.into_iter().map(Value::Str).collect()))
}

fn builtin_map_values(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-values", args, 1)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-values`: argument must be a Map".into(),
        )),
    };
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort();
    Ok(Value::List(keys.into_iter().map(|k| m[k].clone()).collect()))
}

fn builtin_map_size(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("map-size", args, 1)?;
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(RuntimeError::TypeError(
            "`map-size`: argument must be a Map".into(),
        )),
    };
    Ok(Value::Int(m.len() as i64))
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
        Value::Map(_) => "Map",
        Value::Function(_) => "Function",
        Value::Lambda(_) => "Lambda",
        Value::BuiltinFn(_) => "BuiltinFn",
        Value::IRClosure(_) => "IRClosure",
        Value::IRFuncRef(_) => "IRFuncRef",
        Value::BytecodeClosure(_) => "BytecodeClosure",
    }
}

// ── File I/O implementations ────────────────────────────

/// Validate that a path is relative and doesn't escape the working directory.
fn validate_sandboxed_path(name: &str, path: &str) -> Result<std::path::PathBuf, RuntimeError> {
    if path.starts_with('/') {
        return Err(RuntimeError::Custom(format!(
            "{}: path must be relative, got absolute path '{}'", name, path
        )));
    }
    if path.contains("..") {
        return Err(RuntimeError::Custom(format!(
            "{}: path cannot contain '..': '{}'", name, path
        )));
    }
    Ok(std::path::PathBuf::from(path))
}

fn builtin_read_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("read-file", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("read-file: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("read-file", &path)?;
    match std::fs::read_to_string(&validated) {
        Ok(content) => Ok(Value::Str(content)),
        Err(e) => Err(RuntimeError::Custom(format!("read-file: {}: {}", path, e))),
    }
}

fn builtin_write_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("write-file", args, 2)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("write-file: first argument must be a string path".into())),
    };
    let content = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("write-file: second argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("write-file", &path)?;
    // Create parent directories if needed
    if let Some(parent) = validated.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RuntimeError::Custom(format!("write-file: cannot create directory: {}", e))
            })?;
        }
    }
    match std::fs::write(&validated, content) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(e) => Err(RuntimeError::Custom(format!("write-file: {}: {}", path, e))),
    }
}

fn builtin_file_exists(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("file-exists?", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("file-exists?: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("file-exists?", &path)?;
    Ok(Value::Bool(validated.exists()))
}

// ── IR VM implementation ─────────────────────────────────

fn builtin_run_ir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("run-ir", args, 1)?;
    let ir_list = match &args[0] {
        Value::List(items) => items.clone(),
        _ => return Err(RuntimeError::TypeError("run-ir: expected list of IR nodes".into())),
    };
    let ir_nodes: Vec<crate::ir::IRNode> = ir_list
        .iter()
        .map(crate::ir_marshal::value_to_ir)
        .collect::<Result<_, _>>()?;
    let mut vm = crate::ir_vm::IrVm::new();
    vm.exec_program(&ir_nodes)
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
    fn length_str_non_ascii() {
        let b = builtins();
        // "café" is 4 characters but 5 bytes (é is 2 bytes in UTF-8)
        assert_eq!(
            call(&b, "length", &[Value::Str("café".into())]).unwrap(),
            Value::Int(4)
        );
        // em-dash "—" is 1 character but 3 bytes
        assert_eq!(
            call(&b, "length", &[Value::Str("a—b".into())]).unwrap(),
            Value::Int(3)
        );
        // length and char-at must agree: char-at at last valid index should succeed
        let s = Value::Str("café".into());
        let len = call(&b, "length", &[s.clone()]).unwrap();
        if let Value::Int(n) = len {
            // char-at at index n-1 should work, index n should fail
            assert!(call(&b, "char-at", &[s.clone(), Value::Int(n - 1)]).is_ok());
            assert!(call(&b, "char-at", &[s, Value::Int(n)]).is_err());
        }
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
        assert!(b.has("char-at"));
        assert!(!b.has("nonexistent"));
    }

    // ── String ─────────────────────────────────────────

    #[test]
    fn char_at() {
        let b = builtins();
        assert_eq!(
            call(&b, "char-at", &[Value::Str("hello".into()), Value::Int(0)]).unwrap(),
            Value::Str("h".into())
        );
        assert_eq!(
            call(&b, "char-at", &[Value::Str("hello".into()), Value::Int(4)]).unwrap(),
            Value::Str("o".into())
        );
    }

    #[test]
    fn char_at_out_of_bounds() {
        let b = builtins();
        let r = call(&b, "char-at", &[Value::Str("hi".into()), Value::Int(5)]);
        assert!(matches!(r, Err(RuntimeError::Custom(_))));
    }

    #[test]
    fn substring_basic() {
        let b = builtins();
        assert_eq!(
            call(&b, "substring", &[Value::Str("hello world".into()), Value::Int(0), Value::Int(5)]).unwrap(),
            Value::Str("hello".into())
        );
    }

    #[test]
    fn split_and_join() {
        let b = builtins();
        let split_result = call(&b, "split", &[Value::Str("a,b,c".into()), Value::Str(",".into())]).unwrap();
        assert_eq!(
            split_result,
            Value::List(vec![Value::Str("a".into()), Value::Str("b".into()), Value::Str("c".into())])
        );
        let join_result = call(&b, "join", &[split_result, Value::Str("-".into())]).unwrap();
        assert_eq!(join_result, Value::Str("a-b-c".into()));
    }

    #[test]
    fn contains_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "contains", &[Value::Str("hello world".into()), Value::Str("world".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "contains", &[Value::Str("hello".into()), Value::Str("xyz".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn starts_ends_with() {
        let b = builtins();
        assert_eq!(
            call(&b, "starts-with", &[Value::Str("hello".into()), Value::Str("hel".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "ends-with", &[Value::Str("hello".into()), Value::Str("llo".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn trim_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "trim", &[Value::Str("  hello  ".into())]).unwrap(),
            Value::Str("hello".into())
        );
    }

    #[test]
    fn to_upper_lower() {
        let b = builtins();
        assert_eq!(
            call(&b, "to-upper", &[Value::Str("hello".into())]).unwrap(),
            Value::Str("HELLO".into())
        );
        assert_eq!(
            call(&b, "to-lower", &[Value::Str("HELLO".into())]).unwrap(),
            Value::Str("hello".into())
        );
    }

    #[test]
    fn replace_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "replace", &[Value::Str("hello world".into()), Value::Str("world".into()), Value::Str("AIRL".into())]).unwrap(),
            Value::Str("hello AIRL".into())
        );
    }

    #[test]
    fn index_of_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "index-of", &[Value::Str("hello world".into()), Value::Str("world".into())]).unwrap(),
            Value::Int(6)
        );
        assert_eq!(
            call(&b, "index-of", &[Value::Str("hello".into()), Value::Str("xyz".into())]).unwrap(),
            Value::Int(-1)
        );
    }

    #[test]
    fn chars_str() {
        let b = builtins();
        assert_eq!(
            call(&b, "chars", &[Value::Str("abc".into())]).unwrap(),
            Value::List(vec![Value::Str("a".into()), Value::Str("b".into()), Value::Str("c".into())])
        );
    }

    #[test]
    fn chars_unicode() {
        let b = builtins();
        let result = call(&b, "chars", &[Value::Str("hi".into())]).unwrap();
        // Unicode: each emoji is one char
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 2);
        }
    }

    // ── File I/O tests ──────────────────────────────────

    #[test]
    fn file_exists_true() {
        let b = builtins();
        // Cargo.toml always exists at the workspace root (CWD during tests)
        let result = call(&b, "file-exists?", &[Value::Str("Cargo.toml".into())]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn file_exists_false() {
        let b = builtins();
        let result = call(&b, "file-exists?", &[Value::Str("nonexistent_file_xyz.airl".into())]).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn read_write_file_round_trip() {
        let b = builtins();
        let tmp_path = "test_output_rw_roundtrip.tmp";
        let content = "hello from AIRL";

        // Write
        let write_result = call(&b, "write-file", &[
            Value::Str(tmp_path.into()),
            Value::Str(content.into()),
        ]).unwrap();
        assert_eq!(write_result, Value::Bool(true));

        // Read back
        let read_result = call(&b, "read-file", &[Value::Str(tmp_path.into())]).unwrap();
        assert_eq!(read_result, Value::Str(content.into()));

        // Exists
        let exists = call(&b, "file-exists?", &[Value::Str(tmp_path.into())]).unwrap();
        assert_eq!(exists, Value::Bool(true));

        // Clean up
        std::fs::remove_file(tmp_path).ok();
    }

    #[test]
    fn read_file_not_found() {
        let b = builtins();
        let result = call(&b, "read-file", &[Value::Str("no_such_file.txt".into())]);
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_rejects_absolute_path() {
        let b = builtins();
        let result = call(&b, "read-file", &[Value::Str("/etc/passwd".into())]);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("must be relative"), "error: {}", err);
    }

    #[test]
    fn sandbox_rejects_dotdot() {
        let b = builtins();
        let result = call(&b, "read-file", &[Value::Str("../../../etc/passwd".into())]);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains(".."), "error: {}", err);
    }

    #[test]
    fn write_file_sandbox_rejects_absolute() {
        let b = builtins();
        let result = call(&b, "write-file", &[
            Value::Str("/tmp/airl_sandbox_test.txt".into()),
            Value::Str("nope".into()),
        ]);
        assert!(result.is_err());
    }
}
