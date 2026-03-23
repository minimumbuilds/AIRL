# Bytecode→Cranelift JIT Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** JIT-compile eligible bytecode functions to native x86-64 via Cranelift, achieving Python parity on numeric code (fib(30) from ~4,500ms to ~200-400ms).

**Architecture:** `BytecodeJit` in `airl-runtime` translates `BytecodeFunc` instruction arrays to Cranelift IR. Bytecode registers map to Cranelift variables, jumps map to basic blocks. The bytecode VM's `Op::Call` checks the JIT cache before pushing a bytecode call frame — transparent fallback for ineligible functions. All JIT code is behind `#[cfg(feature = "jit")]`.

**Tech Stack:** Rust, Cranelift 0.130 (cranelift-codegen, cranelift-frontend, cranelift-jit, cranelift-module, target-lexicon). No new external dependencies beyond Cranelift.

**Spec:** `docs/superpowers/specs/2026-03-23-bytecode-jit-design.md`

**Reference files:**
- Existing Cranelift JIT: `crates/airl-codegen/src/jit.rs` (~360 lines — JITModule, compile, call_native patterns)
- Existing Cranelift lowerer: `crates/airl-codegen/src/lower.rs` (~310 lines — FunctionBuilder, Variable, block creation patterns)
- Bytecode types: `crates/airl-runtime/src/bytecode.rs` (Op enum, Instruction, BytecodeFunc)
- Bytecode VM: `crates/airl-runtime/src/bytecode_vm.rs` (~500 lines — BytecodeVm, push_frame, run loop)
- Bytecode compiler: `crates/airl-runtime/src/bytecode_compiler.rs` (~930 lines)
- Pipeline: `crates/airl-driver/src/pipeline.rs` (run_source_bytecode pattern)
- CLI: `crates/airl-driver/src/main.rs` (--bytecode flag pattern)
- Value: `crates/airl-runtime/src/value.rs`
- Error: `crates/airl-runtime/src/error.rs`

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `crates/airl-runtime/src/bytecode_jit.rs` | `BytecodeJit`: eligibility check, Cranelift IR emission, compilation cache, native dispatch, marshaling |
| Modify: `crates/airl-runtime/src/bytecode_vm.rs` | Add `#[cfg(feature = "jit")] jit` field, `new_with_jit()`, `jit_compile_all()`, JIT dispatch in `Op::Call` handler |
| Modify: `crates/airl-runtime/src/lib.rs` | Add `#[cfg(feature = "jit")] pub mod bytecode_jit;` |
| Modify: `crates/airl-runtime/Cargo.toml` | Add optional Cranelift deps + `jit` feature |
| Modify: `crates/airl-driver/src/pipeline.rs` | Add `run_source_jit()`, `run_file_jit()` |
| Modify: `crates/airl-driver/src/main.rs` | Add `--jit` flag |
| Modify: `crates/airl-driver/Cargo.toml` | Forward `jit` feature |

---

### Task 1: Feature Flag and Dependencies

**Files:**
- Modify: `crates/airl-runtime/Cargo.toml`
- Modify: `crates/airl-runtime/src/lib.rs`
- Create: `crates/airl-runtime/src/bytecode_jit.rs`
- Modify: `crates/airl-driver/Cargo.toml`

Set up the feature flag, dependencies, and an empty module that compiles.

- [ ] **Step 1: Add Cranelift dependencies to `airl-runtime/Cargo.toml`**

Add after the existing `[features]` section:

```toml
[features]
mlir = ["airl-mlir"]
cuda = ["mlir", "airl-mlir/cuda"]
jit = ["dep:cranelift-codegen", "dep:cranelift-frontend", "dep:cranelift-jit", "dep:cranelift-module", "dep:target-lexicon"]
```

Add to `[dependencies]`:

```toml
cranelift-codegen = { version = "0.130", optional = true }
cranelift-frontend = { version = "0.130", optional = true }
cranelift-jit = { version = "0.130", optional = true }
cranelift-module = { version = "0.130", optional = true }
target-lexicon = { version = "0.13", optional = true }
```

- [ ] **Step 2: Forward feature in `airl-driver/Cargo.toml`**

Add to `[features]`:

```toml
jit = ["airl-runtime/jit"]
```

- [ ] **Step 3: Create empty `bytecode_jit.rs`**

