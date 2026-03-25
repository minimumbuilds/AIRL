use crate::value::Value;
use crate::error::RuntimeError;
use crate::tensor::TensorValue;
use crate::bytecode::{Op, BytecodeFunc, BytecodeClosureValue};
use airl_types::ty::PrimTy;
use std::collections::HashMap;

pub type BuiltinFnPtr = fn(&[Value]) -> Result<Value, RuntimeError>;

/// Trait for calling AIRL values (closures, functions) from within builtins.
/// Implemented by BytecodeVm to allow VM-aware builtins to invoke closures.
pub trait VmCaller {
    fn call_value(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, RuntimeError>;
    fn get_func(&self, name: &str) -> Option<BytecodeFunc>;
}

pub type BuiltinWithVmFn = fn(&mut dyn VmCaller, &[Value]) -> Result<Value, RuntimeError>;

pub struct Builtins {
    fns: HashMap<String, BuiltinFnPtr>,
    fns_with_vm: HashMap<String, BuiltinWithVmFn>,
}

impl Builtins {
    pub fn new() -> Self {
        let mut b = Builtins {
            fns: HashMap::new(),
            fns_with_vm: HashMap::new(),
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
        b.register_path();
        b.register_regex();
        b.register_crypto();
        b.register_vm_aware();
        b.register_bytes();
        b.register_tcp();
        b.register_threads();
        b
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinFnPtr> {
        self.fns.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.fns.contains_key(name) || self.fns_with_vm.contains_key(name)
    }

    fn register(&mut self, name: &str, f: BuiltinFnPtr) {
        self.fns.insert(name.to_string(), f);
    }

    pub fn register_with_vm(&mut self, name: &str, f: BuiltinWithVmFn) {
        self.fns_with_vm.insert(name.to_string(), f);
    }

    pub fn get_with_vm(&self, name: &str) -> Option<&BuiltinWithVmFn> {
        self.fns_with_vm.get(name)
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
        self.register("reverse", builtin_reverse);
        self.register("concat", builtin_concat);
        self.register("flatten", builtin_flatten);
        self.register("range", builtin_range);
        self.register("take", builtin_take);
        self.register("drop", builtin_drop);
        self.register("zip", builtin_zip);
        self.register("enumerate", builtin_enumerate);
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
        self.register("char-count", builtin_char_count);
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
        self.register("read-lines", builtin_read_lines);
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
        Value::IntList(xs) => Ok(Value::Int(xs.len() as i64)),
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
        (Value::IntList(xs), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= xs.len() {
                Err(RuntimeError::IndexOutOfBounds {
                    index: i,
                    len: xs.len(),
                })
            } else {
                Ok(Value::Int(xs[i]))
            }
        }
        _ => Err(RuntimeError::TypeError(
            "`at` expects (List, Int)".into(),
        )),
    }
}

fn builtin_append(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("append", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::IntList(xs), Value::Int(n)) => {
            let mut new_xs = xs.clone();
            new_xs.push(*n);
            Ok(Value::IntList(new_xs))
        }
        (Value::IntList(xs), _) => {
            // Element is not Int — promote to List
            let mut new_items: Vec<Value> = xs.iter().map(|x| Value::Int(*x)).collect();
            new_items.push(args[1].clone());
            Ok(Value::List(new_items))
        }
        (Value::List(items), _) => {
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
        (Value::IntList(xs), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= xs.len() {
                Ok(args[2].clone()) // default
            } else {
                Ok(Value::Int(xs[i]))
            }
        }
        _ => Err(RuntimeError::TypeError("`at-or` expects (List, Int, default)".into())),
    }
}

fn builtin_set_at(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("set-at", args, 3)?;
    match (&args[0], &args[1]) {
        (Value::IntList(xs), Value::Int(idx)) => {
            let i = *idx as usize;
            if i >= xs.len() {
                Err(RuntimeError::IndexOutOfBounds { index: i, len: xs.len() })
            } else if let Value::Int(n) = &args[2] {
                let mut new_xs = xs.clone();
                new_xs[i] = *n;
                Ok(Value::IntList(new_xs))
            } else {
                // Setting non-Int in IntList — promote to List
                let mut new_items: Vec<Value> = xs.iter().map(|x| Value::Int(*x)).collect();
                new_items[i] = args[2].clone();
                Ok(Value::List(new_items))
            }
        }
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
    match (&args[0], &args[1]) {
        (Value::IntList(xs), Value::Int(n)) => Ok(Value::Bool(xs.contains(n))),
        (Value::IntList(_), _) => Ok(Value::Bool(false)), // non-Int can't be in IntList
        (Value::List(items), _) => Ok(Value::Bool(items.contains(&args[1]))),
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
        Value::IntList(xs) => {
            if xs.is_empty() {
                Err(RuntimeError::TypeError("head: empty list".into()))
            } else {
                Ok(Value::Int(xs[0]))
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
        Value::IntList(xs) => {
            if xs.is_empty() {
                Err(RuntimeError::TypeError("tail: empty list".into()))
            } else {
                Ok(Value::IntList(xs[1..].to_vec()))
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
        Value::IntList(xs) => Ok(Value::Bool(xs.is_empty())),
        _ => Err(RuntimeError::TypeError(format!(
            "`empty?` expects a List, got {}",
            type_name(&args[0])
        ))),
    }
}

fn builtin_cons(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("cons", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(n), Value::IntList(xs)) => {
            let mut new_xs = vec![*n];
            new_xs.extend_from_slice(xs);
            Ok(Value::IntList(new_xs))
        }
        (_, Value::IntList(xs)) => {
            // Element is not Int — promote to List
            let mut new_items = vec![args[0].clone()];
            new_items.extend(xs.iter().map(|x| Value::Int(*x)));
            Ok(Value::List(new_items))
        }
        (_, Value::List(items)) => {
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

// ── Native list builtins (shadow stdlib recursive versions) ──

fn builtin_reverse(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("reverse", args, 1)?;
    match &args[0] {
        Value::List(xs) => {
            let mut reversed = xs.clone();
            reversed.reverse();
            Ok(Value::List(reversed))
        }
        Value::IntList(xs) => {
            let mut reversed = xs.clone();
            reversed.reverse();
            Ok(Value::IntList(reversed))
        }
        _ => Err(RuntimeError::TypeError(
            "reverse: argument must be a list".into(),
        )),
    }
}

fn builtin_concat(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("concat", args, 2)?;
    match (&args[0], &args[1]) {
        (Value::IntList(xs), Value::IntList(ys)) => {
            let mut result = xs.clone();
            result.extend_from_slice(ys);
            Ok(Value::IntList(result))
        }
        (Value::IntList(xs), Value::List(ys)) => {
            let mut result: Vec<Value> = xs.iter().map(|x| Value::Int(*x)).collect();
            result.extend(ys.iter().cloned());
            Ok(Value::List(result))
        }
        (Value::List(xs), Value::IntList(ys)) => {
            let mut result = xs.clone();
            result.extend(ys.iter().map(|y| Value::Int(*y)));
            Ok(Value::List(result))
        }
        (Value::List(xs), Value::List(ys)) => {
            let mut result = xs.clone();
            result.extend(ys.iter().cloned());
            Ok(Value::List(result))
        }
        _ => Err(RuntimeError::TypeError(
            "concat: both arguments must be lists".into(),
        )),
    }
}

fn builtin_flatten(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("flatten", args, 1)?;
    match &args[0] {
        Value::List(xss) => {
            // Check if all inner lists are IntList for fast path
            let all_intlist = xss.iter().all(|xs| matches!(xs, Value::IntList(_)));
            if all_intlist && !xss.is_empty() {
                let mut result = Vec::new();
                for xs in xss {
                    if let Value::IntList(inner) = xs {
                        result.extend_from_slice(inner);
                    }
                }
                Ok(Value::IntList(result))
            } else {
                let mut result = Vec::new();
                for xs in xss {
                    match xs {
                        Value::List(inner) => result.extend(inner.iter().cloned()),
                        Value::IntList(inner) => result.extend(inner.iter().map(|x| Value::Int(*x))),
                        other => result.push(other.clone()),
                    }
                }
                Ok(Value::List(result))
            }
        }
        _ => Err(RuntimeError::TypeError(
            "flatten: argument must be a list".into(),
        )),
    }
}

fn builtin_range(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("range", args, 2)?;
    let start = match &args[0] {
        Value::Int(n) => *n,
        _ => {
            return Err(RuntimeError::TypeError(
                "range: start must be integer".into(),
            ))
        }
    };
    let end = match &args[1] {
        Value::Int(n) => *n,
        _ => {
            return Err(RuntimeError::TypeError(
                "range: end must be integer".into(),
            ))
        }
    };
    let result: Vec<i64> = (start..end).collect();
    Ok(Value::IntList(result))
}

fn builtin_take(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("take", args, 2)?;
    let n = match &args[0] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("take: first arg must be integer".into())),
    };
    match &args[1] {
        Value::List(items) => Ok(Value::List(items[..n.min(items.len())].to_vec())),
        Value::IntList(xs) => Ok(Value::IntList(xs[..n.min(xs.len())].to_vec())),
        _ => Err(RuntimeError::TypeError("take: second arg must be list".into())),
    }
}

fn builtin_drop(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("drop", args, 2)?;
    let n = match &args[0] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("drop: first arg must be integer".into())),
    };
    match &args[1] {
        Value::List(items) => {
            let start = n.min(items.len());
            Ok(Value::List(items[start..].to_vec()))
        }
        Value::IntList(xs) => {
            let start = n.min(xs.len());
            Ok(Value::IntList(xs[start..].to_vec()))
        }
        _ => Err(RuntimeError::TypeError("drop: second arg must be list".into())),
    }
}

fn builtin_zip(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("zip", args, 2)?;
    let xs_vec: Vec<Value> = match &args[0] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("zip: first arg must be list".into())),
    };
    let ys_vec: Vec<Value> = match &args[1] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|y| Value::Int(*y)).collect(),
        _ => return Err(RuntimeError::TypeError("zip: second arg must be list".into())),
    };
    let pairs: Vec<Value> = xs_vec.iter().zip(ys_vec.iter())
        .map(|(x, y)| Value::List(vec![x.clone(), y.clone()]))
        .collect();
    Ok(Value::List(pairs))
}

fn builtin_enumerate(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("enumerate", args, 1)?;
    match &args[0] {
        Value::List(items) => {
            let pairs: Vec<Value> = items.iter().enumerate()
                .map(|(i, x)| Value::List(vec![Value::Int(i as i64), x.clone()]))
                .collect();
            Ok(Value::List(pairs))
        }
        Value::IntList(xs) => {
            let pairs: Vec<Value> = xs.iter().enumerate()
                .map(|(i, x)| Value::List(vec![Value::Int(i as i64), Value::Int(*x)]))
                .collect();
            Ok(Value::List(pairs))
        }
        _ => Err(RuntimeError::TypeError("enumerate: argument must be list".into())),
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
        (Value::IntList(xs), Value::Str(sep)) => {
            let parts: Vec<String> = xs.iter().map(|x| x.to_string()).collect();
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

fn builtin_char_count(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("char-count", args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        _ => Err(RuntimeError::TypeError("char-count: argument must be a string".into())),
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
        Value::IntList(_) => "List",  // IntList is transparent — reports as "List"
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

fn builtin_read_lines(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("read-lines", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("read-lines: argument must be a string path".into())),
    };
    let validated = validate_sandboxed_path("read-lines", &path)?;
    use std::io::BufRead;
    let file = std::fs::File::open(&validated)
        .map_err(|e| RuntimeError::Custom(format!("read-lines: {}: {}", path, e)))?;
    let reader = std::io::BufReader::new(file);
    let lines: Vec<Value> = reader.lines()
        .map(|line| line.map(Value::Str))
        .collect::<std::io::Result<_>>()
        .map_err(|e| RuntimeError::Custom(format!("read-lines: {}: {}", path, e)))?;
    Ok(Value::List(lines))
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
        self.register("sleep", builtin_sleep);
        self.register("format-time", builtin_format_time);
        self.register("getenv", builtin_getenv);
        self.register("http-request", builtin_http_request);
        self.register("json-parse", builtin_json_parse);
        self.register("json-stringify", builtin_json_stringify);
        self.register("shell-exec", builtin_shell_exec);
        self.register("format", builtin_format);
        self.register("exit", builtin_exit);
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

    // ── Path ────────────────────────────────────────────

    fn register_path(&mut self) {
        self.register("path-join", builtin_path_join);
        self.register("path-parent", builtin_path_parent);
        self.register("path-filename", builtin_path_filename);
        self.register("path-extension", builtin_path_extension);
        self.register("is-absolute?", builtin_is_absolute);
    }

    // ── Regex ───────────────────────────────────────────

    fn register_regex(&mut self) {
        self.register("regex-match", builtin_regex_match);
        self.register("regex-find-all", builtin_regex_find_all);
        self.register("regex-replace", builtin_regex_replace);
        self.register("regex-split", builtin_regex_split);
    }

    // ── Crypto ──────────────────────────────────────────

    fn register_crypto(&mut self) {
        self.register("sha256", builtin_sha256);
        self.register("hmac-sha256", builtin_hmac_sha256);
        self.register("base64-encode", builtin_base64_encode);
        self.register("base64-decode", builtin_base64_decode);
        self.register("random-bytes", builtin_random_bytes);
        self.register("sha512", builtin_sha512);
        self.register("hmac-sha512", builtin_hmac_sha512);
        self.register("sha256-bytes", builtin_sha256_bytes);
        self.register("sha512-bytes", builtin_sha512_bytes);
        self.register("hmac-sha256-bytes", builtin_hmac_sha256_bytes);
        self.register("hmac-sha512-bytes", builtin_hmac_sha512_bytes);
        self.register("pbkdf2-sha256", builtin_pbkdf2_sha256);
        self.register("pbkdf2-sha512", builtin_pbkdf2_sha512);
        self.register("base64-decode-bytes", builtin_base64_decode_bytes);
        self.register("base64-encode-bytes", builtin_base64_encode_bytes);
        self.register("bitwise-xor", builtin_bitwise_xor);
        self.register("bitwise-and", builtin_bitwise_and);
        self.register("bitwise-or", builtin_bitwise_or);
    }

    // ── VM-aware builtins (require closure calling) ─────

    fn register_vm_aware(&mut self) {
        self.register_with_vm("map", builtin_map_vm);
        self.register_with_vm("filter", builtin_filter_vm);
        self.register_with_vm("fold", builtin_fold_vm);
        self.register_with_vm("sort", builtin_sort_vm);
        self.register_with_vm("any", builtin_any_vm);
        self.register_with_vm("all", builtin_all_vm);
        self.register_with_vm("find", builtin_find_vm);
    }

    // ── Byte encoding builtins ──────────────────────────

    fn register_bytes(&mut self) {
        self.register("bytes-from-int16", builtin_bytes_from_int16);
        self.register("bytes-from-int32", builtin_bytes_from_int32);
        self.register("bytes-from-int64", builtin_bytes_from_int64);
        self.register("bytes-to-int16", builtin_bytes_to_int16);
        self.register("bytes-to-int32", builtin_bytes_to_int32);
        self.register("bytes-to-int64", builtin_bytes_to_int64);
        self.register("bytes-from-string", builtin_bytes_from_string);
        self.register("bytes-to-string", builtin_bytes_to_string);
        self.register("bytes-concat", builtin_bytes_concat);
        self.register("bytes-slice", builtin_bytes_slice);
        self.register("crc32c", builtin_crc32c);
    }

    // ── TCP socket builtins ─────────────────────────────

    fn register_tcp(&mut self) {
        self.register("tcp-connect", builtin_tcp_connect);
        self.register("tcp-close", builtin_tcp_close);
        self.register("tcp-send", builtin_tcp_send);
        self.register("tcp-recv", builtin_tcp_recv);
        self.register("tcp-recv-exact", builtin_tcp_recv_exact);
        self.register("tcp-set-timeout", builtin_tcp_set_timeout);
    }

    fn register_threads(&mut self) {
        // thread-spawn is handled specially in bytecode_vm.rs CallBuiltin dispatch
        // but must be registered so the bytecode compiler emits CallBuiltin for it
        self.register("thread-spawn", |_| Err(RuntimeError::Custom(
            "thread-spawn: must be called through VM dispatch".into())));
        self.register("thread-join", builtin_thread_join);
        self.register("channel-new", builtin_channel_new);
        self.register("channel-send", builtin_channel_send);
        self.register("channel-recv", builtin_channel_recv);
        self.register("channel-recv-timeout", builtin_channel_recv_timeout);
        self.register("channel-close", builtin_channel_close);
    }
}

// ── Closure pattern detectors for fast-path specialization ────────

/// Detect if a 2-arg closure body is a single binary arithmetic op.
/// Matches patterns like (fn [a b] (+ a b)), (fn [a b] (* a b)), etc.
fn detect_binary_op(func: &BytecodeFunc, closure: &BytecodeClosureValue) -> Option<Op> {
    if func.arity != 2 || !closure.captured.is_empty() { return None; }
    let mut found_op = None;
    for instr in &func.instructions {
        match instr.op {
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod => {
                if found_op.is_some() { return None; } // multiple arith ops
                found_op = Some(instr.op);
            }
            Op::Move | Op::Return | Op::LoadConst | Op::MarkMoved | Op::CheckNotMoved => {}
            _ => return None,
        }
    }
    found_op
}

/// Detect if a 1-arg closure body is a unary op with optional constant.
/// Matches: (fn [x] (* x x)), (fn [x] (* x 2)), (fn [x] (+ x 1)), (fn [x] (> x 3))
fn detect_unary_op(func: &BytecodeFunc, closure: &BytecodeClosureValue) -> Option<(Op, Option<i64>)> {
    if func.arity != 1 || !closure.captured.is_empty() { return None; }
    let mut found_op = None;
    let mut const_val = None;
    for instr in &func.instructions {
        match instr.op {
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod
            | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                if found_op.is_some() { return None; }
                found_op = Some(instr.op);
            }
            Op::LoadConst => {
                if instr.a as usize >= func.constants.len() { return None; }
                if let Value::Int(n) = &func.constants[instr.a as usize] {
                    const_val = Some(*n);
                }
            }
            Op::Move | Op::Return | Op::MarkMoved | Op::CheckNotMoved => {}
            _ => return None,
        }
    }
    found_op.map(|op| (op, const_val))
}

/// Detect a compound predicate: (fn [x] (CMP (ARITH x CONST1) CONST2))
/// Example: (fn [x] (= (% x 2) 0)) → arith=Mod, arith_const=2, cmp=Eq, cmp_const=0
/// Returns (arith_op, arith_const, cmp_op, cmp_const) if matched.
fn detect_compound_predicate(
    func: &BytecodeFunc,
    closure: &BytecodeClosureValue,
) -> Option<(Op, i64, Op, i64)> {
    if func.arity != 1 || !closure.captured.is_empty() { return None; }
    let mut arith_op = None;
    let mut cmp_op = None;
    let mut constants: Vec<i64> = Vec::new();
    for instr in &func.instructions {
        match instr.op {
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod => {
                if arith_op.is_some() { return None; }
                arith_op = Some(instr.op);
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                if cmp_op.is_some() { return None; }
                cmp_op = Some(instr.op);
            }
            Op::LoadConst => {
                if let Some(Value::Int(n)) = func.constants.get(instr.a as usize) {
                    constants.push(*n);
                }
            }
            Op::Move | Op::Return | Op::MarkMoved | Op::CheckNotMoved => {}
            _ => return None,
        }
    }
    // Need exactly: 1 arith op + 1 cmp op + 2 constants
    match (arith_op, cmp_op, constants.len()) {
        (Some(a), Some(c), 2) => Some((a, constants[0], c, constants[1])),
        _ => None,
    }
}

// ── VM-aware list builtins (require closure calling) ──────────────

fn builtin_map_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`map` expects 2 argument(s), got {}", args.len()
        )));
    }
    let f = args[0].clone();

