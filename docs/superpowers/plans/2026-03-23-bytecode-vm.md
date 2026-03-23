# Register-Based Bytecode VM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A register-based flat bytecode VM providing 3-5x speedup over the current IR VM, accessible as `--bytecode` execution mode.

**Architecture:** IRNode trees are lowered to flat `Vec<Instruction>` arrays by a bytecode compiler. The bytecode VM executes these via a tight loop with register-indexed local variables, eliminating HashMap lookup and pointer-chasing overhead. Coexists with the IR VM (`--compiled`) and interpreter (default).

**Tech Stack:** Rust (all new code in `airl-runtime` and `airl-driver`). No external dependencies.

**Spec:** `docs/superpowers/specs/2026-03-23-bytecode-vm-design.md`

**Reference files:**
- IR types: `crates/airl-runtime/src/ir.rs` (IRNode enum, ~72 lines)
- IR VM: `crates/airl-runtime/src/ir_vm.rs` (~650 lines, pattern to follow)
- Pipeline: `crates/airl-driver/src/pipeline.rs` (run_source_compiled pattern)
- CLI: `crates/airl-driver/src/main.rs` (--compiled flag pattern)
- Builtins: `crates/airl-runtime/src/builtins.rs` (BuiltinFnPtr, Builtins::get)
- Value: `crates/airl-runtime/src/value.rs` (Value enum)
- Error: `crates/airl-runtime/src/error.rs` (RuntimeError enum)

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `crates/airl-runtime/src/bytecode.rs` | `Op` enum, `Instruction`, `BytecodeFunc`, `BytecodeClosureValue` types |
| Create: `crates/airl-runtime/src/bytecode_compiler.rs` | `BytecodeCompiler`: IRNode → bytecode, register allocation |
| Create: `crates/airl-runtime/src/bytecode_vm.rs` | `BytecodeVm`: execution loop, call frames, pattern matching |
| Modify: `crates/airl-runtime/src/lib.rs` | Add `pub mod bytecode; pub mod bytecode_compiler; pub mod bytecode_vm;` |
| Modify: `crates/airl-runtime/src/value.rs` | Add `BytecodeClosure(BytecodeClosureValue)` variant |
| Modify: `crates/airl-runtime/src/builtins.rs` | Add `type_name` arm for BytecodeClosure |
| Modify: `crates/airl-driver/src/pipeline.rs` | Add `run_source_bytecode()`, `run_file_bytecode()` |
| Modify: `crates/airl-driver/src/main.rs` | Add `--bytecode` flag |

---

### Task 1: Bytecode Types

**Files:**
- Create: `crates/airl-runtime/src/bytecode.rs`
- Modify: `crates/airl-runtime/src/lib.rs`
- Modify: `crates/airl-runtime/src/value.rs`
- Modify: `crates/airl-runtime/src/builtins.rs`

Define the bytecode instruction set, function type, and closure value. No logic — just types.

- [ ] **Step 1: Create `bytecode.rs`**

```rust
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
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod bytecode;` to `crates/airl-runtime/src/lib.rs`.

- [ ] **Step 3: Add BytecodeClosure variant to Value**

In `crates/airl-runtime/src/value.rs`, add to the `Value` enum:
```rust
BytecodeClosure(crate::bytecode::BytecodeClosureValue),
```

Add Display arm:
```rust
Value::BytecodeClosure(_) => write!(f, "<bytecode-closure>"),
```

Add PartialEq arm:
```rust
(Value::BytecodeClosure(_), _) => false,
```

In `crates/airl-runtime/src/builtins.rs`, add to `type_name`:
```rust
Value::BytecodeClosure(_) => "BytecodeClosure",
```

- [ ] **Step 4: Build to verify**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime 2>&1 | head -20`
Expected: Build succeeds

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/bytecode.rs crates/airl-runtime/src/lib.rs crates/airl-runtime/src/value.rs crates/airl-runtime/src/builtins.rs
git commit -m "feat(bytecode): add instruction set, BytecodeFunc, BytecodeClosureValue types"
```

---

### Task 2: Bytecode Compiler — Literals, Variables, Arithmetic

**Files:**
- Create: `crates/airl-runtime/src/bytecode_compiler.rs`
- Modify: `crates/airl-runtime/src/lib.rs`

Compile simple IRNode expressions to bytecode. No function calls or control flow yet.

- [ ] **Step 1: Create `bytecode_compiler.rs` with core structure**