```rust
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
}
```

- [ ] **Step 4: Add module to `lib.rs`**

Add to `crates/airl-runtime/src/lib.rs`:
```rust
#[cfg(feature = "jit")]
pub mod bytecode_jit;
```

- [ ] **Step 5: Build with feature flag**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime --features jit 2>&1 | tail -5`
Expected: Build succeeds (first build pulls Cranelift — may take 1-2 min)

Also verify build without feature:
Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime 2>&1 | tail -5`
Expected: Build succeeds, no Cranelift code compiled

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/Cargo.toml crates/airl-runtime/src/bytecode_jit.rs crates/airl-runtime/src/lib.rs crates/airl-driver/Cargo.toml
git commit -m "feat(jit): add Cranelift dependencies and empty bytecode_jit module behind feature flag"
```

---

### Task 2: Eligibility Check and Marshaling

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit.rs`

Add the eligibility scanner and Value↔u64 marshaling functions.

- [ ] **Step 1: Add eligibility check**

Add to `BytecodeJit` impl:

```rust
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
```

- [ ] **Step 2: Add marshaling functions**

Add as free functions in the module:

```rust
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
```

- [ ] **Step 3: Add `try_call_native`**

```rust
impl BytecodeJit {
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
```

- [ ] **Step 4: Add eligibility tests**

```rust
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
```

- [ ] **Step 5: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode_jit -- --nocapture 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/bytecode_jit.rs
git commit -m "feat(jit): eligibility scanner, marshaling, and try_call_native dispatch"
```

---

### Task 3: Cranelift IR Emission — Literals, Arithmetic, Control Flow

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit.rs`

The core JIT compiler: translate BytecodeFunc instructions to Cranelift IR. This task handles all opcodes except `Call` and `TailCall` (Task 4).

- [ ] **Step 1: Add `try_compile` method with block scanning and IR emission**

Add to `BytecodeJit` impl:

```rust
/// Try to JIT-compile a BytecodeFunc. On success, stores the native function pointer.
/// On failure (ineligible or compilation error), marks the function as ineligible.
pub fn try_compile(&mut self, func: &BytecodeFunc, all_functions: &HashMap<String, BytecodeFunc>) {
    if self.compiled.contains_key(&func.name) || self.ineligible.contains(&func.name) {
        return;
    }
    if !Self::is_eligible(func, all_functions, &self.compiled, &self.ineligible) {
        if std::env::var("AIRL_JIT_DEBUG").is_ok() {
            eprintln!("[jit] skipped {}: ineligible", func.name);
        }
        self.ineligible.insert(func.name.clone());
        return;
    }
    match self.compile_func(func, all_functions) {
        Ok(hint) => {
            if std::env::var("AIRL_JIT_DEBUG").is_ok() {
                eprintln!("[jit] compiled {}: {} bytecode instructions", func.name, func.instructions.len());
            }
            // Pointer was stored in compile_func
            let _ = hint;
        }
        Err(e) => {
            if std::env::var("AIRL_JIT_DEBUG").is_ok() {
                eprintln!("[jit] failed {}: {}", func.name, e);
            }
            self.ineligible.insert(func.name.clone());
        }
    }
}

fn compile_func(&mut self, func: &BytecodeFunc, _all_functions: &HashMap<String, BytecodeFunc>) -> Result<TypeHint, String> {
    // 1. Build signature: all params and return as I64
    let mut sig = self.module.make_signature();
    for _ in 0..func.arity {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));

    // 2. Declare function
    let func_id = self.module
        .declare_function(&func.name, Linkage::Local, &sig)
        .map_err(|e| format!("declare: {}", e))?;

    // 3. Build function body
    let mut ctx = self.module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    let return_hint;
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Pass 1: find basic block boundaries
        let mut block_starts: HashSet<usize> = HashSet::new();
        block_starts.insert(0); // entry block
        for (i, instr) in func.instructions.iter().enumerate() {
            let offset = match instr.op {
                Op::Jump => Some(instr.a as i16),
                Op::JumpIfFalse | Op::JumpIfTrue => Some(instr.b as i16),
                Op::JumpIfNoMatch => Some(instr.a as i16),
                _ => None,
            };
            if let Some(off) = offset {
                let target = (i as i32 + 1 + off as i32) as usize;
                block_starts.insert(target);
                // The instruction after a conditional jump also starts a block
                if instr.op != Op::Jump {
                    block_starts.insert(i + 1);
                }
            }
        }

        // Create Cranelift blocks
        let mut blocks: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
        for &start in &block_starts {
            blocks.insert(start, builder.create_block());
        }
        let entry_block = blocks[&0];
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        // Declare variables for all bytecode registers
        let mut vars: Vec<Variable> = Vec::new();
        let mut type_hints: Vec<TypeHint> = Vec::new();
        for i in 0..func.register_count {
            let var = Variable::new(i as usize);
            builder.declare_var(var, types::I64);
            vars.push(var);
            type_hints.push(TypeHint::Int); // default
        }

        // Bind function parameters to their register variables
        for i in 0..func.arity as usize {
            let param_val = builder.block_params(entry_block)[i];
            builder.def_var(vars[i], param_val);
        }

        // Initialize non-parameter registers to 0
        for i in func.arity as usize..func.register_count as usize {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(vars[i], zero);
        }

        // Track return type hint
        let mut ret_hint: Option<TypeHint> = None;

        // Pass 2: emit Cranelift IR
        for (ip, instr) in func.instructions.iter().enumerate() {
            // Switch to new block if this instruction starts one
            if ip > 0 && block_starts.contains(&ip) {
                let new_block = blocks[&ip];
                // If the previous instruction wasn't a jump/return, fall through
                let prev = &func.instructions[ip - 1];
                if !matches!(prev.op, Op::Jump | Op::Return | Op::TailCall) {
                    builder.ins().jump(new_block, &[]);
                }
                builder.switch_to_block(new_block);
            }

            let dst = instr.dst as usize;
            let a = instr.a as usize;
            let b = instr.b as usize;

            match instr.op {
                Op::LoadConst => {
                    let val = &func.constants[a];
                    match val {
                        Value::Int(n) => {
                            let v = builder.ins().iconst(types::I64, *n);
                            builder.def_var(vars[dst], v);
                            type_hints[dst] = TypeHint::Int;
                        }
                        Value::Float(f) => {
                            let fv = builder.ins().f64const(*f);
                            let iv = builder.ins().bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), fv);
                            builder.def_var(vars[dst], iv);
                            type_hints[dst] = TypeHint::Float;
                        }
                        Value::Bool(bv) => {
                            let v = builder.ins().iconst(types::I64, *bv as i64);
                            builder.def_var(vars[dst], v);
                            type_hints[dst] = TypeHint::Bool;
                        }
                        _ => return Err(format!("unsupported constant type: {}", val)),
                    }
                }
                Op::LoadNil => {
                    let v = builder.ins().iconst(types::I64, 0);
                    builder.def_var(vars[dst], v);
                    type_hints[dst] = TypeHint::Int;
                }
                Op::LoadTrue => {
                    let v = builder.ins().iconst(types::I64, 1);
                    builder.def_var(vars[dst], v);
                    type_hints[dst] = TypeHint::Bool;
                }
                Op::LoadFalse => {
                    let v = builder.ins().iconst(types::I64, 0);
                    builder.def_var(vars[dst], v);
                    type_hints[dst] = TypeHint::Bool;
                }
                Op::Move => {
                    let v = builder.use_var(vars[a]);
                    builder.def_var(vars[dst], v);
                    type_hints[dst] = type_hints[a];
                }

                // Arithmetic — dispatch on type hints
                Op::Add | Op::Sub | Op::Mul => {
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let is_float = type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let result = if is_float {
                        let fa = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), vb);
                        let fr = match instr.op {
                            Op::Add => builder.ins().fadd(fa, fb),
                            Op::Sub => builder.ins().fsub(fa, fb),
                            Op::Mul => builder.ins().fmul(fa, fb),
                            _ => unreachable!(),
                        };
                        builder.ins().bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), fr)
                    } else {
                        match instr.op {
                            Op::Add => builder.ins().iadd(va, vb),
                            Op::Sub => builder.ins().isub(va, vb),
                            Op::Mul => builder.ins().imul(va, vb),
                            _ => unreachable!(),
                        }
                    };
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = if is_float { TypeHint::Float } else { TypeHint::Int };
                }
                Op::Div => {
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let is_float = type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let result = if is_float {
                        let fa = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), vb);
                        let fr = builder.ins().fdiv(fa, fb);
                        builder.ins().bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), fr)
                    } else {
                        builder.ins().sdiv(va, vb)
                    };
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = if is_float { TypeHint::Float } else { TypeHint::Int };
                }
                Op::Mod => {
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let result = builder.ins().srem(va, vb);
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = TypeHint::Int;
                }
                Op::Neg => {
                    let va = builder.use_var(vars[a]);
                    let is_float = type_hints[a] == TypeHint::Float;
                    let result = if is_float {
                        let fa = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), va);
                        let fr = builder.ins().fneg(fa);
                        builder.ins().bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), fr)
                    } else {
                        builder.ins().ineg(va)
                    };
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = type_hints[a];
                }

                // Comparisons
                Op::Eq | Op::Ne => {
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let is_float = type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let cmp = if is_float {
                        let fa = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), vb);
                        let cc = if instr.op == Op::Eq { cranelift_codegen::ir::condcodes::FloatCC::Equal } else { cranelift_codegen::ir::condcodes::FloatCC::NotEqual };
                        builder.ins().fcmp(cc, fa, fb)
                    } else {
                        let cc = if instr.op == Op::Eq { cranelift_codegen::ir::condcodes::IntCC::Equal } else { cranelift_codegen::ir::condcodes::IntCC::NotEqual };
                        builder.ins().icmp(cc, va, vb)
                    };
                    let result = builder.ins().uextend(types::I64, cmp);
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = TypeHint::Bool;
                }
                Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let is_float = type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let cmp = if is_float {
                        let fa = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), vb);
                        use cranelift_codegen::ir::condcodes::FloatCC;
                        let cc = match instr.op {
                            Op::Lt => FloatCC::LessThan,
                            Op::Le => FloatCC::LessThanOrEqual,
                            Op::Gt => FloatCC::GreaterThan,
                            Op::Ge => FloatCC::GreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        builder.ins().fcmp(cc, fa, fb)
                    } else {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let cc = match instr.op {
                            Op::Lt => IntCC::SignedLessThan,
                            Op::Le => IntCC::SignedLessThanOrEqual,
                            Op::Gt => IntCC::SignedGreaterThan,
                            Op::Ge => IntCC::SignedGreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        builder.ins().icmp(cc, va, vb)
                    };
                    let result = builder.ins().uextend(types::I64, cmp);
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = TypeHint::Bool;
                }
                Op::Not => {
                    let va = builder.use_var(vars[a]);
                    let one = builder.ins().iconst(types::I64, 1);
                    let result = builder.ins().isub(one, va);
                    builder.def_var(vars[dst], result);
                    type_hints[dst] = TypeHint::Bool;
                }

                // Control flow
                Op::Jump => {
                    let offset = instr.a as i16;
                    let target = (ip as i32 + 1 + offset as i32) as usize;
                    let target_block = blocks[&target];
                    builder.ins().jump(target_block, &[]);
                }
                Op::JumpIfFalse => {
                    let cond = builder.use_var(vars[a]);
                    let offset = instr.b as i16;
                    let target = (ip as i32 + 1 + offset as i32) as usize;
                    let target_block = blocks[&target];
                    let fallthrough = blocks.get(&(ip + 1)).copied()
                        .unwrap_or_else(|| {
                            let b = builder.create_block();
                            b
                        });
                    // brif branches to first block if nonzero, second if zero
                    // JumpIfFalse = jump if zero, so: brif cond, fallthrough, target
                    builder.ins().brif(cond, fallthrough, &[], target_block, &[]);
                }
                Op::JumpIfTrue => {
                    let cond = builder.use_var(vars[a]);
                    let offset = instr.b as i16;
                    let target = (ip as i32 + 1 + offset as i32) as usize;
                    let target_block = blocks[&target];
                    let fallthrough = blocks.get(&(ip + 1)).copied()
                        .unwrap_or_else(|| {
                            let b = builder.create_block();
                            b
                        });
                    builder.ins().brif(cond, target_block, &[], fallthrough, &[]);
                }

                Op::Return => {
                    let val = builder.use_var(vars[a]);
                    builder.ins().return_(&[val]);
                    // Track return type
                    let hint = type_hints[a];
                    if let Some(existing) = ret_hint {
                        if existing != hint {
                            return Err("inconsistent return types".into());
                        }
                    } else {
                        ret_hint = Some(hint);
                    }
                }

                // Call and TailCall handled in Task 4
                Op::Call | Op::TailCall => {
                    return Err("Call/TailCall not yet implemented".into());
                }

                // These should have been caught by eligibility check
                _ => {
                    return Err(format!("unsupported opcode in JIT: {:?}", instr.op));
                }
            }
        }

        // Seal all blocks
        for &block in blocks.values() {
            builder.seal_block(block);
        }

        builder.finalize();
        return_hint = ret_hint.unwrap_or(TypeHint::Int);
    }

    // 4. Compile
    self.module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("define: {}", e))?;
    self.module.clear_context(&mut ctx);
    self.module.finalize_definitions()
        .map_err(|e| format!("finalize: {}", e))?;

    // 5. Store function pointer
    let ptr = self.module.get_finalized_function(func_id);
    self.compiled.insert(func.name.clone(), (ptr, return_hint));

    Ok(return_hint)
}
```

