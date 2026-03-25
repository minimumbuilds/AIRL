// crates/airl-runtime/src/bytecode_vm.rs
use std::collections::HashMap;
use crate::bytecode::*;
use crate::value::Value;
use crate::builtins::{Builtins, VmCaller};
use crate::error::RuntimeError;

/// Maximum instruction count for a "simple" closure eligible for inline eval.
const SIMPLE_CLOSURE_MAX_INSTRS: usize = 15;
/// Maximum parameter count for inline eval (keeps the register bank small).
const SIMPLE_CLOSURE_MAX_PARAMS: usize = 8;

/// Check whether a compiled function is "simple" enough to evaluate inline
/// without pushing a full VM call frame.
///
/// A function is simple if:
/// - It has ≤ SIMPLE_CLOSURE_MAX_INSTRS instructions
/// - It has ≤ SIMPLE_CLOSURE_MAX_PARAMS parameters
/// - It contains no ops that require a nested frame (Call, CallReg, MakeClosure, TryUnwrap)
///   or complex control flow (Jump*, MatchTag, JumpIfNoMatch, MatchWild, TailCall)
fn is_simple_closure(func: &BytecodeFunc) -> bool {
    if func.instructions.len() > SIMPLE_CLOSURE_MAX_INSTRS {
        return false;
    }
    if func.arity as usize > SIMPLE_CLOSURE_MAX_PARAMS {
        return false;
    }
    for instr in &func.instructions {
        match instr.op {
            // Allowed ops
            Op::LoadConst | Op::LoadNil | Op::LoadTrue | Op::LoadFalse | Op::Move
            | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Neg
            | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Not
            | Op::Return
            | Op::CallBuiltin
            | Op::MakeList
            | Op::MarkMoved | Op::CheckNotMoved => {}
            // Everything else disqualifies
            _ => return false,
        }
    }
    true
}

struct CallFrame {
    registers: Vec<Value>,
    func_name: String,
    ip: usize,
    /// Register in the CALLER's frame where the return value should be stored.
    /// Ignored for the bottom-most frame (returns via Ok(result)).
    return_reg: u16,
    match_flag: bool,
    /// Tracks which registers have been moved (ownership consumed).
    moved: Vec<bool>,
}

pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    fn_metadata: HashMap<String, crate::bytecode::FnDefMetadata>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
    #[cfg(feature = "jit")]
    jit_full: Option<std::sync::Arc<crate::bytecode_jit_full::BytecodeJitFull>>,
}

impl BytecodeVm {
    pub fn new() -> Self {
        BytecodeVm {
            functions: HashMap::new(),
            fn_metadata: HashMap::new(),
            builtins: Builtins::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
            #[cfg(feature = "jit")]
            jit_full: None,
        }
    }

    #[cfg(feature = "jit")]
    pub fn new_with_full_jit() -> Self {
        BytecodeVm {
            functions: HashMap::new(),
            fn_metadata: HashMap::new(),
            builtins: Builtins::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
            jit_full: crate::bytecode_jit_full::BytecodeJitFull::new().ok().map(std::sync::Arc::new),
        }
    }

    #[cfg(feature = "jit")]
    pub fn jit_full_compile_all(&mut self) {
        if let Some(ref mut jit) = self.jit_full {
            let jit = std::sync::Arc::get_mut(jit)
                .expect("jit_full_compile_all: Arc must have single owner during compilation");
            let names: Vec<String> = self.functions.keys().cloned().collect();
            for name in &names {
                if let Some(func) = self.functions.get(name) {
                    jit.try_compile_full(func, &self.functions);
                }
            }
        }
    }

    /// Create a child VM for thread-spawn: clones function registry,
    /// fresh builtins and call stack, shares JIT via Arc.
    pub fn spawn_child(&self) -> BytecodeVm {
        BytecodeVm {
            functions: self.functions.clone(),
            fn_metadata: self.fn_metadata.clone(),
            builtins: Builtins::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
            #[cfg(feature = "jit")]
            jit_full: self.jit_full.clone(),
        }
    }

    pub fn load_function(&mut self, func: BytecodeFunc) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Store function metadata (param types, intent, contracts) for runtime introspection.
    pub fn store_fn_metadata(&mut self, meta: crate::bytecode::FnDefMetadata) {
        self.fn_metadata.insert(meta.name.clone(), meta);
    }

