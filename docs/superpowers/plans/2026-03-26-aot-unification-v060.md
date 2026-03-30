# v0.6.0 AOT Unification — Single Value Path

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the dual-value architecture (`Value` + `RtValue`) with a single representation (`RtValue`) used by both the bytecode VM and AOT/JIT-compiled code. Eliminate the 27-builtin gap. Unblock G3 self-compilation fixpoint.

**Architecture:** The bytecode VM's register bank changes from `Vec<Value>` to `Vec<*mut RtValue>`. `CallBuiltin` calls the same `extern "C"` functions that AOT uses. The `Value` enum and `builtins.rs` (3000 lines of duplicate implementations) are deleted. The `airl-rt` crate's 190 `extern "C"` functions become the single source of truth.

**Tech Stack:** Rust, Cranelift (unchanged), `airl-rt` crate (existing)

---

## Current State

- `Value` enum (409 lines, 20+ variants) — used by bytecode VM, parser, type checker
- `RtValue` struct (319 lines, 10 variants) — used by JIT/AOT native code
- `builtins.rs` (3000 lines) — 188 builtins as `fn(&[Value]) -> Result<Value>`
- `airl-rt/` (3000+ lines) — 190 `extern "C"` functions as `fn(*mut RtValue, ...) -> *mut RtValue`
- `bytecode_jit_full.rs` — marshals `Value` ↔ `RtValue` at every JIT boundary (lines 972-1093)
- 27 builtins missing from AOT path, `fold`/`map`/`filter` intentionally excluded due to closure convention mismatch

## Target State

- `RtValue` is the only value type. Used everywhere: VM registers, constants, function args/returns.
- `extern "C"` functions in `airl-rt` are the only builtin implementations.
- VM `CallBuiltin` dispatches to the same functions AOT uses.
- No `Value` ↔ `RtValue` marshaling anywhere.
- JIT-full boundary is zero-cost (same representation on both sides).
- G3 self-compiled binary uses identical code paths to Rust-compiled binary.

## Critical Constraint

`Value` is used by the **parser** and **type checker** (in `airl-syntax` and `airl-types`) for constant folding and type representations. These crates do NOT depend on `airl-runtime` or `airl-rt`. We must either:
- Keep a minimal `Value` in `airl-syntax` for AST constants (Int, Float, Bool, Str, Nil) — parser-only, never reaches the VM
- Or make `airl-syntax` depend on `airl-rt` (bad — adds C runtime dep to the parser)

