# Known Issues Roadmap — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 4 known issues in AIRL v0.2, in dependency order, culminating in all functions JIT-compiled to native code with contracts and ownership enforced.

**Architecture:** Four issues, executed in sequence. Issue 1 (quantifiers) unblocks bytecode completeness. Issue 2 (ownership) closes the safety gap. Issue 3 (jit-full) makes native compilation universal. Issue 4 (MLIR packaging) is independent cleanup.

**Tech Stack:** Rust, Cranelift, airl-rt C FFI runtime, bytecode compiler/VM

---

## Dependency Graph

```
Issue 1: Quantifiers → Bytecode (small, ~1 hour)
    │ (quantifiers must exist in bytecode before jit-full can compile them)
    │
Issue 2: Ownership in Bytecode VM (medium, ~2 hours)
    │ (ownership must be enforced before jit-full becomes the default)
    │
Issue 3: Fix JIT-Full + Make Default (large, ~4-6 hours)
    │ (5 bugs to fix: string corruption, verifier errors, closure dispatch, UTF-8, segfault)
    │
Issue 4: MLIR Packaging (independent, ~1 hour)
```

---

## Issue 1: Compile Quantifier Expressions to Bytecode

**Problem:** `forall` and `exists` expressions compile to `IRNode::Nil` in the bytecode path. They only worked in the tree-walking interpreter via `eval_quantifier`, which iterated over a bounded domain and checked a predicate. Currently these are in `tests/fixtures/interpreter_only/`.

**Solution:** Compile quantifiers to bytecode as bounded loops. `(forall [i : Nat] (where (< i N)) body)` becomes: iterate i from 0 to N, evaluate body for each i, return false if any body is false (forall) or true if any is true (exists).

### Files

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-driver/src/pipeline.rs` | Modify | `compile_expr`: handle `ExprKind::Forall`/`Exists` instead of returning Nil |
| `crates/airl-runtime/src/ir.rs` | Modify | Add `IRNode::Forall` and `IRNode::Exists` variants if needed, or desugar to loops |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Modify | Compile loop-based quantifier evaluation |
| `tests/fixtures/interpreter_only/` → `tests/fixtures/valid/` | Move | Move forall_expr.airl and exists_expr.airl back once they work |

### Approach: Desugar in pipeline.rs

The simplest approach is to desugar quantifiers into loops at the IR level in `compile_expr`, avoiding new IR nodes entirely:

```
(forall [i : Nat] (where (< i 10)) (>= i 0))
```

Desugars to:

```
(let (result true)
  (let (i 0)
    (let (loop (fn [i result]
      (if (not (< i 10))
        result
        (if (not (>= i 0))
          false
          (loop (+ i 1) result)))))
      (loop i result))))
```

This is a recursive function that iterates and short-circuits. The bytecode compiler already handles recursion, closures, and if-expressions. No new opcodes needed.

### Tasks

- [ ] **Step 1:** In `compile_expr` in pipeline.rs, add cases for `ExprKind::Forall` and `ExprKind::Exists` that desugar to a recursive let-bound loop
- [ ] **Step 2:** The desugared IR uses `IRNode::Let`, `IRNode::Lambda`, `IRNode::Call`, `IRNode::If` — all already supported by bytecode compiler
- [ ] **Step 3:** Move `forall_expr.airl` and `exists_expr.airl` back from `interpreter_only/` to `valid/`
- [ ] **Step 4:** Run fixture tests: `cargo test -p airl-driver --test fixtures --features jit`
- [ ] **Step 5:** Commit

### Edge Cases

- The interpreter capped iterations at 10,000 (MAX_ITERATIONS). The desugared version uses the `(where ...)` clause as the loop bound, so it naturally terminates. If no where clause, default to 10,000 iterations.
- `exists` is the dual: return `true` on first success, `false` if all fail.

---

## Issue 2: Ownership Tracking in Bytecode VM

**Problem:** The linearity checker catches use-after-move at compile time for explicitly annotated parameters, but the bytecode VM has no runtime enforcement. Use-after-move in top-level expressions or through dynamic dispatch is not caught. We compensated by making linearity errors fatal in Run mode, but that's a blunt instrument — it prevents execution rather than catching violations at runtime.

**Solution:** Add move tracking to the bytecode VM. Each register gets a "moved" flag. Passing a value to an `own` parameter marks the source register as moved. Accessing a moved register raises a RuntimeError.

### Files

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/bytecode.rs` | Modify | Add `MarkMoved` and `CheckNotMoved` opcodes |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Modify | Emit ownership opcodes when compiling calls with `own` params |
| `crates/airl-runtime/src/bytecode_vm.rs` | Modify | Add `moved` flags to CallFrame, handle new opcodes |
| `crates/airl-runtime/src/bytecode_jit.rs` | Modify | Add new opcodes to eligibility check (disqualify for now) |
| `crates/airl-runtime/src/bytecode_jit_full.rs` | Modify | Handle new opcodes (no-op or native check) |
| `crates/airl-driver/src/pipeline.rs` | Modify | Pass param ownership info to bytecode compiler |
| `tests/fixtures/interpreter_only/` → `tests/fixtures/linearity_errors/` | Move | Move use_after_move_own.airl and move_while_borrowed.airl back |