- [ ] **Step 2: Add compilation + execution test**

```rust
#[test]
fn test_jit_add_ints() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // Compile: (defn add [a b] (+ a b))
    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("add", &["a".into(), "b".into()],
        &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);
    assert!(jit.compiled.contains_key("add"));

    let result = jit.try_call_native("add", &[Value::Int(3), Value::Int(4)]).unwrap().unwrap();
    assert_eq!(result, Value::Int(7));
}

#[test]
fn test_jit_if_expression() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // (defn max2 [a b] (if (> a b) a b))
    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("max2", &["a".into(), "b".into()],
        &IRNode::If(
            Box::new(IRNode::Call(">".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())])),
            Box::new(IRNode::Load("a".into())),
            Box::new(IRNode::Load("b".into())),
        ));

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);
    assert!(jit.compiled.contains_key("max2"));

    let r1 = jit.try_call_native("max2", &[Value::Int(10), Value::Int(3)]).unwrap().unwrap();
    assert_eq!(r1, Value::Int(10));
    let r2 = jit.try_call_native("max2", &[Value::Int(2), Value::Int(8)]).unwrap().unwrap();
    assert_eq!(r2, Value::Int(8));
}

#[test]
fn test_jit_let_binding() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // (defn sq_plus1 [x] (let (y (+ x 1)) (* y y)))
    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("sq_plus1", &["x".into()],
        &IRNode::Let(
            vec![IRBinding { name: "y".into(), expr: IRNode::Call("+".into(), vec![IRNode::Load("x".into()), IRNode::Int(1)]) }],
            Box::new(IRNode::Call("*".into(), vec![IRNode::Load("y".into()), IRNode::Load("y".into())])),
        ));

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);

    let r = jit.try_call_native("sq_plus1", &[Value::Int(4)]).unwrap().unwrap();
    assert_eq!(r, Value::Int(25)); // (4+1)^2 = 25
}

#[test]
fn test_jit_ineligible_skipped() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // Function with MakeList — should be skipped
    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("make_list", &[],
        &IRNode::List(vec![IRNode::Int(1), IRNode::Int(2)]));

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);
    assert!(!jit.compiled.contains_key("make_list"));
    assert!(jit.ineligible.contains("make_list"));
}
```

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode_jit -- --nocapture 2>&1 | tail -15`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/bytecode_jit.rs
git commit -m "feat(jit): Cranelift IR emission for literals, arithmetic, comparisons, control flow"
```

