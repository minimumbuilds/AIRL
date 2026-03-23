# Bytecode→Cranelift JIT Design

**Date:** 2026-03-23
**Status:** Draft
**Scope:** JIT compilation of eligible bytecode functions to native x86-64 via Cranelift, targeting Python parity on numeric code

## Overview

A JIT compiler that translates `BytecodeFunc` instruction sequences to native machine code via Cranelift. Eligible functions (primitive-typed, no list/variant/closure operations) are compiled eagerly at load time. The bytecode VM transparently dispatches to native code when available, falling back to bytecode interpretation for ineligible functions. Designed so List/Variant/closure support can be added as a Phase 2 extension.

## Goals

1. **Python parity on numeric code** — fib(30) from ~4,500ms to ~200-400ms (Python: 302ms)
2. **Transparent acceleration** — `--jit` flag, no source changes required, all existing programs work
3. **Zero overhead when disabled** — behind `#[cfg(feature = "jit")]`, compiled out entirely without the feature flag
4. **Phase 2 extensible** — architecture supports adding runtime helper calls for non-primitive operations later

## Non-Goals

- JIT-compiling functions that use Lists, Variants, closures, or pattern matching (Phase 2)
- Call-count-based tiered compilation (future optimization)
- Replacing the bytecode VM (JIT is an acceleration layer, bytecode remains the fallback)
- Self-hosted JIT compiler in AIRL

## Architecture

### Pipeline Position

```
Source → Lex → Parse → IR Compile → Bytecode Compile → JIT Compile → Native Execution
                                         ↓                   ↓              ↓
                                    BytecodeFunc         fn pointer    direct x86 call
                                         ↓
                                    bytecode_vm.run()  (fallback)
```

The JIT compiler takes `BytecodeFunc` as input — the same flat instruction arrays the bytecode VM executes. This means both the Rust-side IR compiler and the AIRL-side bootstrap compiler feed into it automatically.

### Why Compile from Bytecode, Not AST or IR?

The existing `airl-codegen::JitCache` compiles from AST (`FnDef`), which requires threading parsed AST nodes through the bytecode pipeline. Compiling from bytecode instead:

1. **Natural mapping** — bytecode registers → Cranelift variables, bytecode jumps → Cranelift blocks
2. **Single source of truth** — the bytecode is what actually executes; JIT produces the same semantics
3. **No AST dependency** — the bytecode pipeline doesn't need to carry AST nodes for JIT purposes
4. **Phase 2 ready** — bytecode opcodes for List/Variant operations can be lowered to runtime helper calls

### Dependency Structure

```
airl-codegen (existing, unchanged)
    ↓
airl-runtime
    ├── bytecode.rs (types — unchanged)
    ├── bytecode_compiler.rs (IR→bytecode — unchanged)
    ├── bytecode_vm.rs (execution — minor: JIT dispatch in call_function)
    └── bytecode_jit.rs (NEW — Cranelift IR emission + compilation cache)
```

Cranelift crates are added as dependencies of `airl-runtime` behind a feature flag. This breaks the "zero external deps for core crates" convention but is justified: the JIT is optional (feature-gated), and the alternative (moving bytecode types to a shared crate) adds structural complexity for no benefit.

## Eligibility

A `BytecodeFunc` is JIT-eligible if its instruction stream contains **none** of the following disqualifying opcodes:

| Disqualifying Opcode | Why |
|----------------------|-----|
| `MakeList` | Creates `Value::List` — not representable as i64 |
| `MakeVariant` / `MakeVariant0` | Creates `Value::Variant` |
| `MakeClosure` | Creates `Value::BytecodeClosure` |
| `MatchTag` / `JumpIfNoMatch` / `MatchWild` | Pattern matching on variants |
| `TryUnwrap` | Error handling on Result variants |
| `CallBuiltin` | Calls runtime builtins that operate on non-primitive values |
| `CallReg` | Calls a closure or function reference in a register — implies non-primitive callable values |

If none of these opcodes appear, all values flowing through the function must be primitives (Int, Float, Bool) — there's no way to create or inspect non-primitive values without them.

**Marshal-time safety net:** Even if a function passes the opcode eligibility check, `marshal_arg` returns `None` for non-primitive `Value`s (e.g., `Value::Str`). This means a function that receives string arguments at runtime will transparently fall back to bytecode dispatch — the JIT path is never entered with incompatible types. This is the final guard against type-confused JIT execution (e.g., a function containing only `Add` that happens to receive strings instead of ints).

`Call` to other functions is allowed **only if** the target is also JIT-eligible. During eager compilation, functions are compiled in dependency order: if `f` calls `g`, `g` must be compiled first. If `g` is ineligible, `f` is also ineligible.

### Type Inference

