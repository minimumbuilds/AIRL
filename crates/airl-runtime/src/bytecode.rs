// crates/airl-runtime/src/bytecode.rs
use crate::value::Value;

/// Bytecode opcodes for the register-based VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    // Literals
    LoadConst,      // dst, const_idx, _
    LoadNil,        // dst, _, _
    LoadTrue,       // dst, _, _
    LoadFalse,      // dst, _, _
    Move,           // dst, src, _

    // Arithmetic
    Add,            // dst, a, b
    Sub,            // dst, a, b
    Mul,            // dst, a, b
    Div,            // dst, a, b
    Mod,            // dst, a, b

    // Comparison
    Eq,             // dst, a, b
    Ne,             // dst, a, b
    Lt,             // dst, a, b
    Le,             // dst, a, b
    Gt,             // dst, a, b
    Ge,             // dst, a, b

    // Logic
    Not,            // dst, a, _

    // Control flow
    Jump,           // _, a(offset), _       -- signed i16 in a
    JumpIfFalse,    // _, a(reg), b(offset)  -- signed i16 in b
    JumpIfTrue,     // _, a(reg), b(offset)  -- signed i16 in b

    // Functions
    Call,           // dst, func_idx, argc   -- args in [dst+1..dst+1+argc]
    CallBuiltin,    // dst, name_idx, argc   -- name from constants
    CallReg,        // dst, callee_reg, argc -- closure/funcref in register
    TailCall,       // _, func_idx, argc     -- rebind args, reset ip
    Return,         // _, src, _

    // Data
    MakeList,       // dst, start, count
    MakeVariant,    // dst, tag_idx, a       -- 1-arg variant, inner in reg a
    MakeVariant0,   // dst, tag_idx, _       -- 0-arg variant (Nil inner)
    MakeClosure,    // dst, func_idx, capture_start

    // Pattern matching
    MatchTag,       // dst, scrutinee, tag_idx -- extract inner if tag matches
    JumpIfNoMatch,  // _, a(offset), _         -- jump if match_flag is false
    MatchWild,      // dst, scrutinee, _       -- always matches

    // Error handling
    TryUnwrap,      // dst, src, err_offset    -- unwrap Ok or jump to error
}

/// A single bytecode instruction — fixed size for cache-friendly execution.
#[derive(Debug, Clone, Copy)]
pub struct Instruction {
    pub op: Op,
    pub dst: u16,
    pub a: u16,
    pub b: u16,
}

impl Instruction {
    pub fn new(op: Op, dst: u16, a: u16, b: u16) -> Self {
        Instruction { op, dst, a, b }
    }
}

/// A compiled function ready for bytecode execution.
#[derive(Debug, Clone)]
pub struct BytecodeFunc {
    pub name: String,
    pub arity: u16,
    pub register_count: u16,
    pub capture_count: u16,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
}

/// A bytecode closure: function index + captured values.
#[derive(Debug, Clone)]
pub struct BytecodeClosureValue {
    pub func_name: String,
    pub captured: Vec<Value>,
}
