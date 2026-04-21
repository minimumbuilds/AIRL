# NaN-Boxed RtValue — Unified Inline Primitive Representation

**Date:** 2026-04-21
**Status:** Proposed
**Priority:** Medium — structural fix; depends on BCFunc Native spec landing first for maximum benefit
**Scope:** `crates/airl-rt/src/value.rs`, all of `crates/airl-rt/src/*.rs`, `crates/airl-runtime/src/bytecode_aot.rs`, `crates/airl-runtime/src/bytecode_vm.rs`

## Problem

Every primitive AIRL value (Int, Bool, Float, Nil, Unit) currently allocates a full `RtValue` struct:

```rust
pub struct RtValue {
    pub tag: u8,      // 1 byte
    pub rc: AtomicU32, // 4 bytes
    pub data: RtData, // enum tag (1 byte) + payload (8 bytes for Int)
}
// With alignment: 24 bytes per RtValue + Box header (8) = 32 bytes
// mimalloc overhead per alloc: ~16-48 bytes for metadata + slack
// Total effective: ~60-80 bytes per primitive
```

The small-int pool (Step 2.5) eliminates allocations for -256..=255, but every Int OUTSIDE that range, every non-pooled Float, and every per-call Bool evaluation still hits `RtValue::alloc`. Measured: during AIRL_castle-sized compiles, tens of millions of transient Int/Float/Bool allocations, each ~60-80 bytes effective.

A NaN-boxed representation stores primitives **inline** in the 64-bit pointer value itself, using the unused high bits of a NaN float payload:

```
Pointer:   00000000 XXXXXXXX XXXXXXXX XXXXXXXX XXXXXXXX XXXXXXXX XXXXXXXX XXXXXXXX  (tag = 0, high bit 0)
Int63:     01000000 <------------------- 63-bit signed integer ------------------>  (tag = 1)
Bool:      01100000 ... 00000000/00000001                                           (tag = 3)
Nil:       01100001 00000000 ...                                                    (tag = 4)
Unit:      01100001 00000001 ...                                                    (tag = 5)
Float:     <------ IEEE 754 double, NOT a canonical NaN ------>                     (tag = via NaN bits)
```

This is how V8 (JavaScript), LuaJIT, SpiderMonkey, and Tcl all represent values. Zero heap allocation for the common case.

For AIRL specifically:
- **Int**: 63-bit inline (99%+ of Ints fit). Only bigint / i64 boundary cases allocate.
- **Bool**: 2 inline singletons — true/false.
- **Nil/Unit**: 1 bit pattern each.
- **Float**: inline as IEEE 754 double.
- **String/List/Map/Variant/Closure/Bytes**: still heap-allocated `Box<HeapRtValue>`, but the outer is a raw pointer — no wrapping `RtValue::alloc`.

**Expected saving**: ~60-80 bytes per primitive operation × millions of ops per compile = several hundred MB to a few GB reduction for AIRL_castle-scale builds.

## Current architecture

Every value is `*mut RtValue`. AIRL's AOT compiler calls `airl_int`, `airl_bool`, etc. to construct. The runtime wraps every value in a Box.

The small-int pool (Step 2.5) intercepts `rt_int(n)` for `n ∈ [-256, 255]` and returns a static singleton pointer. Good for small-int-dominated code (AST positions, indices). Does not help for Ints outside the range (string lengths > 256, file offsets, hash values) or for Floats / Bool results.

## Proposed design

### Encoding

```rust
// value.rs — new representation
pub type RtValueRepr = u64;

const TAG_MASK:      u64 = 0xFFFF_0000_0000_0000;
const HEAP_TAG:      u64 = 0x0000_0000_0000_0000; // aligned pointer, top bits zero
const INT_TAG:       u64 = 0x4000_0000_0000_0000;
const BOOL_FALSE_TAG:u64 = 0x6000_0000_0000_0000;
const BOOL_TRUE_TAG: u64 = 0x6000_0000_0000_0001;
const NIL_TAG:       u64 = 0x6000_0100_0000_0000;
const UNIT_TAG:      u64 = 0x6000_0200_0000_0000;
// Floats encode as raw f64 bits, exploiting the fact that
// NaN bit patterns with specific payload sentinels don't collide.
```

Heap values are aligned to 8+ bytes; the top 16 bits of the pointer are zero on x86_64 (canonical addresses). We steal those bits for the tag.