The bytecode carries no type annotations, but the JIT needs to know whether to emit integer or float instructions. Solution: **type propagation during Cranelift IR emission**.

Each Cranelift variable tracks a `TypeHint`:

```rust
enum TypeHint {
    Int,    // i64 — default
    Float,  // f64 stored as i64 bit pattern
    Bool,   // i8 widened to i64
}
```

Propagation rules:
- `LoadConst` with `Value::Int` → Int; `Value::Float` → Float; `Value::Bool` → Bool
- `LoadTrue` / `LoadFalse` → Bool
- Arithmetic: if either operand is Float → Float result (emit `fadd`/`fsub`/etc. with bitcasts); both Int → Int result (emit `iadd`/`isub`/etc.)
- Comparisons: always produce Bool; choose `icmp` vs `fcmp` based on operand types
- `Move`: inherits source type
- Parameters: inferred as Int by default; refined if a float constant is passed at a call site

### Return Type Inference

The return type is inferred by tracking the type of the value in the `Return` instruction's source register. If multiple `Return` instructions exist (e.g., in if/else branches), they must agree. If they disagree, the function is ineligible (conservative — avoids incorrect marshaling).

## Cranelift IR Emission

### Register Mapping

Each bytecode register `r0..rN` maps to a Cranelift `Variable(0)..Variable(N)`. All variables are declared as Cranelift type `I64` (the uniform ABI type). Float values are stored as their IEEE 754 bit pattern via `bitcast`.

### Basic Block Construction

**Pass 1 — find block boundaries:** Scan the instruction array for jump targets. Any instruction index that is the destination of a `Jump`, `JumpIfFalse`, `JumpIfTrue`, or `JumpIfNoMatch` starts a new basic block. Index 0 is always the entry block.

**Jump target calculation:** The bytecode VM increments `ip` **before** applying the jump offset. So a jump instruction at index `i` with offset `o` targets instruction `i + 1 + o` (where `o` is a signed i16 cast from u16). The block boundary scanner must use this `i + 1 + offset` formula to compute correct targets. Getting this wrong produces silent control-flow mismatches between JIT and bytecode.

**Pass 2 — emit IR:** Walk instructions linearly. When crossing a block boundary, seal the previous block and switch to the new one. Emit Cranelift instructions for each bytecode opcode.

### Opcode Translation

