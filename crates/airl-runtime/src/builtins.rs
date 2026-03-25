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
        b.register_bytecode();
        b.register_system();
        b.register_float_math();
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
        self.register("at-or", builtin_at_or);
        self.register("set-at", builtin_set_at);
        self.register("list-contains?", builtin_list_contains);
    }

    // ── String ──────────────────────────────────────────

    fn register_string(&mut self) {
        self.register("str", builtin_str);
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
        self.register("println", builtin_println);
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
        self.register("get-args", builtin_get_args);
        self.register("append-file", builtin_append_file);
        self.register("delete-file", builtin_delete_file);
        self.register("delete-dir", builtin_delete_dir);
        self.register("rename-file", builtin_rename_file);
        self.register("create-dir", builtin_create_dir);
        self.register("read-dir", builtin_read_dir);
        self.register("file-size", builtin_file_size);
        self.register("is-dir?", builtin_is_dir);
        #[cfg(feature = "aot")]
        self.register("compile-to-executable", builtin_compile_to_executable);
    }

    // ── Bytecode VM ──────────────────────────────────────

    fn register_bytecode(&mut self) {
        self.register("run-bytecode", builtin_run_bytecode);
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

fn builtin_at_or(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("at-or", args, 3)?;
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= items.len() {
                Ok(args[2].clone()) // default
            } else {
                Ok(items[i].clone())
            }
        }
        _ => Err(RuntimeError::TypeError("`at-or` expects (List, Int, default)".into())),
    }
}

fn builtin_set_at(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("set-at", args, 3)?;
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= items.len() {
                Err(RuntimeError::IndexOutOfBounds { index: i, len: items.len() })
            } else {
                let mut new_items = items.clone();
                new_items[i] = args[2].clone();
                Ok(Value::List(new_items))
            }
        }
        _ => Err(RuntimeError::TypeError("`set-at` expects (List, Int, value)".into())),
    }
}

fn builtin_list_contains(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("list-contains?", args, 2)?;
    match &args[0] {
        Value::List(items) => Ok(Value::Bool(items.contains(&args[1]))),
        _ => Err(RuntimeError::TypeError("`list-contains?` expects (List, value)".into())),
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

/// Display-formatted print (strings quoted, with newline).
/// Matches the driver's `println!("{}", val)` for program results.
fn builtin_println(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("println", args, 1)?;
    println!("{}", args[0]);
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

/// Variadic string concatenation with auto-coercion.
/// (str "hello" " " 42 " " true) → "hello 42 true"
/// Strings are included without quotes; all other types use Display.
fn builtin_str(args: &[Value]) -> Result<Value, RuntimeError> {
    let mut buf = String::new();
    for arg in args {
        match arg {
            Value::Str(s) => buf.push_str(s),
            other => buf.push_str(&format!("{}", other)),
        }
    }
    Ok(Value::Str(buf))
}

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

fn builtin_append_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("append-file", args, 2)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("append-file: first argument must be a string path".into())),
    };
    let content = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("append-file: second argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("append-file", &path)?;
    // Create parent directories if needed
    if let Some(parent) = validated.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RuntimeError::Custom(format!("append-file: cannot create directory: {}", e))
            })?;
        }
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&validated)
        .map_err(|e| RuntimeError::Custom(format!("append-file: {}: {}", path, e)))?;
    file.write_all(content.as_bytes())
        .map_err(|e| RuntimeError::Custom(format!("append-file: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}

fn builtin_delete_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("delete-file", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("delete-file: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("delete-file", &path)?;
    if validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "delete-file: '{}' is a directory, use delete-dir", path
        )));
    }
    std::fs::remove_file(&validated)
        .map_err(|e| RuntimeError::Custom(format!("delete-file: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}

fn builtin_delete_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("delete-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("delete-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("delete-dir", &path)?;
    if validated.exists() && !validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "delete-dir: '{}' is not a directory", path
        )));
    }
    std::fs::remove_dir_all(&validated)
        .map_err(|e| RuntimeError::Custom(format!("delete-dir: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}

fn builtin_rename_file(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("rename-file", args, 2)?;
    let old_path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("rename-file: first argument must be a string path".into())),
    };
    let new_path = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("rename-file: second argument must be a string path".into())),
    };
    let validated_old = validate_sandboxed_path("rename-file", &old_path)?;
    let validated_new = validate_sandboxed_path("rename-file", &new_path)?;
    std::fs::rename(&validated_old, &validated_new)
        .map_err(|e| RuntimeError::Custom(format!("rename-file: {}: {}", old_path, e)))?;
    Ok(Value::Bool(true))
}