```rust
// crates/airl-runtime/src/bytecode_compiler.rs
use std::collections::HashMap;
use crate::ir::*;
use crate::value::Value;
use crate::bytecode::*;

pub struct BytecodeCompiler {
    instructions: Vec<Instruction>,
    constants: Vec<Value>,
    locals: HashMap<String, u16>,       // variable name → register slot
    next_reg: u16,
    max_reg: u16,
    lambda_counter: usize,              // unique lambda name counter
    compiled_lambdas: Vec<BytecodeFunc>, // lambdas compiled during expression compilation
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        BytecodeCompiler {
            instructions: Vec::new(),
            constants: Vec::new(),
            locals: HashMap::new(),
            next_reg: 0,
            max_reg: 0,
            lambda_counter: 0,
            compiled_lambdas: Vec::new(),
        }
    }

    fn alloc_reg(&mut self) -> u16 {
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        r
    }

    fn free_reg_to(&mut self, r: u16) {
        self.next_reg = r;
    }

    fn emit(&mut self, op: Op, dst: u16, a: u16, b: u16) {
        self.instructions.push(Instruction::new(op, dst, a, b));
    }

    fn add_constant(&mut self, val: Value) -> u16 {
        // Reuse existing constant if identical
        for (i, c) in self.constants.iter().enumerate() {
            if c == &val {
                return i as u16;
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.push(val);
        idx
    }

    /// Compile an IRNode expression, placing the result in `dst`.
    pub fn compile_expr(&mut self, node: &IRNode, dst: u16) {
        match node {
            IRNode::Int(v) => {
                let idx = self.add_constant(Value::Int(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Float(v) => {
                let idx = self.add_constant(Value::Float(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Str(s) => {
                let idx = self.add_constant(Value::Str(s.clone()));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Bool(true) => self.emit(Op::LoadTrue, dst, 0, 0),
            IRNode::Bool(false) => self.emit(Op::LoadFalse, dst, 0, 0),
            IRNode::Nil => self.emit(Op::LoadNil, dst, 0, 0),

            IRNode::Load(name) => {
                if let Some(&slot) = self.locals.get(name) {
                    if slot != dst {
                        self.emit(Op::Move, dst, slot, 0);
                    }
                } else {
                    // Will be handled in Task 3 (function refs, builtins)
                    // For now, treat as constant string lookup
                    let idx = self.add_constant(Value::Str(name.clone()));
                    self.emit(Op::LoadConst, dst, idx, 0);
                }
            }

            IRNode::Do(exprs) => {
                if exprs.is_empty() {
                    self.emit(Op::LoadNil, dst, 0, 0);
                } else {
                    let save = self.next_reg;
                    for (i, expr) in exprs.iter().enumerate() {
                        if i == exprs.len() - 1 {
                            self.compile_expr(expr, dst);
                        } else {
                            let tmp = self.alloc_reg();
                            self.compile_expr(expr, tmp);
                        }
                    }
                    self.free_reg_to(save.max(dst + 1));
                }
            }

            IRNode::List(items) => {
                let start = self.next_reg;
                for item in items {
                    let r = self.alloc_reg();
                    self.compile_expr(item, r);
                }
                self.emit(Op::MakeList, dst, start, items.len() as u16);
                self.free_reg_to(start.max(dst + 1));
            }

            // Stubs for remaining node types — implemented in Tasks 3-5
            _ => {
                self.emit(Op::LoadNil, dst, 0, 0); // placeholder
            }
        }
    }

    /// Compile a top-level function definition.
    pub fn compile_function(&mut self, name: &str, params: &[String], body: &IRNode) -> BytecodeFunc {
        let mut compiler = BytecodeCompiler::new();
        // Bind params to first N registers
        for (i, param) in params.iter().enumerate() {
            compiler.locals.insert(param.clone(), i as u16);
            compiler.next_reg = (i as u16) + 1;
            compiler.max_reg = compiler.next_reg;
        }
        let dst = compiler.alloc_reg();
        compiler.compile_expr(body, dst);
        compiler.emit(Op::Return, 0, dst, 0);

        BytecodeFunc {
            name: name.to_string(),
            arity: params.len() as u16,
            register_count: compiler.max_reg,
            capture_count: 0,
            instructions: compiler.instructions,
            constants: compiler.constants,
        }
    }

    /// Compile a list of top-level IRNodes into a list of BytecodeFuncs + a main function.
    pub fn compile_program(&mut self, nodes: &[IRNode]) -> (Vec<BytecodeFunc>, BytecodeFunc) {
        let mut functions = Vec::new();
        let mut main_nodes = Vec::new();

        for node in nodes {
            match node {
                IRNode::Func(name, params, body) => {
                    let func = self.compile_function(name, params, body);
                    functions.push(func);
                }
                _ => main_nodes.push(node.clone()),
            }
        }

        // Compile remaining top-level expressions as __main__
        let main_body = if main_nodes.is_empty() {
            IRNode::Nil
        } else if main_nodes.len() == 1 {
            main_nodes.into_iter().next().unwrap()
        } else {
            IRNode::Do(main_nodes)
        };

        let main_func = self.compile_function("__main__", &[], &main_body);
        // Collect any lambdas compiled during function/main compilation
        functions.extend(self.compiled_lambdas.drain(..));
        (functions, main_func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_int() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Int(42), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Int(42));
    }

    #[test]
    fn test_compile_bool() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Bool(true), 0);
        assert_eq!(c.instructions[0].op, Op::LoadTrue);
    }

    #[test]
    fn test_compile_do() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![IRNode::Int(1), IRNode::Int(2)]), 0);
        // Should have LoadConst for 1 (temp), LoadConst for 2 (dst=0)
        assert!(c.instructions.len() >= 2);
    }

    #[test]
    fn test_compile_function() {
        let mut c = BytecodeCompiler::new();
        // (defn id [x] x)
        let func = c.compile_function("id", &["x".to_string()], &IRNode::Load("x".into()));
        assert_eq!(func.name, "id");
        assert_eq!(func.arity, 1);
        // Should have Move (x→dst) + Return
        assert!(func.instructions.len() >= 1);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod bytecode_compiler;` to `crates/airl-runtime/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime bytecode_compiler -- --nocapture 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/bytecode_compiler.rs crates/airl-runtime/src/lib.rs
git commit -m "feat(bytecode): compiler for literals, variables, do, list"
```

