# BCFunc Native Representation — Structural RtValue Reduction

**Date:** 2026-04-21
**Status:** Proposed
**Priority:** High — addresses the 100× RtValue-per-LOC structural overhead
**Scope:** `crates/airl-rt/src/value.rs`, `crates/airl-rt/src/bc_func.rs` (new), `bootstrap/bc_compiler.airl`, `crates/airl-runtime/src/bytecode_marshal.rs`

## Problem

A BCFunc in the AIRL runtime is represented as a nested RtValue tree:

```
(BCFunc name arity reg_count capture_count [constants] [[op dst a b] ...])
```

That is:
- 1 `RtValue::Variant` for the BCFunc tag
- 1 `RtValue::List` for the outer wrapper
- 1 `RtValue::Str` for `name`
- 3 `RtValue::Int` for arity / reg_count / capture_count (pooled now)
- 1 `RtValue::List` for constants, + N `RtValue` per constant (~average 2 per function)
- 1 `RtValue::List` for instructions
- For each instruction: 1 `RtValue::List` + 4 `RtValue::Int`

For a 100-instruction function with 10 constants:
- 1 (outer) + 1 (const list) + 10 (consts) + 1 (instr list) + 100 × (1 list + 4 ints) = **513 RtValue allocations per function**.

For the G3 bootstrap (~2 500 functions across all files) at compile peak:
- ~2 500 × 500 = **1.25 M RtValues just for BCFuncs** — matches observed 1.6 M alive lists at OOM.

Each RtValue is a separate `Box<RtValue>` (~24 byte struct + mimalloc overhead = ~80 bytes effective). 1.25 M × 80 = **~100 MB** for the BCFunc representation alone, per batch held in memory. After the small-int pool covers the instruction ints, it's still ~50 MB per batch for the `Vec<*mut RtValue>` backing each list and the list RtValue wrappers.

None of this is inherent complexity. A native Rust representation would be:

```rust
pub struct Instruction { op: u8, dst: u32, a: u32, b: u32 } // 16 bytes
pub struct BytecodeFunc {
    name: String,
    arity: u16,
    reg_count: u16,
    capture_count: u16,
    constants: Vec<Value>,
    instructions: Vec<Instruction>,
}
```

For the same function: 1 `String` (~32 B + name bytes), 1 `Vec<Value>` (~24 B + 10 × 24 B for consts), 1 `Vec<Instruction>` (~24 B + 100 × 16 B = 1.6 KB). Total ~2 KB per function. **50× smaller than the RtValue nesting.**

## Current architecture

### AIRL-level (`bootstrap/bc_compiler.airl`)

`bc-emit-func` and related functions build BCFunc values via AIRL list construction:

```clojure
(BCFunc name arity regs captures constants instructions)
```

Every instruction is `[op-code dst a b]` — an AIRL list literal.

### Rust marshaling (`crates/airl-runtime/src/bytecode_marshal.rs`)

`value_to_bytecode_func` converts the AIRL RtValue representation to `Vec<BytecodeFunc>` (native Rust) at the Rust/AIRL boundary. This is what the AOT compiler consumes. So the native form already exists internally — the cost is the **AIRL-side representation** held in memory through the per-file compile pipeline.

## Proposed design

Introduce an opaque `RtData::BCFuncNative(Arc<BytecodeFunc>)` variant: AIRL sees it as a single RtValue (tag `TAG_BCFUNC = 12`); internally the data is a native Rust struct. Every operation AIRL currently performs on the BCFunc decomposes through new builtins.

### New RtValue variant

```rust
// value.rs
pub const TAG_BCFUNC: u8 = 12;

pub enum RtData {
    ...existing...
    BCFuncNative(std::sync::Arc<BytecodeFunc>),
}
```

Arc to allow cheap cloning when the same BCFunc is referenced multiple times (e.g., closure capture, func_map).

### New builtins for BCFunc manipulation

Replace the nested-list decomposition pattern with opaque accessors:

| Builtin | Replaces AIRL pattern |
|---|---|
| `bc-func-new(name, arity, reg_count, capture_count)` | `(BCFunc name arity regs caps [] [])` initial |
| `bc-func-push-const(bcf, value) -> bcf` | `(append consts value)` |
| `bc-func-push-instr(bcf, op, dst, a, b) -> bcf` | `(append instrs [op dst a b])` |
| `bc-func-name(bcf) -> String` | `(match bcf (BCFunc n _ _ _ _ _) n)` |
| `bc-func-arity(bcf) -> Int` | pattern match arity |
| `bc-func-reg-count(bcf) -> Int` | pattern match |
| `bc-func-capture-count(bcf) -> Int` | pattern match |
| `bc-func-constants(bcf) -> List` | for filter/inspection; constructs a List lazily |
| `bc-func-instructions(bcf) -> List` | same |
| `bc-func-is-main?(bcf) -> Bool` | replaces the filter in `bc-move-to-slots` etc. |

COW semantics: if `Arc::strong_count == 1`, mutate in place; else clone first. Matches the existing `airl_append` / `airl_cons` COW pattern for lists.

### Compatibility

The existing `(BCFunc name arity ...)` pattern matching in `bootstrap/bc_compiler.airl` and `bootstrap/g3_compiler.airl` must be rewritten to use the new accessors. This is mechanical but spread across the file — estimated 30-50 call sites.

For the transition period: add an Op::Match handler in the VM and AOT that recognizes TAG_BCFUNC and virtually "decomposes" it — so existing pattern-match code continues to work while incrementally migrated. When all pattern-match sites are migrated, remove the virtual decomposition.

## Expected impact

- **Per-batch memory**: 50-100 MB → ~2-5 MB. For the G3 build's held-in-memory batch of one file (largest = bc_compiler.airl, ~500 functions), peak drops from ~50 MB to ~1 MB.
- **Across 33 AIRL_castle files**: since Step 3's per-file emit drops each batch after compile, the across-file effect is captured per-file. A single peak file's RSS drops proportionally.
- **Secondary**: fewer Rt allocations → less mimalloc fragmentation → potentially better cache behavior.

## Implementation phases

1. **Phase 1 — add RtData::BCFuncNative + basic accessors.** No AIRL-side changes yet; verify all tests still pass with the type added but unused.
2. **Phase 2 — migrate `bc_compile-program-with-prefix-and-proofs` to emit BCFuncNative.** Internal to bc_compiler; no caller changes. AOT side adds a `value_to_bytecode_func` branch recognizing the native tag and just cloning the Arc.
3. **Phase 3 — migrate consumers.** Everything that pattern-matches on BCFunc (the filter in g3_compiler, etc.) switches to `bc-func-is-main?`. AOT's `value_to_bytecode_func` can drop the legacy branch.
4. **Phase 4 — delete legacy construction.** `(BCFunc ...)` variant constructor removed from AIRL parse.

Each phase is independently testable via the existing AOT suite.

## Files to modify

| File | Phase | Change |
|---|---|---|
| `crates/airl-rt/src/value.rs` | 1 | Add TAG_BCFUNC, RtData::BCFuncNative |
| `crates/airl-rt/src/bc_func.rs` (new) | 1 | Builtin fns: bc_func_new, bc_func_push_*, bc_func_* accessors |
| `crates/airl-runtime/src/bytecode_marshal.rs` | 2 | value_to_bytecode_func: recognize TAG_BCFUNC; path 0-copy |
| `crates/airl-runtime/src/bytecode_vm.rs` | 1 | Dispatch new builtins; optionally virtual-match TAG_BCFUNC |
| `crates/airl-runtime/src/bytecode_aot.rs` | 1 | Same for AOT |
| `bootstrap/bc_compiler.airl` | 2 | Use bc-func-push-* in emit path |
| `bootstrap/g3_compiler.airl` | 3 | Use bc-func-is-main? |

## Non-goals

- **Does not remove AIRL-level introspection.** `bc-func-instructions` still returns a List when asked; just materialized on demand.
- **Does not change the on-disk bytecode format.** Marshal/unmarshal stays the same.

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Pattern-match compatibility shim has perf overhead | Low | Virtual decomposition is rare on the hot path; only used during migration. |
| Arc cycles via closure capture | Medium | Audit closure builders to ensure they don't hold BCFunc refs in a cycle. |
| Hash-map keyed on BCFunc identity now uses pointer equality | Low | AIRL code doesn't key on BCFunc identity today; verify with grep. |