fn builtin_create_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("create-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("create-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("create-dir", &path)?;
    std::fs::create_dir_all(&validated)
        .map_err(|e| RuntimeError::Custom(format!("create-dir: {}: {}", path, e)))?;
    Ok(Value::Bool(true))
}

fn builtin_read_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("read-dir", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("read-dir: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("read-dir", &path)?;
    if !validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "read-dir: '{}' is not a directory", path
        )));
    }
    let mut entries: Vec<String> = std::fs::read_dir(&validated)
        .map_err(|e| RuntimeError::Custom(format!("read-dir: {}: {}", path, e)))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    entries.sort();
    Ok(Value::List(entries.into_iter().map(Value::Str).collect()))
}

fn builtin_file_size(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("file-size", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("file-size: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("file-size", &path)?;
    if validated.is_dir() {
        return Err(RuntimeError::Custom(format!(
            "file-size: '{}' is a directory", path
        )));
    }
    let meta = std::fs::metadata(&validated)
        .map_err(|e| RuntimeError::Custom(format!("file-size: {}: {}", path, e)))?;
    Ok(Value::Int(meta.len() as i64))
}

fn builtin_is_dir(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("is-dir?", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("is-dir?: argument must be a string".into())),
    };
    let validated = validate_sandboxed_path("is-dir?", &path)?;
    Ok(Value::Bool(validated.is_dir()))
}

fn builtin_get_args(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("get-args", args, 0)?;
    let argv: Vec<Value> = std::env::args().map(Value::Str).collect();
    Ok(Value::List(argv))
}

#[cfg(feature = "aot")]
fn builtin_compile_to_executable(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("compile-to-executable", args, 2)?;
    let paths = match &args[0] {
        Value::List(items) => items.iter().map(|v| match v {
            Value::Str(s) => Ok(s.clone()),
            _ => Err(RuntimeError::TypeError("compile-to-executable: paths must be strings".into())),
        }).collect::<Result<Vec<_>, _>>()?,
        _ => return Err(RuntimeError::TypeError("compile-to-executable: first arg must be list of paths".into())),
    };
    let output = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("compile-to-executable: second arg must be output path string".into())),
    };
    // Delegate to the AOT pipeline
    crate::bytecode_aot::compile_to_executable_impl(&paths, &output)
        .map_err(|e| RuntimeError::Custom(e))?;
    Ok(Value::Unit)
}

// ── Bytecode VM implementation ───────────────────────────

fn builtin_run_bytecode(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("run-bytecode", args, 1)?;
    let func_list = match &args[0] {
        Value::List(items) => items.clone(),
        _ => return Err(RuntimeError::TypeError("run-bytecode: expected list of BCFunc".into())),
    };
    crate::bytecode_marshal::run_bytecode_program(&func_list)
}

// ── System builtins (type conversions, timing, env, http, json, shell) ──────

impl Builtins {
    fn register_system(&mut self) {
        self.register("int-to-string", builtin_int_to_string);
        self.register("float-to-string", builtin_float_to_string);
        self.register("string-to-int", builtin_string_to_int);
        self.register("string-to-float", builtin_string_to_float);
        self.register("char-code", builtin_char_code);
        self.register("char-from-code", builtin_char_from_code);
        self.register("panic", builtin_panic);
        self.register("assert", builtin_assert);
        self.register("time-now", builtin_time_now);
        self.register("getenv", builtin_getenv);
        self.register("http-request", builtin_http_request);
        self.register("http-post", builtin_http_post); // deprecated: use http-request
        self.register("json-parse", builtin_json_parse);
        self.register("json-stringify", builtin_json_stringify);
        self.register("shell-exec", builtin_shell_exec);
    }

    // ── Float Math ──────────────────────────────────────

    fn register_float_math(&mut self) {
        // Transcendentals
        self.register("sqrt", builtin_sqrt);
        self.register("sin", builtin_sin);
        self.register("cos", builtin_cos);
        self.register("tan", builtin_tan);
        self.register("log", builtin_log);
        self.register("exp", builtin_exp);
        // Rounding
        self.register("floor", builtin_floor);
        self.register("ceil", builtin_ceil);
        self.register("round", builtin_round);
        // Conversion
        self.register("float-to-int", builtin_float_to_int);
        self.register("int-to-float", builtin_int_to_float);
        // IEEE 754 special values
        self.register("infinity", builtin_infinity);
        self.register("nan", builtin_nan);
        self.register("is-nan?", builtin_is_nan);
        self.register("is-infinite?", builtin_is_infinite);
    }
}