---

### Task 3: Bytecode Compiler — Control Flow, Let, Calls

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_compiler.rs`

Add If, Let, Call, CallExpr, Variant, Lambda, Try to the compiler.

- [ ] **Step 1: Add If compilation**

Replace the `_ =>` stub in `compile_expr` with proper implementations. Add these arms before the wildcard:

```rust
IRNode::If(cond, then_, else_) => {
    // Compile condition
    let cond_reg = self.alloc_reg();
    self.compile_expr(cond, cond_reg);
    // JumpIfFalse to else
    let jump_to_else = self.instructions.len();
    self.emit(Op::JumpIfFalse, 0, cond_reg, 0); // offset patched later
    self.free_reg_to(cond_reg.max(dst + 1));
    // Then branch
    self.compile_expr(then_, dst);
    let jump_to_end = self.instructions.len();
    self.emit(Op::Jump, 0, 0, 0); // offset patched later
    // Else branch
    let else_start = self.instructions.len();
    self.compile_expr(else_, dst);
    let end = self.instructions.len();
    // Patch jumps
    self.instructions[jump_to_else].b = (else_start as i16 - jump_to_else as i16 - 1) as u16;
    self.instructions[jump_to_end].a = (end as i16 - jump_to_end as i16 - 1) as u16;
}

IRNode::Let(bindings, body) => {
    let save_regs = self.next_reg;
    for binding in bindings {
        let r = self.alloc_reg();
        self.compile_expr(&binding.expr, r);
        self.locals.insert(binding.name.clone(), r);
    }
    self.compile_expr(body, dst);
    // Remove bindings from locals
    for binding in bindings {
        self.locals.remove(&binding.name);
    }
    self.free_reg_to(save_regs.max(dst + 1));
}

IRNode::Call(name, args) => {
    // Check if it's a known arithmetic/comparison builtin for direct opcodes
    let direct_op = match name.as_str() {
        "+" => Some(Op::Add),
        "-" => Some(Op::Sub),
        "*" => Some(Op::Mul),
        "/" => Some(Op::Div),
        "%" => Some(Op::Mod),
        "=" => Some(Op::Eq),
        "!=" => Some(Op::Ne),
        "<" => Some(Op::Lt),
        "<=" => Some(Op::Le),
        ">" => Some(Op::Gt),
        ">=" => Some(Op::Ge),
        "not" => Some(Op::Not),
        _ => None,
    };

    if let Some(op) = direct_op {
        if args.len() == 2 {
            let a_reg = self.alloc_reg();
            self.compile_expr(&args[0], a_reg);
            let b_reg = self.alloc_reg();
            self.compile_expr(&args[1], b_reg);
            self.emit(op, dst, a_reg, b_reg);
            self.free_reg_to(a_reg.max(dst + 1));
        } else if args.len() == 1 {
            let a_reg = self.alloc_reg();
            self.compile_expr(&args[0], a_reg);
            self.emit(op, dst, a_reg, 0);
            self.free_reg_to(a_reg.max(dst + 1));
        }
    } else {
        // General function call
        // Place args in consecutive registers starting at dst+1
        let arg_start = dst + 1;
        let save = self.next_reg;
        self.next_reg = arg_start;
        if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
        for arg in args {
            let r = self.alloc_reg();
            self.compile_expr(arg, r);
        }
        // Determine if user-defined or builtin
        if self.locals.contains_key(name) {
            // Closure/funcref in register
            let callee_reg = *self.locals.get(name).unwrap();
            self.emit(Op::CallReg, dst, callee_reg, args.len() as u16);
        } else {
            // Try as named function first, fall back to builtin
            let name_idx = self.add_constant(Value::Str(name.clone()));
            self.emit(Op::Call, dst, name_idx, args.len() as u16);
        }
        self.free_reg_to(save.max(dst + 1));
    }
}

IRNode::CallExpr(callee, args) => {
    let callee_reg = self.alloc_reg();
    self.compile_expr(callee, callee_reg);
    let arg_start = dst + 1;
    let save = self.next_reg;
    self.next_reg = arg_start;
    if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
    for arg in args {
        let r = self.alloc_reg();
        self.compile_expr(arg, r);
    }
    self.emit(Op::CallReg, dst, callee_reg, args.len() as u16);
    self.free_reg_to(save.max(dst + 1));
}

IRNode::Variant(tag, args) => {
    let tag_idx = self.add_constant(Value::Str(tag.clone()));
    if args.is_empty() {
        self.emit(Op::MakeVariant0, dst, tag_idx, 0);
    } else if args.len() == 1 {
        let a_reg = self.alloc_reg();
        self.compile_expr(&args[0], a_reg);
        self.emit(Op::MakeVariant, dst, tag_idx, a_reg);
        self.free_reg_to(a_reg.max(dst + 1));
    } else {
        // Multi-arg variant: wrap in list
        let start = self.next_reg;
        for arg in args {
            let r = self.alloc_reg();
            self.compile_expr(arg, r);
        }
        let list_reg = self.alloc_reg();
        self.emit(Op::MakeList, list_reg, start, args.len() as u16);
        self.emit(Op::MakeVariant, dst, tag_idx, list_reg);
        self.free_reg_to(start.max(dst + 1));
    }
}