    // Fast path: IntList input
    if let Value::IntList(xs) = &args[1] {
        // Closure pattern detection: avoid per-element VM calls
        if let Value::BytecodeClosure(ref closure) = f {
            if let Some(func) = vm.get_func(&closure.func_name) {
                if let Some((op, const_val)) = detect_unary_op(&func, closure) {
                    let result: Option<Vec<i64>> = match (op, const_val) {
                        (Op::Mul, None) => Some(xs.iter().map(|x| x.wrapping_mul(*x)).collect()),
                        (Op::Mul, Some(c)) => Some(xs.iter().map(|x| x.wrapping_mul(c)).collect()),
                        (Op::Add, Some(c)) => Some(xs.iter().map(|x| x.wrapping_add(c)).collect()),
                        (Op::Sub, Some(c)) => Some(xs.iter().map(|x| x.wrapping_sub(c)).collect()),
                        _ => None,
                    };
                    if let Some(ints) = result {
                        return Ok(Value::IntList(ints));
                    }
                }
            }
        }

        let mut results_int = Vec::with_capacity(xs.len());
        let mut all_int = true;
        let mut results_mixed: Vec<Value> = Vec::new();

        for x in xs {
            let result = vm.call_value(&f, vec![Value::Int(*x)])?;
            if all_int {
                if let Value::Int(n) = result {
                    results_int.push(n);
                } else {
                    // Switch to mixed mode
                    all_int = false;
                    results_mixed = results_int.iter().map(|n| Value::Int(*n)).collect();
                    results_mixed.push(result);
                }
            } else {
                results_mixed.push(result);
            }
        }

        if all_int {
            return Ok(Value::IntList(results_int));
        } else {
            return Ok(Value::List(results_mixed));
        }
    }