fn builtin_int_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("int-to-string", args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Str(n.to_string())),
        _ => Err(RuntimeError::TypeError("int-to-string: expected integer".into())),
    }
}

fn builtin_float_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("float-to-string", args, 1)?;
    match &args[0] {
        Value::Float(f) => Ok(Value::Str(f.to_string())),
        _ => Err(RuntimeError::TypeError("float-to-string: expected float".into())),
    }
}

fn builtin_string_to_int(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("string-to-int", args, 1)?;
    match &args[0] {
        Value::Str(s) => match s.parse::<i64>() {
            Ok(n) => Ok(Value::Variant("Ok".into(), Box::new(Value::Int(n)))),
            Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("invalid integer".into())))),
        },
        _ => Err(RuntimeError::TypeError("string-to-int: expected string".into())),
    }
}

fn builtin_string_to_float(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("string-to-float", args, 1)?;
    match &args[0] {
        Value::Str(s) => match s.parse::<f64>() {
            Ok(f) => Ok(Value::Variant("Ok".into(), Box::new(Value::Float(f)))),
            Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("invalid float".into())))),
        },
        _ => Err(RuntimeError::TypeError("string-to-float: expected string".into())),
    }
}

fn builtin_char_code(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("char-code", args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            if let Some(ch) = s.chars().next() {
                Ok(Value::Int(ch as i64))
            } else {
                Err(RuntimeError::Custom("char-code: empty string".into()))
            }
        }
        _ => Err(RuntimeError::TypeError("char-code: expected string".into())),
    }
}

fn builtin_char_from_code(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("char-from-code", args, 1)?;
    match &args[0] {
        Value::Int(n) => {
            if let Some(ch) = char::from_u32(*n as u32) {
                Ok(Value::Str(ch.to_string()))
            } else {
                Err(RuntimeError::Custom(format!("char-from-code: invalid codepoint {}", n)))
            }
        }
        _ => Err(RuntimeError::TypeError("char-from-code: expected integer".into())),
    }
}

/// `(panic msg)` — abort execution with a custom error message.
fn builtin_panic(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("panic", args, 1)?;
    let msg = match &args[0] {
        Value::Str(s) => s.clone(),
        other => format!("{}", other),
    };
    Err(RuntimeError::Custom(format!("panic: {}", msg)))
}

/// `(assert condition msg)` — abort if condition is false.
fn builtin_assert(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("assert", args, 2)?;
    let condition = match &args[0] {
        Value::Bool(b) => *b,
        Value::Nil => false,
        _ => true, // non-nil, non-bool is truthy
    };
    if condition {
        Ok(Value::Bool(true))
    } else {
        let msg = match &args[1] {
            Value::Str(s) => s.clone(),
            other => format!("{}", other),
        };
        Err(RuntimeError::Custom(format!("assertion failed: {}", msg)))
    }
}

fn builtin_time_now(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("time-now", args, 0)?;
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    Ok(Value::Int(millis))
}

fn builtin_getenv(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("getenv", args, 1)?;
    match &args[0] {
        Value::Str(name) => match std::env::var(name) {
            Ok(val) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(val)))),
            Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("not set".into())))),
        },
        _ => Err(RuntimeError::TypeError("getenv: expected string".into())),
    }
}

/// Generic HTTP request: (http-request method url body headers) → Result[Str, Str]
fn builtin_http_request(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("http-request", args, 4)?;
    let method = match &args[0] {
        Value::Str(s) => s.to_uppercase(),
        _ => return Err(RuntimeError::TypeError("http-request: method must be string".into())),
    };
    let url = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("http-request: url must be string".into())),
    };
    let body = match &args[2] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("http-request: body must be string".into())),
    };
    let headers = match &args[3] {
        Value::Map(m) => m.clone(),
        _ => return Err(RuntimeError::TypeError("http-request: headers must be map".into())),
    };

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(300))
        .build();

    let mut req = match method.as_str() {
        "GET" => agent.get(&url),
        "POST" => agent.post(&url),
        "PUT" => agent.put(&url),
        "DELETE" => agent.delete(&url),
        "PATCH" => agent.patch(&url),
        "HEAD" => agent.head(&url),
        _ => return Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("http-request: unsupported method '{}'", method)
        )))),
    };
    for (k, v) in &headers {
        if let Value::Str(val) = v {
            req = req.set(k, val);
        }
    }

    let response = if method == "GET" || method == "HEAD" || method == "DELETE" {
        req.call()
    } else {
        req.send_string(&body)
    };

    match response {
        Ok(resp) => {
            match resp.into_string() {
                Ok(text) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(text)))),
                Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
            }
        }
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
    }
}