IRNode::Lambda(params, body) => {
    // Compile lambda body as a named function stored in a side table.
    // The compiler must track compiled lambdas so the pipeline can
    // register them in the VM before execution begins.
    let lambda_name = format!("__lambda_{}", self.lambda_counter);
    self.lambda_counter += 1;

    // Compile the lambda body as a standalone function.
    // Captured variables become additional parameters prepended before the user params.
    let captured_names: Vec<(String, u16)> = self.locals.iter()
        .map(|(k, &v)| (k.clone(), v))
        .collect();

    let mut all_params: Vec<String> = captured_names.iter().map(|(n, _)| n.clone()).collect();
    all_params.extend(params.iter().cloned());

    let func = self.compile_function(&lambda_name, &all_params, body);
    self.compiled_lambdas.push(func);

    // Emit MakeClosure: copy captured values to consecutive regs, then emit opcode
    let capture_start = self.next_reg;
    for (_, &slot) in &captured_names {
        let r = self.alloc_reg();
        self.emit(Op::Move, r, slot, 0);
    }
    let name_idx = self.add_constant(Value::Str(lambda_name));
    self.emit(Op::MakeClosure, dst, name_idx, capture_start);
    self.free_reg_to(capture_start.max(dst + 1));
}

IRNode::Try(expr) => {
    let src = self.alloc_reg();
    self.compile_expr(expr, src);
    let err_jump = self.instructions.len();
    self.emit(Op::TryUnwrap, dst, src, 0); // err_offset patched later
    self.free_reg_to(src.max(dst + 1));
    // Note: error handling jump target patched in context
}
```

- [ ] **Step 2: Add Match compilation**

```rust
IRNode::Match(scrutinee, arms) => {
    let scr_reg = self.alloc_reg();
    self.compile_expr(scrutinee, scr_reg);

    let mut end_jumps = Vec::new();

    for arm in arms {
        match &arm.pattern {
            IRPattern::Wild => {
                self.emit(Op::MatchWild, dst, scr_reg, 0);
                // No jump needed — wildcard always matches
                self.compile_expr(&arm.body, dst);
            }
            IRPattern::Bind(name) => {
                // Bind scrutinee to name
                self.locals.insert(name.clone(), scr_reg);
                self.compile_expr(&arm.body, dst);
                self.locals.remove(name);
            }
            IRPattern::Lit(val) => {
                let val_reg = self.alloc_reg();
                let idx = self.add_constant(val.clone());
                self.emit(Op::LoadConst, val_reg, idx, 0);
                self.emit(Op::Eq, val_reg, scr_reg, val_reg);
                let skip = self.instructions.len();
                self.emit(Op::JumpIfFalse, 0, val_reg, 0); // patch later
                self.free_reg_to(val_reg.max(dst + 1));
                self.compile_expr(&arm.body, dst);
                end_jumps.push(self.instructions.len());
                self.emit(Op::Jump, 0, 0, 0); // jump to end, patch later
                let here = self.instructions.len();
                self.instructions[skip].b = (here as i16 - skip as i16 - 1) as u16;
            }
            IRPattern::Variant(tag, sub_pats) => {
                let tag_idx = self.add_constant(Value::Str(tag.clone()));
                let inner_reg = self.alloc_reg();
                self.emit(Op::MatchTag, inner_reg, scr_reg, tag_idx);
                let skip = self.instructions.len();
                self.emit(Op::JumpIfNoMatch, 0, 0, 0); // patch later
                // Bind sub-patterns
                if sub_pats.len() == 1 {
                    if let IRPattern::Bind(name) = &sub_pats[0] {
                        self.locals.insert(name.clone(), inner_reg);
                        self.compile_expr(&arm.body, dst);
                        self.locals.remove(name);
                    } else if let IRPattern::Wild = &sub_pats[0] {
                        self.compile_expr(&arm.body, dst);
                    } else {
                        // Nested pattern — compile body with inner bound
                        self.compile_expr(&arm.body, dst);
                    }
                } else if sub_pats.is_empty() {
                    self.compile_expr(&arm.body, dst);
                } else {
                    // Multi-field variant destructuring not common, handle basic case
                    self.compile_expr(&arm.body, dst);
                }
                self.free_reg_to(inner_reg.max(dst + 1));
                end_jumps.push(self.instructions.len());
                self.emit(Op::Jump, 0, 0, 0); // jump to end
                let here = self.instructions.len();
                self.instructions[skip].a = (here as i16 - skip as i16 - 1) as u16;
            }
        }
    }
    // Patch all end jumps
    let end = self.instructions.len();
    for j in end_jumps {
        self.instructions[j].a = (end as i16 - j as i16 - 1) as u16;
    }
    self.free_reg_to(scr_reg.max(dst + 1));
}
```

- [ ] **Step 3: Add tests for control flow and calls**

```rust
#[test]
fn test_compile_if() {
    let mut c = BytecodeCompiler::new();
    let node = IRNode::If(
        Box::new(IRNode::Bool(true)),
        Box::new(IRNode::Int(1)),
        Box::new(IRNode::Int(2)),
    );
    c.compile_expr(&node, 0);
    // Should have: LoadTrue, JumpIfFalse, LoadConst(1), Jump, LoadConst(2)
    assert!(c.instructions.len() >= 5);
}