| Bytecode Op | Cranelift IR | Notes |
|-------------|-------------|-------|
| `LoadConst` (int) | `iconst.i64 value` | Direct constant |
| `LoadConst` (float) | `f64const value` then `bitcast.i64` | Store as i64 bit pattern |
| `LoadTrue` | `iconst.i64 1` | |
| `LoadFalse` / `LoadNil` | `iconst.i64 0` | |
| `Move` | `def_var(dst, use_var(src))` | Variable assignment |
| `Add` (int+int) | `iadd` | |
| `Add` (float+float) | `bitcast.f64` both, `fadd`, `bitcast.i64` | |
| `Sub/Mul` | `isub/imul` or `fsub/fmul` | Same int/float dispatch |
| `Div` (int) | `sdiv` | Signed division |
| `Div` (float) | `fdiv` | |
| `Mod` | `srem` | Int only |
| `Neg` (int) | `ineg` | |
| `Neg` (float) | `fneg` with bitcasts | |
| `Eq` (int) | `icmp eq` → `uextend.i64` | `icmp` returns `i8`, widened to i64 |
| `Ne/Lt/Le/Gt/Ge` | `icmp`/`fcmp` variants | Same pattern |
| `Not` | `iconst 1`, `isub` (1 - val) | Flip boolean |
| `Jump` | `jump block_target` | Resolved from offset |
| `JumpIfFalse` | `brif val, block_next, block_target` | Note: brif branches on nonzero, so condition is inverted |
| `JumpIfTrue` | `brif val, block_target, block_next` | |
| `Call` (JIT'd target) | `call fn_ref, [args...]` | Direct native call |
| `TailCall` | Verify self-call, move args → param vars, `jump entry_block` | See TailCall section below |
| `Return` | `return val` | |

### TailCall Handling

The `TailCall` opcode stores a `func_idx` (constant pool index to the callee name string). The bytecode compiler only emits `TailCall` for **self-recursive** calls (verified in `compile_expr_tail` where `name == fn_name`). The JIT must verify this:

1. Decode `func_idx` from `instr.a`, look up the callee name in the function's constant pool
2. If the callee name equals the current function name → emit the loop-back pattern (see below)
3. If the callee name differs → **the function is ineligible** (mutual tail-call optimization requires trampolines, which is Phase 2)

**Self-TailCall as Loop:**

Cranelift doesn't have a tail-call instruction. Instead, the function body is emitted inside an implicit loop structure:

```
entry_block(param0, param1, ...):
    ; function body
    ; ...
    ; TailCall (self-call) becomes:
    def_var(0, new_arg0)
    def_var(1, new_arg1)
    jump entry_block
```

The entry block's parameters become loop-carried values. This matches the bytecode VM's behavior (reset ip to 0, reuse registers).

### Call to Other JIT'd Functions

When a `Call` opcode targets a function that has been JIT-compiled, emit a Cranelift `call` instruction to the native function's `FuncRef`. The function is declared as an import in the current function's Cranelift module with signature `(i64, i64, ...) -> i64`.

If the target is not JIT-compiled, the calling function is ineligible (conservative — Phase 2 can add trampolines back to the bytecode VM).

## Marshaling

Reuses the `RawValue` pattern from `airl-codegen::marshal`:

**Value → native (before call):**
```rust
fn marshal_arg(val: &Value) -> Option<u64> {
    match val {
        Value::Int(n) => Some(*n as u64),
        Value::Float(f) => Some(f.to_bits()),
        Value::Bool(b) => Some(*b as u64),
        _ => None,  // ineligible
    }
}
```

**Native → Value (after call):**
```rust
fn unmarshal_result(raw: u64, hint: TypeHint) -> Value {
    match hint {
        TypeHint::Int => Value::Int(raw as i64),
        TypeHint::Float => Value::Float(f64::from_bits(raw)),
        TypeHint::Bool => Value::Bool(raw != 0),
    }
}
```

If any argument fails to marshal (non-primitive `Value`), the JIT path is skipped and the call falls through to bytecode.

## BytecodeJit Struct

```rust
pub struct BytecodeJit {
    module: JITModule,
    /// Compiled function cache: name → (function pointer, return type hint)
    compiled: HashMap<String, (*const u8, TypeHint)>,
    /// Functions known to be ineligible — skip on future lookups
    ineligible: HashSet<String>,
}
```

### Key Methods

**`try_compile(&mut self, func: &BytecodeFunc, all_functions: &HashMap<String, BytecodeFunc>)`**
- Check eligibility (opcode scan + dependency check)
- If ineligible, insert into `self.ineligible`, return
- Build Cranelift function signature: `(I64, I64, ...) -> I64` (one I64 per param + return)
- Create `FunctionBuilder`, declare variables for each bytecode register
- Pass 1: scan for jump targets, create Cranelift blocks
- Pass 2: emit Cranelift IR for each instruction
- Finalize, compile, store function pointer + return type hint

**`try_call_native(&self, name: &str, args: &[Value]) -> Option<Result<Value, RuntimeError>>`**
- Look up in `self.compiled`; return `None` if not found
- Marshal args; return `None` if any arg is non-primitive
- `unsafe` call through function pointer (dispatch on arity 0-8)
- If arity > 8, return `None` (fall back to bytecode — functions with >8 params are rare in numeric code)
- Unmarshal result using stored `TypeHint`
- Return `Some(Ok(value))`

**Arity limit:** Functions with more than 8 parameters are not JIT-compiled. The `unsafe` call dispatch uses a match on arity to transmute the function pointer to the correct typed signature (`fn(u64) -> u64`, `fn(u64, u64) -> u64`, etc.). Supporting arbitrary arity would require a calling-convention abstraction (varargs or indirect call). 8 params covers all practical numeric functions.

## VM Integration

### BytecodeVm Changes

```rust
pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
    #[cfg(feature = "jit")]
    jit: Option<BytecodeJit>,
}
```

**`load_function`** — unchanged, just inserts into the function table.

**`jit_compile_all`** — new method, called **after** all functions are loaded (two-pass approach). This ensures inter-function dependencies are resolved correctly:

```rust
pub fn jit_compile_all(&mut self) {
    #[cfg(feature = "jit")]
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

**`call_function`** — check JIT cache before bytecode dispatch:

```rust
// At the top of call_function, before builtin check:
#[cfg(feature = "jit")]
if let Some(ref jit) = self.jit {
    if let Some(result) = jit.try_call_native(name, args) {
        return result;
    }
}
```

### Pipeline Changes

New function `run_source_jit()` in `pipeline.rs` — identical to `run_source_bytecode()` but constructs `BytecodeVm::new_with_jit()`. After loading all stdlib and user functions, calls `vm.jit_compile_all()` before `vm.exec_main()`. This two-pass approach (load all, then compile all) ensures inter-function dependencies are resolved correctly.

### CLI

`--jit` flag added to `cmd_run` in `main.rs`:

```
cargo run --features jit -- run --jit file.airl
```

`--jit` implies the bytecode pipeline with JIT enabled.

## Feature Flag

`airl-runtime/Cargo.toml`:
```toml
[features]
default = []
jit = ["dep:cranelift-codegen", "dep:cranelift-frontend", "dep:cranelift-jit", "dep:cranelift-module", "dep:target-lexicon"]

[dependencies]
cranelift-codegen = { version = "0.130", optional = true }
cranelift-frontend = { version = "0.130", optional = true }
cranelift-jit = { version = "0.130", optional = true }
cranelift-module = { version = "0.130", optional = true }
target-lexicon = { version = "0.13", optional = true }
```

`airl-driver/Cargo.toml`:
```toml
[features]
jit = ["airl-runtime/jit"]
```

All JIT code is gated with `#[cfg(feature = "jit")]`. Without the feature, the bytecode VM is identical to today — zero overhead, zero new dependencies.

## Debug Output

When `AIRL_JIT_DEBUG=1` is set:

```
[jit] compiled fibonacci: 14 bytecode instructions → 82 bytes native
[jit] compiled fact-helper: 10 bytecode instructions → 64 bytes native
[jit] skipped run-evens: contains MakeList at instruction 12
[jit] skipped __main__: contains Call to non-JIT'd function "print"
```

## Testing Strategy

### Unit Tests (in `bytecode_jit.rs`)

- Eligibility: functions with/without disqualifying opcodes
- Int arithmetic: add, sub, mul, div, mod — compile and call, verify result
- Float arithmetic: same operations with f64 constants
- Comparisons: int and float, verify Bool results
- Control flow: if/else branches, verify correct path taken
- Recursion: factorial with `Call` to self
- TailCall: count-down with TailCall, verify loop-back works (100K iterations, no stack overflow)
- Type propagation: mixed int/float detection, verify correct IR emission
- Marshaling round-trip: Int/Float/Bool through marshal → native → unmarshal

### Integration Tests

Run all 26 fixture tests through `--jit` mode. All must produce identical output to `--bytecode` mode. The JIT is transparent — it should accelerate eligible functions without changing semantics.

### Benchmarks

Re-run the three stress tests with all modes:

| Benchmark | Python | Interpreted | IR VM | Bytecode | JIT (expected) |
|-----------|--------|------------|-------|----------|---------------|
| fib(30) | 302ms | 42,006ms | 7,355ms | 4,572ms | ~200-400ms |
| fact(12)x10K | 52ms | 829ms | 161ms | 159ms | ~40-60ms |
| sum-evens x5K | 52ms | 3,497ms | 1,175ms | 819ms | ~800ms (not eligible) |

## Phase 2: Extending Beyond Primitives

The architecture supports future extension to handle non-primitive operations by emitting Cranelift `call` instructions to runtime helper functions:

| Operation | Runtime Helper | Signature |
|-----------|---------------|-----------|
| `MakeList` | `extern "C" fn airl_make_list(items: *const u64, count: u64) -> u64` | Returns opaque handle |
| `CallBuiltin` | `extern "C" fn airl_call_builtin(name: *const u8, args: *const u64, argc: u64) -> u64` | Trampoline to builtins |
| `MakeVariant` | `extern "C" fn airl_make_variant(tag: *const u8, inner: u64) -> u64` | Returns opaque handle |
| `MatchTag` | `extern "C" fn airl_match_tag(val: u64, tag: *const u8) -> u64` | Returns inner or sentinel |

These helpers would be registered with Cranelift's `JITModule` as importable symbols. The JIT-compiled function calls them like any other function. Values would be passed as opaque `u64` handles (pointers to heap-allocated `Value`s or inline primitives with tag bits).

This is the standard approach used by LuaJIT, V8, and other production JITs. The architecture we're building in Phase 1 (function-pointer dispatch, marshaling layer, eligibility checking) is the foundation these extensions plug into.

## File Structure

| File | Responsibility | Est. Lines |
|------|---------------|-----------|
| Create: `crates/airl-runtime/src/bytecode_jit.rs` | `BytecodeJit`: eligibility, Cranelift IR emission, compilation cache, native dispatch | ~500 |
| Modify: `crates/airl-runtime/src/bytecode_vm.rs` | Add `jit` field, JIT dispatch in `call_function`, `new_with_jit()` | ~30 |
| Modify: `crates/airl-runtime/src/lib.rs` | Add `#[cfg(feature = "jit")] pub mod bytecode_jit;` | ~2 |
| Modify: `crates/airl-runtime/Cargo.toml` | Add optional Cranelift dependencies + `jit` feature | ~10 |
| Modify: `crates/airl-driver/src/pipeline.rs` | Add `run_source_jit()`, `run_file_jit()` | ~40 |
| Modify: `crates/airl-driver/src/main.rs` | Add `--jit` flag | ~10 |
| Modify: `crates/airl-driver/Cargo.toml` | Forward `jit` feature | ~3 |