---

### Task 4: Call and TailCall Support

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit.rs`

Add self-recursive `Call` (Cranelift direct call to self) and `TailCall` (loop-back to entry block).

- [ ] **Step 1: Replace Call/TailCall stubs with full implementation**

**Borrow checker constraint:** `self.module.declare_func_in_func(func_id, builder.func)` can only be called inside the `FunctionBuilder` scope. But `self.module.declare_function(...)` must be called **before** the builder scope (it mutably borrows `self.module` while `ctx.func` is also borrowed by the builder). The solution: `func_id` for self-calls is already declared before the builder scope. For cross-function calls, pre-declare all callee `FuncId`s before entering the builder.

**Step 1a:** Pre-declare callee function IDs before the builder scope. In `compile_func`, add this **before** `let mut builder_ctx = FunctionBuilderContext::new();`:

```rust
// Pre-declare function references for Call targets (before builder scope to avoid borrow conflicts)
let mut call_targets: HashMap<String, cranelift_module::FuncId> = HashMap::new();
for instr in &func.instructions {
    if instr.op == Op::Call {
        if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
            if callee_name != &func.name && !call_targets.contains_key(callee_name) {
                let argc = instr.b as usize;
                let mut call_sig = self.module.make_signature();
                for _ in 0..argc {
                    call_sig.params.push(AbiParam::new(types::I64));
                }
                call_sig.returns.push(AbiParam::new(types::I64));
                let callee_id = self.module
                    .declare_function(callee_name, Linkage::Import, &call_sig)
                    .map_err(|e| format!("call declare: {}", e))?;
                call_targets.insert(callee_name.clone(), callee_id);
            }
        }
    }
}
```

**Step 1b:** Inside the builder scope, replace the `Op::Call | Op::TailCall` arm with:

```rust
Op::Call => {
    let callee_name = match &func.constants[instr.a as usize] {
        Value::Str(s) => s.clone(),
        _ => return Err("call: func name must be string".into()),
    };
    let argc = instr.b as usize;

    // Get the FuncRef — self-call uses func_id (already declared), cross-call uses pre-declared target
    let callee_func_id = if callee_name == func.name {
        func_id
    } else if let Some(&id) = call_targets.get(&callee_name) {
        id
    } else {
        return Err(format!("call target '{}' not declared", callee_name));
    };
    let func_ref = self.module.declare_func_in_func(callee_func_id, builder.func);

    let mut call_args = Vec::new();
    for i in 0..argc {
        let arg = builder.use_var(vars[instr.dst as usize + 1 + i]);
        call_args.push(arg);
    }
    let call = builder.ins().call(func_ref, &call_args);
    let result = builder.inst_results(call)[0];
    builder.def_var(vars[dst], result);
}
Op::TailCall => {
    // Self-recursive tail call — jump back to entry block.
    // The bytecode compiler emits Move instructions before TailCall to place
    // new arg values into r0..rN. Those Moves are already compiled by earlier
    // iterations of this loop, so the parameter variables already hold the
    // correct values. We just need to jump back.
    let callee_name = match &func.constants[instr.a as usize] {
        Value::Str(s) => s.clone(),
        _ => return Err("tailcall: func name must be string".into()),
    };
    if callee_name != func.name {
        return Err(format!("cross-function TailCall to '{}' not supported", callee_name));
    }
    builder.ins().jump(entry_block, &[]);
}
```

**Note:** `self.module.declare_func_in_func(...)` is allowed inside the builder scope because it borrows `self.module` immutably (it's a read operation) while `builder.func` is a separate mutable reference to `ctx.func`. The mutable borrow conflict only applies to `declare_function` (which modifies the module's declaration tables).

- [ ] **Step 2: Add recursion and TailCall tests**

```rust
#[test]
fn test_jit_factorial_recursive() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // (defn fact [n] (if (<= n 1) 1 (* n (fact (- n 1)))))
    let body = IRNode::If(
        Box::new(IRNode::Call("<=".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)])),
        Box::new(IRNode::Int(1)),
        Box::new(IRNode::Call("*".into(), vec![
            IRNode::Load("n".into()),
            IRNode::Call("fact".into(), vec![
                IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
            ]),
        ])),
    );

    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("fact", &["n".into()], &body);

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);
    assert!(jit.compiled.contains_key("fact"), "factorial should be JIT-eligible");

    let r = jit.try_call_native("fact", &[Value::Int(5)]).unwrap().unwrap();
    assert_eq!(r, Value::Int(120));
}