    /// Look up metadata for a function by name.
    pub fn get_fn_metadata(&self, name: &str) -> Option<&crate::bytecode::FnDefMetadata> {
        self.fn_metadata.get(name)
    }

    /// Dispatch the `fn-metadata` builtin: look up function metadata and return
    /// a Result-wrapped Map with name, intent, param info, and contracts.
    fn dispatch_fn_metadata(&self, args: &[Value]) -> Result<Value, RuntimeError> {
        let fname = match args.first() {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(RuntimeError::Custom(
                "fn-metadata: requires 1 string argument (function name)".into())),
        };
        match self.fn_metadata.get(&fname) {
            Some(meta) => {
                let mut m = HashMap::new();
                m.insert("name".to_string(), Value::Str(meta.name.clone()));
                m.insert("intent".to_string(), match &meta.intent {
                    Some(s) => Value::Str(s.clone()),
                    None => Value::Nil,
                });
                m.insert("param-names".to_string(), Value::List(
                    meta.param_names.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("param-types".to_string(), Value::List(
                    meta.param_types.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("return-type".to_string(), Value::Str(meta.return_type.clone()));
                m.insert("requires".to_string(), Value::List(
                    meta.requires.iter().map(|s| Value::Str(s.clone())).collect()));
                m.insert("ensures".to_string(), Value::List(
                    meta.ensures.iter().map(|s| Value::Str(s.clone())).collect()));
                Ok(Value::Variant("Ok".to_string(), Box::new(Value::Map(m))))
            }
            None => {
                Ok(Value::Variant("Err".to_string(),
                    Box::new(Value::Str(format!("function not found: {}", fname)))))
            }
        }
    }

    /// Execute a function by name with no arguments. Used to run __main__.
    pub fn exec_main(&mut self) -> Result<Value, RuntimeError> {
        self.push_frame("__main__", &[], 0)
            .and_then(|_| self.run())
    }

    /// Push a new call frame for the named function with the given args.
    /// `return_reg` is the register in the CALLER's frame where the return value goes.
    fn push_frame(&mut self, name: &str, args: &[Value], return_reg: u16) -> Result<(), RuntimeError> {
        let func = self.functions.get(name)
            .ok_or_else(|| RuntimeError::UndefinedSymbol(name.to_string()))?;

        self.recursion_depth += 1;
        if self.recursion_depth > 50_000 {
            self.recursion_depth -= 1;
            return Err(RuntimeError::Custom("stack overflow".into()));
        }

        let reg_count = func.register_count as usize;
        let mut registers = vec![Value::Nil; reg_count];
        for (i, arg) in args.iter().enumerate() {
            if i < registers.len() {
                registers[i] = arg.clone();
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
        self.run_with_min_depth(0)
    }

    /// Run the VM loop until the call stack depth drops to `min_depth`.
    /// When `min_depth` is 0, this behaves identically to the original `run()`.
    /// When called from `invoke_in_nested_frame`, `min_depth` is the stack
    /// depth of the caller, so we stop as soon as the nested frame returns.
    fn run_with_min_depth(&mut self, min_depth: usize) -> Result<Value, RuntimeError> {
        loop {
            let (func_name, ip, func_len) = {
                let frame = self.call_stack.last().unwrap();
                (frame.func_name.clone(), frame.ip, {
                    let f = self.functions.get(&frame.func_name).unwrap();
                    f.instructions.len()
                })
            };

            if ip >= func_len {
                // Implicit return nil
                let return_reg = self.call_stack.last().unwrap().return_reg;
                self.call_stack.pop();
                self.recursion_depth = self.recursion_depth.saturating_sub(1);
                if self.call_stack.len() <= min_depth {
                    return Ok(Value::Nil);
                }
                let caller = self.call_stack.last_mut().unwrap();
                caller.registers[return_reg as usize] = Value::Nil;
                continue;
            }

            let instr = {
                let f = self.functions.get(&func_name).unwrap();
                f.instructions[ip]
            };
            self.call_stack.last_mut().unwrap().ip += 1;

            match instr.op {
                Op::LoadConst => {
                    let val = {
                        let f = self.functions.get(&func_name).unwrap();
                        f.constants[instr.a as usize].clone()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                }
                Op::LoadNil => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Nil;
                }
                Op::LoadTrue => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Bool(true);
                }
                Op::LoadFalse => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Bool(false);
                }
                Op::Move => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                }

                // Arithmetic
                Op::Add => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                        (Value::Str(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
                        _ => return Err(RuntimeError::TypeError("add: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Sub => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Mul => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Div => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(_), Value::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Mod => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x % y),
                        _ => return Err(RuntimeError::TypeError("mod: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Neg => {
                    let a = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let result = match &a {
                        Value::Int(x) => Value::Int(-x),
                        Value::Float(x) => Value::Float(-x),
                        _ => return Err(RuntimeError::TypeError("neg: expected number".into())),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }

                // Comparison
                Op::Eq => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = Value::Bool(a == b);
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Ne => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = Value::Bool(a != b);
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Lt => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Le => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Gt => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Ge => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x >= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x >= y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Not => {
                    let a = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let result = match &a {
                        Value::Bool(b) => Value::Bool(!b),
                        _ => return Err(RuntimeError::TypeError("not: expected bool".into())),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }

                // Control flow
                Op::Jump => {
                    let offset = instr.a as i16;
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i32 + offset as i32) as usize;
                }
                Op::JumpIfFalse => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    if let Value::Bool(false) = val {
                        let offset = instr.b as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    if let Value::Bool(true) = val {
                        let offset = instr.b as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }

                // Data
                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let items: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (start..start+count).map(|i| frame.registers[i].clone()).collect()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::List(items);
                }
                Op::MakeVariant => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                        }
                    };
                    let inner = self.call_stack.last().unwrap().registers[instr.b as usize].clone();
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::Variant(tag, Box::new(inner));
                }
                Op::MakeVariant0 => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                        }
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::Variant(tag, Box::new(Value::Nil));
                }

                // Pattern matching
                Op::MatchTag => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.b as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("match tag must be string".into())),
                        }
                    };
                    let scr = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let frame = self.call_stack.last_mut().unwrap();
                    match scr {
                        Value::Variant(ref vtag, ref inner) if *vtag == tag => {
                            frame.registers[instr.dst as usize] = *inner.clone();
                            frame.match_flag = true;
                        }
                        _ => {
                            frame.match_flag = false;
                        }
                    }
                }
                Op::JumpIfNoMatch => {
                    let matched = self.call_stack.last().unwrap().match_flag;
                    if !matched {
                        let offset = instr.a as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::MatchWild => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.registers[instr.dst as usize] = val;
                    frame.match_flag = true;
                }

                // Try
                Op::TryUnwrap => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    match val {
                        Value::Variant(ref tag, ref inner) if tag == "Ok" => {
                            let inner = *inner.clone();
                            self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = inner;
                        }
                        Value::Variant(ref tag, ref inner) if tag == "Err" => {
                            return Err(RuntimeError::Custom(format!("{}", inner)));
                        }
                        _ => return Err(RuntimeError::TryOnNonResult(format!("{}", val))),
                    }
                }

                // Contract assertions — check a boolean register, error if not true
                Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
                    let frame = self.call_stack.last().unwrap();
                    let bool_val = frame.registers[instr.a as usize].clone();
                    let is_true = matches!(&bool_val, Value::Bool(true));
                    if !is_true {
                        let f = self.functions.get(&func_name).unwrap();
                        let fn_name_str = match &f.constants[instr.dst as usize] {
                            Value::Str(s) => s.clone(),
                            _ => func_name.clone(),
                        };
                        let clause_source = match &f.constants[instr.b as usize] {
                            Value::Str(s) => s.clone(),
                            _ => "?".to_string(),
                        };
                        let contract_kind = match instr.op {
                            Op::AssertRequires => airl_contracts::violation::ContractKind::Requires,
                            Op::AssertEnsures => airl_contracts::violation::ContractKind::Ensures,
                            _ => airl_contracts::violation::ContractKind::Invariant,
                        };
                        // Capture parameter bindings from the current frame
                        let frame = self.call_stack.last().unwrap();
                        let arity = f.arity as usize;
                        let mut bindings = Vec::new();
                        // We don't have param names in BytecodeFunc, so use positional names
                        for i in 0..arity {
                            if i < frame.registers.len() {
                                bindings.push((format!("arg{}", i), format!("{}", frame.registers[i])));
                            }
                        }
                        return Err(RuntimeError::ContractViolation(
                            airl_contracts::violation::ContractViolation {
                                function: fn_name_str,
                                contract_kind,
                                clause_source,
                                bindings,
                                evaluated: format!("{}", bool_val),
                                span: airl_syntax::Span::dummy(),
                            }
                        ));
                    }
                }

                // Ownership tracking
                Op::MarkMoved => {
                    let reg = instr.a as usize;
                    let frame = self.call_stack.last_mut().unwrap();
                    if reg < frame.moved.len() {
                        frame.moved[reg] = true;
                    }
                }
                Op::CheckNotMoved => {
                    let reg = instr.a as usize;
                    let frame = self.call_stack.last().unwrap();
                    if reg < frame.moved.len() && frame.moved[reg] {
                        let f = self.functions.get(&func_name).unwrap();
                        let msg = if (instr.b as usize) < f.constants.len() {
                            let name_val = &f.constants[instr.b as usize];
                            if let Value::Str(s) = name_val {
                                if s.contains(' ') {
                                    // Full error message (e.g., borrow+move conflict)
                                    s.clone()
                                } else {
                                    format!("use of moved value: `{}` was already moved", s)
                                }
                            } else {
                                format!("use of moved value: `{}` was already moved", name_val)
                            }
                        } else {
                            format!("use of moved value: register {} was already moved", reg)
                        };
                        return Err(RuntimeError::Custom(msg));
                    }
                }

                // Function calls — push new frame and let the main loop execute it
                Op::Call => {
                    let name = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("call: func name must be string".into())),
                        }
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };

                    // Try JIT-full first
                    #[cfg(feature = "jit")]
                    if let Some(ref jit_full) = self.jit_full {
                        if let Some(val) = jit_full.try_call_native(&name, &args) {
                            // Check if a contract violation was signaled via the thread-local error cell
                            if let Some((kind, fn_name_idx, clause_idx)) = crate::jit_contract::take_jit_contract_error() {
                                let f = self.functions.get(&name);
                                let fn_name_str = f.and_then(|f| f.constants.get(fn_name_idx as usize))
                                    .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                                    .unwrap_or_else(|| name.clone());
                                let clause_source = f.and_then(|f| f.constants.get(clause_idx as usize))
                                    .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                                    .unwrap_or_else(|| "?".into());
                                let contract_kind = match kind {
                                    0 => airl_contracts::violation::ContractKind::Requires,
                                    1 => airl_contracts::violation::ContractKind::Ensures,
                                    _ => airl_contracts::violation::ContractKind::Invariant,
                                };
                                return Err(RuntimeError::ContractViolation(
                                    airl_contracts::violation::ContractViolation {
                                        function: fn_name_str,
                                        contract_kind,
                                        clause_source,
                                        bindings: vec![],
                                        evaluated: "false".into(),
                                        span: airl_syntax::Span::dummy(),
                                    }
                                ));
                            }
                            self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                            continue;
                        }
                    }