### Approach: Ownership Assertion Opcodes

Follow the same pattern as contract assertions — compile ownership checks as assertion opcodes:

```
New opcodes:
  MarkMoved    dst_reg, _, _       — mark register as moved (after passing to own param)
  CheckNotMoved src_reg, _, name_idx — error if register was moved (before accessing value)
```

The bytecode compiler needs to know which parameters have `own` ownership. This requires passing ownership annotations from the AST `FnDef.params` through to the bytecode compiler.

### Data Flow

1. `pipeline.rs`: When compiling a `FnCall`, look up the target function's param list to find `own` annotations
2. Emit `CheckNotMoved` before loading a value that might have been moved
3. Emit `MarkMoved` after passing a value to an `own` parameter

**Challenge:** The bytecode compiler operates on IR nodes, which don't carry ownership info. We need a side-channel (similar to how contracts are passed) mapping function names to parameter ownership annotations.

### Tasks

- [ ] **Step 1:** Add `MarkMoved` and `CheckNotMoved` opcodes to `bytecode.rs`
- [ ] **Step 2:** Add `moved: Vec<bool>` to `CallFrame` in `bytecode_vm.rs`, initialized to all false
- [ ] **Step 3:** Implement `MarkMoved` handler: set `moved[dst]` to true
- [ ] **Step 4:** Implement `CheckNotMoved` handler: if `moved[src]` is true, return `RuntimeError::UseAfterMove`
- [ ] **Step 5:** In `pipeline.rs`, build ownership map from FnDef params and pass to bytecode compiler
- [ ] **Step 6:** In `bytecode_compiler.rs`, emit `MarkMoved` after calls to functions with `own` params, emit `CheckNotMoved` before accessing variables that could have been moved
- [ ] **Step 7:** Add to JIT disqualification list (or handle as native checks like contracts)
- [ ] **Step 8:** Add to jit-full as no-ops or native branches
- [ ] **Step 9:** Move linearity test fixtures back from `interpreter_only/` to `linearity_errors/`
- [ ] **Step 10:** Run fixture tests
- [ ] **Step 11:** Revert the "linearity errors fatal in Run mode" change in pipeline.rs (runtime enforcement replaces it)
- [ ] **Step 12:** Commit

### Important Notes

- Only `own` parameters trigger move tracking. `ref`, `mut`, and default ownership don't.
- `copy` parameters explicitly copy, so the source is NOT marked moved.
- Multiple uses in the same expression (e.g., `(+ v v)` where v is own) need careful handling — the second use should fail.
- Lambda captures of owned values should also mark the source as moved.

---

## Issue 3: Fix JIT-Full Bugs and Make It the Default

**Problem:** The primitive JIT only compiles functions with no lists, closures, variants, or builtins — which excludes most real AIRL programs. The jit-full path (`bytecode_jit_full.rs`, 1,923 lines) already handles ALL opcodes by compiling them to Cranelift IR with `airl-rt` runtime helper calls, but it has 5 blocking bugs (17/26 fixtures pass).

**Solution:** Fix the 5 bugs, make jit-full the default execution path. The primitive JIT stays as a fast path for pure-arithmetic functions (unboxed i64, no allocation overhead).

### The 5 Bugs (from docs/superpowers/plans/2026-03-23-jit-full-bugs.md)

**Bug 1: Variant Tag String Corruption**
- String constants from BytecodeFunc.constants passed as raw pointers to `airl_str`
- Pointers dangle if BytecodeFunc moves/drops after JIT compile
- Fix: Copy string bytes to `stable_strings` Vec during compilation, use stable pointers

**Bug 2: Cranelift Verifier Errors on `__main__`**
- `__main__` function fails IR verification (unreachable code, missing terminators)
- Falls back to bytecode, creating mixed execution
- Fix: Ensure every Cranelift block has a terminator, add block boundaries after Return/Jump

**Bug 3: Closure Dispatch — "not a Closure"**
- Lambda functions aren't compiled before the functions that reference them
- BytecodeClosure values marshaled to rt_nil instead of proper RtValue Closures
- Fix: Compile lambdas first (dependency ordering), fix marshal for BytecodeClosure

**Bug 4: Invalid UTF-8 in String Construction**
- Same root cause as Bug 1 (dangling string pointers)
- Fix: Same as Bug 1

**Bug 5: Segfault After Partial Success**
- Cascades from Bugs 1-2
- Fix: Fixing 1-2 should resolve this