#[test]
fn test_jit_tailcall_no_overflow() {
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    // (defn countdown [n] (if (= n 0) 0 (countdown (- n 1))))
    let body = IRNode::If(
        Box::new(IRNode::Call("=".into(), vec![IRNode::Load("n".into()), IRNode::Int(0)])),
        Box::new(IRNode::Int(0)),
        Box::new(IRNode::Call("countdown".into(), vec![
            IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
        ])),
    );

    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function("countdown", &["n".into()], &body);

    let all_funcs = HashMap::new();
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all_funcs);
    assert!(jit.compiled.contains_key("countdown"));

    // 100K iterations — would overflow stack without TailCall→loop
    let r = jit.try_call_native("countdown", &[Value::Int(100_000)]).unwrap().unwrap();
    assert_eq!(r, Value::Int(0));
}
```

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode_jit -- --nocapture 2>&1 | tail -15`
Expected: All tests pass including recursion and TailCall

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/bytecode_jit.rs
git commit -m "feat(jit): self-recursive Call and TailCall-as-loop support"
```

---

### Task 5: VM Integration

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_vm.rs`

Wire the JIT into the bytecode VM.

- [ ] **Step 1: Add JIT field and constructor**

In `bytecode_vm.rs`, update the struct and add `new_with_jit`:

```rust
pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
    #[cfg(feature = "jit")]
    jit: Option<crate::bytecode_jit::BytecodeJit>,
}
```

Update `new()` to initialize jit to `None`:

```rust
pub fn new() -> Self {
    BytecodeVm {
        functions: HashMap::new(),
        builtins: Builtins::new(),
        call_stack: Vec::new(),
        recursion_depth: 0,
        #[cfg(feature = "jit")]
        jit: None,
    }
}
```

Add:

```rust
#[cfg(feature = "jit")]
pub fn new_with_jit() -> Self {
    BytecodeVm {
        functions: HashMap::new(),
        builtins: Builtins::new(),
        call_stack: Vec::new(),
        recursion_depth: 0,
        jit: crate::bytecode_jit::BytecodeJit::new().ok(),
    }
}

#[cfg(feature = "jit")]
pub fn jit_compile_all(&mut self) {
    if let Some(ref mut jit) = self.jit {
        let names: Vec<String> = self.functions.keys().cloned().collect();
        for name in &names {
            if let Some(func) = self.functions.get(name) {
                jit.try_compile(func, &self.functions);
            }
        }
    }
}
```

- [ ] **Step 2: Add JIT dispatch in the `Op::Call` handler**

In the `run()` method, in the `Op::Call` arm, add JIT dispatch before the builtin check. After extracting `name` and `args`, add:

```rust
// Try JIT first
#[cfg(feature = "jit")]
if let Some(ref jit) = self.jit {
    if let Some(result) = jit.try_call_native(&name, &args) {
        match result {
            Ok(val) => {
                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                continue; // skip bytecode dispatch for this call
            }
            Err(e) => return Err(e),
        }
    }
}
```

- [ ] **Step 3: Build with and without feature**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime --features jit 2>&1 | tail -3`
Expected: Build succeeds

Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime 2>&1 | tail -3`
Expected: Build succeeds (no JIT code compiled)

- [ ] **Step 4: Run all existing bytecode tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode -- --nocapture 2>&1 | tail -5`
Expected: All 32+ bytecode tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/bytecode_vm.rs
git commit -m "feat(jit): wire BytecodeJit into BytecodeVm with Op::Call dispatch"
```

---

### Task 6: Pipeline and CLI Integration

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`

Add `--jit` execution mode.

- [ ] **Step 1: Add JIT pipeline functions to `pipeline.rs`**

Add functions (follow the `run_source_bytecode` pattern exactly, but use `new_with_jit` and call `jit_compile_all`). No new imports needed — `BytecodeCompiler` is already imported unconditionally at the top of `pipeline.rs`:

```rust
#[cfg(feature = "jit")]
pub fn run_source_jit(source: &str) -> Result<Value, PipelineError> {
    // Same lex/parse/IR compile as run_source_bytecode
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

    let ir_nodes: Vec<airl_runtime::ir::IRNode> = tops.iter().map(compile_top_level).collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix("user");
    let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);

    // Create JIT-enabled VM
    let mut vm = airl_runtime::bytecode_vm::BytecodeVm::new_with_jit();

    // Load stdlib
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        compile_and_load_stdlib_bytecode(&mut vm, src, name)?;
    }

    // Load user functions
    for func in funcs {
        vm.load_function(func);
    }
    vm.load_function(main_func);

    // Two-pass: compile all loaded functions, then execute
    vm.jit_compile_all();
    vm.exec_main().map_err(PipelineError::Runtime)
}