#[test]
fn test_compile_let() {
    let mut c = BytecodeCompiler::new();
    let node = IRNode::Let(
        vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }],
        Box::new(IRNode::Load("x".into())),
    );
    c.compile_expr(&node, 0);
    assert!(c.instructions.len() >= 2);
}

#[test]
fn test_compile_call_add() {
    let mut c = BytecodeCompiler::new();
    let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
    c.compile_expr(&node, 0);
    // Should use direct Add opcode, not CallBuiltin
    let has_add = c.instructions.iter().any(|i| i.op == Op::Add);
    assert!(has_add, "arithmetic should compile to direct opcode");
}
```

- [ ] **Step 4: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime bytecode_compiler -- --nocapture 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/bytecode_compiler.rs
git commit -m "feat(bytecode): compiler for if, let, calls, match, variant, lambda, try"
```

---

### Task 4: Bytecode VM — Core Execution Loop

**Files:**
- Create: `crates/airl-runtime/src/bytecode_vm.rs`
- Modify: `crates/airl-runtime/src/lib.rs`

Implement the VM execution loop for all opcodes.

- [ ] **Step 1: Create `bytecode_vm.rs`**

```rust
// crates/airl-runtime/src/bytecode_vm.rs
use std::collections::HashMap;
use crate::bytecode::*;
use crate::value::Value;
use crate::builtins::Builtins;
use crate::error::RuntimeError;

struct CallFrame {
    registers: Vec<Value>,
    func_name: String,
    ip: usize,
    return_reg: u16,
    match_flag: bool,
}

pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
}

impl BytecodeVm {
    pub fn new() -> Self {
        BytecodeVm {
            functions: HashMap::new(),
            builtins: Builtins::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
        }
    }

    pub fn load_function(&mut self, func: BytecodeFunc) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Execute a function by name with no arguments. Used to run __main__.
    pub fn exec_main(&mut self) -> Result<Value, RuntimeError> {
        self.call_function("__main__", &[])
    }

    fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        // Try builtin first
        if let Some(func) = self.builtins.get(name) {
            return func(args);
        }

        let func = self.functions.get(name)
            .ok_or_else(|| RuntimeError::UndefinedSymbol(name.to_string()))?
            .clone();

        self.recursion_depth += 1;
        if self.recursion_depth > 50_000 {
            self.recursion_depth -= 1;
            return Err(RuntimeError::Custom("stack overflow".into()));
        }

        let mut registers = vec![Value::Nil; func.register_count as usize];
        for (i, arg) in args.iter().enumerate() {
            if i < registers.len() {
                registers[i] = arg.clone();
            }
        }

        self.call_stack.push(CallFrame {
            registers,
            func_name: name.to_string(),
            ip: 0,
            return_reg: 0,
            match_flag: false,
        });

        let result = self.run();
        self.recursion_depth -= 1;
        result
    }

    fn run(&mut self) -> Result<Value, RuntimeError> {
        loop {
            let frame = self.call_stack.last().unwrap();
            let func = self.functions.get(&frame.func_name).unwrap().clone();

            if frame.ip >= func.instructions.len() {
                // Implicit return nil
                self.call_stack.pop();
                if self.call_stack.is_empty() {
                    return Ok(Value::Nil);
                }
                continue;
            }

            let instr = func.instructions[frame.ip];
            let frame = self.call_stack.last_mut().unwrap();
            frame.ip += 1;

            match instr.op {
                Op::LoadConst => {
                    frame.registers[instr.dst as usize] = func.constants[instr.a as usize].clone();
                }
                Op::LoadNil => {
                    frame.registers[instr.dst as usize] = Value::Nil;
                }
                Op::LoadTrue => {
                    frame.registers[instr.dst as usize] = Value::Bool(true);
                }
                Op::LoadFalse => {
                    frame.registers[instr.dst as usize] = Value::Bool(false);
                }
                Op::Move => {
                    frame.registers[instr.dst as usize] = frame.registers[instr.a as usize].clone();
                }

                // Arithmetic
                Op::Add => {
                    let a = &frame.registers[instr.a as usize];
                    let b = &frame.registers[instr.b as usize];
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                        (Value::Str(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
                        _ => return Err(RuntimeError::TypeError(format!("add: incompatible types")))
                    };
                }
                Op::Sub => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into()))
                    };
                }
                Op::Mul => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into()))
                    };
                }
                Op::Div => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(_, ), Value::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into()))
                    };
                }
                Op::Mod => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x % y),
                        _ => return Err(RuntimeError::TypeError("mod: incompatible types".into()))
                    };
                }

                // Comparison
                Op::Eq => {
                    let a = &frame.registers[instr.a as usize];
                    let b = &frame.registers[instr.b as usize];
                    frame.registers[instr.dst as usize] = Value::Bool(a == b);
                }
                Op::Ne => {
                    let a = &frame.registers[instr.a as usize];
                    let b = &frame.registers[instr.b as usize];
                    frame.registers[instr.dst as usize] = Value::Bool(a != b);
                }
                Op::Lt => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Le => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Gt => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Ge => {
                    let (a, b) = (&frame.registers[instr.a as usize], &frame.registers[instr.b as usize]);
                    frame.registers[instr.dst as usize] = match (a, b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x >= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x >= y),
                        _ => Value::Bool(false),
                    };
                }
                Op::Not => {
                    let a = &frame.registers[instr.a as usize];
                    frame.registers[instr.dst as usize] = match a {
                        Value::Bool(b) => Value::Bool(!b),
                        _ => return Err(RuntimeError::TypeError("not: expected bool".into())),
                    };
                }

                // Control flow
                Op::Jump => {
                    let offset = instr.a as i16;
                    frame.ip = (frame.ip as i32 + offset as i32) as usize;
                }
                Op::JumpIfFalse => {
                    let val = &frame.registers[instr.a as usize];
                    if let Value::Bool(false) = val {
                        let offset = instr.b as i16;
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let val = &frame.registers[instr.a as usize];
                    if let Value::Bool(true) = val {
                        let offset = instr.b as i16;
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }

                // Data
                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let items: Vec<Value> = (start..start+count)
                        .map(|i| frame.registers[i].clone())
                        .collect();
                    frame.registers[instr.dst as usize] = Value::List(items);
                }
                Op::MakeVariant => {
                    let tag = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                    };
                    let inner = frame.registers[instr.b as usize].clone();
                    frame.registers[instr.dst as usize] = Value::Variant(tag, Box::new(inner));
                }
                Op::MakeVariant0 => {
                    let tag = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                    };
                    frame.registers[instr.dst as usize] = Value::Variant(tag, Box::new(Value::Nil));
                }

                // Pattern matching
                Op::MatchTag => {
                    let scr = &frame.registers[instr.a as usize];
                    let tag = match &func.constants[instr.b as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("match tag must be string".into())),
                    };
                    match scr {
                        Value::Variant(vtag, inner) if *vtag == tag => {
                            frame.registers[instr.dst as usize] = *inner.clone();
                            frame.match_flag = true;
                        }
                        _ => {
                            frame.match_flag = false;
                        }
                    }
                }
                Op::JumpIfNoMatch => {
                    if !frame.match_flag {
                        let offset = instr.a as i16;
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::MatchWild => {
                    frame.registers[instr.dst as usize] = frame.registers[instr.a as usize].clone();
                    frame.match_flag = true;
                }

                // Try
                Op::TryUnwrap => {
                    let val = frame.registers[instr.a as usize].clone();
                    match val {
                        Value::Variant(ref tag, ref inner) if tag == "Ok" => {
                            frame.registers[instr.dst as usize] = *inner.clone();
                        }
                        Value::Variant(ref tag, ref inner) if tag == "Err" => {
                            return Err(RuntimeError::Custom(format!("{}", inner)));
                        }
                        _ => return Err(RuntimeError::TryOnNonResult(format!("{}", val))),
                    }
                }

                // Function calls
                Op::Call => {
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("call: func name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = (0..argc)
                        .map(|i| frame.registers[instr.dst as usize + 1 + i].clone())
                        .collect();

                    let result = self.call_function(&name, &args)?;
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.registers[instr.dst as usize] = result;
                }
                Op::CallBuiltin => {
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                    };
                    let argc = instr.b as usize;
                    let frame = self.call_stack.last().unwrap();
                    let args: Vec<Value> = (0..argc)
                        .map(|i| frame.registers[instr.dst as usize + 1 + i].clone())
                        .collect();
                    if let Some(f) = self.builtins.get(&name) {
                        let result = f(&args)?;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.registers[instr.dst as usize] = result;
                    } else {
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }
                Op::CallReg => {
                    let callee = frame.registers[instr.a as usize].clone();
                    let argc = instr.b as usize;
                    let args: Vec<Value> = (0..argc)
                        .map(|i| frame.registers[instr.dst as usize + 1 + i].clone())
                        .collect();
                    match callee {
                        Value::BytecodeClosure(closure) => {
                            let result = self.call_function(&closure.func_name, &args)?;
                            let frame = self.call_stack.last_mut().unwrap();
                            frame.registers[instr.dst as usize] = result;
                        }
                        Value::IRFuncRef(name) => {
                            let result = self.call_function(&name, &args)?;
                            let frame = self.call_stack.last_mut().unwrap();
                            frame.registers[instr.dst as usize] = result;
                        }
                        Value::BuiltinFn(name) => {
                            if let Some(f) = self.builtins.get(&name) {
                                let result = f(&args)?;
                                let frame = self.call_stack.last_mut().unwrap();
                                frame.registers[instr.dst as usize] = result;
                            } else {
                                return Err(RuntimeError::UndefinedSymbol(name));
                            }
                        }
                        _ => return Err(RuntimeError::NotCallable(format!("{}", callee))),
                    }
                }
                Op::TailCall => {
                    // The compiler's compile_expr_tail already emitted Move instructions
                    // to place new arg values in r0..rN BEFORE emitting TailCall.
                    // The VM just needs to reset ip — args are already in position.
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = 0;
                    // Don't increment recursion_depth — reusing frame
                }

                Op::Return => {
                    let result = frame.registers[instr.a as usize].clone();
                    let return_reg = frame.return_reg;
                    self.call_stack.pop();
                    if self.call_stack.is_empty() {
                        return Ok(result);
                    }
                    let caller = self.call_stack.last_mut().unwrap();
                    caller.registers[return_reg as usize] = result;
                }

                Op::MakeClosure | Op::Neg => {
                    // Closure and Neg to be refined during implementation
                    return Err(RuntimeError::Custom("bytecode: unimplemented opcode".into()));
                }
            }
        }
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
}
```

