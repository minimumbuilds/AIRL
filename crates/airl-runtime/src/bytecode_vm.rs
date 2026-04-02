// crates/airl-runtime/src/bytecode_vm.rs
//
// v0.6.0 Phase 3: Builtins struct removed. All builtin dispatch goes through
// dispatch_rt_builtin() calling airl-rt extern "C" functions, plus special-case
// handlers for thread-spawn, fn-metadata, thread-join, channel-*, compile-*, run-bytecode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use crate::bytecode::*;
use crate::value::Value;
use crate::error::RuntimeError;
use airl_rt::value::{RtValue, RtData, rt_nil, rt_int, rt_float, rt_bool, rt_str, rt_list, rt_map, rt_variant};
use airl_rt::memory::{airl_value_retain, airl_value_release};

/// Maximum instruction count for a "simple" closure eligible for inline eval.
const SIMPLE_CLOSURE_MAX_INSTRS: usize = 15;
/// Maximum parameter count for inline eval (keeps the register bank small).
const SIMPLE_CLOSURE_MAX_PARAMS: usize = 8;

/// Check whether a compiled function is "simple" enough to evaluate inline
/// without pushing a full VM call frame.
fn is_simple_closure(func: &BytecodeFunc) -> bool {
    if func.instructions.len() > SIMPLE_CLOSURE_MAX_INSTRS {
        return false;
    }
    if func.arity as usize > SIMPLE_CLOSURE_MAX_PARAMS {
        return false;
    }
    for instr in &func.instructions {
        match instr.op {
            Op::LoadConst | Op::LoadNil | Op::LoadTrue | Op::LoadFalse | Op::Move
            | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Neg
            | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Not
            | Op::Return
            | Op::CallBuiltin
            | Op::MakeList
            | Op::MarkMoved | Op::CheckNotMoved => {}
            _ => return false,
        }
    }
    true
}

// ── Value / RtValue conversion helpers ──────────────────────────────

/// Convert interpreter Value to *mut RtValue. Caller owns the returned pointer (rc=1).
pub fn value_to_rt(v: &Value) -> *mut RtValue {
    match v {
        Value::Int(n)   => rt_int(*n),
        Value::Float(f) => rt_float(*f),
        Value::Bool(b)  => rt_bool(*b),
        Value::Str(s)   => rt_str(s.clone()),
        Value::Nil      => rt_nil(),
        Value::Unit     => airl_rt::value::rt_unit(),
        Value::List(items) => {
            let ptrs: Vec<*mut RtValue> = items.iter().map(|i| value_to_rt(i)).collect();
            rt_list(ptrs)
        }
        Value::IntList(ints) => {
            let ptrs: Vec<*mut RtValue> = ints.iter().map(|n| rt_int(*n)).collect();
            rt_list(ptrs)
        }
        Value::Tuple(items) => {
            let ptrs: Vec<*mut RtValue> = items.iter().map(|i| value_to_rt(i)).collect();
            rt_list(ptrs)
        }
        Value::Variant(tag, inner) => {
            let inner_ptr = value_to_rt(inner);
            rt_variant(tag.clone(), inner_ptr)
        }
        Value::Map(map) => {
            let mut rt_map_data: HashMap<String, *mut RtValue> = HashMap::new();
            for (k, val) in map {
                rt_map_data.insert(k.clone(), value_to_rt(val));
            }
            rt_map(rt_map_data)
        }
        Value::BytecodeClosure(bc) => {
            let name_rt = rt_str(bc.func_name.clone());
            let mut caps: Vec<*mut RtValue> = vec![name_rt];
            for c in &bc.captured {
                caps.push(value_to_rt(c));
            }
            airl_rt::closure::airl_make_closure(
                std::ptr::null(),
                if caps.is_empty() { std::ptr::null() } else { caps.as_ptr() },
                caps.len(),
            )
        }
        Value::Bytes(v) => airl_rt::value::rt_bytes(v.clone()),
        Value::BuiltinFn(_) | Value::IRFuncRef(_) => rt_nil(),
    }
}

/// Convert *mut RtValue back to interpreter Value (non-owning read).
pub fn rt_to_value_no_release(ptr: *mut RtValue) -> Value {
    if ptr.is_null() {
        return Value::Nil;
    }
    unsafe {
        match &(*ptr).data {
            RtData::Nil      => Value::Nil,
            RtData::Unit     => Value::Unit,
            RtData::Int(n)   => Value::Int(*n),
            RtData::Float(f) => Value::Float(*f),
            RtData::Bool(b)  => Value::Bool(*b),
            RtData::Str(s)   => Value::Str(s.clone()),
            RtData::List { .. } => {
                let slice = airl_rt::list::list_items(&(*ptr).data);
                let vals: Vec<Value> = slice.iter().map(|&item| rt_to_value_no_release(item)).collect();
                Value::List(vals)
            }
            RtData::Map(map) => {
                let mut result_map = HashMap::new();
                for (k, &val) in map {
                    result_map.insert(k.clone(), rt_to_value_no_release(val));
                }
                Value::Map(result_map)
            }
            RtData::Variant { tag_name, inner } => {
                Value::Variant(tag_name.clone(), Box::new(rt_to_value_no_release(*inner)))
            }
            RtData::Closure { captures, func_ptr } if !captures.is_empty() => {
                let first = &*captures[0];
                if let RtData::Str(name) = &first.data {
                    let mut captured_values = Vec::new();
                    for i in 1..captures.len() {
                        captured_values.push(rt_to_value_no_release(captures[i]));
                    }
                    Value::BytecodeClosure(BytecodeClosureValue {
                        func_name: name.clone(),
                        captured: captured_values,
                    })
                } else {
                    // First capture is not a name string — preserve identity instead of nil.
                    Value::BuiltinFn(format!("<closure@{:p}>", func_ptr))
                }
            }
            RtData::Closure { func_ptr, .. } => {
                // Empty-captures closure — preserve identity instead of nil.
                Value::BuiltinFn(format!("<closure@{:p}>", func_ptr))
            }
            RtData::Bytes(v) => Value::Bytes(v.clone()),
        }
    }
}

/// Convert *mut RtValue back to interpreter Value. Also releases the pointer.
pub fn rt_to_value(ptr: *mut RtValue) -> Value {
    let result = rt_to_value_no_release(ptr);
    airl_value_release(ptr);
    result
}

/// Extract bool exactly (must be Bool(true)).
fn rt_is_bool_true(ptr: *mut RtValue) -> bool {
    if ptr.is_null() { return false; }
    let v = unsafe { rt_ref(ptr) };
    v.try_as_bool() == Some(true)
}

fn rt_is_bool_false(ptr: *mut RtValue) -> bool {
    if ptr.is_null() { return false; }
    let v = unsafe { rt_ref(ptr) };
    v.try_as_bool() == Some(false)
}

/// Display an RtValue for error messages (non-owning read).
fn rt_display(ptr: *mut RtValue) -> String {
    if ptr.is_null() { return "nil".to_string(); }
    let v = unsafe { rt_ref(ptr) };
    format!("{}", v)
}

/// Dereference a non-null `*mut RtValue` to a shared reference.
/// Centralises the single unsafe dereference for the bytecode VM.
///
/// # Safety
///
/// `ptr` must be a valid, non-null, properly retained `*mut RtValue`.
#[inline(always)]
unsafe fn rt_ref(ptr: *mut RtValue) -> &'static RtValue {
    &*ptr
}

// ── Register helpers ──────────────────────────────────────────────

/// Store a new value in a register, releasing the old value.
#[inline(always)]
fn reg_set(regs: &mut [*mut RtValue], idx: usize, val: *mut RtValue) {
    let old = regs[idx];
    regs[idx] = val;
    if !old.is_null() {
        airl_value_release(old);
    }
}

/// Read a register value (no ownership change).
#[inline(always)]
fn reg_get(regs: &[*mut RtValue], idx: usize) -> *mut RtValue {
    regs[idx]
}

/// Release all registers in a frame.
fn release_registers(regs: &mut [*mut RtValue]) {
    for r in regs.iter_mut() {
        if !r.is_null() {
            airl_value_release(*r);
            *r = std::ptr::null_mut();
        }
    }
}

// ── CallFrame ──────────────────────────────────────────────────────

struct CallFrame {
    registers: Vec<*mut RtValue>,
    func_name: String,
    ip: usize,
    return_reg: u16,
    match_flag: bool,
    moved: Vec<bool>,
}

// ── Thread/channel globals ──────────────────────────────────────────

pub(crate) static NEXT_THREAD_HANDLE: AtomicI64 = AtomicI64::new(1);

pub(crate) fn thread_handles() -> &'static std::sync::Mutex<HashMap<i64, std::thread::JoinHandle<Result<Value, String>>>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<HashMap<i64, std::thread::JoinHandle<Result<Value, String>>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_CHANNEL_HANDLE: AtomicI64 = AtomicI64::new(1);

fn channel_senders() -> &'static std::sync::Mutex<HashMap<i64, std::sync::mpsc::Sender<Value>>> {
    use std::sync::{Mutex, OnceLock};
    static SENDERS: OnceLock<Mutex<HashMap<i64, std::sync::mpsc::Sender<Value>>>> = OnceLock::new();
    SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn channel_receivers() -> &'static std::sync::Mutex<HashMap<i64, std::sync::mpsc::Receiver<Value>>> {
    use std::sync::{Mutex, OnceLock};
    static RECEIVERS: OnceLock<Mutex<HashMap<i64, std::sync::mpsc::Receiver<Value>>>> = OnceLock::new();
    RECEIVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Thread/channel dispatch helpers (RtValue interface) ─────────────