                    // Special dispatch: thread-spawn needs to clone the VM
                    if name == "thread-spawn" {
                        use crate::builtins::{NEXT_THREAD_HANDLE, thread_handles};
                        let closure = args.into_iter().next()
                            .ok_or_else(|| RuntimeError::Custom("thread-spawn: requires 1 argument".into()))?;
                        let mut child_vm = self.spawn_child();
                        let handle = std::thread::Builder::new()
                            .stack_size(64 * 1024 * 1024)
                            .spawn(move || -> Result<Value, String> {
                                match closure {
                                    Value::BytecodeClosure(bc) => {
                                        child_vm.call_by_name(&bc.func_name, bc.captured)
                                            .map_err(|e| format!("{}", e))
                                    }
                                    _ => Err("thread-spawn: argument must be a closure".into()),
                                }
                            })
                            .map_err(|e| RuntimeError::Custom(format!("thread-spawn: {}", e)))?;
                        let id = NEXT_THREAD_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        thread_handles().lock().unwrap().insert(id, handle);
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Int(id);
                    }
                    // Special dispatch: fn-metadata needs access to VM's metadata map
                    else if name == "fn-metadata" {
                        let result = self.dispatch_fn_metadata(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    }
                    // Try VM-aware builtins first (map, filter, fold, sort)
                    else if let Some(f) = self.builtins.get_with_vm(&name) {
                        let f = *f; // Copy fn pointer to release borrow on self
                        let result = f(self, &args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else if let Some(f) = self.builtins.get(&name) {
                        // Try regular builtin
                        let result = f(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else {
                        // Push a new frame; return_reg is instr.dst in the current frame
                        self.push_frame(&name, &args, instr.dst)?;
                        // The main loop continues executing the new frame
                    }
                }
                Op::CallBuiltin => {
                    let name = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                        }
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };
                    // Special dispatch: thread-spawn needs to clone the VM
                    if name == "thread-spawn" {
                        use crate::builtins::{NEXT_THREAD_HANDLE, thread_handles};
                        let closure = args.into_iter().next()
                            .ok_or_else(|| RuntimeError::Custom("thread-spawn: requires 1 argument".into()))?;
                        let mut child_vm = self.spawn_child();
                        let handle = std::thread::Builder::new()
                            .stack_size(64 * 1024 * 1024)
                            .spawn(move || -> Result<Value, String> {
                                match closure {
                                    Value::BytecodeClosure(bc) => {
                                        child_vm.call_by_name(&bc.func_name, bc.captured)
                                            .map_err(|e| format!("{}", e))
                                    }
                                    _ => Err("thread-spawn: argument must be a closure".into()),
                                }
                            })
                            .map_err(|e| RuntimeError::Custom(format!("thread-spawn: {}", e)))?;
                        let id = NEXT_THREAD_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        thread_handles().lock().unwrap().insert(id, handle);
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Int(id);
                    }
                    // Special dispatch: fn-metadata needs access to VM's metadata map
                    else if name == "fn-metadata" {
                        let result = self.dispatch_fn_metadata(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    }
                    // Try VM-aware builtins first (map, filter, fold, sort)
                    else if let Some(f) = self.builtins.get_with_vm(&name) {
                        let f = *f; // Copy fn pointer to release borrow on self
                        let result = f(self, &args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else if let Some(f) = self.builtins.get(&name) {
                        let result = f(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else {
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }
                Op::CallReg => {
                    let callee = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };
                    match callee {
                        Value::BytecodeClosure(ref closure) => {
                            let mut full_args = closure.captured.clone();
                            full_args.extend(args);
                            let name = closure.func_name.clone();

                            // Fast path: simple closures execute inline
                            if let Some(func) = self.functions.get(&name) {
                                if is_simple_closure(func) {
                                    let func = func.clone();
                                    let result = self.eval_simple(&func, full_args)?;
                                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                                    continue;
                                }
                            }

                            self.push_frame(&name, &full_args, instr.dst)?;
                        }
                        Value::IRFuncRef(ref name) => {
                            let name = name.clone();
                            if name == "thread-spawn" {
                                use crate::builtins::{NEXT_THREAD_HANDLE, thread_handles};
                                let closure = args.into_iter().next()
                                    .ok_or_else(|| RuntimeError::Custom("thread-spawn: requires 1 argument".into()))?;
                                let mut child_vm = self.spawn_child();
                                let handle = std::thread::Builder::new()
                                    .stack_size(64 * 1024 * 1024)
                                    .spawn(move || -> Result<Value, String> {
                                        match closure {
                                            Value::BytecodeClosure(bc) => {
                                                child_vm.call_by_name(&bc.func_name, bc.captured)
                                                    .map_err(|e| format!("{}", e))
                                            }
                                            _ => Err("thread-spawn: argument must be a closure".into()),
                                        }
                                    })
                                    .map_err(|e| RuntimeError::Custom(format!("thread-spawn: {}", e)))?;
                                let id = NEXT_THREAD_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                thread_handles().lock().unwrap().insert(id, handle);
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Int(id);
                            } else if name == "fn-metadata" {
                                let result = self.dispatch_fn_metadata(&args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else if let Some(f) = self.builtins.get_with_vm(&name) {
                                let f = *f;
                                let result = f(self, &args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else if let Some(f) = self.builtins.get(&name) {
                                let result = f(&args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else {
                                self.push_frame(&name, &args, instr.dst)?;
                            }
                        }
                        Value::BuiltinFn(ref name) => {
                            let name = name.clone();
                            if let Some(f) = self.builtins.get_with_vm(&name) {
                                let f = *f;
                                let result = f(self, &args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else if let Some(f) = self.builtins.get(&name) {
                                let result = f(&args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else {
                                return Err(RuntimeError::UndefinedSymbol(name));
                            }
                        }
                        _ => return Err(RuntimeError::NotCallable(format!("{}", callee))),
                    }
                }
                Op::TailCall => {
                    // Reset ip to 0 for self-recursion (args already rebound by compiler)
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = 0;
                    // Reset moved flags for the new iteration
                    for m in frame.moved.iter_mut() { *m = false; }
                }

                Op::Return => {
                    let result = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let return_reg = self.call_stack.last().unwrap().return_reg;
                    self.call_stack.pop();
                    self.recursion_depth = self.recursion_depth.saturating_sub(1);
                    if self.call_stack.len() <= min_depth {
                        return Ok(result);
                    }
                    let caller = self.call_stack.last_mut().unwrap();
                    caller.registers[return_reg as usize] = result;
                }

                Op::MakeClosure => {
                    let func_name_const = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("closure: func name must be string".into())),
                        }
                    };
                    let capture_count = self.functions.get(&func_name_const)
                        .map(|f| f.capture_count as usize)
                        .unwrap_or(0);
                    let capture_start = instr.b as usize;
                    let captured: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (capture_start..capture_start + capture_count)
                            .map(|i| frame.registers[i].clone())
                            .collect()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::BytecodeClosure(BytecodeClosureValue {
                            func_name: func_name_const,
                            captured,
                        });
                }
            }
        }
    }

    /// Execute a simple closure inline without pushing a full call frame.
    /// The function must have been verified by `is_simple_closure` first.
    /// `args` should already include captured values prepended (same layout as push_frame).
    fn eval_simple(
        &mut self,
        func: &BytecodeFunc,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        let mut regs: Vec<Value> = vec![Value::Nil; func.register_count as usize];

        // Load args into registers (same layout as push_frame: args occupy regs 0..N)
        for (i, arg) in args.into_iter().enumerate() {
            if i < regs.len() {
                regs[i] = arg;
            }
        }

        let mut pc = 0;
        while pc < func.instructions.len() {
            let instr = func.instructions[pc];
            match instr.op {
                Op::LoadConst => {
                    regs[instr.dst as usize] = func.constants[instr.a as usize].clone();
                }
                Op::LoadNil => {
                    regs[instr.dst as usize] = Value::Nil;
                }
                Op::LoadTrue => {
                    regs[instr.dst as usize] = Value::Bool(true);
                }
                Op::LoadFalse => {
                    regs[instr.dst as usize] = Value::Bool(false);
                }
                Op::Move => {
                    regs[instr.dst as usize] = regs[instr.a as usize].clone();
                }

                // Arithmetic — exact semantics from the main VM loop
                Op::Add => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                        (Value::Str(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
                        _ => return Err(RuntimeError::TypeError("add: incompatible types".into())),
                    };
                }
                Op::Sub => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into())),
                    };
                }
                Op::Mul => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into())),
                    };
                }
                Op::Div => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(_), Value::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into())),
                    };
                }
                Op::Mod => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
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

                // Comparison
                Op::Eq => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = Value::Bool(a == b);
                }
                Op::Ne => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = Value::Bool(a != b);
                }
                Op::Lt => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Le => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Gt => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Ge => {
                    let a = &regs[instr.a as usize];
                    let b = &regs[instr.b as usize];
                    regs[instr.dst as usize] = match (a, b) {
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

                // Builtins — same layout as main loop: name from constants[a], argc in b,
                // args in registers [dst+1 .. dst+1+argc]
                Op::CallBuiltin => {
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = (0..argc)
                        .map(|i| regs[instr.dst as usize + 1 + i].clone())
                        .collect();
                    // VM-aware builtins need &mut self, so fall back to full frame for those.
                    if self.builtins.get_with_vm(&name).is_some() {
                        // Cannot call VM-aware builtins from eval_simple; fall through would
                        // require &mut self. This shouldn't happen for truly simple closures
                        // (map/filter/fold closures don't themselves call map/filter/fold).
                        // Return a sentinel error — the caller should not have classified this
                        // as simple.
                        return Err(RuntimeError::Custom(
                            format!("eval_simple: VM-aware builtin '{}' not supported inline", name),
                        ));
                    }
                    if let Some(f) = self.builtins.get(&name) {
                        regs[instr.dst as usize] = f(&args)?;
                    } else {
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }

                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let items: Vec<Value> = (start..start + count)
                        .map(|i| regs[i].clone())
                        .collect();
                    regs[instr.dst as usize] = Value::List(items);
                }

                Op::Return => {
                    return Ok(regs[instr.a as usize].clone());
                }

                // Ownership tracking — no-op in simple eval (these are compile-time
                // safety checks, not semantically required for correctness)
                Op::MarkMoved | Op::CheckNotMoved => {}

                _ => {
                    // Should never happen if is_simple_closure is correct
                    return Err(RuntimeError::Custom(format!(
                        "eval_simple: unsupported op {:?}", instr.op
                    )));
                }
            }
            pc += 1;
        }
        // Fell off the end without Return — implicit nil
        Ok(Value::Nil)
    }

    /// Invoke a function in a nested frame, returning its result without
    /// disturbing the outer call stack. Safe to call from within VM-aware builtins.
    fn invoke_in_nested_frame(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        if !self.functions.contains_key(name) {
            return Err(RuntimeError::UndefinedSymbol(name.to_string()));
        }
        let target_depth = self.call_stack.len();
        self.push_frame(name, &args, 0)?;
        self.run_with_min_depth(target_depth)
    }

    /// Call a named function with the given argument values.
    /// The function must already be loaded in the VM.
    /// This pushes a fresh call frame and runs the VM until the function returns.
    pub fn call_by_name(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        // Verify function exists before pushing frame
        if !self.functions.contains_key(name) {
            return Err(RuntimeError::UndefinedSymbol(name.to_string()));
        }
        self.push_frame(name, &args, 0)?;
        self.run()
    }

    /// Load functions and execute __main__
    pub fn exec_program(&mut self, functions: Vec<BytecodeFunc>, main_func: BytecodeFunc) -> Result<Value, RuntimeError> {
        for func in functions {
            self.load_function(func);
        }
        self.load_function(main_func);
        self.exec_main()
    }
}