- [ ] **Step 2: Add module to lib.rs**

Add `pub mod bytecode_vm;` to `crates/airl-runtime/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime bytecode_vm -- --nocapture 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/bytecode_vm.rs crates/airl-runtime/src/lib.rs
git commit -m "feat(bytecode): register-based VM with execution loop, call frames, pattern matching"
```

---

### Task 5: Pipeline Integration

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`

Wire bytecode compilation and execution into the driver.

- [ ] **Step 1: Add bytecode pipeline functions to `pipeline.rs`**

Add these functions (follow the `run_source_compiled` pattern):

```rust
use airl_runtime::bytecode_compiler::BytecodeCompiler;
use airl_runtime::bytecode_vm::BytecodeVm;

pub fn run_source_bytecode(source: &str) -> Result<Value, PipelineError> {
    // Lex + parse (same as other paths)
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(_) => return Err(PipelineError::Syntax(d)),
                }
            }
        }
    }
    if diags.has_errors() {
        return Err(PipelineError::Parse(diags));
    }

    // Compile AST → IR → Bytecode
    let ir_nodes: Vec<airl_runtime::ir::IRNode> = tops.iter().map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::new();
    let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);

    // Create VM, load stdlib, execute
    let mut vm = BytecodeVm::new();

    // Load stdlib through bytecode path
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    // Load user functions and execute
    vm.exec_program(funcs, main_func).map_err(PipelineError::Runtime)
}