    // Generic path
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        _ => return Err(RuntimeError::TypeError("map: second argument must be a list".into())),
    };
    let mut results = Vec::with_capacity(xs.len());
    for x in xs {
        results.push(vm.call_value(&f, vec![x])?);
    }
    Ok(Value::List(results))
}

fn builtin_filter_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`filter` expects 2 argument(s), got {}", args.len()
        )));
    }
    let pred = args[0].clone();

    // Fast path: IntList
    if let Value::IntList(xs) = &args[1] {
        // Closure pattern detection: avoid per-element VM calls
        if let Value::BytecodeClosure(ref closure) = pred {
            if let Some(func) = vm.get_func(&closure.func_name) {
                // Single-op patterns: (fn [x] (> x 3)), (fn [x] (= x 0))
                if let Some((op, Some(c))) = detect_unary_op(&func, closure) {
                    let pred_fn: Option<Box<dyn Fn(i64) -> bool>> = match op {
                        Op::Gt => Some(Box::new(move |x| x > c)),
                        Op::Lt => Some(Box::new(move |x| x < c)),
                        Op::Ge => Some(Box::new(move |x| x >= c)),
                        Op::Le => Some(Box::new(move |x| x <= c)),
                        Op::Eq => Some(Box::new(move |x| x == c)),
                        Op::Ne => Some(Box::new(move |x| x != c)),
                        _ => None,
                    };
                    if let Some(pred_fn) = pred_fn {
                        return Ok(Value::IntList(xs.iter().filter(|x| pred_fn(**x)).copied().collect()));
                    }
                }
                // Compound patterns: (fn [x] (= (% x 2) 0)), (fn [x] (> (* x 3) 10))
                if let Some((arith, ac, cmp, cc)) = detect_compound_predicate(&func, closure) {
                    let apply_arith: Option<Box<dyn Fn(i64) -> i64>> = match arith {
                        Op::Mod => Some(Box::new(move |x| if ac != 0 { x % ac } else { 0 })),
                        Op::Add => Some(Box::new(move |x| x.wrapping_add(ac))),
                        Op::Sub => Some(Box::new(move |x| x.wrapping_sub(ac))),
                        Op::Mul => Some(Box::new(move |x| x.wrapping_mul(ac))),
                        Op::Div => Some(Box::new(move |x| if ac != 0 { x / ac } else { 0 })),
                        _ => None,
                    };
                    let apply_cmp: Option<Box<dyn Fn(i64) -> bool>> = match cmp {
                        Op::Eq => Some(Box::new(move |v| v == cc)),
                        Op::Ne => Some(Box::new(move |v| v != cc)),
                        Op::Lt => Some(Box::new(move |v| v < cc)),
                        Op::Le => Some(Box::new(move |v| v <= cc)),
                        Op::Gt => Some(Box::new(move |v| v > cc)),
                        Op::Ge => Some(Box::new(move |v| v >= cc)),
                        _ => None,
                    };
                    if let (Some(arith_fn), Some(cmp_fn)) = (apply_arith, apply_cmp) {
                        return Ok(Value::IntList(
                            xs.iter().filter(|x| cmp_fn(arith_fn(**x))).copied().collect()
                        ));
                    }
                }
            }
        }

        let mut results = Vec::new();
        for x in xs {
            let keep = vm.call_value(&pred, vec![Value::Int(*x)])?;
            match keep {
                Value::Bool(true) => results.push(*x),
                Value::Bool(false) | Value::Nil => {}
                _ => results.push(*x), // truthy
            }
        }
        return Ok(Value::IntList(results));
    }

    // Generic path
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        _ => return Err(RuntimeError::TypeError("filter: second argument must be a list".into())),
    };
    let mut results = Vec::new();
    for x in xs {
        let keep = vm.call_value(&pred, vec![x.clone()])?;
        match keep {
            Value::Bool(true) => results.push(x),
            Value::Bool(false) => {}
            Value::Nil => {}              // nil is falsy
            _ => results.push(x),         // everything else is truthy
        }
    }
    Ok(Value::List(results))
}

fn builtin_fold_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 3 {
        return Err(RuntimeError::TypeError(format!(
            "`fold` expects 3 argument(s), got {}", args.len()
        )));
    }
    let f = args[0].clone();
    let init = args[1].clone();

    // Fast path: IntList + Int accumulator + builtin arithmetic
    if let (Value::Int(init_n), Value::IntList(xs)) = (&init, &args[2]) {
        let mut acc = *init_n;
        if let Value::BuiltinFn(name) = &f {
            match name.as_str() {
                "+" => {
                    for x in xs { acc = acc.wrapping_add(*x); }
                    return Ok(Value::Int(acc));
                }
                "*" => {
                    for x in xs { acc = acc.wrapping_mul(*x); }
                    return Ok(Value::Int(acc));
                }
                _ => {} // fall through to generic IntList path
            }
        }
        // Closure pattern detection: avoid per-element VM calls
        if let Value::BytecodeClosure(ref closure) = f {
            if let Some(func) = vm.get_func(&closure.func_name) {
                if let Some(op) = detect_binary_op(&func, closure) {
                    match op {
                        Op::Add => {
                            for x in xs { acc = acc.wrapping_add(*x); }
                            return Ok(Value::Int(acc));
                        }
                        Op::Sub => {
                            for x in xs { acc = acc.wrapping_sub(*x); }
                            return Ok(Value::Int(acc));
                        }
                        Op::Mul => {
                            for x in xs { acc = acc.wrapping_mul(*x); }
                            return Ok(Value::Int(acc));
                        }
                        Op::Div => {
                            for x in xs {
                                if *x == 0 {
                                    return Err(RuntimeError::Custom("division by zero in fold".into()));
                                }
                                acc /= x;
                            }
                            return Ok(Value::Int(acc));
                        }
                        Op::Mod => {
                            for x in xs {
                                if *x == 0 {
                                    return Err(RuntimeError::Custom("modulo by zero in fold".into()));
                                }
                                acc %= x;
                            }
                            return Ok(Value::Int(acc));
                        }
                        _ => {} // fall through
                    }
                }
            }
        }
        // Generic IntList path: call function but avoid boxing where possible
        for (i, x) in xs.iter().enumerate() {
            let result = vm.call_value(&f, vec![Value::Int(acc), Value::Int(*x)])?;
            match result {
                Value::Int(n) => acc = n,
                other => {
                    // Accumulator is no longer Int — continue with remaining elements
                    let mut generic_acc = other;
                    for rx in &xs[i + 1..] {
                        generic_acc = vm.call_value(&f, vec![generic_acc, Value::Int(*rx)])?;
                    }
                    return Ok(generic_acc);
                }
            }
        }
        return Ok(Value::Int(acc));
    }

    // Generic path
    let mut acc = init;
    let xs = match &args[2] {
        Value::List(items) => items.clone(),
        Value::IntList(items) => items.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("fold: third argument must be a list".into())),
    };
    for x in xs {
        acc = vm.call_value(&f, vec![acc, x])?;
    }
    Ok(acc)
}

fn builtin_sort_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`sort` expects 2 argument(s), got {}", args.len()
        )));
    }
    let cmp = args[0].clone();
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("sort: second argument must be a list".into())),
    };

    fn merge_sort(vm: &mut dyn VmCaller, cmp: &Value, xs: Vec<Value>) -> Result<Vec<Value>, RuntimeError> {
        if xs.len() <= 1 { return Ok(xs); }
        let mid = xs.len() / 2;
        let left = merge_sort(vm, cmp, xs[..mid].to_vec())?;
        let right = merge_sort(vm, cmp, xs[mid..].to_vec())?;
        let mut result = Vec::with_capacity(left.len() + right.len());
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            let is_less = vm.call_value(cmp, vec![left[i].clone(), right[j].clone()])?;
            if matches!(is_less, Value::Bool(true)) {
                result.push(left[i].clone());
                i += 1;
            } else {
                result.push(right[j].clone());
                j += 1;
            }
        }
        result.extend_from_slice(&left[i..]);
        result.extend_from_slice(&right[j..]);
        Ok(result)
    }

    let sorted = merge_sort(vm, &cmp, xs)?;
    Ok(Value::List(sorted))
}

fn builtin_any_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`any` expects 2 argument(s), got {}", args.len()
        )));
    }
    let pred = args[0].clone();
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("any: second argument must be a list".into())),
    };
    for x in xs {
        let result = vm.call_value(&pred, vec![x])?;
        match result {
            Value::Bool(false) | Value::Nil => {}
            _ => return Ok(Value::Bool(true)),
        }
    }
    Ok(Value::Bool(false))
}

fn builtin_all_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`all` expects 2 argument(s), got {}", args.len()
        )));
    }
    let pred = args[0].clone();
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("all: second argument must be a list".into())),
    };
    for x in xs {
        let result = vm.call_value(&pred, vec![x])?;
        match result {
            Value::Bool(false) | Value::Nil => return Ok(Value::Bool(false)),
            _ => {}
        }
    }
    Ok(Value::Bool(true))
}

fn builtin_find_vm(vm: &mut dyn VmCaller, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 2 {
        return Err(RuntimeError::TypeError(format!(
            "`find` expects 2 argument(s), got {}", args.len()
        )));
    }
    let pred = args[0].clone();
    let xs = match &args[1] {
        Value::List(items) => items.clone(),
        Value::IntList(ints) => ints.iter().map(|x| Value::Int(*x)).collect(),
        _ => return Err(RuntimeError::TypeError("find: second argument must be a list".into())),
    };
    for x in xs {
        let result = vm.call_value(&pred, vec![x.clone()])?;
        match result {
            Value::Bool(false) | Value::Nil => {}
            _ => return Ok(x),
        }
    }
    Ok(Value::Nil)
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

/// `(sleep ms)` — pause execution for the given number of milliseconds.
fn builtin_sleep(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sleep", args, 1)?;
    let ms = match &args[0] {
        Value::Int(n) => {
            if *n < 0 {
                return Err(RuntimeError::Custom("sleep: duration must be non-negative".into()));
            }
            *n as u64
        }
        _ => return Err(RuntimeError::TypeError("sleep: expected integer (milliseconds)".into())),
    };
    std::thread::sleep(std::time::Duration::from_millis(ms));
    Ok(Value::Nil)
}

/// `(format-time millis fmt)` — format a Unix timestamp (millis since epoch).
/// Supports: %Y (year), %m (month), %d (day), %H (hour), %M (minute), %S (second).
/// Uses UTC. No external dependency — manual formatting from timestamp arithmetic.
fn builtin_format_time(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("format-time", args, 2)?;
    let millis = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("format-time: first arg must be integer (millis)".into())),
    };
    let fmt = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("format-time: second arg must be format string".into())),
    };

    // Convert millis to components (UTC)
    let secs = millis / 1000;
    let sec = (secs % 60) as u32;
    let min = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;

    // Days since epoch → year/month/day (civil calendar, Howard Hinnant algorithm)
    let mut days = (secs / 86400) as i64;
    days += 719468; // shift epoch from 1970-01-01 to 0000-03-01
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    let result = fmt
        .replace("%Y", &format!("{:04}", year))
        .replace("%m", &format!("{:02}", m))
        .replace("%d", &format!("{:02}", d))
        .replace("%H", &format!("{:02}", hour))
        .replace("%M", &format!("{:02}", min))
        .replace("%S", &format!("{:02}", sec));

    Ok(Value::Str(result))
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