fn dispatch_thread_join(args: &[*mut RtValue]) -> *mut RtValue {
    let handle_id = match args.first() {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_variant("Err".into(), rt_str("thread-join: handle must be Int".into())),
        },
        _ => return rt_variant("Err".into(), rt_str("thread-join: requires 1 argument".into())),
    };
    let join_handle = match thread_handles().lock().expect("thread handles lock poisoned").remove(&handle_id) {
        Some(h) => h,
        None => return rt_variant("Err".into(), rt_str(format!("thread-join: invalid or already-joined handle {}", handle_id))),
    };
    match join_handle.join() {
        Ok(Ok(val)) => {
            let inner = value_to_rt(&val);
            rt_variant("Ok".into(), inner)
        }
        Ok(Err(msg)) => rt_variant("Err".into(), rt_str(msg)),
        Err(_) => rt_variant("Err".into(), rt_str("thread panicked".into())),
    }
}

fn dispatch_channel_new(_args: &[*mut RtValue]) -> *mut RtValue {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    let rx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    channel_senders().lock().expect("channel lock poisoned").insert(tx_id, tx);
    channel_receivers().lock().expect("channel lock poisoned").insert(rx_id, rx);
    rt_list(vec![rt_int(tx_id), rt_int(rx_id)])
}

fn dispatch_channel_send(args: &[*mut RtValue]) -> *mut RtValue {
    let tx_id = match args.first() {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_variant("Err".into(), rt_str("channel-send: handle must be Int".into())),
        },
        _ => return rt_variant("Err".into(), rt_str("channel-send: requires 2 arguments".into())),
    };
    let value = args.get(1).map(|&p| rt_to_value_no_release(p)).unwrap_or(Value::Nil);
    let senders = channel_senders().lock().expect("channel lock poisoned");
    match senders.get(&tx_id) {
        Some(tx) => match tx.send(value) {
            Ok(()) => rt_variant("Ok".into(), rt_bool(true)),
            Err(_) => rt_variant("Err".into(), rt_str("channel closed".into())),
        },
        None => rt_variant("Err".into(), rt_str(format!("channel-send: invalid sender handle {}", tx_id))),
    }
}

fn dispatch_channel_recv(args: &[*mut RtValue]) -> *mut RtValue {
    let rx_id = match args.first() {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_variant("Err".into(), rt_str("channel-recv: handle must be Int".into())),
        },
        _ => return rt_variant("Err".into(), rt_str("channel-recv: requires 1 argument".into())),
    };
    let rx = channel_receivers().lock().expect("channel lock poisoned").remove(&rx_id);
    match rx {
        Some(rx) => {
            let result = match rx.recv() {
                Ok(val) => rt_variant("Ok".into(), value_to_rt(&val)),
                Err(_) => rt_variant("Err".into(), rt_str("channel closed".into())),
            };
            channel_receivers().lock().expect("channel lock poisoned").insert(rx_id, rx);
            result
        },
        None => rt_variant("Err".into(), rt_str(format!("channel-recv: invalid receiver handle {}", rx_id))),
    }
}

fn dispatch_channel_recv_timeout(args: &[*mut RtValue]) -> *mut RtValue {
    let rx_id = match args.first() {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_variant("Err".into(), rt_str("channel-recv-timeout: handle must be Int".into())),
        },
        _ => return rt_variant("Err".into(), rt_str("channel-recv-timeout: requires 2 arguments".into())),
    };
    let timeout_ms = match args.get(1) {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_variant("Err".into(), rt_str("channel-recv-timeout: timeout must be Int".into())),
        },
        _ => return rt_variant("Err".into(), rt_str("channel-recv-timeout: requires 2 arguments".into())),
    };
    let rx = channel_receivers().lock().expect("channel lock poisoned").remove(&rx_id);
    match rx {
        Some(rx) => {
            let duration = std::time::Duration::from_millis(timeout_ms as u64);
            let result = match rx.recv_timeout(duration) {
                Ok(val) => rt_variant("Ok".into(), value_to_rt(&val)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => rt_variant("Err".into(), rt_str("timeout".into())),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => rt_variant("Err".into(), rt_str("channel closed".into())),
            };
            channel_receivers().lock().expect("channel lock poisoned").insert(rx_id, rx);
            result
        },
        None => rt_variant("Err".into(), rt_str(format!("channel-recv-timeout: invalid receiver handle {}", rx_id))),
    }
}

fn dispatch_channel_close(args: &[*mut RtValue]) -> *mut RtValue {
    let handle_id = match args.first() {
        Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_int() {
            Some(n) => n,
            None => return rt_bool(false),
        },
        _ => return rt_bool(false),
    };
    let removed_tx = channel_senders().lock().expect("channel lock poisoned").remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().expect("channel lock poisoned").remove(&handle_id).is_some();
    rt_bool(removed_tx || removed_rx)
}

// ── Compile/bytecode dispatch helpers (bridge through Value) ────────

fn dispatch_compile_to_executable(args: &[*mut RtValue]) -> *mut RtValue {
    let value_args: Vec<Value> = args.iter().map(|&p| rt_to_value_no_release(p)).collect();
    let paths = match value_args.first() {
        Some(Value::List(items)) => {
            match items.iter().map(|v| match v { Value::Str(s) => Ok(s.clone()), _ => Err(()) }).collect::<Result<Vec<_>, _>>() {
                Ok(ps) => ps,
                Err(_) => return rt_variant("Err".into(), rt_str("compile-to-executable: paths must be strings".into())),
            }
        }
        _ => return rt_variant("Err".into(), rt_str("compile-to-executable: first arg must be list of paths".into())),
    };
    let output = match value_args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        _ => return rt_variant("Err".into(), rt_str("compile-to-executable: second arg must be output path string".into())),
    };
    #[cfg(feature = "aot")]
    {
        match crate::bytecode_aot::compile_to_executable_impl(&paths, &output) {
            Ok(()) => airl_rt::value::rt_unit(),
            Err(e) => rt_variant("Err".into(), rt_str(e)),
        }
    }
    #[cfg(not(feature = "aot"))]
    {
        let _ = (paths, output);
        rt_variant("Err".into(), rt_str("compile-to-executable: AOT feature not enabled".into()))
    }
}

fn dispatch_compile_bytecode_to_executable(args: &[*mut RtValue]) -> *mut RtValue {
    let value_args: Vec<Value> = args.iter().map(|&p| rt_to_value_no_release(p)).collect();
    let func_list = match value_args.first() {
        Some(Value::List(items)) => items.clone(),
        _ => return rt_variant("Err".into(), rt_str("compile-bytecode-to-executable: first arg must be list of BCFunc".into())),
    };
    let output_path = match value_args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        _ => return rt_variant("Err".into(), rt_str("compile-bytecode-to-executable: second arg must be output path string".into())),
    };
    #[cfg(feature = "aot")]
    {
        match crate::bytecode_marshal::compile_bytecode_to_executable(&func_list, &output_path) {
            Ok(()) => rt_variant("Ok".into(), rt_str(format!("Compiled to {}", output_path))),
            Err(e) => rt_variant("Err".into(), rt_str(format!("{}", e))),
        }
    }
    #[cfg(not(feature = "aot"))]
    {
        let _ = (func_list, output_path);
        rt_variant("Err".into(), rt_str("compile-bytecode-to-executable: AOT feature not enabled".into()))
    }
}