**Decision:** Keep `Value` in the AST for parse-time constants. Convert `Value` → `*mut RtValue` once during bytecode compilation (in the constants table). The VM never sees `Value` — only `*mut RtValue`.

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/value.rs` | Gut | Keep only AST-level variants (Int, Float, Bool, Str, Nil, Unit, Variant) for parser/type-checker. Delete List, Map, Lambda, BytecodeClosure, Function, etc. |
| `crates/airl-runtime/src/builtins.rs` | Delete | All 3000 lines. Replaced by `airl-rt` extern C functions. |
| `crates/airl-runtime/src/bytecode_vm.rs` | Rewrite | Register bank: `Vec<*mut RtValue>`. CallBuiltin: dispatch to extern C functions. Call/CallReg: pass `*mut RtValue` args. |
| `crates/airl-runtime/src/bytecode.rs` | Modify | `BytecodeFunc.constants`: change from `Vec<Value>` to `Vec<*mut RtValue>`. |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Modify | Emit `*mut RtValue` constants (convert Value → RtValue during compilation). |
| `crates/airl-runtime/src/bytecode_aot.rs` | Simplify | Remove `build_builtin_map` — use same dispatch as VM. Remove unboxed path (YAGNI for v0.6.0, re-add later). |
| `crates/airl-runtime/src/bytecode_jit_full.rs` | Simplify | Remove `value_to_rt`/`rt_to_value` marshaling — same type on both sides. |
| `crates/airl-runtime/src/bytecode_marshal.rs` | Modify | BCFunc unmarshaling produces `*mut RtValue` constants directly. |
| `crates/airl-rt/src/value.rs` | Modify | Add `Display`, `PartialEq`, `Debug` impls. Add `value_to_ast()` for parser interop. |
| `crates/airl-rt/src/lib.rs` | Modify | Re-export key types for `airl-runtime` to use. |
| `crates/airl-runtime/Cargo.toml` | Modify | Add `airl-rt` as dependency (currently only linked at AOT time). |
| `crates/airl-driver/src/pipeline.rs` | Modify | Stdlib loading: constants are `*mut RtValue`. Remove Value→RtValue conversions. |

## Phased Approach

The refactor is too large for a single pass. Do it in 4 phases, each leaving the codebase compilable and testable.

---

### Phase 1: Make `airl-runtime` depend on `airl-rt`

Currently `airl-rt` is only linked at AOT compile time. Make it a regular Rust dependency so the VM can call its functions directly.

- [ ] **Task 1.1:** Add `airl-rt` to `crates/airl-runtime/Cargo.toml` dependencies
- [ ] **Task 1.2:** Verify `cargo build` succeeds with the new dependency
- [ ] **Task 1.3:** Verify no circular dependency issues (airl-rt has zero deps on airl-runtime)
- [ ] **Task 1.4:** Commit: `feat(v0.6.0): add airl-rt as runtime dependency`

---

### Phase 2: Dual-mode VM — RtValue registers, Value compatibility layer

Change the VM's register bank from `Vec<Value>` to `Vec<*mut RtValue>`. Add thin conversion functions at the boundaries (constants loading, result extraction). Keep `builtins.rs` temporarily for the ~27 builtins not in airl-rt.

- [ ] **Task 2.1:** Add conversion functions `value_to_rt(Value) -> *mut RtValue` and `rt_to_value(*mut RtValue) -> Value` in a new `crates/airl-runtime/src/rt_bridge.rs`. These already exist in `bytecode_jit_full.rs` lines 974-1093 — extract and generalize them.

- [ ] **Task 2.2:** Change `BytecodeFunc.constants` from `Vec<Value>` to `Vec<*mut airl_rt::value::RtValue>`. Update `bytecode_compiler.rs` to convert Value constants to RtValue during compilation. Update `bytecode_marshal.rs` similarly.

- [ ] **Task 2.3:** Change the VM's `CallFrame.registers` from `Vec<Value>` to `Vec<*mut RtValue>`. Update every opcode handler to work with `*mut RtValue`:
  - `LoadConst`: copy from constants (retain)
  - `Move`: pointer copy + retain
  - `Add/Sub/Mul/Div/Mod`: call `airl_add`/`airl_sub`/etc. directly
  - `Eq/Ne/Lt/Le/Gt/Ge`: call `airl_eq`/etc.
  - `Not/Neg`: call `airl_not`/`airl_neg`
  - `MakeList`: call `airl_list_new`
  - `MakeVariant`: call `airl_make_variant`
  - `MakeClosure`: call `airl_make_closure`
  - `MatchTag`: call `airl_match_tag`
  - `Return`: return `*mut RtValue`
  - `CallBuiltin`: dispatch to extern C function (see Task 2.4)

- [ ] **Task 2.4:** Build a CallBuiltin dispatch table: `HashMap<String, fn(*mut RtValue, ...) -> *mut RtValue>`. For each builtin name, store a wrapper that:
  1. Extracts args from registers as `*mut RtValue` pointers
  2. Calls the corresponding `airl_*` extern C function
  3. Stores result back to register

  This replaces `Builtins::get()`. Use a macro or codegen to build the table from the existing `airl-rt` function names.

- [ ] **Task 2.5:** Handle VM-aware builtins (map/filter/fold/sort/any/all/find). These need to call closures via the VM. Two options:
  - (a) Use the stdlib AIRL implementations (head/tail recursion) — already works, just remove the VM-aware fast path
  - (b) Call `airl_fold` etc. from airl-rt, passing a C function pointer that calls back into the VM

  Go with (a) for v0.6.0 — the AIRL stdlib versions work and are correct. The VM-aware fast path is a performance optimization that can be re-added later.

- [ ] **Task 2.6:** Handle thread-spawn and fn-metadata (VM-special builtins). These need VM state:
  - `thread-spawn`: clone the function registry, create a new VM. The closure is already an `RtValue::Closure`. The child VM uses the same RtValue registers.
  - `fn-metadata`: access the VM's metadata map. Return an `*mut RtValue` map.
  Keep these as special cases in the VM's CallBuiltin handler.

- [ ] **Task 2.7:** Update `bytecode_jit_full.rs` to remove Value↔RtValue marshaling. `try_call_native` receives `*mut RtValue` args directly (same type). Returns `*mut RtValue` directly.

- [ ] **Task 2.8:** Run `cargo test -p airl-runtime` — fix any failures. Run fixture tests. Run bootstrap tests.

- [ ] **Task 2.9:** Commit: `feat(v0.6.0): VM uses RtValue registers, single builtin dispatch`

---

### Phase 3: Delete `builtins.rs` and dead Value variants

- [ ] **Task 3.1:** Delete `crates/airl-runtime/src/builtins.rs` (3000 lines). Remove all imports/references.

- [ ] **Task 3.2:** Slim down `Value` enum to AST-only variants: `Int(i64)`, `Float(f64)`, `Bool(bool)`, `Str(String)`, `Nil`, `Unit`, `Variant(String, Box<Value>)`. Delete: `List`, `IntList`, `Tuple`, `Struct`, `Map`, `Function`, `Lambda`, `BuiltinFn`, `IRClosure`, `IRFuncRef`, `BytecodeClosure`, `UInt`, `Tensor`. These are only needed at runtime (now RtValue) or are dead code.

- [ ] **Task 3.3:** Fix compilation errors from the slimmed Value. Update `airl-syntax`, `airl-types`, `airl-contracts`, `airl-driver` as needed. The parser and type checker only use Int/Float/Bool/Str/Nil — this should be straightforward.

- [ ] **Task 3.4:** Delete `rt_bridge.rs` conversion functions if no longer needed (all runtime paths use RtValue directly).

- [ ] **Task 3.5:** Run full test suite. Fix any failures.

- [ ] **Task 3.6:** Commit: `refactor(v0.6.0): delete builtins.rs, slim Value to AST-only`

---

### Phase 4: Simplify AOT, test G3 fixpoint

- [ ] **Task 4.1:** Simplify `bytecode_aot.rs`: remove `build_builtin_map()` (750+ lines). The AOT compiler now uses the same function names as the VM dispatch table — just emit `call airl_<name>` for each CallBuiltin.

- [ ] **Task 4.2:** Remove the unboxed AOT path for v0.6.0 (can re-add as optimization later). All functions compile boxed with `*mut RtValue` — identical to the VM path.

- [ ] **Task 4.3:** Run full test suite including AOT: `cargo test -p airl-driver`

- [ ] **Task 4.4:** Test G3 compilation: `cargo run --release --features jit,aot -- run --load bootstrap/*.airl bootstrap/g3_compiler.airl -- <test.airl> -o <test_bin>`

- [ ] **Task 4.5:** G3 self-compilation: compile the 4 bootstrap modules + g3_compiler.airl into `g3`. Test that `./g3 test.airl -o test_bin && ./test_bin` works.

- [ ] **Task 4.6:** If G3 self-compiled binary works, attempt fixpoint: `./g3 bootstrap/*.airl bootstrap/g3_compiler.airl -o g3_v2`. Compare `g3` and `g3_v2`.

- [ ] **Task 4.7:** Update CLAUDE.md: document v0.6.0 architecture, remove dual-path warnings, update milestones.

- [ ] **Task 4.8:** Commit: `milestone(v0.6.0): single value path, AOT unification complete`

---

## Verification

After each phase:
```bash
# Rust tests (all crates)
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver

# Bootstrap AIRL tests
cargo run --release --features jit -- run bootstrap/lexer_test.airl
cargo run --release --features jit -- run bootstrap/parser_test.airl
cargo run --release --features jit -- run bootstrap/eval_test.airl

# Forge tests
cargo run --release -- run --load lib/forge/chain.airl tests/forge/chain_test.airl
cargo run --release -- run --load lib/forge/codec.airl tests/forge/codec_test.airl

# AOT (Phase 4)
cargo run --release --features jit,aot -- compile tests/fixtures/valid/fib.airl -o /tmp/fib && /tmp/fib
```

## What This Unlocks

1. **G3 fixpoint** — self-compiled binary uses identical code paths
2. **Zero builtin gaps** — every builtin available in every execution mode
3. **No marshaling overhead** — JIT/VM boundary is zero-cost
4. **Single codebase** — ~3000 lines deleted from builtins.rs, ~750 lines from AOT builtin map
5. **Correct by construction** — impossible for VM and AOT to disagree on builtin behavior