### RtData split

Heap-allocated values move to a new type:

```rust
pub struct HeapRtValue {
    pub tag: u8,         // TAG_STR, TAG_LIST, etc.
    pub rc: AtomicU32,
    pub data: HeapData,
}

pub enum HeapData {
    Str(String),
    List { items: Vec<RtValueRepr>, ... },
    Map(HashMap<String, RtValueRepr>),
    Variant { tag_name: String, inner: RtValueRepr },
    Closure { ... },
    Bytes(Vec<u8>),
    // NO Int, Float, Bool, Nil, Unit — those are inline
}
```

Heap ref counting, retain/release, etc. all operate on `HeapRtValue` when the repr's tag is HEAP.

### FFI compatibility

All `extern "C"` fn signatures change `*mut RtValue` → `u64` (same size, same calling convention on x86_64). Cranelift-emitted code uses the new constants for inline values; retain/release check the tag before dereferencing.

### Retain/release

```rust
pub fn airl_value_retain(repr: u64) {
    if (repr & TAG_MASK) == HEAP_TAG && repr != 0 {
        let heap = repr as *mut HeapRtValue;
        unsafe { (*heap).rc.fetch_add(1, Relaxed); }
    }
    // Inline values: no-op (they're immortal by construction)
}
```

Already handles immortal singletons (repr != 0 check). Inline primitives don't retain/release. This alone drops the atomic-op count for many compile-time operations by 10-100×.

## Expected impact

- **Per-primitive size**: 60-80 bytes → 0 bytes (inline in the pointer).
- **Allocation count drop**: ~50-90% fewer RtValue allocations on typical workloads. Matches the relative reduction seen in V8/LuaJIT when they introduced NaN-boxing.
- **Atomic-op drop**: every retain/release on a primitive becomes zero instructions. In deeply recursive AIRL code (linearity checker, bc_compiler), this is a substantial wall-clock improvement as well as memory reduction.
- **For AIRL_castle**: estimated 40-70% peak RSS reduction on top of Step 2.5 + Step 3 + BCFunc-native, based on comparable language runtimes.

## Implementation phases

1. **Phase 1 — dual-representation support.** Add RtValueRepr type alongside the existing RtValue. Runtime fns accept either. Extensive `#[cfg(feature = "nanbox")]` gating. No AIRL-level changes yet.
2. **Phase 2 — AOT/VM emit.** Switch bytecode_aot and bytecode_vm to the repr type under the feature gate. All existing AOT/VM tests must pass.
3. **Phase 3 — ABI migration.** Every `extern "C"` signature in airl-rt flips. All call sites update to the u64-based API. At the end of Phase 3 the feature gate can be removed.
4. **Phase 4 — clean up `RtValue` legacy**. Remove the old enum variants for Int/Float/Bool/Nil/Unit from RtData. Remove the small-int pool (Step 2.5) which becomes redundant.

Total effort: estimated 800-1 500 LOC changed across airl-rt + airl-runtime. Bounded by the existing `airl-rt` public surface area.

## Dependencies

- **Should land AFTER BCFunc-native spec.** BCFunc's per-function overhead dominates memory; fix that first, then NaN-boxing captures the remaining small-value savings.
- **Should land AFTER alloc-site-tagging** so we can measure impact with confidence.

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Pointer-tagging conflicts with future ARM64 pointer-auth (PAC) or aarch64 top-byte-ignore | Medium | Phase 1 checks HOST_POINTER_WIDTH at build; falls back to boxed on unsupported targets. |
| Float NaN-boxing edge cases (legitimate NaN payloads) | Low | Canonicalize incoming NaNs to our reserved bit pattern at rt_float. |
| Third-party C FFI (Z3, sqlite) expects pointer-sized opaque values | Low | Those FFIs don't see RtValueRepr — they stay on their own representation. |
| AOT codegen size grows due to tag-check branches | Low | Most ops are already type-specialized; tag checks compile to 1-2 branches per op. |
| Debug tooling (diag crate, gdb pretty-printers) needs to understand new encoding | Medium | Update `diag.rs` to decode inline values; add a brief `gdb.py` for pretty-printing. |

## Non-goals

- **Does not change on-disk bytecode format.** Bytecode constants still serialize as the existing Value enum.
- **Does not change AIRL-level semantics.** The AIRL programmer sees no difference; all values behave identically.