fn dispatch_compile_bytecode_to_executable_with_target(args: &[*mut RtValue]) -> *mut RtValue {
    let value_args: Vec<Value> = args.iter().map(|&p| rt_to_value_no_release(p)).collect();
    let func_list = match value_args.first() {
        Some(Value::List(items)) => items.clone(),
        _ => return rt_variant("Err".into(), rt_str("compile-bytecode-to-executable-with-target: first arg must be list of BCFunc".into())),
    };
    let output_path = match value_args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        _ => return rt_variant("Err".into(), rt_str("compile-bytecode-to-executable-with-target: second arg must be output path string".into())),
    };
    let target_str = match value_args.get(2) {
        Some(Value::Str(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    #[cfg(feature = "aot")]
    {
        match crate::bytecode_marshal::compile_bytecode_to_executable_with_target(&func_list, &output_path, target_str.as_deref()) {
            Ok(()) => rt_variant("Ok".into(), rt_str(format!("Compiled to {} (target: {})", output_path, target_str.as_deref().unwrap_or("host")))),
            Err(e) => rt_variant("Err".into(), rt_str(format!("{}", e))),
        }
    }
    #[cfg(not(feature = "aot"))]
    {
        let _ = (func_list, output_path, target_str);
        rt_variant("Err".into(), rt_str("compile-bytecode-to-executable-with-target: AOT feature not enabled".into()))
    }
}

fn dispatch_run_bytecode(args: &[*mut RtValue]) -> *mut RtValue {
    let value_args: Vec<Value> = args.iter().map(|&p| rt_to_value_no_release(p)).collect();
    let func_list = match value_args.first() {
        Some(Value::List(items)) => items.clone(),
        _ => return rt_nil(),
    };
    match crate::bytecode_marshal::run_bytecode_program(&func_list) {
        Ok(val) => value_to_rt(&val),
        Err(e) => {
            eprintln!("run-bytecode error: {}", e);
            rt_nil()
        }
    }
}

// ── Builtin dispatcher ─────────────────────────────────────────────

/// Dispatch a builtin call to the appropriate airl-rt extern "C" function.
/// Returns Some(result) if the builtin is handled, None if not found.
/// The returned pointer has rc=1 (caller owns it).
fn dispatch_rt_builtin(name: &str, args: &[*mut RtValue]) -> Option<*mut RtValue> {
    let argc = args.len();
    macro_rules! a0 { () => { args.get(0).copied().unwrap_or(std::ptr::null_mut()) }; }
    macro_rules! a1 { () => { args.get(1).copied().unwrap_or(std::ptr::null_mut()) }; }
    macro_rules! a2 { () => { args.get(2).copied().unwrap_or(std::ptr::null_mut()) }; }
    macro_rules! a3 { () => { args.get(3).copied().unwrap_or(std::ptr::null_mut()) }; }

    let result = match name {
        // Arithmetic
        "+" => airl_rt::arithmetic::airl_add(a0!(), a1!()),
        "-" => airl_rt::arithmetic::airl_sub(a0!(), a1!()),
        "*" => airl_rt::arithmetic::airl_mul(a0!(), a1!()),
        "/" => airl_rt::arithmetic::airl_div(a0!(), a1!()),
        "%" => airl_rt::arithmetic::airl_mod(a0!(), a1!()),

        // Comparison
        "=" => airl_rt::comparison::airl_eq(a0!(), a1!()),
        "!=" => airl_rt::comparison::airl_ne(a0!(), a1!()),
        "<" => airl_rt::comparison::airl_lt(a0!(), a1!()),
        ">" => airl_rt::comparison::airl_gt(a0!(), a1!()),
        "<=" => airl_rt::comparison::airl_le(a0!(), a1!()),
        ">=" => airl_rt::comparison::airl_ge(a0!(), a1!()),

        // Logic
        "and" => airl_rt::logic::airl_and(a0!(), a1!()),
        "or" => airl_rt::logic::airl_or(a0!(), a1!()),
        "not" => airl_rt::logic::airl_not(a0!()),
        "xor" => airl_rt::logic::airl_xor(a0!(), a1!()),

        // Collections
        "length" => airl_rt::list::airl_length(a0!()),
        "at" => airl_rt::list::airl_at(a0!(), a1!()),
        "append" => airl_rt::list::airl_append(a0!(), a1!()),
        "head" => airl_rt::list::airl_head(a0!()),
        "tail" => airl_rt::list::airl_tail(a0!()),
        "empty?" => airl_rt::list::airl_empty(a0!()),
        "cons" => airl_rt::list::airl_cons(a0!(), a1!()),
        "at-or" => airl_rt::list::airl_at_or(a0!(), a1!(), a2!()),
        "set-at" => airl_rt::list::airl_set_at(a0!(), a1!(), a2!()),
        "list-contains?" => airl_rt::list::airl_list_contains(a0!(), a1!()),
        // reverse, concat, flatten, range, take, drop, zip, enumerate
        // deregistered — AIRL stdlib equivalents in prelude.airl take over

        // String
        "char-at" => airl_rt::string::airl_char_at(a0!(), a1!()),
        "substring" => airl_rt::string::airl_substring(a0!(), a1!(), a2!()),
        "split" => airl_rt::string::airl_split(a0!(), a1!()),
        "join" => airl_rt::string::airl_join(a0!(), a1!()),
        "replace" => airl_rt::string::airl_replace(a0!(), a1!(), a2!()),
        // contains, starts-with, ends-with, index-of, trim, to-upper, to-lower
        // deregistered — AIRL stdlib equivalents in string.airl take over
        "chars" => airl_rt::string::airl_chars(a0!()),
        "char-count" => airl_rt::misc::airl_char_count(a0!()),
        "char-code" => airl_rt::string::airl_char_code(a0!()),
        "char-from-code" => airl_rt::string::airl_char_from_code(a0!()),
        // char-alpha?, char-digit?, char-whitespace?
        // deregistered — AIRL stdlib equivalents in string.airl take over
        "char-upper?" => airl_rt::string::airl_char_upper(a0!()),
        "char-lower?" => airl_rt::string::airl_char_lower(a0!()),
        "string-ci=?" => airl_rt::string::airl_string_ci_eq(a0!(), a1!()),

        // Print (variadic)
        "print" => {
            if argc == 1 {
                airl_rt::io::airl_print(a0!())
            } else {
                airl_rt::io::airl_print_values(args.as_ptr(), argc as i64)
            }
        }
        "println" => airl_rt::io::airl_println(a0!()),
        "eprint" => airl_rt::io::airl_eprint(a0!()),
        "eprintln" => airl_rt::io::airl_eprintln(a0!()),
        // read-line, read-stdin deregistered — AIRL stdlib equivalents in io.airl take over

        // Variadic str / format
        "str" => airl_rt::misc::airl_str_variadic(args.as_ptr(), argc as i64),
        "format" => airl_rt::misc::airl_format_variadic(args.as_ptr(), argc as i64),

        // Map
        "map-new" => airl_rt::map::airl_map_new(),
        "map-get" => airl_rt::map::airl_map_get(a0!(), a1!()),
        "map-set" => airl_rt::map::airl_map_set(a0!(), a1!(), a2!()),
        "map-has" => airl_rt::map::airl_map_has(a0!(), a1!()),
        "map-remove" => airl_rt::map::airl_map_remove(a0!(), a1!()),
        "map-keys" => airl_rt::map::airl_map_keys(a0!()),
        // map-from, map-get-or, map-values, map-size
        // deregistered — AIRL stdlib equivalents in map.airl take over

        // File I/O, Directory I/O, Stream I/O (read-line, read-lines, read-stdin),
        // System (get-args, getenv, exit, sleep, time-now, cpu-count, format-time, get-cwd)
        // deregistered — AIRL stdlib equivalents in io.airl take over

        // Utility
        "type-of" => airl_rt::io::airl_type_of(a0!()),
        "valid" => airl_rt::io::airl_valid(a0!()),

        // System / type conversion
        "int-to-string" => airl_rt::misc::airl_int_to_string(a0!()),
        "float-to-string" => airl_rt::misc::airl_float_to_string(a0!()),
        "string-to-int" => airl_rt::misc::airl_string_to_int(a0!()),
        "string-to-float" => airl_rt::string::airl_string_to_float(a0!()),
        "panic" => airl_rt::misc::airl_panic(a0!()),
        "assert" => airl_rt::misc::airl_assert(a0!(), a1!()),
        // cpu-count, time-now, sleep, format-time, getenv
        // deregistered — AIRL stdlib equivalents in io.airl take over
        // json-parse, json-stringify
        // deregistered — AIRL stdlib equivalents in json.airl take over
        "shell-exec" => airl_rt::misc::airl_shell_exec(a0!(), a1!()),
        "shell-exec-with-stdin" => airl_rt::misc::airl_shell_exec_with_stdin(a0!(), a1!(), a2!()),
        // exit deregistered — AIRL stdlib equivalent in io.airl takes over
        "parse-int-radix" => airl_rt::misc::airl_parse_int_radix(a0!(), a1!()),
        "int-to-string-radix" => airl_rt::misc::airl_int_to_string_radix(a0!(), a1!()),
        // get-cwd deregistered — AIRL stdlib equivalent in io.airl takes over

        // Float math
        "sqrt" => airl_rt::math::airl_sqrt(a0!()),
        "sin" => airl_rt::math::airl_sin(a0!()),
        "cos" => airl_rt::math::airl_cos(a0!()),
        "tan" => airl_rt::math::airl_tan(a0!()),
        "log" => airl_rt::math::airl_log(a0!()),
        "exp" => airl_rt::math::airl_exp(a0!()),
        "floor" => airl_rt::math::airl_floor(a0!()),
        "ceil" => airl_rt::math::airl_ceil(a0!()),
        "round" => airl_rt::math::airl_round(a0!()),
        "float-to-int" => airl_rt::math::airl_float_to_int(a0!()),
        "int-to-float" => airl_rt::math::airl_int_to_float(a0!()),
        "infinity" => airl_rt::math::airl_infinity(),
        "nan" => airl_rt::math::airl_nan(),
        "is-nan?" => airl_rt::math::airl_is_nan(a0!()),
        "is-infinite?" => airl_rt::math::airl_is_infinite(a0!()),

        // path-join, path-parent, path-filename, path-extension, is-absolute?
        // deregistered — AIRL stdlib equivalents in path.airl take over

        // Regex
        "regex-match" => airl_rt::misc::airl_regex_match(a0!(), a1!()),
        "regex-find-all" => airl_rt::misc::airl_regex_find_all(a0!(), a1!()),
        "regex-replace" => airl_rt::misc::airl_regex_replace(a0!(), a1!(), a2!()),
        "regex-split" => airl_rt::misc::airl_regex_split(a0!(), a1!()),

        // Crypto
        // sha256, hmac-sha256 deregistered — AIRL stdlib equivalents take over
        // base64-encode, base64-decode, base64-encode-bytes, base64-decode-bytes
        // deregistered — AIRL stdlib equivalents in base64.airl take over
        "random-bytes" => airl_rt::misc::airl_random_bytes(a0!()),
        "sha512" => airl_rt::misc::airl_sha512(a0!()),
        "hmac-sha512" => airl_rt::misc::airl_hmac_sha512(a0!(), a1!()),
        // sha256-bytes, hmac-sha256-bytes, pbkdf2-sha256
        // deregistered — AIRL stdlib equivalents take over
        "sha512-bytes" => airl_rt::misc::airl_sha512_bytes(a0!()),
        "hmac-sha512-bytes" => airl_rt::misc::airl_hmac_sha512_bytes(a0!(), a1!()),
        "pbkdf2-sha512" => airl_rt::misc::airl_pbkdf2_sha512(a0!(), a1!(), a2!(), a3!()),
        // base64-decode-bytes, base64-encode-bytes removed above
        "bitwise-xor" => airl_rt::misc::airl_bitwise_xor(a0!(), a1!()),
        "bitwise-and" => airl_rt::misc::airl_bitwise_and(a0!(), a1!()),
        "bitwise-or" => airl_rt::misc::airl_bitwise_or(a0!(), a1!()),
        "bitwise-shr" => airl_rt::misc::airl_bitwise_shr(a0!(), a1!()),
        "bitwise-shl" => airl_rt::misc::airl_bitwise_shl(a0!(), a1!()),

        // Byte-array intrinsics
        "bytes-alloc" => airl_rt::misc::airl_bytes_alloc(a0!()),
        "bytes-get" => airl_rt::misc::airl_bytes_get(a0!(), a1!()),
        "bytes-set!" => airl_rt::misc::airl_bytes_set(a0!(), a1!(), a2!()),
        "bytes-length" => airl_rt::list::airl_length(a0!()),

        // Byte encoding
        "bytes-new" => airl_rt::misc::airl_bytes_new_empty(),
        "bytes-from-int8" => airl_rt::misc::airl_bytes_from_int8(a0!()),
        "bytes-from-int16" => airl_rt::misc::airl_bytes_from_int16(a0!()),
        "bytes-from-int32" => airl_rt::misc::airl_bytes_from_int32(a0!()),
        "bytes-from-int64" => airl_rt::misc::airl_bytes_from_int64(a0!()),
        "bytes-to-int16" => airl_rt::misc::airl_bytes_to_int16(a0!(), a1!()),
        "bytes-to-int32" => airl_rt::misc::airl_bytes_to_int32(a0!(), a1!()),
        "bytes-to-int64" => airl_rt::misc::airl_bytes_to_int64(a0!(), a1!()),
        "bytes-from-string" => airl_rt::misc::airl_bytes_from_string(a0!()),
        "bytes-to-string" => airl_rt::misc::airl_bytes_to_string(a0!(), a1!(), a2!()),
        "bytes-concat" => airl_rt::misc::airl_bytes_concat(a0!(), a1!()),
        "bytes-concat-all" => airl_rt::misc::airl_bytes_concat_all(a0!()),
        "bytes-slice" => airl_rt::misc::airl_bytes_slice(a0!(), a1!(), a2!()),
        "crc32c" => airl_rt::misc::airl_crc32c(a0!()),

        // TCP
        "tcp-connect" => airl_rt::misc::airl_tcp_connect(a0!(), a1!()),
        "tcp-close" => airl_rt::misc::airl_tcp_close(a0!()),
        "tcp-send" => airl_rt::misc::airl_tcp_send(a0!(), a1!()),
        "tcp-recv" => airl_rt::misc::airl_tcp_recv(a0!(), a1!()),
        "tcp-recv-exact" => airl_rt::misc::airl_tcp_recv_exact(a0!(), a1!()),
        "tcp-set-timeout" => airl_rt::misc::airl_tcp_set_timeout(a0!(), a1!()),
        "tcp-connect-tls" => airl_rt::misc::airl_tcp_connect_tls(a0!(), a1!(), a2!(), a3!(),
            args.get(4).copied().unwrap_or(std::ptr::null_mut())),
        "tcp-listen" => airl_rt::misc::airl_tcp_listen(a0!(), a1!()),
        "tcp-accept" => airl_rt::misc::airl_tcp_accept(a0!()),
        "tcp-accept-tls" => airl_rt::misc::airl_tcp_accept_tls(a0!(), a1!(), a2!()),
        "thread-set-affinity" => airl_rt::thread::airl_thread_set_affinity(a0!()),

        // Compression
        "gzip-compress" => airl_rt::misc::airl_gzip_compress(a0!()),
        "gzip-decompress" => airl_rt::misc::airl_gzip_decompress(a0!()),
        "snappy-compress" => airl_rt::misc::airl_snappy_compress(a0!()),
        "snappy-decompress" => airl_rt::misc::airl_snappy_decompress(a0!()),
        "lz4-compress" => airl_rt::misc::airl_lz4_compress(a0!()),
        "lz4-decompress" => airl_rt::misc::airl_lz4_decompress(a0!()),
        "zstd-compress" => airl_rt::misc::airl_zstd_compress(a0!()),
        "zstd-decompress" => airl_rt::misc::airl_zstd_decompress(a0!()),

        // Thread/channel
        "thread-join" => dispatch_thread_join(args),
        "channel-new" => dispatch_channel_new(args),
        "channel-send" => dispatch_channel_send(args),
        "channel-recv" => dispatch_channel_recv(args),
        "channel-recv-timeout" => dispatch_channel_recv_timeout(args),
        "channel-drain" => airl_rt::thread::airl_channel_drain(a0!()),
        "channel-close" => dispatch_channel_close(args),

        // Compiler/bytecode
        "compile-to-executable" => dispatch_compile_to_executable(args),
        "compile-bytecode-to-executable" => dispatch_compile_bytecode_to_executable(args),
        "compile-bytecode-to-executable-with-target" => dispatch_compile_bytecode_to_executable_with_target(args),
        "run-bytecode" => dispatch_run_bytecode(args),

        // extern-c name aliases — used by io.airl AIRL wrappers to call C functions
        // File I/O
        "airl_read_file" => airl_rt::io::airl_read_file(a0!()),
        "airl_write_file" => airl_rt::io::airl_write_file(a0!(), a1!()),
        "airl_append_file" => airl_rt::io::airl_append_file(a0!(), a1!()),
        "airl_delete_file" => airl_rt::io::airl_delete_file(a0!()),
        "airl_file_exists" => airl_rt::io::airl_file_exists(a0!()),
        "airl_file_size" => airl_rt::io::airl_file_size(a0!()),
        "airl_file_mtime" => airl_rt::io::airl_file_mtime(a0!()),
        "airl_rename_file" => airl_rt::io::airl_rename_file(a0!(), a1!()),
        "airl_temp_file" => airl_rt::io::airl_temp_file(a0!()),
        // Directory I/O
        "airl_read_dir" => airl_rt::io::airl_read_dir(a0!()),
        "airl_create_dir" => airl_rt::io::airl_create_dir(a0!()),
        "airl_delete_dir" => airl_rt::io::airl_delete_dir(a0!()),
        "airl_is_dir" => airl_rt::io::airl_is_dir(a0!()),
        "airl_get_cwd" => airl_rt::misc::airl_get_cwd(),
        "airl_temp_dir" => airl_rt::io::airl_temp_dir(a0!()),
        // Stream I/O
        "airl_read_line" => airl_rt::io::airl_read_line(),
        "airl_read_lines" => airl_rt::misc::airl_read_lines(a0!()),
        "airl_read_stdin" => airl_rt::io::airl_read_stdin(),
        // System
        "airl_get_args" => airl_rt::io::airl_get_args(),
        "airl_getenv" => airl_rt::misc::airl_getenv(a0!()),
        "airl_exit" => airl_rt::misc::airl_exit(a0!()),
        "airl_sleep" => airl_rt::misc::airl_sleep(a0!()),
        "airl_time_now" => airl_rt::misc::airl_time_now(),
        "airl_cpu_count" => airl_rt::misc::airl_cpu_count(),
        "airl_format_time" => airl_rt::misc::airl_format_time(a0!(), a1!()),

        // Not found
        _ => return None,
    };
    Some(result)
}

// ── BytecodeVm ─────────────────────────────────────────────────────

// SAFETY: BytecodeVm contains *mut RtValue in CallFrame registers.
// These pointers are owned (rc-managed) and never shared across threads.
// Each child VM (from spawn_child) gets a fresh empty call stack.
unsafe impl Send for BytecodeVm {}

/// Default maximum call depth for the VM (SEC-11).
const DEFAULT_MAX_CALL_DEPTH: usize = 10_000;

pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    fn_metadata: HashMap<String, crate::bytecode::FnDefMetadata>,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
    max_call_depth: usize,
}

impl BytecodeVm {
    pub fn new() -> Self {
        BytecodeVm {
            functions: HashMap::new(),
            fn_metadata: HashMap::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
        }
    }

    /// Set the maximum call depth for this VM instance.
    pub fn set_max_call_depth(&mut self, depth: usize) {
        self.max_call_depth = depth;
    }

    /// Create a child VM for thread-spawn.
    pub fn spawn_child(&self) -> BytecodeVm {
        BytecodeVm {
            functions: self.functions.clone(),
            fn_metadata: self.fn_metadata.clone(),
            call_stack: Vec::new(),
            recursion_depth: 0,
            max_call_depth: self.max_call_depth,
        }
    }

    pub fn load_function(&mut self, func: BytecodeFunc) {
        self.functions.insert(func.name.clone(), func);
    }

    pub fn store_fn_metadata(&mut self, meta: crate::bytecode::FnDefMetadata) {
        self.fn_metadata.insert(meta.name.clone(), meta);
    }

    pub fn get_fn_metadata(&self, name: &str) -> Option<&crate::bytecode::FnDefMetadata> {
        self.fn_metadata.get(name)
    }

    /// Dispatch fn-metadata using RtValue args, returning *mut RtValue.
    fn dispatch_fn_metadata_rt(&self, args: &[*mut RtValue]) -> Result<*mut RtValue, RuntimeError> {
        let fname = match args.first() {
            Some(&ptr) if !ptr.is_null() => match unsafe { rt_ref(ptr) }.try_as_str() {
                Some(s) => s.to_string(),
                None => return Err(RuntimeError::Custom("fn-metadata: requires string arg".into())),
            },
            _ => return Err(RuntimeError::Custom("fn-metadata: requires 1 argument".into())),
        };
        match self.fn_metadata.get(&fname) {
            Some(meta) => {
                let mut m: HashMap<String, *mut RtValue> = HashMap::new();
                m.insert("name".into(), rt_str(meta.name.clone()));
                m.insert("intent".into(), meta.intent.as_ref().map_or_else(rt_nil, |s| rt_str(s.clone())));
                m.insert("param-names".into(), rt_list(meta.param_names.iter().map(|s| rt_str(s.clone())).collect()));
                m.insert("param-types".into(), rt_list(meta.param_types.iter().map(|s| rt_str(s.clone())).collect()));
                m.insert("return-type".into(), rt_str(meta.return_type.clone()));
                m.insert("requires".into(), rt_list(meta.requires.iter().map(|s| rt_str(s.clone())).collect()));
                m.insert("ensures".into(), rt_list(meta.ensures.iter().map(|s| rt_str(s.clone())).collect()));
                Ok(rt_variant("Ok".into(), rt_map(m)))
            }
            None => Ok(rt_variant("Err".into(), rt_str(format!("function not found: {}", fname)))),
        }
    }

    /// Dispatch fn-metadata using Value args (legacy bridge for tests).
    #[allow(dead_code)]
    fn dispatch_fn_metadata(&self, args: &[Value]) -> Result<Value, RuntimeError> {
        let fname = match args.first() {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(RuntimeError::Custom("fn-metadata: requires string arg".into())),
        };
        match self.fn_metadata.get(&fname) {
            Some(meta) => {
                let mut m = HashMap::new();
                m.insert("name".into(), Value::Str(meta.name.clone()));
                m.insert("intent".into(), meta.intent.as_ref().map_or(Value::Nil, |s| Value::Str(s.clone())));
                m.insert("param-names".into(), Value::List(meta.param_names.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("param-types".into(), Value::List(meta.param_types.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("return-type".into(), Value::Str(meta.return_type.clone()));
                m.insert("requires".into(), Value::List(meta.requires.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("ensures".into(), Value::List(meta.ensures.iter().map(|s| Value::Str(s.clone())).collect()));
                Ok(Value::Variant("Ok".into(), Box::new(Value::Map(m))))
            }
            None => Ok(Value::Variant("Err".into(), Box::new(Value::Str(format!("function not found: {}", fname))))),
        }
    }

    /// Validate all loaded bytecode functions for out-of-bounds register and
    /// constant references (SEC-12, SEC-13). Must be called before execution.
    fn validate_bytecode(&self) -> Result<(), RuntimeError> {
        for (name, func) in &self.functions {
            let rc = func.register_count as usize;
            let cc = func.constants.len();

            for (pc, instr) in func.instructions.iter().enumerate() {
                let err = |msg: String| -> RuntimeError {
                    RuntimeError::BytecodeValidation(format!(
                        "function '{}' instruction {}: {}", name, pc, msg
                    ))
                };

                // Helper closures for bounds checking
                let check_reg = |idx: u16, field: &str| -> Result<(), RuntimeError> {
                    if (idx as usize) >= rc {
                        Err(err(format!(
                            "{} register index {} >= register_count {}", field, idx, rc
                        )))
                    } else {
                        Ok(())
                    }
                };
                let check_const = |idx: u16, field: &str| -> Result<(), RuntimeError> {
                    if (idx as usize) >= cc {
                        Err(err(format!(
                            "{} constant index {} >= constants.len() {}", field, idx, cc
                        )))
                    } else {
                        Ok(())
                    }
                };
                // Check a register range [start..start+count)
                let check_reg_range = |start: u16, count: u16, field: &str| -> Result<(), RuntimeError> {
                    let end = start as usize + count as usize;
                    if end > rc {
                        Err(err(format!(
                            "{} register range {}..{} exceeds register_count {}", field, start, end, rc
                        )))
                    } else {
                        Ok(())
                    }
                };

                match instr.op {
                    // dst=reg, a=const_idx
                    Op::LoadConst => {
                        check_reg(instr.dst, "dst")?;
                        check_const(instr.a, "a")?;
                    }
                    // dst=reg only
                    Op::LoadNil | Op::LoadTrue | Op::LoadFalse => {
                        check_reg(instr.dst, "dst")?;
                    }
                    // dst=reg, a=reg
                    Op::Move | Op::Not | Op::Neg => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                    }
                    // dst=reg, a=reg, b=reg
                    Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod
                    | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                        check_reg(instr.b, "b")?;
                    }
                    // a=offset (signed i16), no register validation needed
                    Op::Jump => {}
                    // a=reg, b=offset
                    Op::JumpIfFalse | Op::JumpIfTrue => {
                        check_reg(instr.a, "a")?;
                    }
                    // dst=reg(return+args base), a=const_idx(func name), b=argc
                    // args occupy registers [dst+1..dst+1+argc]
                    Op::Call | Op::CallBuiltin => {
                        check_reg(instr.dst, "dst")?;
                        check_const(instr.a, "a")?;
                        if instr.b > 0 {
                            check_reg_range(instr.dst + 1, instr.b, "args")?;
                        }
                    }
                    // dst=reg(return+args base), a=reg(callee), b=argc
                    Op::CallReg => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                        if instr.b > 0 {
                            check_reg_range(instr.dst + 1, instr.b, "args")?;
                        }
                    }
                    // TailCall reuses current frame, no register indices to validate
                    Op::TailCall => {}
                    // a=reg (return value)
                    Op::Return => {
                        check_reg(instr.a, "a")?;
                    }
                    // dst=reg, a=start_reg, b=count; regs [a..a+b]
                    Op::MakeList => {
                        check_reg(instr.dst, "dst")?;
                        if instr.b > 0 {
                            check_reg_range(instr.a, instr.b, "list elements")?;
                        }
                    }
                    // dst=reg, a=const_idx(tag), b=reg(inner)
                    Op::MakeVariant => {
                        check_reg(instr.dst, "dst")?;
                        check_const(instr.a, "a")?;
                        check_reg(instr.b, "b")?;
                    }
                    // dst=reg, a=const_idx(tag)
                    Op::MakeVariant0 => {
                        check_reg(instr.dst, "dst")?;
                        check_const(instr.a, "a")?;
                    }
                    // dst=reg, a=const_idx(func name), b=capture_start
                    // capture count comes from the target function, validated dynamically
                    Op::MakeClosure => {
                        check_reg(instr.dst, "dst")?;
                        check_const(instr.a, "a")?;
                        // b is capture_start — at minimum it must be a valid register
                        // (capture_count may be 0, in which case b is unused)
                        // We validate b < rc only if there are captures to read
                        // Since we don't know capture_count statically here, just
                        // validate b itself is in range (it's used as start index).
                        if (instr.b as usize) > rc {
                            return Err(err(format!(
                                "b capture_start {} > register_count {}", instr.b, rc
                            )));
                        }
                    }
                    // dst=reg, a=reg(scrutinee), b=const_idx(tag)
                    Op::MatchTag => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                        check_const(instr.b, "b")?;
                    }
                    // a=offset, no register validation needed
                    Op::JumpIfNoMatch => {}
                    // dst=reg, a=reg(scrutinee)
                    Op::MatchWild => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                    }
                    // dst=reg, a=reg(source)
                    Op::TryUnwrap => {
                        check_reg(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                    }
                    // dst=const_idx(fn name), a=reg(bool), b=const_idx(clause)
                    Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
                        check_const(instr.dst, "dst")?;
                        check_reg(instr.a, "a")?;
                        check_const(instr.b, "b")?;
                    }
                    // a=reg
                    Op::MarkMoved => {
                        check_reg(instr.a, "a")?;
                    }
                    // a=reg, b=const_idx (optional — used for error messages)
                    Op::CheckNotMoved => {
                        check_reg(instr.a, "a")?;
                        // b is used as a const index only if in range (see dispatch code)
                        // so we validate it if non-zero
                        // Actually the dispatch code does: if (instr.b as usize) < f.constants.len()
                        // so out-of-bounds b is handled gracefully — skip validation for b
                    }
                }
            }
        }
        Ok(())
    }

    pub fn exec_main(&mut self) -> Result<Value, RuntimeError> {
        self.validate_bytecode()?;
        self.push_frame("__main__", &[], 0)
            .and_then(|_| self.run())
    }

    /// Push a frame with Value args (converts to RtValue).
    fn push_frame(&mut self, name: &str, args: &[Value], return_reg: u16) -> Result<(), RuntimeError> {
        let func = self.functions.get(name)
            .ok_or_else(|| RuntimeError::UndefinedSymbol(name.to_string()))?;

        self.recursion_depth += 1;
        if self.call_stack.len() >= self.max_call_depth || self.recursion_depth > self.max_call_depth {
            self.recursion_depth -= 1;
            return Err(RuntimeError::StackOverflow { depth: self.max_call_depth });
        }

        let reg_count = func.register_count as usize;
        let mut registers: Vec<*mut RtValue> = Vec::with_capacity(reg_count);
        for _ in 0..reg_count { registers.push(rt_nil()); }
        for (i, arg) in args.iter().enumerate() {
            if i < registers.len() {
                airl_value_release(registers[i]);
                registers[i] = value_to_rt(arg);
            }
        }

        self.call_stack.push(CallFrame {
            registers,
            func_name: name.to_string(),
            ip: 0,
            return_reg,
            match_flag: false,
            moved: vec![false; reg_count],
        });
        Ok(())
    }

    /// Push a frame with RtValue args (retains each arg).
    fn push_frame_rt(&mut self, name: &str, args: &[*mut RtValue], return_reg: u16) -> Result<(), RuntimeError> {
        let func = self.functions.get(name)
            .ok_or_else(|| RuntimeError::UndefinedSymbol(name.to_string()))?;

        self.recursion_depth += 1;
        if self.call_stack.len() >= self.max_call_depth || self.recursion_depth > self.max_call_depth {
            self.recursion_depth -= 1;
            return Err(RuntimeError::StackOverflow { depth: self.max_call_depth });
        }

        let reg_count = func.register_count as usize;
        let mut registers: Vec<*mut RtValue> = Vec::with_capacity(reg_count);
        for _ in 0..reg_count { registers.push(rt_nil()); }
        for (i, &arg) in args.iter().enumerate() {
            if i < registers.len() {
                airl_value_release(registers[i]);
                airl_value_retain(arg);
                registers[i] = arg;
            }
        }

        self.call_stack.push(CallFrame {
            registers,
            func_name: name.to_string(),
            ip: 0,
            return_reg,
            match_flag: false,
            moved: vec![false; reg_count],
        });
        Ok(())
    }

    fn run(&mut self) -> Result<Value, RuntimeError> {
        let result_rt = self.run_rt_with_min_depth(0)?;
        let result = rt_to_value_no_release(result_rt);
        airl_value_release(result_rt);
        Ok(result)
    }

    /// Main VM loop. Returns *mut RtValue with rc >= 1 (caller must release).
    fn run_rt_with_min_depth(&mut self, min_depth: usize) -> Result<*mut RtValue, RuntimeError> {
        loop {
            let (func_name, ip, func_len) = {
                let frame = self.call_stack.last().expect("internal: call stack empty");
                (frame.func_name.clone(), frame.ip, {
                    self.functions.get(&frame.func_name).expect("internal: function not in map").instructions.len()
                })
            };

            if ip >= func_len {
                let return_reg = self.call_stack.last().expect("internal: call stack empty").return_reg;
                let mut frame = self.call_stack.pop().expect("internal: call stack empty");
                release_registers(&mut frame.registers);
                self.recursion_depth = self.recursion_depth.saturating_sub(1);
                if self.call_stack.len() <= min_depth {
                    return Ok(rt_nil());
                }
                let caller = self.call_stack.last_mut().expect("internal: call stack empty");
                reg_set(&mut caller.registers, return_reg as usize, rt_nil());
                continue;
            }

            let instr = self.functions.get(&func_name).expect("internal: function not in map").instructions[ip];
            self.call_stack.last_mut().expect("internal: call stack empty").ip += 1;

            match instr.op {
                Op::LoadConst => {
                    let val = value_to_rt(&self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize]);
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, val);
                }
                Op::LoadNil => {
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, rt_nil());
                }
                Op::LoadTrue => {
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, rt_bool(true));
                }
                Op::LoadFalse => {
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, rt_bool(false));
                }
                Op::Move => {
                    let src = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    airl_value_retain(src);
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, src);
                }

                // ── Arithmetic (inline for proper error returns) ──
                Op::Add => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_add(*y)),
                        (RtData::Float(x), RtData::Float(y)) => rt_float(x + y),
                        (RtData::Str(x), RtData::Str(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            rt_str(s)
                        }
                        _ => return Err(RuntimeError::TypeError("add: incompatible types".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Sub => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_sub(*y)),
                        (RtData::Float(x), RtData::Float(y)) => rt_float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Mul => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_mul(*y)),
                        (RtData::Float(x), RtData::Float(y)) => rt_float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Div => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(_), RtData::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (RtData::Int(x), RtData::Int(y)) => rt_int(x / y),
                        (RtData::Float(x), RtData::Float(y)) => rt_float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Mod => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_int(x % y),
                        _ => return Err(RuntimeError::TypeError("mod: incompatible types".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Neg => {
                    let a = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    let va = unsafe { rt_ref(a) };
                    let result = match va.data() {
                        RtData::Int(x) => rt_int(-x),
                        RtData::Float(x) => rt_float(-x),
                        _ => return Err(RuntimeError::TypeError("neg: expected number".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }

                // ── Comparison ──
                Op::Eq => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let result = rt_bool(airl_rt::comparison::rt_values_equal(a, b));
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Ne => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let result = rt_bool(!airl_rt::comparison::rt_values_equal(a, b));
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Lt => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_bool(x < y),
                        (RtData::Float(x), RtData::Float(y)) => rt_bool(x < y),
                        (RtData::Str(x), RtData::Str(y)) => rt_bool(x < y),
                        _ => rt_bool(false),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Le => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_bool(x <= y),
                        (RtData::Float(x), RtData::Float(y)) => rt_bool(x <= y),
                        (RtData::Str(x), RtData::Str(y)) => rt_bool(x <= y),
                        _ => rt_bool(false),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Gt => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_bool(x > y),
                        (RtData::Float(x), RtData::Float(y)) => rt_bool(x > y),
                        (RtData::Str(x), RtData::Str(y)) => rt_bool(x > y),
                        _ => rt_bool(false),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Ge => {
                    let (a, b) = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (reg_get(r, instr.a as usize), reg_get(r, instr.b as usize))
                    };
                    let (va, vb) = unsafe { (rt_ref(a), rt_ref(b)) };
                    let result = match (va.data(), vb.data()) {
                        (RtData::Int(x), RtData::Int(y)) => rt_bool(x >= y),
                        (RtData::Float(x), RtData::Float(y)) => rt_bool(x >= y),
                        (RtData::Str(x), RtData::Str(y)) => rt_bool(x >= y),
                        _ => rt_bool(false),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }
                Op::Not => {
                    let a = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    let va = unsafe { rt_ref(a) };
                    let result = match va.try_as_bool() {
                        Some(b) => rt_bool(!b),
                        None => return Err(RuntimeError::TypeError("not: expected bool".into())),
                    };
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                }

                // ── Control flow ──
                Op::Jump => {
                    let offset = instr.a as i16;
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    frame.ip = (frame.ip as i32 + offset as i32) as usize;
                }
                Op::JumpIfFalse => {
                    let val = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    if rt_is_bool_false(val) {
                        let offset = instr.b as i16;
                        self.call_stack.last_mut().expect("internal: call stack empty").ip =
                            (self.call_stack.last().expect("internal: call stack empty").ip as i32 + offset as i32) as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let val = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    if rt_is_bool_true(val) {
                        let offset = instr.b as i16;
                        self.call_stack.last_mut().expect("internal: call stack empty").ip =
                            (self.call_stack.last().expect("internal: call stack empty").ip as i32 + offset as i32) as usize;
                    }
                }

                // ── Data ──
                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let items: Vec<*mut RtValue> = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (start..start+count).map(|i| { let p = reg_get(r, i); airl_value_retain(p); p }).collect()
                    };
                    let list = rt_list(items);
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, list);
                }
                Op::MakeVariant => {
                    let tag = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                    };
                    let inner = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.b as usize);
                    airl_value_retain(inner);
                    let variant = rt_variant(tag, inner);
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, variant);
                }
                Op::MakeVariant0 => {
                    let tag = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                    };
                    let variant = rt_variant(tag, rt_nil());
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, variant);
                }

                // ── Pattern matching ──
                Op::MatchTag => {
                    let tag = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.b as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("match tag must be string".into())),
                    };
                    let scr = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    let v_scr = unsafe { rt_ref(scr) };
                    match v_scr.try_as_variant() {
                        Some((tag_name, inner)) if tag_name == tag => {
                            airl_value_retain(inner);
                            let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                            reg_set(&mut frame.registers, instr.dst as usize, inner);
                            frame.match_flag = true;
                        }
                        _ => {
                            self.call_stack.last_mut().expect("internal: call stack empty").match_flag = false;
                        }
                    }
                }
                Op::JumpIfNoMatch => {
                    if !self.call_stack.last().expect("internal: call stack empty").match_flag {
                        let offset = instr.a as i16;
                        let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::MatchWild => {
                    let val = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    airl_value_retain(val);
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    reg_set(&mut frame.registers, instr.dst as usize, val);
                    frame.match_flag = true;
                }

                // ── TryUnwrap ──
                Op::TryUnwrap => {
                    let val = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    let v = unsafe { rt_ref(val) };
                    match v.try_as_variant() {
                        Some(("Ok", inner)) => {
                            airl_value_retain(inner);
                            reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, inner);
                        }
                        Some(("Err", inner)) => {
                            let inner_v = unsafe { rt_ref(inner) };
                            return Err(RuntimeError::Custom(format!("{}", inner_v)));
                        }
                        _ => return Err(RuntimeError::TryOnNonResult(rt_display(val))),
                    }
                }

                // ── Contract assertions ──
                Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
                    let bool_val = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    if !rt_is_bool_true(bool_val) {
                        let f = self.functions.get(&func_name).expect("internal: function not in map");
                        let fn_name_str = match &f.constants[instr.dst as usize] {
                            Value::Str(s) => s.clone(),
                            _ => func_name.clone(),
                        };
                        let clause_source = match &f.constants[instr.b as usize] {
                            Value::Str(s) => s.clone(),
                            _ => "?".into(),
                        };
                        let contract_kind = match instr.op {
                            Op::AssertRequires => airl_contracts::violation::ContractKind::Requires,
                            Op::AssertEnsures => airl_contracts::violation::ContractKind::Ensures,
                            _ => airl_contracts::violation::ContractKind::Invariant,
                        };
                        let frame = self.call_stack.last().expect("internal: call stack empty");
                        let arity = f.arity as usize;
                        let bindings: Vec<(String, String)> = (0..arity)
                            .filter(|&i| i < frame.registers.len())
                            .map(|i| (format!("arg{}", i), rt_display(frame.registers[i])))
                            .collect();
                        return Err(RuntimeError::ContractViolation(
                            airl_contracts::violation::ContractViolation {
                                function: fn_name_str, contract_kind, clause_source, bindings,
                                evaluated: rt_display(bool_val),
                                span: airl_syntax::Span::dummy(),
                            }
                        ));
                    }
                }

                // ── Ownership tracking ──
                Op::MarkMoved => {
                    let reg = instr.a as usize;
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    if reg < frame.moved.len() { frame.moved[reg] = true; }
                }
                Op::CheckNotMoved => {
                    let reg = instr.a as usize;
                    let frame = self.call_stack.last().expect("internal: call stack empty");
                    if reg < frame.moved.len() && frame.moved[reg] {
                        let f = self.functions.get(&func_name).expect("internal: function not in map");
                        let msg = if (instr.b as usize) < f.constants.len() {
                            match &f.constants[instr.b as usize] {
                                Value::Str(s) if s.contains(' ') => s.clone(),
                                Value::Str(s) => format!("use of moved value: `{}` was already moved", s),
                                other => format!("use of moved value: `{}` was already moved", other),
                            }
                        } else {
                            format!("use of moved value: register {} was already moved", reg)
                        };
                        return Err(RuntimeError::Custom(msg));
                    }
                }

                // ── Function calls ──
                Op::Call => {
                    let name = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("call: func name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let rt_args: Vec<*mut RtValue> = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (0..argc).map(|i| reg_get(r, instr.dst as usize + 1 + i)).collect()
                    };

                    // Dispatch chain: special cases first, then airl-rt, then push frame
                    if name == "thread-spawn" {
                        let result = self.dispatch_thread_spawn_rt(&rt_args)?;
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else if name == "fn-metadata" {
                        let result = self.dispatch_fn_metadata_rt(&rt_args)?;
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else if let Some(result) = dispatch_rt_builtin(&name, &rt_args) {
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else {
                        self.push_frame_rt(&name, &rt_args, instr.dst)?;
                    }
                }

                Op::CallBuiltin => {
                    let name = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let rt_args: Vec<*mut RtValue> = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (0..argc).map(|i| reg_get(r, instr.dst as usize + 1 + i)).collect()
                    };

                    if name == "thread-spawn" {
                        let result = self.dispatch_thread_spawn_rt(&rt_args)?;
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else if name == "fn-metadata" {
                        let result = self.dispatch_fn_metadata_rt(&rt_args)?;
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else if let Some(result) = dispatch_rt_builtin(&name, &rt_args) {
                        reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                    } else {
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }

                Op::CallReg => {
                    let callee = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    let argc = instr.b as usize;
                    let rt_args: Vec<*mut RtValue> = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (0..argc).map(|i| reg_get(r, instr.dst as usize + 1 + i)).collect()
                    };
                    let callee_val = rt_to_value_no_release(callee);
                    match callee_val {
                        Value::BytecodeClosure(ref closure) => {
                            let mut full_args = closure.captured.clone();
                            let args_val: Vec<Value> = rt_args.iter().map(|&p| rt_to_value_no_release(p)).collect();
                            full_args.extend(args_val);
                            let name = closure.func_name.clone();

                            if let Some(func) = self.functions.get(&name) {
                                if is_simple_closure(func) {
                                    let func = func.clone();
                                    let result_val = self.eval_simple(&func, full_args)?;
                                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, value_to_rt(&result_val));
                                    continue;
                                }
                            }
                            self.push_frame(&name, &full_args, instr.dst)?;
                        }
                        Value::IRFuncRef(ref name) | Value::BuiltinFn(ref name) => {
                            let name = name.clone();
                            if name == "thread-spawn" {
                                let result = self.dispatch_thread_spawn_rt(&rt_args)?;
                                reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                            } else if name == "fn-metadata" {
                                let result = self.dispatch_fn_metadata_rt(&rt_args)?;
                                reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                            } else if let Some(result) = dispatch_rt_builtin(&name, &rt_args) {
                                reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, result);
                            } else {
                                let value_args: Vec<Value> = rt_args.iter().map(|&p| rt_to_value_no_release(p)).collect();
                                self.push_frame(&name, &value_args, instr.dst)?;
                            }
                        }
                        _ => return Err(RuntimeError::NotCallable(rt_display(callee))),
                    }
                }

                Op::TailCall => {
                    let frame = self.call_stack.last_mut().expect("internal: call stack empty");
                    frame.ip = 0;
                    for m in frame.moved.iter_mut() { *m = false; }
                }

                Op::Return => {
                    let result = reg_get(&self.call_stack.last().expect("internal: call stack empty").registers, instr.a as usize);
                    airl_value_retain(result);
                    let return_reg = self.call_stack.last().expect("internal: call stack empty").return_reg;
                    let mut frame = self.call_stack.pop().expect("internal: call stack empty");
                    release_registers(&mut frame.registers);
                    self.recursion_depth = self.recursion_depth.saturating_sub(1);
                    if self.call_stack.len() <= min_depth {
                        return Ok(result);
                    }
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, return_reg as usize, result);
                }

                Op::MakeClosure => {
                    let func_name_const = match &self.functions.get(&func_name).expect("internal: function not in map").constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("closure: func name must be string".into())),
                    };
                    let capture_count = self.functions.get(&func_name_const)
                        .map(|f| f.capture_count as usize).unwrap_or(0);
                    let capture_start = instr.b as usize;
                    let captured: Vec<Value> = {
                        let r = &self.call_stack.last().expect("internal: call stack empty").registers;
                        (capture_start..capture_start + capture_count)
                            .map(|i| rt_to_value_no_release(reg_get(r, i)))
                            .collect()
                    };
                    let closure_val = Value::BytecodeClosure(BytecodeClosureValue {
                        func_name: func_name_const,
                        captured,
                    });
                    let closure_rt = value_to_rt(&closure_val);
                    reg_set(&mut self.call_stack.last_mut().expect("internal: call stack empty").registers, instr.dst as usize, closure_rt);
                }
            }
        }
    }

    /// Dispatch thread-spawn with RtValue arguments.
    fn dispatch_thread_spawn_rt(&mut self, args: &[*mut RtValue]) -> Result<*mut RtValue, RuntimeError> {
        let closure_val = args.first()
            .map(|&p| rt_to_value_no_release(p))
            .ok_or_else(|| RuntimeError::Custom("thread-spawn: requires 1 argument".into()))?;
        let mut child_vm = self.spawn_child();
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || -> Result<Value, String> {
                match closure_val {
                    Value::BytecodeClosure(bc) => {
                        child_vm.call_by_name(&bc.func_name, bc.captured)
                            .map_err(|e| format!("{}", e))
                    }
                    _ => Err("thread-spawn: argument must be a closure".into()),
                }
            })
            .map_err(|e| RuntimeError::Custom(format!("thread-spawn: {}", e)))?;
        let id = NEXT_THREAD_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        thread_handles().lock().expect("thread handles lock poisoned").insert(id, handle);
        Ok(rt_int(id))
    }

    /// Execute a simple closure inline (still uses Value for speed).
    fn eval_simple(&mut self, func: &BytecodeFunc, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let mut regs: Vec<Value> = vec![Value::Nil; func.register_count as usize];
        for (i, arg) in args.into_iter().enumerate() {
            if i < regs.len() { regs[i] = arg; }
        }

        let mut pc = 0;
        while pc < func.instructions.len() {
            let instr = func.instructions[pc];
            match instr.op {
                Op::LoadConst => { regs[instr.dst as usize] = func.constants[instr.a as usize].clone(); }
                Op::LoadNil => { regs[instr.dst as usize] = Value::Nil; }
                Op::LoadTrue => { regs[instr.dst as usize] = Value::Bool(true); }
                Op::LoadFalse => { regs[instr.dst as usize] = Value::Bool(false); }
                Op::Move => { regs[instr.dst as usize] = regs[instr.a as usize].clone(); }
                Op::Add => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                        (Value::Str(x), Value::Str(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            Value::Str(s)
                        }
                        _ => return Err(RuntimeError::TypeError("add: incompatible types".into())),
                    };
                }
                Op::Sub => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into())),
                    };
                }
                Op::Mul => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into())),
                    };
                }
                Op::Div => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(_), Value::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into())),
                    };
                }
                Op::Mod => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x % y),
                        _ => return Err(RuntimeError::TypeError("mod: incompatible types".into())),
                    };
                }
                Op::Neg => {
                    regs[instr.dst as usize] = match &regs[instr.a as usize] {
                        Value::Int(x) => Value::Int(-x),
                        Value::Float(x) => Value::Float(-x),
                        _ => return Err(RuntimeError::TypeError("neg: expected number".into())),
                    };
                }
                Op::Eq => { regs[instr.dst as usize] = Value::Bool(&regs[instr.a as usize] == &regs[instr.b as usize]); }
                Op::Ne => { regs[instr.dst as usize] = Value::Bool(&regs[instr.a as usize] != &regs[instr.b as usize]); }
                Op::Lt => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Le => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Gt => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Ge => {
                    regs[instr.dst as usize] = match (&regs[instr.a as usize], &regs[instr.b as usize]) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x >= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x >= y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Not => {
                    regs[instr.dst as usize] = match &regs[instr.a as usize] {
                        Value::Bool(b) => Value::Bool(!b),
                        _ => return Err(RuntimeError::TypeError("not: expected bool".into())),
                    };
                }
                Op::CallBuiltin => {
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = (0..argc).map(|i| regs[instr.dst as usize + 1 + i].clone()).collect();
                    let rt_args: Vec<*mut RtValue> = args.iter().map(|v| value_to_rt(v)).collect();
                    if let Some(result_rt) = dispatch_rt_builtin(&name, &rt_args) {
                        regs[instr.dst as usize] = rt_to_value_no_release(result_rt);
                        airl_value_release(result_rt);
                        for p in &rt_args { airl_value_release(*p); }
                    } else {
                        for p in &rt_args { airl_value_release(*p); }
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }
                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    regs[instr.dst as usize] = Value::List((start..start + count).map(|i| regs[i].clone()).collect());
                }
                Op::Return => { return Ok(regs[instr.a as usize].clone()); }
                Op::MarkMoved | Op::CheckNotMoved => {}
                _ => return Err(RuntimeError::Custom(format!("eval_simple: unsupported op {:?}", instr.op))),
            }
            pc += 1;
        }
        Ok(Value::Nil)
    }

    pub fn call_by_name(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        if !self.functions.contains_key(name) {
            return Err(RuntimeError::UndefinedSymbol(name.to_string()));
        }
        self.push_frame(name, &args, 0)?;
        self.run()
    }

    pub fn exec_program(&mut self, functions: Vec<BytecodeFunc>, main_func: BytecodeFunc) -> Result<Value, RuntimeError> {
        for func in functions { self.load_function(func); }
        self.load_function(main_func);
        self.exec_main()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    fn compile_and_run(nodes: &[IRNode]) -> Value {
        let mut compiler = BytecodeCompiler::new();
        let (funcs, main_func) = compiler.compile_program(nodes);
        let mut vm = BytecodeVm::new();
        vm.exec_program(funcs, main_func).expect("test: exec_program failed")
    }

    #[test]
    fn test_int_literal() { assert_eq!(compile_and_run(&[IRNode::Int(42)]), Value::Int(42)); }

    #[test]
    fn test_bool_literal() { assert_eq!(compile_and_run(&[IRNode::Bool(true)]), Value::Bool(true)); }

    #[test]
    fn test_arithmetic() {
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        assert_eq!(compile_and_run(&[node]), Value::Int(7));
    }

    #[test]
    fn test_if_true() {
        let node = IRNode::If(Box::new(IRNode::Bool(true)), Box::new(IRNode::Int(1)), Box::new(IRNode::Int(2)));
        assert_eq!(compile_and_run(&[node]), Value::Int(1));
    }

    #[test]
    fn test_if_false() {
        let node = IRNode::If(Box::new(IRNode::Bool(false)), Box::new(IRNode::Int(1)), Box::new(IRNode::Int(2)));
        assert_eq!(compile_and_run(&[node]), Value::Int(2));
    }

    #[test]
    fn test_let() {
        let node = IRNode::Let(vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }], Box::new(IRNode::Load("x".into())));
        assert_eq!(compile_and_run(&[node]), Value::Int(42));
    }

    #[test]
    fn test_function_call() {
        let nodes = vec![
            IRNode::Func("double".into(), vec!["x".into()],
                Box::new(IRNode::Call("*".into(), vec![IRNode::Load("x".into()), IRNode::Int(2)]))),
            IRNode::Call("double".into(), vec![IRNode::Int(21)]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(42));
    }

    #[test]
    fn test_recursion() {
        let fact_body = IRNode::If(
            Box::new(IRNode::Call("<=".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)])),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Call("*".into(), vec![
                IRNode::Load("n".into()),
                IRNode::Call("fact".into(), vec![
                    IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
                ]),
            ])),
        );
        let nodes = vec![
            IRNode::Func("fact".into(), vec!["n".into()], Box::new(fact_body)),
            IRNode::Call("fact".into(), vec![IRNode::Int(5)]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(120));
    }

    #[test]
    fn test_match_variant() {
        let node = IRNode::Match(
            Box::new(IRNode::Variant("Ok".into(), vec![IRNode::Int(42)])),
            vec![
                IRArm { pattern: IRPattern::Variant("Ok".into(), vec![IRPattern::Bind("v".into())]), body: IRNode::Load("v".into()) },
                IRArm { pattern: IRPattern::Wild, body: IRNode::Int(0) },
            ],
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(42));
    }

    #[test]
    fn test_list() {
        let node = IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]);
        match compile_and_run(&[node]) {
            Value::List(items) => assert_eq!(items.len(), 3),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_tco_no_overflow() {
        let body = IRNode::If(
            Box::new(IRNode::Call("=".into(), vec![IRNode::Load("n".into()), IRNode::Int(0)])),
            Box::new(IRNode::Int(0)),
            Box::new(IRNode::Call("count-down".into(), vec![
                IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
            ])),
        );
        let nodes = vec![
            IRNode::Func("count-down".into(), vec!["n".into()], Box::new(body)),
            IRNode::Call("count-down".into(), vec![IRNode::Int(100_000)]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(0));
    }

    // NOTE: map/filter/fold tests removed in v0.6.0 — these are now stdlib AIRL functions
    // (not builtins), so they require the full stdlib to be loaded. They're tested by
    // the E2E fixtures and bootstrap tests instead.

    #[test]
    fn test_is_simple_closure_check() {
        let simple_func = BytecodeFunc {
            name: "test_simple".into(), arity: 1, register_count: 3, capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadConst, 1, 0, 0),
                Instruction::new(Op::Add, 2, 0, 1),
                Instruction::new(Op::Return, 0, 2, 0),
            ],
            constants: vec![Value::Int(1)],
        };
        assert!(is_simple_closure(&simple_func));

        let complex_func = BytecodeFunc {
            name: "test_complex".into(), arity: 1, register_count: 3, capture_count: 0,
            instructions: vec![
                Instruction::new(Op::Call, 1, 0, 1),
                Instruction::new(Op::Return, 0, 1, 0),
            ],
            constants: vec![Value::Str("foo".into())],
        };
        assert!(!is_simple_closure(&complex_func));
    }

    // ── SEC-11: Stack overflow detection ──

    #[test]
    fn test_stack_overflow_detected() {
        // Non-tail recursion: f(n) = f(n) + 1
        // The `+ 1` after the recursive call prevents tail-call optimization.
        let body = IRNode::Call("+".into(), vec![
            IRNode::Call("inf".into(), vec![IRNode::Load("n".into())]),
            IRNode::Int(1),
        ]);
        let nodes = vec![
            IRNode::Func("inf".into(), vec!["n".into()], Box::new(body)),
            IRNode::Call("inf".into(), vec![IRNode::Int(0)]),
        ];
        let mut compiler = BytecodeCompiler::new();
        let (funcs, main_func) = compiler.compile_program(&nodes);
        let mut vm = BytecodeVm::new();
        vm.set_max_call_depth(100);
        let result = vm.exec_program(funcs, main_func);
        match result {
            Err(RuntimeError::StackOverflow { depth: 100 }) => {} // expected
            other => panic!("expected StackOverflow at depth 100, got: {:?}", other),
        }
    }

    #[test]
    fn test_max_call_depth_default() {
        let vm = BytecodeVm::new();
        assert_eq!(vm.max_call_depth, DEFAULT_MAX_CALL_DEPTH);
    }

    #[test]
    fn test_set_max_call_depth() {
        let mut vm = BytecodeVm::new();
        vm.set_max_call_depth(500);
        assert_eq!(vm.max_call_depth, 500);
    }

    #[test]
    fn test_spawn_child_inherits_max_call_depth() {
        let mut vm = BytecodeVm::new();
        vm.set_max_call_depth(42);
        let child = vm.spawn_child();
        assert_eq!(child.max_call_depth, 42);
    }

    // ── SEC-12: Register index validation ──

    #[test]
    fn test_validate_register_oob_dst() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 2, capture_count: 0,
            instructions: vec![
                // dst=5 but register_count=2
                Instruction::new(Op::LoadNil, 5, 0, 0),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("register index 5"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_register_oob_a() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 2, capture_count: 0,
            instructions: vec![
                // Move dst=0, a=10 — a is out of bounds
                Instruction::new(Op::Move, 0, 10, 0),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("register index 10"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_register_oob_b() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 3, capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadNil, 0, 0, 0),
                Instruction::new(Op::LoadNil, 1, 0, 0),
                // Add dst=0, a=0, b=99 — b is out of bounds
                Instruction::new(Op::Add, 0, 0, 99),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("register index 99"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_valid_bytecode_passes() {
        // A valid program that should pass validation
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 3, capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadConst, 0, 0, 0),
                Instruction::new(Op::Return, 0, 0, 0),
            ],
            constants: vec![Value::Int(42)],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        assert_eq!(vm.exec_main().unwrap(), Value::Int(42));
    }

    // ── SEC-13: Constant index validation ──

    #[test]
    fn test_validate_constant_oob_loadconst() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 2, capture_count: 0,
            instructions: vec![
                // LoadConst dst=0, a=5 but constants is empty
                Instruction::new(Op::LoadConst, 0, 5, 0),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("constant index 5"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_constant_oob_call() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 2, capture_count: 0,
            instructions: vec![
                // Call dst=0, a=3(const idx for func name), b=0(argc) — but no constants
                Instruction::new(Op::Call, 0, 3, 0),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("constant index 3"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_constant_oob_make_variant() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 3, capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadNil, 1, 0, 0),
                // MakeVariant dst=0, a=10(const tag idx), b=1(inner reg) — const oob
                Instruction::new(Op::MakeVariant, 0, 10, 1),
            ],
            constants: vec![],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("constant index 10"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_call_args_register_range() {
        let func = BytecodeFunc {
            name: "__main__".into(), arity: 0, register_count: 3, capture_count: 0,
            instructions: vec![
                // Call dst=0, a=0(const func name), b=5(argc) — args would be regs 1..6 but only 3 regs
                Instruction::new(Op::Call, 0, 0, 5),
            ],
            constants: vec![Value::Str("foo".into())],
        };
        let mut vm = BytecodeVm::new();
        vm.load_function(func);
        match vm.exec_main() {
            Err(RuntimeError::BytecodeValidation(msg)) => {
                assert!(msg.contains("register range"), "msg was: {}", msg);
            }
            other => panic!("expected BytecodeValidation, got: {:?}", other),
        }
    }
}