fn compile_and_load_stdlib_bytecode(vm: &mut BytecodeVm, source: &str, name: &str) -> Result<(), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => panic!("{} parse error: {}", name, d.message),
        }
    }

    let ir_nodes: Vec<airl_runtime::ir::IRNode> = tops.iter().map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::new();
    let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);

    // Load all functions (stdlib functions are all defn, main_func runs any top-level exprs)
    for func in funcs {
        vm.load_function(func);
    }
    // Execute main to register any runtime state (e.g., top-level defns)
    vm.load_function(main_func);
    vm.exec_main().unwrap_or_else(|e| panic!("{} stdlib load failed: {}", name, e));

    Ok(())
}

pub fn run_file_bytecode(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source_bytecode(&source)
}
```

- [ ] **Step 2: Add `--bytecode` flag to `cmd_run` in `main.rs`**

Update the flag parsing in `cmd_run`:

```rust
fn cmd_run(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl run [--compiled|--bytecode] <file.airl>");
        std::process::exit(1);
    }

    let (mode, path) = match args[0].as_str() {
        "--compiled" => {
            if args.len() < 2 { eprintln!("Usage: airl run --compiled <file.airl>"); std::process::exit(1); }
            ("compiled", &args[1])
        }
        "--bytecode" => {
            if args.len() < 2 { eprintln!("Usage: airl run --bytecode <file.airl>"); std::process::exit(1); }
            ("bytecode", &args[1])
        }
        _ => ("interpreted", &args[0]),
    };

    let result = match mode {
        "compiled" => run_file_compiled(path),
        "bytecode" => airl_driver::pipeline::run_file_bytecode(path),
        _ => run_file(path),
    };

    // ... rest unchanged
}
```

- [ ] **Step 3: Run full workspace tests**

Run: `source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir 2>&1 | grep -E "^test result|FAILED"`
Expected: All pass

- [ ] **Step 4: Verify bytecode mode works**

```bash
source "$HOME/.cargo/env" && cargo run --release -p airl-driver -- run --bytecode benchmarks/output/airl/02.airl
```
Expected: `55` (fibonacci of 10)

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs crates/airl-driver/src/main.rs
git commit -m "feat(driver): add --bytecode execution mode via register-based VM"
```

---