/// Deprecated: use (http-request "POST" url body headers) instead.
fn builtin_http_post(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("http-post", args, 3)?;
    let url = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("http-post: url must be string".into())),
    };
    let body = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("http-post: body must be string".into())),
    };
    let headers = match &args[2] {
        Value::Map(m) => m.clone(),
        _ => return Err(RuntimeError::TypeError("http-post: headers must be map".into())),
    };

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(300))
        .build();

    let mut req = agent.post(&url);
    for (k, v) in &headers {
        if let Value::Str(val) = v {
            req = req.set(k, val);
        }
    }

    match req.send_string(&body) {
        Ok(response) => {
            match response.into_string() {
                Ok(text) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(text)))),
                Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
            }
        }
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
    }
}

fn builtin_json_parse(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("json-parse", args, 1)?;
    let text = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("json-parse: expected string".into())),
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => Ok(Value::Variant("Ok".into(), Box::new(json_to_value(json)))),
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
    }
}

fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => {
            Value::List(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_value(v));
            }
            Value::Map(map)
        }
    }
}

fn builtin_json_stringify(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("json-stringify", args, 1)?;
    let json = value_to_json(&args[0]);
    Ok(Value::Str(json.to_string()))
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Nil => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> =
                map.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        Value::Variant(tag, inner) => {
            let mut obj = serde_json::Map::new();
            obj.insert("tag".into(), serde_json::Value::String(tag.clone()));
            obj.insert("value".into(), value_to_json(inner));
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}

fn builtin_shell_exec(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("shell-exec", args, 2)?;
    let command = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("shell-exec: command must be string".into())),
    };
    let cmd_args: Vec<String> = match &args[1] {
        Value::List(items) => items.iter().map(|v| match v {
            Value::Str(s) => Ok(s.clone()),
            _ => Err(RuntimeError::TypeError("shell-exec: args must be list of strings".into())),
        }).collect::<Result<Vec<_>, _>>()?,
        _ => return Err(RuntimeError::TypeError("shell-exec: args must be list".into())),
    };

    match std::process::Command::new(&command)
        .args(&cmd_args)
        .output()
    {
        Ok(output) => {
            let mut result_map = std::collections::HashMap::new();
            result_map.insert("stdout".into(), Value::Str(String::from_utf8_lossy(&output.stdout).into_owned()));
            result_map.insert("stderr".into(), Value::Str(String::from_utf8_lossy(&output.stderr).into_owned()));
            result_map.insert("exit-code".into(), Value::Int(output.status.code().unwrap_or(-1) as i64));
            Ok(Value::Variant("Ok".into(), Box::new(Value::Map(result_map))))
        }
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
    }
}

// ── Float math builtins ─────────────────────────────────────────────────────

fn as_float(name: &str, v: &Value) -> Result<f64, RuntimeError> {
    match v {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f64),
        _ => Err(RuntimeError::TypeError(format!("{}: expected number", name))),
    }
}

fn builtin_sqrt(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sqrt", args, 1)?;
    Ok(Value::Float(as_float("sqrt", &args[0])?.sqrt()))
}

fn builtin_sin(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sin", args, 1)?;
    Ok(Value::Float(as_float("sin", &args[0])?.sin()))
}

fn builtin_cos(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("cos", args, 1)?;
    Ok(Value::Float(as_float("cos", &args[0])?.cos()))
}

fn builtin_tan(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tan", args, 1)?;
    Ok(Value::Float(as_float("tan", &args[0])?.tan()))
}

fn builtin_log(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("log", args, 1)?;
    Ok(Value::Float(as_float("log", &args[0])?.ln()))
}

fn builtin_exp(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("exp", args, 1)?;
    Ok(Value::Float(as_float("exp", &args[0])?.exp()))
}

fn builtin_floor(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("floor", args, 1)?;
    Ok(Value::Int(as_float("floor", &args[0])?.floor() as i64))
}

fn builtin_ceil(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("ceil", args, 1)?;
    Ok(Value::Int(as_float("ceil", &args[0])?.ceil() as i64))
}

fn builtin_round(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("round", args, 1)?;
    Ok(Value::Int(as_float("round", &args[0])?.round() as i64))
}