impl VmCaller for BytecodeVm {
    fn call_value(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match callee {
            Value::BytecodeClosure(closure) => {
                let mut full_args = closure.captured.clone();
                full_args.extend(args);

                // Fast path: simple closures execute inline without a frame push
                let func_name = closure.func_name.clone();
                if let Some(func) = self.functions.get(&func_name) {
                    if is_simple_closure(func) {
                        let func = func.clone();
                        return self.eval_simple(&func, full_args);
                    }
                }

                self.invoke_in_nested_frame(&func_name, full_args)
            }
            Value::IRFuncRef(ref name) | Value::BuiltinFn(ref name) => {
                let name = name.clone();
                // Check VM-aware builtins first
                if let Some(f) = self.builtins.get_with_vm(&name) {
                    let f = *f;
                    return f(self, &args);
                }
                // Check regular builtins
                if let Some(f) = self.builtins.get(&name) {
                    return f(&args);
                }
                // Then try nested frame for user functions
                self.invoke_in_nested_frame(&name, args)
            }
            _ => Err(RuntimeError::Custom(format!("not callable: {}", callee))),
        }
    }

    fn get_func(&self, name: &str) -> Option<BytecodeFunc> {
        self.functions.get(name).cloned()
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
        vm.exec_program(funcs, main_func).unwrap()
    }

