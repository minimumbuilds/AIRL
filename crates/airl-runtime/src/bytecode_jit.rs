// crates/airl-runtime/src/bytecode_jit.rs
//! Bytecode→Cranelift JIT compiler.
//!
//! Compiles eligible BytecodeFunc instructions to native x86-64 via Cranelift.
//! Eligible = primitive-typed functions with no list/variant/closure/builtin opcodes.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use crate::bytecode::*;
use crate::value::Value;
use crate::error::RuntimeError;

/// Type hint for marshaling native results back to Value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeHint {
    Int,
    Float,
    Bool,
}

pub struct BytecodeJit {
    module: JITModule,
    compiled: HashMap<String, (*const u8, TypeHint)>,
    ineligible: HashSet<String>,
}

impl BytecodeJit {
    pub fn new() -> Result<Self, String> {
        let builder = cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
            ineligible: HashSet::new(),
        })
    }

    /// Check if a BytecodeFunc is eligible for JIT compilation.
    /// Returns false if any instruction uses non-primitive operations.
    fn is_eligible(func: &BytecodeFunc, all_functions: &HashMap<String, BytecodeFunc>, compiled: &HashMap<String, (*const u8, TypeHint)>, ineligible: &HashSet<String>) -> bool {
        for instr in &func.instructions {
            match instr.op {
                // Disqualifying opcodes — require non-primitive Value types
                Op::MakeList | Op::MakeVariant | Op::MakeVariant0 |
                Op::MakeClosure | Op::MatchTag | Op::JumpIfNoMatch |
                Op::MatchWild | Op::TryUnwrap | Op::CallBuiltin | Op::CallReg => {
                    return false;
                }
                Op::Call => {
                    // Check if the call target is JIT-eligible
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => return false,
                    };
                    // Self-calls are fine (handled as recursion)
                    if name == &func.name {
                        continue;
                    }
                    // Target must be already compiled or at least not ineligible
                    if ineligible.contains(name) {
                        return false;
                    }
                    if !compiled.contains_key(name) {
                        // Target not yet compiled — check if it exists and is eligible
                        if let Some(target) = all_functions.get(name) {
                            if !Self::is_eligible(target, all_functions, compiled, ineligible) {
                                return false;
                            }
                        } else {
                            // Unknown function (builtin like "print") — ineligible
                            return false;
                        }
                    }
                }
                Op::TailCall => {
                    // Verify it's a self-call
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => return false,
                    };
                    if name != &func.name {
                        return false; // Cross-function tail call — not supported
                    }
                }
                // All other opcodes are fine for primitives
                _ => {}
            }
        }
        // Check arity limit
        if func.arity > 8 {
            return false;
        }
        true
    }

    pub fn try_call_native(&self, name: &str, args: &[Value]) -> Option<Result<Value, RuntimeError>> {
        let (ptr, return_hint) = self.compiled.get(name)?;

        // Marshal args — bail if any is non-primitive
        let raw_args: Vec<u64> = args.iter()
            .map(marshal_arg)
            .collect::<Option<Vec<_>>>()?;

        let raw_result: u64 = unsafe {
            match raw_args.len() {
                0 => {
                    let f: fn() -> u64 = std::mem::transmute(*ptr);
                    f()
                }
                1 => {
                    let f: fn(u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0])
                }
                2 => {
                    let f: fn(u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1])
                }
                3 => {
                    let f: fn(u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2])
                }
                4 => {
                    let f: fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3])
                }
                5 => {
                    let f: fn(u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4])
                }
                6 => {
                    let f: fn(u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5])
                }
                7 => {
                    let f: fn(u64, u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5], raw_args[6])
                }
                8 => {
                    let f: fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5], raw_args[6], raw_args[7])
                }
                _ => return None, // >8 args — fall back to bytecode
            }
        };

        Some(Ok(unmarshal_result(raw_result, *return_hint)))
    }
}

fn marshal_arg(val: &Value) -> Option<u64> {
    match val {
        Value::Int(n) => Some(*n as u64),
        Value::Float(f) => Some(f.to_bits()),
        Value::Bool(b) => Some(*b as u64),
        _ => None,
    }
}

fn unmarshal_result(raw: u64, hint: TypeHint) -> Value {
    match hint {
        TypeHint::Int => Value::Int(raw as i64),
        TypeHint::Float => Value::Float(f64::from_bits(raw)),
        TypeHint::Bool => Value::Bool(raw != 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_func(name: &str, arity: u16, ops: Vec<Op>) -> BytecodeFunc {
        BytecodeFunc {
            name: name.to_string(),
            arity,
            register_count: arity + 2,
            capture_count: 0,
            instructions: ops.into_iter().map(|op| Instruction::new(op, 0, 0, 0)).collect(),
            constants: vec![],
        }
    }

    #[test]
    fn test_eligible_arithmetic() {
        let func = make_func("add", 2, vec![Op::Add, Op::Return]);
        let all = HashMap::new();
        assert!(BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_ineligible_make_list() {
        let func = make_func("f", 1, vec![Op::MakeList, Op::Return]);
        let all = HashMap::new();
        assert!(!BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_ineligible_call_reg() {
        let func = make_func("f", 1, vec![Op::CallReg, Op::Return]);
        let all = HashMap::new();
        assert!(!BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_marshal_roundtrip_int() {
        let val = Value::Int(42);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Int);
        assert_eq!(back, Value::Int(42));
    }

    #[test]
    fn test_marshal_roundtrip_float() {
        let val = Value::Float(3.14);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Float);
        assert_eq!(back, Value::Float(3.14));
    }

    #[test]
    fn test_marshal_roundtrip_bool() {
        let val = Value::Bool(true);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Bool);
        assert_eq!(back, Value::Bool(true));
    }

    #[test]
    fn test_marshal_rejects_string() {
        assert!(marshal_arg(&Value::Str("hello".into())).is_none());
    }

    #[test]
    fn test_marshal_rejects_list() {
        assert!(marshal_arg(&Value::List(vec![])).is_none());
    }
}