fn builtin_float_to_int(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("float-to-int", args, 1)?;
    Ok(Value::Int(as_float("float-to-int", &args[0])?.trunc() as i64))
}

fn builtin_int_to_float(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("int-to-float", args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        _ => Err(RuntimeError::TypeError("int-to-float: expected integer".into())),
    }
}

fn builtin_infinity(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("infinity", args, 0)?;
    Ok(Value::Float(f64::INFINITY))
}

fn builtin_nan(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("nan", args, 0)?;
    Ok(Value::Float(f64::NAN))
}

fn builtin_is_nan(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("is-nan?", args, 1)?;
    match &args[0] {
        Value::Float(f) => Ok(Value::Bool(f.is_nan())),
        _ => Ok(Value::Bool(false)),
    }
}

fn builtin_is_infinite(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("is-infinite?", args, 1)?;
    match &args[0] {
        Value::Float(f) => Ok(Value::Bool(f.is_infinite())),
        _ => Ok(Value::Bool(false)),
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

    #[test]
    fn append_file_creates_and_appends() {
        let b = builtins();
        let tmp = format!("test_append_{}.tmp", std::process::id());
        // Initial write via append
        call(&b, "append-file", &[Value::Str(tmp.clone()), Value::Str("hello".into())]).unwrap();
        // Append more
        call(&b, "append-file", &[Value::Str(tmp.clone()), Value::Str(" world".into())]).unwrap();
        // Verify combined content
        let content = call(&b, "read-file", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(content, Value::Str("hello world".into()));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn delete_file_removes_file() {
        let b = builtins();
        let tmp = format!("test_delete_{}.tmp", std::process::id());
        std::fs::write(&tmp, "to delete").unwrap();
        let result = call(&b, "delete-file", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::Bool(true));
        assert!(!std::path::Path::new(&tmp).exists());
    }

    #[test]
    fn delete_file_rejects_directory() {
        let b = builtins();
        let tmp = format!("test_delfile_dir_{}", std::process::id());
        std::fs::create_dir_all(&tmp).unwrap();
        let result = call(&b, "delete-file", &[Value::Str(tmp.clone())]);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("is a directory"), "error: {}", err);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn create_and_delete_dir() {
        let b = builtins();
        let tmp = format!("test_mkdir_{}", std::process::id());
        // Create
        let result = call(&b, "create-dir", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::Bool(true));
        assert!(std::path::Path::new(&tmp).is_dir());
        // Idempotent create
        let result2 = call(&b, "create-dir", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result2, Value::Bool(true));
        // Delete
        let result3 = call(&b, "delete-dir", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result3, Value::Bool(true));
        assert!(!std::path::Path::new(&tmp).exists());
    }

    #[test]
    fn rename_file_works() {
        let b = builtins();
        let old = format!("test_rename_old_{}.tmp", std::process::id());
        let new = format!("test_rename_new_{}.tmp", std::process::id());
        std::fs::write(&old, "rename me").unwrap();
        let result = call(&b, "rename-file", &[Value::Str(old.clone()), Value::Str(new.clone())]).unwrap();
        assert_eq!(result, Value::Bool(true));
        assert!(!std::path::Path::new(&old).exists());
        let content = std::fs::read_to_string(&new).unwrap();
        assert_eq!(content, "rename me");
        std::fs::remove_file(&new).ok();
    }

    #[test]
    fn read_dir_lists_entries() {
        let b = builtins();
        let tmp = format!("test_readdir_{}", std::process::id());
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(format!("{}/beta.txt", tmp), "b").unwrap();
        std::fs::write(format!("{}/alpha.txt", tmp), "a").unwrap();
        let result = call(&b, "read-dir", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::Str("alpha.txt".into()),
            Value::Str("beta.txt".into()),
        ]));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn file_size_returns_bytes() {
        let b = builtins();
        let tmp = format!("test_fsize_{}.tmp", std::process::id());
        std::fs::write(&tmp, "hello").unwrap();
        let result = call(&b, "file-size", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::Int(5));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn is_dir_on_directory() {
        let b = builtins();
        let tmp = format!("test_isdir_{}", std::process::id());
        std::fs::create_dir_all(&tmp).unwrap();
        let result = call(&b, "is-dir?", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::Bool(true));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn is_dir_on_file() {
        let b = builtins();
        let tmp = format!("test_isdir_file_{}.tmp", std::process::id());
        std::fs::write(&tmp, "not a dir").unwrap();
        let result = call(&b, "is-dir?", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::Bool(false));
        std::fs::remove_file(&tmp).ok();
    }
}