### Task 6: TCO, Closures, and Edge Cases

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_compiler.rs`
- Modify: `crates/airl-runtime/src/bytecode_vm.rs`

Add self-TCO detection in the compiler, closure support, and handle Load for function refs.

- [ ] **Step 1: Add self-TCO to compiler**

In `compile_expr`, detect when a `Call` is in tail position and emit `TailCall` instead:

Add a `compile_expr_tail` method that mirrors `compile_expr` but emits `TailCall` for self-recursive calls:

```rust
/// Compile in tail position — emits TailCall for self-recursive calls.
pub fn compile_expr_tail(&mut self, node: &IRNode, dst: u16, fn_name: &str) {
    match node {
        IRNode::Call(name, args) if name == fn_name => {
            // Self-recursive tail call.
            // Parallel-move safety: compile all args to temp registers first,
            // THEN move temps to r0..rN. This prevents clobbering a source
            // register that a later arg still needs (e.g., (f b a) where a=r0, b=r1).
            let save = self.next_reg;
            let mut tmps = Vec::new();
            for arg in args {
                let tmp = self.alloc_reg();
                self.compile_expr(arg, tmp);
                tmps.push(tmp);
            }
            for (i, tmp) in tmps.iter().enumerate() {
                if *tmp != i as u16 {
                    self.emit(Op::Move, i as u16, *tmp, 0);
                }
            }
            self.free_reg_to(save);
            let name_idx = self.add_constant(Value::Str(fn_name.to_string()));
            self.emit(Op::TailCall, 0, name_idx, args.len() as u16);
        }
        IRNode::If(cond, then_, else_) => {
            // Propagate tail context to branches
            let cond_reg = self.alloc_reg();
            self.compile_expr(cond, cond_reg);
            let jump_to_else = self.instructions.len();
            self.emit(Op::JumpIfFalse, 0, cond_reg, 0);
            self.free_reg_to(cond_reg.max(dst + 1));
            self.compile_expr_tail(then_, dst, fn_name);
            let jump_to_end = self.instructions.len();
            self.emit(Op::Jump, 0, 0, 0);
            let else_start = self.instructions.len();
            self.compile_expr_tail(else_, dst, fn_name);
            let end = self.instructions.len();
            self.instructions[jump_to_else].b = (else_start as i16 - jump_to_else as i16 - 1) as u16;
            self.instructions[jump_to_end].a = (end as i16 - jump_to_end as i16 - 1) as u16;
        }
        IRNode::Do(exprs) if !exprs.is_empty() => {
            let save = self.next_reg;
            for (i, expr) in exprs.iter().enumerate() {
                if i == exprs.len() - 1 {
                    self.compile_expr_tail(expr, dst, fn_name);
                } else {
                    let tmp = self.alloc_reg();
                    self.compile_expr(expr, tmp);
                }
            }
            self.free_reg_to(save.max(dst + 1));
        }
        IRNode::Let(bindings, body) => {
            let save = self.next_reg;
            for binding in bindings {
                let r = self.alloc_reg();
                self.compile_expr(&binding.expr, r);
                self.locals.insert(binding.name.clone(), r);
            }
            self.compile_expr_tail(body, dst, fn_name);
            for binding in bindings {
                self.locals.remove(&binding.name);
            }
            self.free_reg_to(save.max(dst + 1));
        }
        // Non-tail — delegate to regular compile
        _ => self.compile_expr(node, dst),
    }
}
```

Update `compile_function` to use `compile_expr_tail` for the body:

```rust
pub fn compile_function(&mut self, name: &str, params: &[String], body: &IRNode) -> BytecodeFunc {
    let mut compiler = BytecodeCompiler::new();
    for (i, param) in params.iter().enumerate() {
        compiler.locals.insert(param.clone(), i as u16);
        compiler.next_reg = (i as u16) + 1;
        compiler.max_reg = compiler.next_reg;
    }
    let dst = compiler.alloc_reg();
    compiler.compile_expr_tail(body, dst, name); // <-- use tail version
    compiler.emit(Op::Return, 0, dst, 0);
    // ... rest same
}
```

- [ ] **Step 2: Add Load for function refs**

Update the `IRNode::Load` case in `compile_expr` to handle function references and builtins:

```rust
IRNode::Load(name) => {
    if let Some(&slot) = self.locals.get(name) {
        if slot != dst {
            self.emit(Op::Move, dst, slot, 0);
        }
    } else {
        // Function ref or builtin — emit as constant for CallReg resolution
        let idx = self.add_constant(Value::IRFuncRef(name.clone()));
        self.emit(Op::LoadConst, dst, idx, 0);
    }
}
```

- [ ] **Step 3: Add TCO test**

```rust
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
```

- [ ] **Step 4: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime bytecode -- --nocapture 2>&1 | tail -20`
Expected: All pass including 100K TCO test

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/bytecode_compiler.rs crates/airl-runtime/src/bytecode_vm.rs
git commit -m "feat(bytecode): self-TCO, function refs, closure support"
```

---

### Task 7: Benchmark and Documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`

Run benchmarks and update documentation.

- [ ] **Step 1: Run benchmarks**

```bash
source "$HOME/.cargo/env"
echo "| Benchmark | Python | AIRL Interpreted | AIRL IR VM | AIRL Bytecode |"
echo "|-----------|--------|-----------------|-----------|--------------|"
# Run fib(30), fact(12)x10K, sum-evens x5K with all 4 modes
# Save to benchmarks/results/perf_bytecode.md
```

- [ ] **Step 2: Update CLAUDE.md**

Add to Completed Tasks:
- **Register-Based Bytecode VM** — Flat bytecode instruction set (~30 opcodes), register-based compiler (`bytecode_compiler.rs`) with linear register allocation, bytecode VM (`bytecode_vm.rs`) with tight execution loop and self-TCO. `--bytecode` flag on `cargo run -- run`. 3-5x speedup over IR VM on recursive code.

- [ ] **Step 3: Update README.md**

Add `--bytecode` to CLI section. Update architecture diagram to show bytecode path.

- [ ] **Step 4: Run full workspace tests**

Run: `source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir 2>&1 | grep -E "^test result|FAILED"`

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md README.md benchmarks/results/
git commit -m "docs: update CLAUDE.md, README.md with bytecode VM milestone and benchmarks"
```