#[cfg(feature = "jit")]
pub fn run_file_jit(path: &str) -> Result<Value, PipelineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipelineError::Io(e.to_string()))?;
    run_source_jit(&source)
}
```

- [ ] **Step 2: Add `--jit` flag to `cmd_run` in `main.rs`**

Update the mode matching to include `--jit`:

```rust
"--jit" => {
    if args.len() < 2 { eprintln!("Usage: airl run --jit <file.airl>"); std::process::exit(1); }
    ("jit", &args[1])
}
```

And in the dispatch:

```rust
#[cfg(feature = "jit")]
"jit" => run_file_jit(path),
#[cfg(not(feature = "jit"))]
"jit" => {
    eprintln!("JIT not available: rebuild with --features jit");
    std::process::exit(1);
}
```

Add `run_file_jit` to imports:

```rust
#[cfg(feature = "jit")]
use airl_driver::pipeline::run_file_jit;
```

- [ ] **Step 3: Build and test**

Run: `source "$HOME/.cargo/env" && cargo build --release --features jit -p airl-driver 2>&1 | tail -3`
Expected: Build succeeds

Quick smoke test:
```bash
echo '(+ 21 21)' > /tmp/jit_test.airl
source "$HOME/.cargo/env" && cargo run --features jit -p airl-driver -- run --jit /tmp/jit_test.airl
```
Expected: `42`

- [ ] **Step 4: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs crates/airl-driver/src/main.rs
git commit -m "feat(driver): add --jit execution mode (bytecode + Cranelift JIT)"
```

---

### Task 7: Fixture Tests, Benchmarks, Documentation

**Files:**
- Modify: `CLAUDE.md`
- Create: `benchmarks/results/perf_2026-03-23_jit.md`

Run the full fixture suite and benchmarks through `--jit` mode.

- [ ] **Step 1: Run fixture compatibility test**

```bash
source "$HOME/.cargo/env" && cargo build --release --features jit -p airl-driver
FIXTURES=tests/fixtures/valid
PASS=0; FAIL=0; SKIP=0
for f in "$FIXTURES"/*.airl; do
  name=$(basename "$f" .airl)
  case "$name" in execute_on_gpu|mlir_tensor|jit_arithmetic|lexer_bootstrap|contracts|invariant|float_contract|forall_contract|forall_expr|exists_expr|proven_contracts|quantifier_proven) SKIP=$((SKIP+1)); continue ;; esac
  interp=$(RUST_MIN_STACK=67108864 timeout 10 target/release/airl-driver run "$f" 2>/dev/null) || { SKIP=$((SKIP+1)); continue; }
  jit=$(timeout 10 target/release/airl-driver run --jit "$f" 2>&1)
  if [ $? -eq 0 ] && [ "$interp" = "$jit" ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); echo "FAIL: $name"; fi
done
echo "PASS: $PASS FAIL: $FAIL SKIP: $SKIP"
```
Expected: 26/26 pass (JIT is transparent — ineligible functions fall back to bytecode)

- [ ] **Step 2: Run benchmarks**

```bash
source "$HOME/.cargo/env"
# Create benchmark files if needed (same as previous benchmarks)
# Run fib(30), fact(12)x10K, sum-evens x5K in --jit mode and compare with Python + bytecode
```

Record results in `benchmarks/results/perf_2026-03-23_jit.md`.

Expected results:
- fib(30): ~200-400ms (vs Python 302ms, bytecode 4,572ms)
- fact(12)x10K: ~40-60ms (vs Python 52ms, bytecode 159ms)
- sum-evens x5K: ~800ms (unchanged — not JIT-eligible)

- [ ] **Step 3: Update CLAUDE.md**

Add to Completed Tasks:
```markdown
- **Bytecode→Cranelift JIT** — JIT compilation of eligible bytecode functions to native x86-64 via Cranelift (`bytecode_jit.rs`). Primitive-typed functions (no lists/variants/closures) are compiled eagerly at load time. `--jit` flag on `cargo run --features jit -- run`. Transparent fallback to bytecode for ineligible functions. Python parity on numeric code (fib(30) ~Xms vs Python 302ms).
```

- [ ] **Step 4: Run full workspace tests**

Run: `source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir 2>&1 | grep -E "^test result|FAILED"`
Expected: All pass (JIT code is feature-gated, doesn't affect default builds)

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md benchmarks/results/perf_2026-03-23_jit.md
git commit -m "docs: add Cranelift JIT benchmarks and update CLAUDE.md"
```