// ── Path implementations ─────────────────────────────────────────────────────

fn builtin_path_join(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("path-join", args, 1)?;
    let parts = match &args[0] {
        Value::List(items) => items,
        _ => return Err(RuntimeError::TypeError(
            "`path-join` expects a List of strings".into(),
        )),
    };
    let mut buf = std::path::PathBuf::new();
    for part in parts {
        match part {
            Value::Str(s) => buf.push(s),
            _ => return Err(RuntimeError::TypeError(
                "`path-join`: all parts must be strings".into(),
            )),
        }
    }
    Ok(Value::Str(buf.to_string_lossy().into_owned()))
}

fn builtin_path_parent(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("path-parent", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`path-parent` expects a Str argument".into(),
        )),
    };
    let p = std::path::Path::new(&path);
    match p.parent() {
        Some(parent) => Ok(Value::Str(parent.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

fn builtin_path_filename(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("path-filename", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`path-filename` expects a Str argument".into(),
        )),
    };
    let p = std::path::Path::new(&path);
    match p.file_name() {
        Some(name) => Ok(Value::Str(name.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

fn builtin_path_extension(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("path-extension", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`path-extension` expects a Str argument".into(),
        )),
    };
    let p = std::path::Path::new(&path);
    match p.extension() {
        Some(ext) => Ok(Value::Str(ext.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

fn builtin_is_absolute(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("is-absolute?", args, 1)?;
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`is-absolute?` expects a Str argument".into(),
        )),
    };
    Ok(Value::Bool(std::path::Path::new(&path).is_absolute()))
}

// ── Regex implementations ────────────────────────────────────────────────────

fn builtin_regex_match(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("regex-match", args, 2)?;
    let pattern = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-match` expects (Str, Str)".into(),
        )),
    };
    let string = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-match` expects (Str, Str)".into(),
        )),
    };
    let anchored = format!("^(?:{})$", pattern);
    match regex::Regex::new(&anchored) {
        Ok(re) => Ok(Value::Bool(re.is_match(&string))),
        Err(e) => Err(RuntimeError::Custom(format!("regex-match: {}", e))),
    }
}

fn builtin_regex_find_all(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("regex-find-all", args, 2)?;
    let pattern = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-find-all` expects (Str, Str)".into(),
        )),
    };
    let string = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-find-all` expects (Str, Str)".into(),
        )),
    };
    match regex::Regex::new(&pattern) {
        Ok(re) => {
            let matches: Vec<Value> = re
                .find_iter(&string)
                .map(|m| Value::Str(m.as_str().to_string()))
                .collect();
            Ok(Value::List(matches))
        }
        Err(e) => Err(RuntimeError::Custom(format!("regex-find-all: {}", e))),
    }
}

fn builtin_regex_replace(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("regex-replace", args, 3)?;
    let pattern = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-replace` expects (Str, Str, Str)".into(),
        )),
    };
    let string = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-replace` expects (Str, Str, Str)".into(),
        )),
    };
    let replacement = match &args[2] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-replace` expects (Str, Str, Str)".into(),
        )),
    };
    match regex::Regex::new(&pattern) {
        Ok(re) => Ok(Value::Str(re.replace_all(&string, replacement.as_str()).into_owned())),
        Err(e) => Err(RuntimeError::Custom(format!("regex-replace: {}", e))),
    }
}

fn builtin_regex_split(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("regex-split", args, 2)?;
    let pattern = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-split` expects (Str, Str)".into(),
        )),
    };
    let string = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`regex-split` expects (Str, Str)".into(),
        )),
    };
    match regex::Regex::new(&pattern) {
        Ok(re) => {
            let parts: Vec<Value> = re
                .split(&string)
                .map(|s| Value::Str(s.to_string()))
                .collect();
            Ok(Value::List(parts))
        }
        Err(e) => Err(RuntimeError::Custom(format!("regex-split: {}", e))),
    }
}

// ── Crypto implementations ───────────────────────────────────────────────────

fn builtin_sha256(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sha256", args, 1)?;
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`sha256` expects a Str argument".into(),
        )),
    };
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(Value::Str(hex))
}

fn builtin_hmac_sha256(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("hmac-sha256", args, 2)?;
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`hmac-sha256` expects (Str, Str)".into(),
        )),
    };
    let data = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`hmac-sha256` expects (Str, Str)".into(),
        )),
    };
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .map_err(|e| RuntimeError::Custom(format!("hmac-sha256: {}", e)))?;
    mac.update(data.as_bytes());
    let result = mac.finalize().into_bytes();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(Value::Str(hex))
}

fn builtin_base64_encode(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("base64-encode", args, 1)?;
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`base64-encode` expects a Str argument".into(),
        )),
    };
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(data.as_bytes());
    Ok(Value::Str(encoded))
}

fn builtin_base64_decode(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("base64-decode", args, 1)?;
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`base64-decode` expects a Str argument".into(),
        )),
    };
    use base64::Engine;
    match base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(s)))),
            Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                format!("base64-decode: invalid UTF-8: {}", e),
            )))),
        },
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("base64-decode: {}", e),
        )))),
    }
}

fn builtin_random_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("random-bytes", args, 1)?;
    let n = match &args[0] {
        Value::Int(n) if *n >= 0 => *n as usize,
        Value::Int(_) => return Err(RuntimeError::Custom(
            "random-bytes: count must be non-negative".into(),
        )),
        _ => return Err(RuntimeError::TypeError(
            "`random-bytes` expects an Int argument".into(),
        )),
    };
    use rand::RngCore;
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(Value::Str(hex))
}

// ── SHA-512, HMAC-SHA-512, bytes variants, PBKDF2, base64-bytes, bitwise ─────

fn builtin_sha512(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sha512", args, 1)?;
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`sha512` expects a Str argument".into(),
        )),
    };
    use sha2::Digest;
    let mut hasher = sha2::Sha512::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(Value::Str(hex))
}

fn builtin_hmac_sha512(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("hmac-sha512", args, 2)?;
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`hmac-sha512` expects (Str, Str)".into(),
        )),
    };
    let data = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`hmac-sha512` expects (Str, Str)".into(),
        )),
    };
    use hmac::{Hmac, Mac};
    type HmacSha512 = Hmac<sha2::Sha512>;
    let mut mac = HmacSha512::new_from_slice(key.as_bytes())
        .map_err(|e| RuntimeError::Custom(format!("hmac-sha512: {}", e)))?;
    mac.update(data.as_bytes());
    let result = mac.finalize().into_bytes();
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(Value::Str(hex))
}

fn builtin_sha256_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sha256-bytes", args, 1)?;
    let data_i64 = extract_byte_list("sha256-bytes", &args[0])?;
    let bytes: Vec<u8> = data_i64.iter().map(|b| *b as u8).collect();
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    Ok(Value::IntList(result.iter().map(|b| *b as i64).collect()))
}

fn builtin_sha512_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("sha512-bytes", args, 1)?;
    let data_i64 = extract_byte_list("sha512-bytes", &args[0])?;
    let bytes: Vec<u8> = data_i64.iter().map(|b| *b as u8).collect();
    use sha2::Digest;
    let mut hasher = sha2::Sha512::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    Ok(Value::IntList(result.iter().map(|b| *b as i64).collect()))
}

fn builtin_hmac_sha256_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("hmac-sha256-bytes", args, 2)?;
    let key_i64 = extract_byte_list("hmac-sha256-bytes", &args[0])?;
    let key: Vec<u8> = key_i64.iter().map(|b| *b as u8).collect();
    let data_i64 = extract_byte_list("hmac-sha256-bytes", &args[1])?;
    let data: Vec<u8> = data_i64.iter().map(|b| *b as u8).collect();
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(&key)
        .map_err(|e| RuntimeError::Custom(format!("hmac-sha256-bytes: {}", e)))?;
    mac.update(&data);
    let result = mac.finalize().into_bytes();
    Ok(Value::IntList(result.iter().map(|b| *b as i64).collect()))
}

fn builtin_hmac_sha512_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("hmac-sha512-bytes", args, 2)?;
    let key_i64 = extract_byte_list("hmac-sha512-bytes", &args[0])?;
    let key: Vec<u8> = key_i64.iter().map(|b| *b as u8).collect();
    let data_i64 = extract_byte_list("hmac-sha512-bytes", &args[1])?;
    let data: Vec<u8> = data_i64.iter().map(|b| *b as u8).collect();
    use hmac::{Hmac, Mac};
    type HmacSha512 = Hmac<sha2::Sha512>;
    let mut mac = HmacSha512::new_from_slice(&key)
        .map_err(|e| RuntimeError::Custom(format!("hmac-sha512-bytes: {}", e)))?;
    mac.update(&data);
    let result = mac.finalize().into_bytes();
    Ok(Value::IntList(result.iter().map(|b| *b as i64).collect()))
}

