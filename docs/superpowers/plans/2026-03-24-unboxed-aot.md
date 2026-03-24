# Unboxed AOT Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an unboxed fast path to the AOT compiler so eligible functions (pure arithmetic, no lists/variants/closures) compile to raw register operations instead of heap-allocated RtValue calls. Expected: fib(35) drops from 3.67s to ~0.1s.

**Architecture:** Two-tier AOT compilation. For each function, check eligibility (same rules as `bytecode_jit.rs`). Eligible functions compile with unboxed `i64`/`f64` values — arithmetic is single CPU instructions, no malloc. Ineligible functions compile with boxed `*mut RtValue` (existing `bytecode_aot.rs` path). At call boundaries between tiers, marshal/unmarshal values.

**Reference:** `crates/airl-runtime/src/bytecode_jit.rs` (951 lines) — the existing unboxed JIT. Same compilation logic, different output backend.

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/bytecode_aot.rs` | Modify | Add `compile_func_unboxed()`, eligibility check, two-tier dispatch |

This is a single-file change. The unboxed compilation is added as a second code path inside the existing AOT compiler. No new files needed.

---

### Task 1: Add Eligibility Check

**Files:** `crates/airl-runtime/src/bytecode_aot.rs`

Port `is_eligible()` from `bytecode_jit.rs:74-136` into `BytecodeAot`. The logic:

- [ ] **Step 1:** Add `is_eligible(func, all_functions, eligible_cache, ineligible_cache) -> bool`

Disqualifying opcodes: `MakeList`, `MakeVariant`, `MakeVariant0`, `MakeClosure`, `MatchTag`, `JumpIfNoMatch`, `MatchWild`, `TryUnwrap`, `CallBuiltin`, `CallReg`.

Allowed: `LoadConst` (int/float/bool only), `LoadNil`, `LoadTrue`, `LoadFalse`, `Move`, `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`, `Not`, `Jump`, `JumpIfFalse`, `JumpIfTrue`, `Call` (to eligible functions only), `TailCall` (self only), `Return`, `AssertRequires`, `AssertEnsures`, `AssertInvariant`.

Cross-function calls: recursively check callee eligibility. Self-calls always eligible.

- [ ] **Step 2:** Test: `fib` is eligible, `fold` is not (uses `CallReg`).

---

### Task 2: Unboxed Compilation

**Files:** `crates/airl-runtime/src/bytecode_aot.rs`

Port `compile_func()` from `bytecode_jit.rs:194-669` as `compile_func_unboxed()` in `BytecodeAot`. Key differences from the JIT version:

- [ ] **Step 1:** Add `TypeHint` enum (Int/Float/Bool) and per-register tracking.

- [ ] **Step 2:** Add `compile_func_unboxed()` that:
  - Uses raw `I64` params/returns (no `PTR`)
  - Arithmetic: `iadd`/`isub`/`imul`/`sdiv` for ints, `fadd`/`fsub`/`fmul`/`fdiv` for floats (via bitcast)
  - Comparisons: `icmp`/`fcmp` → `uextend` to I64
  - `LoadConst`: ints as `iconst`, floats as `f64const` → bitcast to I64
  - `Call`: to other unboxed functions (declared with I64 signatures)
  - `TailCall`: jump to loop_block (same pattern as JIT)
  - Control flow: same as JIT (`Jump`/`JumpIfFalse`/`JumpIfTrue`)
  - `Return`: return raw I64

- [ ] **Step 3:** String constants in `LoadConst` for unboxed functions — skip (only int/float/bool constants are valid in eligible functions).

---

### Task 3: Two-Tier Dispatch in `compile_all`

**Files:** `crates/airl-runtime/src/bytecode_aot.rs`

- [ ] **Step 1:** In `compile_with_deps()`, check eligibility before calling `compile_func()`:

```rust
if self.is_eligible(func, all_functions, &mut eligible, &mut ineligible) {
    self.compile_func_unboxed(func, all_functions)?;
} else {
    self.compile_func(func, all_functions)?;  // existing boxed path
}
```

- [ ] **Step 2:** Functions declared with two different signatures:
  - Eligible: `(I64, I64, ...) -> I64` (Linkage::Local)
  - Ineligible: `(PTR, PTR, ...) -> PTR` (Linkage::Local)

  This means eligible functions and ineligible functions can coexist in the same object file.

---

### Task 4: Boundary Marshaling

**Files:** `crates/airl-runtime/src/bytecode_aot.rs`

When boxed code calls an unboxed function (or vice versa), values must be converted.

- [ ] **Step 1:** In boxed `compile_func()`, when `Op::Call` targets an eligible function:
  - Extract raw i64 from each `*mut RtValue` arg via `airl_as_int_raw` / `airl_as_float_raw`
  - Call the unboxed function with raw values
  - Wrap the result with `airl_int()` / `airl_float()` / `airl_bool()`

- [ ] **Step 2:** Add `airl_as_int_raw(*mut RtValue) -> i64` and `airl_as_float_raw(*mut RtValue) -> i64` to `airl-rt` if not already present. These extract the raw value without allocation.

---

### Task 5: Verify with Benchmarks

- [ ] **Step 1:** Rebuild and compile fib(35):
```bash
cargo build --release --features jit,aot -p airl-driver -p airl-rt -p airl-runtime
target/release/airl-driver compile /tmp/bench/fib35.airl -o /tmp/bench/fib35_unboxed
time /tmp/bench/fib35_unboxed
```
Expected: ~0.1s (was 3.67s).

- [ ] **Step 2:** Verify all 26 fixtures still produce correct output.

- [ ] **Step 3:** Run full benchmark suite: fib35, strings, list_big, map_ops, variants. Only fib35 should speed up (the rest use ineligible ops and stay on the boxed path).

- [ ] **Step 4:** Commit.

---

## Key Design Decisions

1. **Same object file, two compilation tiers.** No separate pass or file. Each function is compiled once via the appropriate tier.

2. **Eligible functions use I64-based signatures.** This differs from the boxed path's PTR-based signatures. The Cranelift module handles both.

3. **Boundary marshaling is caller's responsibility.** The boxed caller unpacks args and repacks results. The unboxed function never touches `RtValue`.

4. **Contract assertions work in both tiers.** The JIT already compiles them to conditional branches + `airl_jit_contract_fail` calls.
