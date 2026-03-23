// crates/airl-runtime/src/bytecode_marshal.rs
//
// Marshals AIRL Value representations of bytecode into BytecodeFunc structs.
//
// Format from AIRL:
//   [(BCFunc "name" arity reg_count capture_count [constants...] [(op dst a b) ...])]
//
// In Rust terms:
//   - A BCFunc is Value::Variant("BCFunc", Box(Value::List([name, arity, reg_count, capture_count, constants_list, instructions_list])))
//   - Each instruction is Value::List([Value::Int(op), Value::Int(dst), Value::Int(a), Value::Int(b)])
//   - Constants are raw Values (Int, Float, Str, Bool, Nil, etc.)

use crate::bytecode::{BytecodeFunc, Instruction, Op};
use crate::value::Value;
use crate::error::RuntimeError;

fn type_err(msg: &str) -> RuntimeError {
    RuntimeError::TypeError(format!("bytecode marshal: {}", msg))
}

/// Convert a Value::Int to u16, for register/index fields.
fn value_to_u16(val: &Value, field: &str) -> Result<u16, RuntimeError> {
    match val {
        Value::Int(n) => Ok(*n as u16),
        _ => Err(type_err(&format!("{}: expected Int, got {:?}", field, val))),
    }
}

/// Map an integer discriminant to the Op enum.
/// Order must match the #[repr(u8)] order in bytecode.rs.
fn int_to_op(n: u16) -> Result<Op, RuntimeError> {
    match n {
        0  => Ok(Op::LoadConst),
        1  => Ok(Op::LoadNil),
        2  => Ok(Op::LoadTrue),
        3  => Ok(Op::LoadFalse),
        4  => Ok(Op::Move),
        5  => Ok(Op::Add),
        6  => Ok(Op::Sub),
        7  => Ok(Op::Mul),
        8  => Ok(Op::Div),
        9  => Ok(Op::Mod),
        10 => Ok(Op::Eq),
        11 => Ok(Op::Ne),
        12 => Ok(Op::Lt),
        13 => Ok(Op::Le),
        14 => Ok(Op::Gt),
        15 => Ok(Op::Ge),
        16 => Ok(Op::Not),
        17 => Ok(Op::Neg),
        18 => Ok(Op::Jump),
        19 => Ok(Op::JumpIfFalse),
        20 => Ok(Op::JumpIfTrue),
        21 => Ok(Op::Call),
        22 => Ok(Op::CallBuiltin),
        23 => Ok(Op::CallReg),
        24 => Ok(Op::TailCall),
        25 => Ok(Op::Return),
        26 => Ok(Op::MakeList),
        27 => Ok(Op::MakeVariant),
        28 => Ok(Op::MakeVariant0),
        29 => Ok(Op::MakeClosure),
        30 => Ok(Op::MatchTag),
        31 => Ok(Op::JumpIfNoMatch),
        32 => Ok(Op::MatchWild),
        33 => Ok(Op::TryUnwrap),
        _  => Err(type_err(&format!("unknown opcode: {}", n))),
    }
}

/// Convert a `[op dst a b]` list Value into an Instruction.
/// The `b` field can hold signed jump offsets; we store as u16 and the VM
/// casts back with `as i16`.
fn value_to_instruction(val: &Value) -> Result<Instruction, RuntimeError> {
    match val {
        Value::List(items) => {
            if items.len() < 4 {
                return Err(type_err(&format!(
                    "instruction: expected 4 elements, got {}",
                    items.len()
                )));
            }
            let op_n = value_to_u16(&items[0], "instruction op")?;
            let op   = int_to_op(op_n)?;
            let dst  = value_to_u16(&items[1], "instruction dst")?;
            let a    = value_to_u16(&items[2], "instruction a")?;
            let b    = value_to_u16(&items[3], "instruction b")?;
            Ok(Instruction::new(op, dst, a, b))
        }
        _ => Err(type_err("expected instruction as list [op dst a b]")),
    }
}