fn builtin_pbkdf2_sha256(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("pbkdf2-sha256", args, 4)?;
    let password = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha256` expects Str as first argument".into(),
        )),
    };
    let salt_i64 = extract_byte_list("pbkdf2-sha256", &args[1])?;
    let salt: Vec<u8> = salt_i64.iter().map(|b| *b as u8).collect();
    let iterations = match &args[2] {
        Value::Int(n) => *n as u32,
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha256` expects Int as third argument (iterations)".into(),
        )),
    };
    let key_len = match &args[3] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha256` expects Int as fourth argument (key_len)".into(),
        )),
    };
    let mut derived = vec![0u8; key_len];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, iterations, &mut derived);
    Ok(Value::IntList(derived.iter().map(|b| *b as i64).collect()))
}

fn builtin_pbkdf2_sha512(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("pbkdf2-sha512", args, 4)?;
    let password = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha512` expects Str as first argument".into(),
        )),
    };
    let salt_i64 = extract_byte_list("pbkdf2-sha512", &args[1])?;
    let salt: Vec<u8> = salt_i64.iter().map(|b| *b as u8).collect();
    let iterations = match &args[2] {
        Value::Int(n) => *n as u32,
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha512` expects Int as third argument (iterations)".into(),
        )),
    };
    let key_len = match &args[3] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError(
            "`pbkdf2-sha512` expects Int as fourth argument (key_len)".into(),
        )),
    };
    let mut derived = vec![0u8; key_len];
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(password.as_bytes(), &salt, iterations, &mut derived);
    Ok(Value::IntList(derived.iter().map(|b| *b as i64).collect()))
}

fn builtin_base64_decode_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("base64-decode-bytes", args, 1)?;
    let data = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`base64-decode-bytes` expects a Str argument".into(),
        )),
    };
    use base64::Engine;
    match base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) {
        Ok(bytes) => Ok(Value::Variant("Ok".into(), Box::new(
            Value::IntList(bytes.iter().map(|b| *b as i64).collect())
        ))),
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
    }
}

fn builtin_base64_encode_bytes(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("base64-encode-bytes", args, 1)?;
    let data_i64 = extract_byte_list("base64-encode-bytes", &args[0])?;
    let bytes: Vec<u8> = data_i64.iter().map(|b| *b as u8).collect();
    use base64::Engine;
    Ok(Value::Str(base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

fn builtin_bitwise_xor(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bitwise-xor", args, 2)?;
    let a = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-xor` expects (Int, Int)".into(),
        )),
    };
    let b = match &args[1] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-xor` expects (Int, Int)".into(),
        )),
    };
    Ok(Value::Int(a ^ b))
}

fn builtin_bitwise_and(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bitwise-and", args, 2)?;
    let a = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-and` expects (Int, Int)".into(),
        )),
    };
    let b = match &args[1] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-and` expects (Int, Int)".into(),
        )),
    };
    Ok(Value::Int(a & b))
}

fn builtin_bitwise_or(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bitwise-or", args, 2)?;
    let a = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-or` expects (Int, Int)".into(),
        )),
    };
    let b = match &args[1] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError(
            "`bitwise-or` expects (Int, Int)".into(),
        )),
    };
    Ok(Value::Int(a | b))
}

// ── Format + Exit implementations ────────────────────────────────────────────

/// `(format template args...)` — replace `{}` placeholders left to right.
/// Variadic: first arg is template, rest are substitution values.
fn builtin_format(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::TypeError(
            "`format` expects at least 1 argument (template string)".into(),
        ));
    }
    let template = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError(
            "`format`: first argument must be a string template".into(),
        )),
    };
    let mut result = String::new();
    let mut arg_idx = 1; // start from second arg
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            if chars.peek() == Some(&'}') {
                chars.next(); // consume '}'
                if arg_idx < args.len() {
                    // Stringify the same way as `str` builtin
                    match &args[arg_idx] {
                        Value::Str(s) => result.push_str(s),
                        other => result.push_str(&format!("{}", other)),
                    }
                    arg_idx += 1;
                } else {
                    // Not enough args — leave placeholder as-is
                    result.push_str("{}");
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    Ok(Value::Str(result))
}

/// `(exit code)` — terminate the process with the given exit code.
fn builtin_exit(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("exit", args, 1)?;
    let code = match &args[0] {
        Value::Int(n) => *n as i32,
        _ => return Err(RuntimeError::TypeError(
            "`exit` expects an Int argument".into(),
        )),
    };
    std::process::exit(code);
}

// ── Byte encoding builtin implementations ────────────────────────────────────

/// Helper: extract an IntList (or List of Ints) as Vec<i64>
fn extract_byte_list(name: &str, val: &Value) -> Result<Vec<i64>, RuntimeError> {
    match val {
        Value::IntList(xs) => Ok(xs.clone()),
        Value::List(xs) => {
            let mut result = Vec::with_capacity(xs.len());
            for v in xs {
                match v {
                    Value::Int(n) => result.push(*n),
                    _ => return Err(RuntimeError::TypeError(
                        format!("`{}`: byte list must contain only integers", name),
                    )),
                }
            }
            Ok(result)
        }
        _ => Err(RuntimeError::TypeError(
            format!("`{}`: expected a byte list (IntList)", name),
        )),
    }
}

fn builtin_bytes_from_int16(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-from-int16", args, 1)?;
    let n = match &args[0] {
        Value::Int(n) => *n as i16,
        _ => return Err(RuntimeError::TypeError("`bytes-from-int16` expects Int".into())),
    };
    let bytes = n.to_be_bytes();
    Ok(Value::IntList(bytes.iter().map(|b| *b as i64).collect()))
}

fn builtin_bytes_from_int32(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-from-int32", args, 1)?;
    let n = match &args[0] {
        Value::Int(n) => *n as i32,
        _ => return Err(RuntimeError::TypeError("`bytes-from-int32` expects Int".into())),
    };
    let bytes = n.to_be_bytes();
    Ok(Value::IntList(bytes.iter().map(|b| *b as i64).collect()))
}

fn builtin_bytes_from_int64(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-from-int64", args, 1)?;
    let n = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`bytes-from-int64` expects Int".into())),
    };
    let bytes = n.to_be_bytes();
    Ok(Value::IntList(bytes.iter().map(|b| *b as i64).collect()))
}

fn builtin_bytes_to_int16(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-to-int16", args, 2)?;
    let buf = extract_byte_list("bytes-to-int16", &args[0])?;
    let offset = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-to-int16`: offset must be Int".into())),
    };
    if offset + 2 > buf.len() {
        return Err(RuntimeError::Custom(format!(
            "`bytes-to-int16`: need 2 bytes at offset {}, buf length {}", offset, buf.len()
        )));
    }
    let val = ((buf[offset] as i16) << 8) | (buf[offset + 1] as i16);
    Ok(Value::Int(val as i64))
}

fn builtin_bytes_to_int32(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-to-int32", args, 2)?;
    let buf = extract_byte_list("bytes-to-int32", &args[0])?;
    let offset = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-to-int32`: offset must be Int".into())),
    };
    if offset + 4 > buf.len() {
        return Err(RuntimeError::Custom(format!(
            "`bytes-to-int32`: need 4 bytes at offset {}, buf length {}", offset, buf.len()
        )));
    }
    let val = ((buf[offset] as i32) << 24)
        | ((buf[offset + 1] as i32) << 16)
        | ((buf[offset + 2] as i32) << 8)
        | (buf[offset + 3] as i32);
    Ok(Value::Int(val as i64))
}

fn builtin_bytes_to_int64(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-to-int64", args, 2)?;
    let buf = extract_byte_list("bytes-to-int64", &args[0])?;
    let offset = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-to-int64`: offset must be Int".into())),
    };
    if offset + 8 > buf.len() {
        return Err(RuntimeError::Custom(format!(
            "`bytes-to-int64`: need 8 bytes at offset {}, buf length {}", offset, buf.len()
        )));
    }
    let mut val: i64 = 0;
    for i in 0..8 {
        val = (val << 8) | (buf[offset + i] & 0xFF);
    }
    Ok(Value::Int(val))
}

fn builtin_bytes_from_string(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-from-string", args, 1)?;
    let s = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(RuntimeError::TypeError("`bytes-from-string` expects String".into())),
    };
    Ok(Value::IntList(s.as_bytes().iter().map(|b| *b as i64).collect()))
}

fn builtin_bytes_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-to-string", args, 3)?;
    let buf = extract_byte_list("bytes-to-string", &args[0])?;
    let offset = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-to-string`: offset must be Int".into())),
    };
    let len = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-to-string`: len must be Int".into())),
    };
    if offset + len > buf.len() {
        return Err(RuntimeError::Custom(format!(
            "`bytes-to-string`: need {} bytes at offset {}, buf length {}", len, offset, buf.len()
        )));
    }
    let bytes: Vec<u8> = buf[offset..offset + len].iter().map(|b| *b as u8).collect();
    match String::from_utf8(bytes) {
        Ok(s) => Ok(Value::Str(s)),
        Err(e) => Err(RuntimeError::Custom(format!("`bytes-to-string`: invalid UTF-8: {}", e))),
    }
}

fn builtin_bytes_concat(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-concat", args, 2)?;
    let mut a = extract_byte_list("bytes-concat", &args[0])?;
    let b = extract_byte_list("bytes-concat", &args[1])?;
    a.extend_from_slice(&b);
    Ok(Value::IntList(a))
}

fn builtin_bytes_slice(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("bytes-slice", args, 3)?;
    let buf = extract_byte_list("bytes-slice", &args[0])?;
    let offset = match &args[1] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-slice`: offset must be Int".into())),
    };
    let len = match &args[2] {
        Value::Int(n) => *n as usize,
        _ => return Err(RuntimeError::TypeError("`bytes-slice`: len must be Int".into())),
    };
    if offset + len > buf.len() {
        return Err(RuntimeError::Custom(format!(
            "`bytes-slice`: need {} bytes at offset {}, buf length {}", len, offset, buf.len()
        )));
    }
    Ok(Value::IntList(buf[offset..offset + len].to_vec()))
}

fn builtin_crc32c(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("crc32c", args, 1)?;
    let buf = extract_byte_list("crc32c", &args[0])?;
    let bytes: Vec<u8> = buf.iter().map(|b| *b as u8).collect();
    let checksum = crc32c::crc32c(&bytes);
    Ok(Value::Int(checksum as i64))
}

// ── TCP socket builtin implementations ───────────────────────────────────────

use std::net::TcpStream;
use std::sync::atomic::{AtomicI64, Ordering};
use std::io::{Read as IoRead, Write as IoWrite};

static NEXT_TCP_HANDLE: AtomicI64 = AtomicI64::new(1);

fn tcp_handles() -> &'static std::sync::Mutex<HashMap<i64, TcpStream>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<HashMap<i64, TcpStream>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn builtin_tcp_connect(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-connect", args, 2)?;
    let host = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err(RuntimeError::TypeError("`tcp-connect`: host must be String".into())),
    };
    let port = match &args[1] {
        Value::Int(n) => *n as u16,
        _ => return Err(RuntimeError::TypeError("`tcp-connect`: port must be Int".into())),
    };
    let addr = format!("{}:{}", host, port);
    match TcpStream::connect(&addr) {
        Ok(stream) => {
            let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
            tcp_handles().lock().unwrap().insert(handle, stream);
            Ok(Value::Variant("Ok".into(), Box::new(Value::Int(handle))))
        }
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-connect: {}", e)
        )))),
    }
}