### Files

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/bytecode_jit_full.rs` | Modify | Fix all 5 bugs |
| `crates/airl-runtime/src/bytecode_vm.rs` | Modify | Change dispatch order: try jit-full first (it already does this) |
| `crates/airl-driver/src/pipeline.rs` | Modify | Make jit-full the default instead of primitive jit |
| `crates/airl-driver/src/main.rs` | Modify | Remove jit-full as separate mode, integrate into default |

### Task Order (bugs have dependencies)

**Phase A: Fix String Corruption (Bugs 1 & 4)**
- [ ] **Step 1:** In `compile_func`, when encountering string constants, copy bytes to `stable_strings` Vec and use the stable pointer instead of the BytecodeFunc.constants pointer
- [ ] **Step 2:** Verify: `MakeVariant` tag strings are correct after JIT compilation
- [ ] **Step 3:** Verify: `CallBuiltin` name strings resolve correctly
- [ ] **Step 4:** Run fixture tests — expect improvement from 17/26

**Phase B: Fix Cranelift Verifier (Bug 2)**
- [ ] **Step 5:** Run with `CRANELIFT_VERIFY=1` to get exact verifier errors
- [ ] **Step 6:** Ensure every block has exactly one terminator (return, jump, or brif)
- [ ] **Step 7:** Add block boundaries after Return instructions for code that follows
- [ ] **Step 8:** Handle `__main__` specifically if needed (it has sequential top-level expressions)
- [ ] **Step 9:** Run fixture tests — expect all non-closure tests to pass

**Phase C: Fix Closure Dispatch (Bug 3)**
- [ ] **Step 10:** Ensure lambda BytecodeFuncs are compiled before functions that reference them
- [ ] **Step 11:** Fix BytecodeClosure → RtValue marshaling (currently marshals to nil)
- [ ] **Step 12:** Test: higher-order stdlib functions (fold, map, filter, sort) with lambda args
- [ ] **Step 13:** Run full fixture tests — expect 26/26

**Phase D: Make JIT-Full Default**
- [ ] **Step 14:** In pipeline.rs, change the default JIT pipeline to use `new_with_full_jit()` + `jit_full_compile_all()`
- [ ] **Step 15:** Keep primitive JIT as a fast path: in VM dispatch, try primitive JIT first (for pure-arithmetic functions), then jit-full, then bytecode fallback
- [ ] **Step 16:** Add contract-aware compilation to jit-full (same pattern as primitive JIT: `airl_jit_contract_fail` call on sad path)
- [ ] **Step 17:** Run all fixture tests + 25-task benchmark
- [ ] **Step 18:** Benchmark: compare jit-full vs primitive JIT on fib(30) (expect primitive to be faster due to unboxed values, but jit-full handles everything else)
- [ ] **Step 19:** Commit

### Expected Performance After Fix

| Benchmark | Primitive JIT | JIT-Full | Bytecode | Python |
|-----------|--------------|----------|----------|--------|
| fib(30) (pure int) | ~13ms | ~30-50ms (boxed overhead) | ~5,800ms | ~250ms |
| stdlib fold/map | N/A (ineligible) | ~50-100ms (native) | ~500-1000ms | ~50ms |
| Real programs | Partial (some funcs) | All native | All bytecode | All CPython |

The primitive JIT stays fastest for pure arithmetic. JIT-full handles everything else at native speed with boxing overhead. The dispatch order (primitive → full → bytecode) gives best-of-both-worlds.

---

## Issue 4: MLIR System Library Packaging

**Problem:** `airl-mlir` requires `libzstd-dev` and LLVM 19+ installed system-wide. These aren't commonly available, and the crate is excluded from default tests.

**Solution:** Improve the build experience with better detection, error messages, and optional Docker-based builds.

### Files

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` (workspace) | Modify | Add MLIR feature flag, exclude airl-mlir from default members |
| `crates/airl-mlir/build.rs` | Create | Better LLVM/zstd detection with actionable error messages |
| `Dockerfile` | Create | Docker build environment with all dependencies |
| `CLAUDE.md` | Modify | Update build instructions |

### Tasks

- [ ] **Step 1:** Add workspace-level `mlir` feature that gates airl-mlir inclusion
- [ ] **Step 2:** Create `build.rs` for airl-mlir that checks for libzstd and LLVM 19+ before attempting compilation, with clear error messages pointing to install instructions
- [ ] **Step 3:** Add a `Dockerfile` with Ubuntu 24.04 + LLVM 19 + libzstd-dev for reproducible builds
- [ ] **Step 4:** Document: `cargo build --features mlir` for GPU support, plain `cargo build` for everything else
- [ ] **Step 5:** Commit

---

## Summary: Execution Order

| Phase | Issue | Scope | Depends On | Outcome |
|-------|-------|-------|------------|---------|
| 1 | Quantifiers → Bytecode | Small (~1hr) | Nothing | `forall`/`exists` work in all execution modes |
| 2 | Ownership in Bytecode | Medium (~2hr) | Nothing | Use-after-move caught at runtime in bytecode/JIT |
| 3 | Fix JIT-Full + Default | Large (~4-6hr) | Issues 1 & 2 | All functions JIT-compiled to native code |
| 4 | MLIR Packaging | Small (~1hr) | Nothing | Clean GPU build experience |

After all 4 are resolved, AIRL v0.3 would have:
- Every function compiled to native x86-64 (jit-full)
- Pure-arithmetic functions optimized further (primitive JIT, unboxed)
- Contracts always enforced (native conditional branches)
- Ownership always enforced (native move tracking)
- Quantifier expressions fully functional
- GPU compilation available via `--features mlir`
- Zero known issues in the execution pipeline