    #[test]
    fn test_int_literal() {
        assert_eq!(compile_and_run(&[IRNode::Int(42)]), Value::Int(42));
    }

    #[test]
    fn test_bool_literal() {
        assert_eq!(compile_and_run(&[IRNode::Bool(true)]), Value::Bool(true));
    }

    #[test]
    fn test_arithmetic() {
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        assert_eq!(compile_and_run(&[node]), Value::Int(7));
    }

    #[test]
    fn test_if_true() {
        let node = IRNode::If(
            Box::new(IRNode::Bool(true)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(1));
    }

    #[test]
    fn test_if_false() {
        let node = IRNode::If(
            Box::new(IRNode::Bool(false)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(2));
    }

    #[test]
    fn test_let() {
        let node = IRNode::Let(
            vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }],
            Box::new(IRNode::Load("x".into())),
        );
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
                IRArm {
                    pattern: IRPattern::Variant("Ok".into(), vec![IRPattern::Bind("v".into())]),
                    body: IRNode::Load("v".into()),
                },
                IRArm {
                    pattern: IRPattern::Wild,
                    body: IRNode::Int(0),
                },
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
        // count-down(n) = if (= n 0) 0 (count-down (- n 1))
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

    // ── eval_simple / inline closure tests ────────────────────────────

    #[test]
    fn test_map_simple_closure() {
        // (map (fn [x] (+ x 1)) [1 2 3]) => [2 3 4]
        let nodes = vec![
            IRNode::Call("map".into(), vec![
                IRNode::Lambda(vec!["x".into()],
                    Box::new(IRNode::Call("+".into(), vec![
                        IRNode::Load("x".into()), IRNode::Int(1),
                    ]))),
                IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]),
            ]),
        ];
        assert_eq!(
            compile_and_run(&nodes),
            Value::List(vec![Value::Int(2), Value::Int(3), Value::Int(4)]),
        );
    }

    #[test]
    fn test_filter_simple_closure() {
        // (filter (fn [x] (> x 2)) [1 2 3 4 5]) => [3 4 5]
        let nodes = vec![
            IRNode::Call("filter".into(), vec![
                IRNode::Lambda(vec!["x".into()],
                    Box::new(IRNode::Call(">".into(), vec![
                        IRNode::Load("x".into()), IRNode::Int(2),
                    ]))),
                IRNode::List(vec![
                    IRNode::Int(1), IRNode::Int(2), IRNode::Int(3),
                    IRNode::Int(4), IRNode::Int(5),
                ]),
            ]),
        ];
        assert_eq!(
            compile_and_run(&nodes),
            Value::List(vec![Value::Int(3), Value::Int(4), Value::Int(5)]),
        );
    }

    #[test]
    fn test_fold_simple_closure() {
        // (fold (fn [acc x] (+ acc x)) 0 [1 2 3]) => 6
        let nodes = vec![
            IRNode::Call("fold".into(), vec![
                IRNode::Lambda(vec!["acc".into(), "x".into()],
                    Box::new(IRNode::Call("+".into(), vec![
                        IRNode::Load("acc".into()), IRNode::Load("x".into()),
                    ]))),
                IRNode::Int(0),
                IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]),
            ]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(6));
    }

    #[test]
    fn test_map_mul_closure() {
        // (map (fn [x] (* x x)) [2 3 4]) => [4 9 16]
        let nodes = vec![
            IRNode::Call("map".into(), vec![
                IRNode::Lambda(vec!["x".into()],
                    Box::new(IRNode::Call("*".into(), vec![
                        IRNode::Load("x".into()), IRNode::Load("x".into()),
                    ]))),
                IRNode::List(vec![IRNode::Int(2), IRNode::Int(3), IRNode::Int(4)]),
            ]),
        ];
        assert_eq!(
            compile_and_run(&nodes),
            Value::List(vec![Value::Int(4), Value::Int(9), Value::Int(16)]),
        );
    }

    #[test]
    fn test_is_simple_closure_check() {
        // Verify that is_simple_closure correctly identifies a simple function
        let simple_func = BytecodeFunc {
            name: "test_simple".into(),
            arity: 1,
            register_count: 3,
            capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadConst, 1, 0, 0),
                Instruction::new(Op::Add, 2, 0, 1),
                Instruction::new(Op::Return, 0, 2, 0),
            ],
            constants: vec![Value::Int(1)],
        };
        assert!(is_simple_closure(&simple_func));

        // A function with Call should NOT be simple
        let complex_func = BytecodeFunc {
            name: "test_complex".into(),
            arity: 1,
            register_count: 3,
            capture_count: 0,
            instructions: vec![
                Instruction::new(Op::Call, 1, 0, 1),
                Instruction::new(Op::Return, 0, 1, 0),
            ],
            constants: vec![Value::Str("foo".into())],
        };
        assert!(!is_simple_closure(&complex_func));
    }
}