fn builtin_tcp_close(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-close", args, 1)?;
    let handle = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-close`: handle must be Int".into())),
    };
    match tcp_handles().lock().unwrap().remove(&handle) {
        Some(_stream) => Ok(Value::Variant("Ok".into(), Box::new(Value::Nil))),
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-close: invalid handle {}", handle)
        )))),
    }
}

fn builtin_tcp_send(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-send", args, 2)?;
    let handle = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-send`: handle must be Int".into())),
    };
    let data = extract_byte_list("tcp-send", &args[1])?;
    let bytes: Vec<u8> = data.iter().map(|b| *b as u8).collect();

    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&handle) {
        Some(stream) => {
            match stream.write_all(&bytes) {
                Ok(()) => Ok(Value::Variant("Ok".into(), Box::new(Value::Int(bytes.len() as i64)))),
                Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                    format!("tcp-send: {}", e)
                )))),
            }
        }
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-send: invalid handle {}", handle)
        )))),
    }
}

fn builtin_tcp_recv(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-recv", args, 2)?;
    let handle = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-recv`: handle must be Int".into())),
    };
    let max_bytes = match &args[1] {
        Value::Int(n) if *n > 0 => *n as usize,
        Value::Int(_) => return Err(RuntimeError::Custom("`tcp-recv`: max-bytes must be positive".into())),
        _ => return Err(RuntimeError::TypeError("`tcp-recv`: max-bytes must be Int".into())),
    };

    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&handle) {
        Some(stream) => {
            let mut buf = vec![0u8; max_bytes];
            match stream.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    Ok(Value::Variant("Ok".into(), Box::new(
                        Value::IntList(buf.iter().map(|b| *b as i64).collect())
                    )))
                }
                Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                    format!("tcp-recv: {}", e)
                )))),
            }
        }
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-recv: invalid handle {}", handle)
        )))),
    }
}

fn builtin_tcp_recv_exact(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-recv-exact", args, 2)?;
    let handle = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-recv-exact`: handle must be Int".into())),
    };
    let n = match &args[1] {
        Value::Int(n) if *n > 0 => *n as usize,
        Value::Int(_) => return Err(RuntimeError::Custom("`tcp-recv-exact`: n must be positive".into())),
        _ => return Err(RuntimeError::TypeError("`tcp-recv-exact`: n must be Int".into())),
    };

    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&handle) {
        Some(stream) => {
            let mut buf = vec![0u8; n];
            match stream.read_exact(&mut buf) {
                Ok(()) => Ok(Value::Variant("Ok".into(), Box::new(
                    Value::IntList(buf.iter().map(|b| *b as i64).collect())
                ))),
                Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                    format!("tcp-recv-exact: {}", e)
                )))),
            }
        }
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-recv-exact: invalid handle {}", handle)
        )))),
    }
}