/// Convert a `BCFunc` variant Value into a BytecodeFunc.
///
/// Expected shape:
///   (BCFunc "name" arity reg_count capture_count [constants...] [(op dst a b) ...])
///
/// which in Value terms is:
///   Variant("BCFunc", List([Str(name), Int(arity), Int(reg_count), Int(capture_count),
///                            List(constants), List(instructions)]))
pub fn value_to_bytecode_func(val: &Value) -> Result<BytecodeFunc, RuntimeError> {
    match val {
        Value::Variant(tag, inner) if tag == "BCFunc" => {
            // When a variant constructor is called with N args in AIRL:
            //   N=1 → inner is the single value
            //   N>1 → inner is Value::Tuple([...])
            // So (BCFunc name arity reg_count capture_count consts instrs) produces
            // inner = Value::Tuple([name, arity, reg_count, capture_count, consts, instrs]).
            // We also accept Value::List for programmatic construction.
            let items: &[Value] = match inner.as_ref() {
                Value::Tuple(items) => items,
                Value::List(items) => items,
                _ => return Err(type_err("BCFunc inner value must be a Tuple or List")),
            };
            if items.len() < 6 {
                return Err(type_err(&format!(
                    "BCFunc: expected 6 fields, got {}",
                    items.len()
                )));
            }
            let name = match &items[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(type_err("BCFunc name: expected Str")),
            };
            let arity          = value_to_u16(&items[1], "BCFunc arity")?;
            let register_count = value_to_u16(&items[2], "BCFunc reg_count")?;
            let capture_count  = value_to_u16(&items[3], "BCFunc capture_count")?;

            let constants = match &items[4] {
                Value::List(cs) => cs.clone(),
                _ => return Err(type_err("BCFunc constants: expected List")),
            };

            let instructions = match &items[5] {
                Value::List(is) => is
                    .iter()
                    .map(value_to_instruction)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(type_err("BCFunc instructions: expected List")),
            };

            Ok(BytecodeFunc {
                name,
                arity,
                register_count,
                capture_count,
                instructions,
                constants,
            })
        }
        Value::Variant(tag, _) => Err(type_err(&format!("expected BCFunc variant, got {}", tag))),
        _ => Err(type_err("expected Variant, got non-variant value")),
    }
}

/// Build a BytecodeVm from a slice of BCFunc Values, load all functions, and
/// run `__main__`.
pub fn run_bytecode_program(funcs: &[Value]) -> Result<Value, RuntimeError> {
    let mut vm = crate::bytecode_vm::BytecodeVm::new();
    for f in funcs {
        let func = value_to_bytecode_func(f)?;
        vm.load_function(func);
    }
    vm.exec_main()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::Op;

    fn make_bcfunc(name: &str, arity: i64, reg_count: i64, capture_count: i64,
                   constants: Vec<Value>, instrs: Vec<Value>) -> Value {
        Value::Variant(
            "BCFunc".into(),
            Box::new(Value::List(vec![
                Value::Str(name.into()),
                Value::Int(arity),
                Value::Int(reg_count),
                Value::Int(capture_count),
                Value::List(constants),
                Value::List(instrs),
            ])),
        )
    }

    fn instr(op: i64, dst: i64, a: i64, b: i64) -> Value {
        Value::List(vec![
            Value::Int(op),
            Value::Int(dst),
            Value::Int(a),
            Value::Int(b),
        ])
    }

    #[test]
    fn test_marshal_simple_func() {
        // LoadConst r1 ← 42; Return r1
        let func_val = make_bcfunc(
            "__main__", 0, 2, 0,
            vec![Value::Int(42)],
            vec![
                instr(0, 1, 0, 0),  // LoadConst dst=1 const_idx=0
                instr(25, 0, 1, 0), // Return src=1
            ],
        );
        let func = value_to_bytecode_func(&func_val).unwrap();
        assert_eq!(func.name, "__main__");
        assert_eq!(func.arity, 0);
        assert_eq!(func.register_count, 2);
        assert_eq!(func.capture_count, 0);
        assert_eq!(func.constants, vec![Value::Int(42)]);
        assert_eq!(func.instructions.len(), 2);
        assert_eq!(func.instructions[0].op, Op::LoadConst);
        assert_eq!(func.instructions[0].dst, 1);
        assert_eq!(func.instructions[0].a, 0);
        assert_eq!(func.instructions[1].op, Op::Return);
        assert_eq!(func.instructions[1].a, 1);
    }

    #[test]
    fn test_int_to_op_roundtrip() {
        // Spot-check a few opcodes
        assert_eq!(int_to_op(0).unwrap(), Op::LoadConst);
        assert_eq!(int_to_op(5).unwrap(), Op::Add);
        assert_eq!(int_to_op(25).unwrap(), Op::Return);
        assert_eq!(int_to_op(33).unwrap(), Op::TryUnwrap);
        assert!(int_to_op(34).is_err());
    }

    #[test]
    fn test_run_bytecode_add() {
        // __main__: r1 = 2, r2 = 3, r3 = r1+r2, return r3
        let func_val = make_bcfunc(
            "__main__", 0, 4, 0,
            vec![Value::Int(2), Value::Int(3)],
            vec![
                instr(0, 1, 0, 0),  // LoadConst r1 ← consts[0] = 2
                instr(0, 2, 1, 0),  // LoadConst r2 ← consts[1] = 3
                instr(5, 3, 1, 2),  // Add r3 = r1 + r2
                instr(25, 0, 3, 0), // Return r3
            ],
        );
        let result = run_bytecode_program(&[func_val]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_bad_variant_tag() {
        let bad = Value::Variant("NotAFunc".into(), Box::new(Value::List(vec![])));
        assert!(value_to_bytecode_func(&bad).is_err());
    }

    #[test]
    fn test_unknown_opcode() {
        assert!(int_to_op(99).is_err());
    }
}