fn builtin_tcp_set_timeout(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("tcp-set-timeout", args, 2)?;
    let handle = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-set-timeout`: handle must be Int".into())),
    };
    let ms = match &args[1] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("`tcp-set-timeout`: ms must be Int".into())),
    };

    let timeout = if ms > 0 {
        Some(std::time::Duration::from_millis(ms as u64))
    } else {
        None
    };

    let handles = tcp_handles().lock().unwrap();
    match handles.get(&handle) {
        Some(stream) => {
            if let Err(e) = stream.set_read_timeout(timeout) {
                return Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                    format!("tcp-set-timeout: {}", e)
                ))));
            }
            if let Err(e) = stream.set_write_timeout(timeout) {
                return Ok(Value::Variant("Err".into(), Box::new(Value::Str(
                    format!("tcp-set-timeout: {}", e)
                ))));
            }
            Ok(Value::Variant("Ok".into(), Box::new(Value::Nil)))
        }
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("tcp-set-timeout: invalid handle {}", handle)
        )))),
    }
}

// ── Thread and channel builtin implementations ────────────────────────────────

pub static NEXT_THREAD_HANDLE: AtomicI64 = AtomicI64::new(1);

pub fn thread_handles() -> &'static std::sync::Mutex<HashMap<i64, std::thread::JoinHandle<Result<Value, String>>>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<HashMap<i64, std::thread::JoinHandle<Result<Value, String>>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_CHANNEL_HANDLE: AtomicI64 = AtomicI64::new(1);

fn channel_senders() -> &'static std::sync::Mutex<HashMap<i64, std::sync::mpsc::Sender<Value>>> {
    use std::sync::{Mutex, OnceLock, mpsc};
    static SENDERS: OnceLock<Mutex<HashMap<i64, mpsc::Sender<Value>>>> = OnceLock::new();
    SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn channel_receivers() -> &'static std::sync::Mutex<HashMap<i64, std::sync::mpsc::Receiver<Value>>> {
    use std::sync::{Mutex, OnceLock, mpsc};
    static RECEIVERS: OnceLock<Mutex<HashMap<i64, mpsc::Receiver<Value>>>> = OnceLock::new();
    RECEIVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn builtin_thread_join(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("thread-join", args, 1)?;
    let handle_id = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("thread-join: handle must be Int".into())),
    };
    let join_handle = thread_handles().lock().unwrap().remove(&handle_id)
        .ok_or_else(|| RuntimeError::Custom(
            format!("thread-join: invalid or already-joined handle {}", handle_id)
        ))?;
    match join_handle.join() {
        Ok(Ok(val)) => Ok(Value::Variant("Ok".into(), Box::new(val))),
        Ok(Err(msg)) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(msg)))),
        Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("thread panicked".into())))),
    }
}

fn builtin_channel_new(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("channel-new", args, 0)?;
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    let rx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    channel_senders().lock().unwrap().insert(tx_id, tx);
    channel_receivers().lock().unwrap().insert(rx_id, rx);
    Ok(Value::List(vec![Value::Int(tx_id), Value::Int(rx_id)]))
}

fn builtin_channel_send(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("channel-send", args, 2)?;
    let tx_id = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("channel-send: handle must be Int".into())),
    };
    let value = args[1].clone();
    let senders = channel_senders().lock().unwrap();
    match senders.get(&tx_id) {
        Some(tx) => match tx.send(value) {
            Ok(()) => Ok(Value::Variant("Ok".into(), Box::new(Value::Bool(true)))),
            Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("channel closed".into())))),
        },
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("channel-send: invalid sender handle {}", tx_id)
        )))),
    }
}

fn builtin_channel_recv(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("channel-recv", args, 1)?;
    let rx_id = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("channel-recv: handle must be Int".into())),
    };
    // Temporarily remove receiver from map to avoid holding the global lock during blocking recv
    let rx = channel_receivers().lock().unwrap().remove(&rx_id);
    match rx {
        Some(rx) => {
            let result = match rx.recv() {
                Ok(val) => Ok(Value::Variant("Ok".into(), Box::new(val))),
                Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str("channel closed".into())))),
            };
            // Put the receiver back
            channel_receivers().lock().unwrap().insert(rx_id, rx);
            result
        },
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("channel-recv: invalid receiver handle {}", rx_id)
        )))),
    }
}

fn builtin_channel_recv_timeout(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("channel-recv-timeout", args, 2)?;
    let rx_id = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("channel-recv-timeout: handle must be Int".into())),
    };
    let timeout_ms = match &args[1] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("channel-recv-timeout: timeout must be Int".into())),
    };
    // Temporarily remove receiver from map to avoid holding the global lock during blocking recv
    let rx = channel_receivers().lock().unwrap().remove(&rx_id);
    match rx {
        Some(rx) => {
            let duration = std::time::Duration::from_millis(timeout_ms as u64);
            let result = match rx.recv_timeout(duration) {
                Ok(val) => Ok(Value::Variant("Ok".into(), Box::new(val))),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) =>
                    Ok(Value::Variant("Err".into(), Box::new(Value::Str("timeout".into())))),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) =>
                    Ok(Value::Variant("Err".into(), Box::new(Value::Str("channel closed".into())))),
            };
            // Put the receiver back
            channel_receivers().lock().unwrap().insert(rx_id, rx);
            result
        }
        None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(
            format!("channel-recv-timeout: invalid receiver handle {}", rx_id)
        )))),
    }
}

fn builtin_channel_close(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("channel-close", args, 1)?;
    let handle_id = match &args[0] {
        Value::Int(n) => *n,
        _ => return Err(RuntimeError::TypeError("channel-close: handle must be Int".into())),
    };
    // Try removing from senders first, then receivers
    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().unwrap().remove(&handle_id).is_some();
    Ok(Value::Bool(removed_tx || removed_rx))
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

    // ── Native list builtins ────────────────────────────────

    #[test]
    fn reverse_list() {
        let b = builtins();
        let input = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = call(&b, "reverse", &[input]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
    }

    #[test]
    fn reverse_empty() {
        let b = builtins();
        let result = call(&b, "reverse", &[Value::List(vec![])]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn reverse_type_error() {
        let b = builtins();
        assert!(call(&b, "reverse", &[Value::Int(42)]).is_err());
    }

    #[test]
    fn concat_lists() {
        let b = builtins();
        let xs = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let ys = Value::List(vec![Value::Int(3), Value::Int(4)]);
        let result = call(&b, "concat", &[xs, ys]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4)
            ])
        );
    }

    #[test]
    fn concat_empty() {
        let b = builtins();
        let xs = Value::List(vec![Value::Int(1)]);
        let ys = Value::List(vec![]);
        let result = call(&b, "concat", &[xs, ys]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1)]));
    }

    #[test]
    fn concat_type_error() {
        let b = builtins();
        assert!(call(&b, "concat", &[Value::Int(1), Value::List(vec![])]).is_err());
    }

    #[test]
    fn flatten_nested_lists() {
        let b = builtins();
        let input = Value::List(vec![
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::List(vec![Value::Int(3), Value::Int(4)]),
        ]);
        let result = call(&b, "flatten", &[input]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4)
            ])
        );
    }

    #[test]
    fn flatten_mixed() {
        let b = builtins();
        let input = Value::List(vec![
            Value::List(vec![Value::Int(1)]),
            Value::Int(2),
            Value::List(vec![Value::Int(3)]),
        ]);
        let result = call(&b, "flatten", &[input]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn flatten_empty() {
        let b = builtins();
        let result = call(&b, "flatten", &[Value::List(vec![])]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn range_basic() {
        let b = builtins();
        let result = call(&b, "range", &[Value::Int(0), Value::Int(5)]).unwrap();
        assert_eq!(result, Value::IntList(vec![0, 1, 2, 3, 4]));
        // IntList should compare equal to the equivalent List
        assert_eq!(
            result,
            Value::List(vec![
                Value::Int(0),
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4)
            ])
        );
    }

    #[test]
    fn range_empty() {
        let b = builtins();
        let result = call(&b, "range", &[Value::Int(5), Value::Int(5)]).unwrap();
        assert_eq!(result, Value::IntList(vec![]));
    }

    #[test]
    fn range_negative_empty() {
        let b = builtins();
        let result = call(&b, "range", &[Value::Int(5), Value::Int(3)]).unwrap();
        assert_eq!(result, Value::IntList(vec![]));
    }

    #[test]
    fn range_type_error() {
        let b = builtins();
        assert!(call(&b, "range", &[Value::Float(0.0), Value::Int(5)]).is_err());
    }

    // ── Float Math Builtins ────────────────────────────

    #[test]
    fn sqrt_positive() {
        let b = builtins();
        assert_eq!(
            call(&b, "sqrt", &[Value::Float(4.0)]).unwrap(),
            Value::Float(2.0)
        );
    }

    #[test]
    fn sqrt_negative_is_nan() {
        let b = builtins();
        let result = call(&b, "sqrt", &[Value::Float(-1.0)]).unwrap();
        match result {
            Value::Float(f) => assert!(f.is_nan(), "sqrt(-1.0) should be NaN"),
            _ => panic!("sqrt should return Float"),
        }
    }

    #[test]
    fn sin_zero() {
        let b = builtins();
        assert_eq!(
            call(&b, "sin", &[Value::Float(0.0)]).unwrap(),
            Value::Float(0.0)
        );
    }

    #[test]
    fn cos_zero() {
        let b = builtins();
        assert_eq!(
            call(&b, "cos", &[Value::Float(0.0)]).unwrap(),
            Value::Float(1.0)
        );
    }

    #[test]
    fn tan_zero() {
        let b = builtins();
        assert_eq!(
            call(&b, "tan", &[Value::Float(0.0)]).unwrap(),
            Value::Float(0.0)
        );
    }

    #[test]
    fn log_one() {
        let b = builtins();
        assert_eq!(
            call(&b, "log", &[Value::Float(1.0)]).unwrap(),
            Value::Float(0.0)
        );
    }

    #[test]
    fn log_zero_is_neg_infinity() {
        let b = builtins();
        assert_eq!(
            call(&b, "log", &[Value::Float(0.0)]).unwrap(),
            Value::Float(f64::NEG_INFINITY)
        );
    }

    #[test]
    fn exp_zero() {
        let b = builtins();
        assert_eq!(
            call(&b, "exp", &[Value::Float(0.0)]).unwrap(),
            Value::Float(1.0)
        );
    }

    #[test]
    fn floor_float() {
        let b = builtins();
        assert_eq!(
            call(&b, "floor", &[Value::Float(3.7)]).unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn ceil_float() {
        let b = builtins();
        assert_eq!(
            call(&b, "ceil", &[Value::Float(3.2)]).unwrap(),
            Value::Int(4)
        );
    }

    #[test]
    fn round_float() {
        let b = builtins();
        assert_eq!(
            call(&b, "round", &[Value::Float(3.5)]).unwrap(),
            Value::Int(4)
        );
    }

    #[test]
    fn float_to_int_truncates() {
        let b = builtins();
        assert_eq!(
            call(&b, "float-to-int", &[Value::Float(3.14)]).unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn int_to_float_converts() {
        let b = builtins();
        assert_eq!(
            call(&b, "int-to-float", &[Value::Int(3)]).unwrap(),
            Value::Float(3.0)
        );
    }

    #[test]
    fn is_nan_true() {
        let b = builtins();
        let nan = call(&b, "nan", &[]).unwrap();
        assert_eq!(
            call(&b, "is-nan?", &[nan]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn is_nan_false() {
        let b = builtins();
        assert_eq!(
            call(&b, "is-nan?", &[Value::Float(1.0)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn is_infinite_true() {
        let b = builtins();
        let inf = call(&b, "infinity", &[]).unwrap();
        assert_eq!(
            call(&b, "is-infinite?", &[inf]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn is_infinite_false() {
        let b = builtins();
        assert_eq!(
            call(&b, "is-infinite?", &[Value::Float(1.0)]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Path tests ──────────────────────────────────────

    #[test]
    fn path_join_basic() {
        let b = builtins();
        let parts = Value::List(vec![
            Value::Str("home".into()),
            Value::Str("user".into()),
            Value::Str("file.txt".into()),
        ]);
        let result = call(&b, "path-join", &[parts]).unwrap();
        // On Unix: "home/user/file.txt"
        if let Value::Str(s) = &result {
            assert!(s.contains("user"));
            assert!(s.contains("file.txt"));
        } else {
            panic!("expected Str");
        }
    }

    #[test]
    fn path_parent_basic() {
        let b = builtins();
        let result = call(&b, "path-parent", &[Value::Str("/home/user/file.txt".into())]).unwrap();
        assert_eq!(result, Value::Str("/home/user".into()));
    }

    #[test]
    fn path_parent_no_parent() {
        let b = builtins();
        let result = call(&b, "path-parent", &[Value::Str("/".into())]).unwrap();
        assert_eq!(result, Value::Str("".into()));
    }

    #[test]
    fn path_filename_basic() {
        let b = builtins();
        let result = call(&b, "path-filename", &[Value::Str("/home/user/file.txt".into())]).unwrap();
        assert_eq!(result, Value::Str("file.txt".into()));
    }

    #[test]
    fn path_extension_basic() {
        let b = builtins();
        assert_eq!(
            call(&b, "path-extension", &[Value::Str("file.txt".into())]).unwrap(),
            Value::Str("txt".into())
        );
        assert_eq!(
            call(&b, "path-extension", &[Value::Str("file".into())]).unwrap(),
            Value::Str("".into())
        );
    }

    #[test]
    fn is_absolute_path() {
        let b = builtins();
        assert_eq!(
            call(&b, "is-absolute?", &[Value::Str("/usr/bin".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "is-absolute?", &[Value::Str("relative/path".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    // ── Regex tests ─────────────────────────────────────

    #[test]
    fn regex_match_full() {
        let b = builtins();
        assert_eq!(
            call(&b, "regex-match", &[Value::Str(r"\d+".into()), Value::Str("12345".into())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "regex-match", &[Value::Str(r"\d+".into()), Value::Str("abc".into())]).unwrap(),
            Value::Bool(false)
        );
        // partial match should fail (full-string match)
        assert_eq!(
            call(&b, "regex-match", &[Value::Str(r"\d+".into()), Value::Str("abc123".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn regex_find_all_basic() {
        let b = builtins();
        let result = call(&b, "regex-find-all", &[
            Value::Str(r"\d+".into()),
            Value::Str("abc 123 def 456".into()),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::Str("123".into()),
            Value::Str("456".into()),
        ]));
    }

    #[test]
    fn regex_replace_basic() {
        let b = builtins();
        let result = call(&b, "regex-replace", &[
            Value::Str(r"\d+".into()),
            Value::Str("abc 123 def 456".into()),
            Value::Str("NUM".into()),
        ]).unwrap();
        assert_eq!(result, Value::Str("abc NUM def NUM".into()));
    }

    #[test]
    fn regex_split_basic() {
        let b = builtins();
        let result = call(&b, "regex-split", &[
            Value::Str(r"\s+".into()),
            Value::Str("hello   world   test".into()),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::Str("hello".into()),
            Value::Str("world".into()),
            Value::Str("test".into()),
        ]));
    }

    #[test]
    fn regex_invalid_pattern() {
        let b = builtins();
        let result = call(&b, "regex-match", &[
            Value::Str(r"[invalid".into()),
            Value::Str("test".into()),
        ]);
        assert!(result.is_err());
    }

    // ── Crypto tests ────────────────────────────────────

    #[test]
    fn sha256_known_hash() {
        let b = builtins();
        // SHA-256 of empty string
        let result = call(&b, "sha256", &[Value::Str("".into())]).unwrap();
        assert_eq!(result, Value::Str(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into()
        ));
    }

    #[test]
    fn sha256_hello() {
        let b = builtins();
        let result = call(&b, "sha256", &[Value::Str("hello".into())]).unwrap();
        assert_eq!(result, Value::Str(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into()
        ));
    }

    #[test]
    fn hmac_sha256_basic() {
        let b = builtins();
        let result = call(&b, "hmac-sha256", &[
            Value::Str("key".into()),
            Value::Str("data".into()),
        ]).unwrap();
        // Known HMAC-SHA256("key", "data")
        if let Value::Str(s) = &result {
            assert_eq!(s.len(), 64); // 32 bytes in hex
        } else {
            panic!("expected Str");
        }
    }

    #[test]
    fn base64_encode_decode_roundtrip() {
        let b = builtins();
        let encoded = call(&b, "base64-encode", &[Value::Str("hello world".into())]).unwrap();
        assert_eq!(encoded, Value::Str("aGVsbG8gd29ybGQ=".into()));

        let decoded = call(&b, "base64-decode", &[Value::Str("aGVsbG8gd29ybGQ=".into())]).unwrap();
        assert_eq!(decoded, Value::Variant("Ok".into(), Box::new(Value::Str("hello world".into()))));
    }

    #[test]
    fn base64_decode_invalid() {
        let b = builtins();
        let result = call(&b, "base64-decode", &[Value::Str("!!!invalid!!!".into())]).unwrap();
        assert!(matches!(result, Value::Variant(ref tag, _) if tag == "Err"));
    }

    #[test]
    fn random_bytes_length() {
        let b = builtins();
        let result = call(&b, "random-bytes", &[Value::Int(16)]).unwrap();
        if let Value::Str(s) = &result {
            assert_eq!(s.len(), 32); // 16 bytes = 32 hex chars
        } else {
            panic!("expected Str");
        }
    }

    #[test]
    fn random_bytes_zero() {
        let b = builtins();
        let result = call(&b, "random-bytes", &[Value::Int(0)]).unwrap();
        assert_eq!(result, Value::Str("".into()));
    }

    // ── Format tests ────────────────────────────────────

    #[test]
    fn format_basic() {
        let b = builtins();
        let result = call(&b, "format", &[
            Value::Str("Hello, {}! You are {} years old.".into()),
            Value::Str("Alice".into()),
            Value::Int(30),
        ]).unwrap();
        assert_eq!(result, Value::Str("Hello, Alice! You are 30 years old.".into()));
    }

    #[test]
    fn format_no_placeholders() {
        let b = builtins();
        let result = call(&b, "format", &[
            Value::Str("no placeholders here".into()),
        ]).unwrap();
        assert_eq!(result, Value::Str("no placeholders here".into()));
    }

    #[test]
    fn format_excess_placeholders() {
        let b = builtins();
        let result = call(&b, "format", &[
            Value::Str("{} and {}".into()),
            Value::Str("one".into()),
        ]).unwrap();
        assert_eq!(result, Value::Str("one and {}".into()));
    }

    #[test]
    fn format_no_args_error() {
        let b = builtins();
        let result = call(&b, "format", &[]);
        assert!(result.is_err());
    }

    // ── Exit test (registration only) ───────────────────

    #[test]
    fn exit_is_registered() {
        let b = builtins();
        assert!(b.has("exit"));
    }

    // ── Registration tests for new builtins ─────────────

    #[test]
    fn new_builtins_registered() {
        let b = builtins();
        // Path
        assert!(b.has("path-join"));
        assert!(b.has("path-parent"));
        assert!(b.has("path-filename"));
        assert!(b.has("path-extension"));
        assert!(b.has("is-absolute?"));
        // Regex
        assert!(b.has("regex-match"));
        assert!(b.has("regex-find-all"));
        assert!(b.has("regex-replace"));
        assert!(b.has("regex-split"));
        // Crypto
        assert!(b.has("sha256"));
        assert!(b.has("hmac-sha256"));
        assert!(b.has("base64-encode"));
        assert!(b.has("base64-decode"));
        assert!(b.has("random-bytes"));
        // Format + Exit
        assert!(b.has("format"));
        assert!(b.has("exit"));
    }

    // ── read-lines ─────────────────────────────────────

    #[test]
    fn read_lines_returns_list_of_strings() {
        let b = builtins();
        let tmp = format!("test_readlines_{}.tmp", std::process::id());
        std::fs::write(&tmp, "alpha\nbeta\ngamma").unwrap();
        let result = call(&b, "read-lines", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::Str("alpha".into()),
            Value::Str("beta".into()),
            Value::Str("gamma".into()),
        ]));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn read_lines_empty_file() {
        let b = builtins();
        let tmp = format!("test_readlines_empty_{}.tmp", std::process::id());
        std::fs::write(&tmp, "").unwrap();
        let result = call(&b, "read-lines", &[Value::Str(tmp.clone())]).unwrap();
        assert_eq!(result, Value::List(vec![]));
        std::fs::remove_file(&tmp).ok();
    }

    // ── char-count ─────────────────────────────────────

    #[test]
    fn char_count_ascii() {
        let b = builtins();
        let result = call(&b, "char-count", &[Value::Str("hello".into())]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn char_count_multibyte() {
        let b = builtins();
        let result = call(&b, "char-count", &[Value::Str("café".into())]).unwrap();
        assert_eq!(result, Value::Int(4));
    }

    #[test]
    fn char_count_empty() {
        let b = builtins();
        let result = call(&b, "char-count", &[Value::Str("".into())]).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    // ── take ──────────────────────────────────────────────

    #[test]
    fn take_partial() {
        let b = builtins();
        let result = call(&b, "take", &[
            Value::Int(2),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn take_more_than_length() {
        let b = builtins();
        let result = call(&b, "take", &[
            Value::Int(5),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2)]));
    }

    // ── drop ──────────────────────────────────────────────

    #[test]
    fn drop_partial() {
        let b = builtins();
        let result = call(&b, "drop", &[
            Value::Int(2),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(3)]));
    }

    #[test]
    fn drop_more_than_length() {
        let b = builtins();
        let result = call(&b, "drop", &[
            Value::Int(5),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    // ── zip ───────────────────────────────────────────────

    #[test]
    fn zip_equal_length() {
        let b = builtins();
        let result = call(&b, "zip", &[
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::List(vec![Value::Int(3), Value::Int(4)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::List(vec![Value::Int(1), Value::Int(3)]),
            Value::List(vec![Value::Int(2), Value::Int(4)]),
        ]));
    }

    #[test]
    fn zip_unequal_length() {
        let b = builtins();
        let result = call(&b, "zip", &[
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::List(vec![Value::Int(10)]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::List(vec![Value::Int(1), Value::Int(10)]),
        ]));
    }

    // ── enumerate ─────────────────────────────────────────

    #[test]
    fn enumerate_basic() {
        let b = builtins();
        let result = call(&b, "enumerate", &[
            Value::List(vec![Value::Str("a".into()), Value::Str("b".into())]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![
            Value::List(vec![Value::Int(0), Value::Str("a".into())]),
            Value::List(vec![Value::Int(1), Value::Str("b".into())]),
        ]));
    }

    #[test]
    fn enumerate_empty() {
        let b = builtins();
        let result = call(&b, "enumerate", &[
            Value::List(vec![]),
        ]).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    // ── any/all/find (VM-aware) ───────────────────────────
    // These builtins require a running VM to call closures, so they
    // need integration tests rather than unit tests. The builtins are
    // registered and can be verified via has():

    #[test]
    fn any_all_find_registered() {
        let b = builtins();
        assert!(b.has("any"), "any builtin should be registered");
        assert!(b.has("all"), "all builtin should be registered");
        assert!(b.has("find"), "find builtin should be registered");
        assert!(b.get_with_vm("any").is_some(), "any should be VM-aware");
        assert!(b.get_with_vm("all").is_some(), "all should be VM-aware");
        assert!(b.get_with_vm("find").is_some(), "find should be VM-aware");
    }

    // ── IntList builtins ─────────────────────────────────

    #[test]
    fn range_returns_intlist() {
        let b = builtins();
        let result = call(&b, "range", &[Value::Int(0), Value::Int(5)]).unwrap();
        assert_eq!(result, Value::IntList(vec![0, 1, 2, 3, 4]));
    }

    #[test]
    fn length_intlist() {
        let b = builtins();
        let result = call(&b, "length", &[Value::IntList(vec![10, 20, 30])]).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn at_intlist() {
        let b = builtins();
        let result = call(&b, "at", &[Value::IntList(vec![10, 20, 30]), Value::Int(2)]).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn at_intlist_out_of_bounds() {
        let b = builtins();
        let result = call(&b, "at", &[Value::IntList(vec![10]), Value::Int(5)]);
        assert!(result.is_err());
    }

    #[test]
    fn head_intlist() {
        let b = builtins();
        let result = call(&b, "head", &[Value::IntList(vec![7, 8, 9])]).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn head_intlist_empty() {
        let b = builtins();
        let result = call(&b, "head", &[Value::IntList(vec![])]);
        assert!(result.is_err());
    }

    #[test]
    fn tail_intlist() {
        let b = builtins();
        let result = call(&b, "tail", &[Value::IntList(vec![1, 2, 3])]).unwrap();
        assert_eq!(result, Value::IntList(vec![2, 3]));
    }

    #[test]
    fn reverse_intlist() {
        let b = builtins();
        let result = call(&b, "reverse", &[Value::IntList(vec![1, 2, 3])]).unwrap();
        assert_eq!(result, Value::IntList(vec![3, 2, 1]));
    }

    #[test]
    fn empty_intlist() {
        let b = builtins();
        assert_eq!(
            call(&b, "empty?", &[Value::IntList(vec![])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "empty?", &[Value::IntList(vec![1])]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn cons_int_into_intlist() {
        let b = builtins();
        let result = call(&b, "cons", &[Value::Int(0), Value::IntList(vec![1, 2])]).unwrap();
        assert_eq!(result, Value::IntList(vec![0, 1, 2]));
    }

    #[test]
    fn cons_nonint_promotes_intlist() {
        let b = builtins();
        let result = call(&b, "cons", &[Value::Str("x".into()), Value::IntList(vec![1, 2])]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Str("x".into()), Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn append_int_to_intlist() {
        let b = builtins();
        let result = call(&b, "append", &[Value::IntList(vec![1, 2]), Value::Int(3)]).unwrap();
        assert_eq!(result, Value::IntList(vec![1, 2, 3]));
    }

    #[test]
    fn append_nonint_promotes_intlist() {
        let b = builtins();
        let result = call(&b, "append", &[Value::IntList(vec![1, 2]), Value::Str("x".into())]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2), Value::Str("x".into())]));
    }

    #[test]
    fn concat_intlists() {
        let b = builtins();
        let result = call(&b, "concat", &[Value::IntList(vec![1, 2]), Value::IntList(vec![3, 4])]).unwrap();
        assert_eq!(result, Value::IntList(vec![1, 2, 3, 4]));
    }

    #[test]
    fn concat_intlist_and_list() {
        let b = builtins();
        let result = call(&b, "concat", &[Value::IntList(vec![1]), Value::List(vec![Value::Str("a".into())])]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Str("a".into())]));
    }

    #[test]
    fn list_contains_intlist() {
        let b = builtins();
        assert_eq!(
            call(&b, "list-contains?", &[Value::IntList(vec![1, 2, 3]), Value::Int(2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(&b, "list-contains?", &[Value::IntList(vec![1, 2, 3]), Value::Int(5)]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            call(&b, "list-contains?", &[Value::IntList(vec![1, 2, 3]), Value::Str("x".into())]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn at_or_intlist() {
        let b = builtins();
        assert_eq!(
            call(&b, "at-or", &[Value::IntList(vec![10, 20]), Value::Int(0), Value::Int(-1)]).unwrap(),
            Value::Int(10)
        );
        assert_eq!(
            call(&b, "at-or", &[Value::IntList(vec![10, 20]), Value::Int(5), Value::Int(-1)]).unwrap(),
            Value::Int(-1)
        );
    }

    #[test]
    fn set_at_intlist_int() {
        let b = builtins();
        let result = call(&b, "set-at", &[Value::IntList(vec![1, 2, 3]), Value::Int(1), Value::Int(99)]).unwrap();
        assert_eq!(result, Value::IntList(vec![1, 99, 3]));
    }

    #[test]
    fn set_at_intlist_promotes() {
        let b = builtins();
        let result = call(&b, "set-at", &[Value::IntList(vec![1, 2, 3]), Value::Int(1), Value::Str("x".into())]).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Str("x".into()), Value::Int(3)]));
    }

    #[test]
    fn take_intlist() {
        let b = builtins();
        let result = call(&b, "take", &[Value::Int(2), Value::IntList(vec![1, 2, 3])]).unwrap();
        assert_eq!(result, Value::IntList(vec![1, 2]));
    }

    #[test]
    fn drop_intlist() {
        let b = builtins();
        let result = call(&b, "drop", &[Value::Int(1), Value::IntList(vec![1, 2, 3])]).unwrap();
        assert_eq!(result, Value::IntList(vec![2, 3]));
    }

    #[test]
    fn intlist_eq_list_via_eq_builtin() {
        let b = builtins();
        let result = call(&b, "=", &[
            Value::IntList(vec![1, 2, 3]),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        ]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn type_of_intlist() {
        let b = builtins();
        let result = call(&b, "type-of", &[Value::IntList(vec![1, 2])]).unwrap();
        assert_eq!(result, Value::Str("List".into()));
    }
}
