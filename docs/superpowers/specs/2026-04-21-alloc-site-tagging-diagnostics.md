# Allocation-Site Tagging for Leak Localization

**Date:** 2026-04-21
**Status:** Proposed
**Priority:** Medium — prerequisite for fixing remaining memory issues
**Scope:** `crates/airl-rt/src/diag.rs`, `crates/airl-rt/src/value.rs`, optional `crates/airl-runtime/src/bytecode_aot.rs`

## Problem

The current `AIRL_RT_TRACE=1` instrumentation reports global per-tag alive counts:

```
[rt-trace] rss=5554MiB alive=3245033 allocs=3647479 freed=402446
  list=1618949 bytes=556238 int=495656 variant=335192 str=152045 map=84967
```

This tells us WHAT is leaking (lists dominate) but not WHERE. For the remaining investigation — why do 1.6 M lists stay alive at OOM after Step 3's per-file emit — we need to know which function allocated the surviving lists. Candidates include AIRL bootstrap code (z3_bridge, bc_compiler internals), Rust builtins (list/map/closure construction), and Z3 FFI.

Without per-site attribution, we're guessing at fixes and iterating blindly. With it, we can rank leak sources and fix them in priority order.

## Proposed design

Each `RtValue` gains an optional 2-byte `site_id` field that identifies the allocation site. When `AIRL_RT_TRACE_SITES=1`, the diag module prints top-N allocation sites by alive count.

### Data structure

```rust
// value.rs
pub struct RtValue {
    pub tag: u8,
    #[cfg(feature = "rt_trace_sites")]
    pub site_id: u16,
    pub rc: AtomicU32,
    pub data: RtData,
}
```

Feature-gated to avoid shipping the overhead in production.

### Site registry

```rust
// diag.rs
static SITE_NAMES: OnceLock<RwLock<Vec<&'static str>>> = OnceLock::new();
static SITE_ALIVE: OnceLock<RwLock<Vec<AtomicU64>>> = OnceLock::new();

pub fn register_site(name: &'static str) -> u16 {
    // Get or allocate a u16 id for this site; first-come first-served.
}

pub fn on_alloc_at_site(tag: u8, site: u16) { ... }
pub fn on_free_at_site(tag: u8, site: u16) { ... }
```

### Hooking allocation sites

Each `RtValue::alloc` call site (there are ~30 across `list.rs`, `map.rs`, `string.rs`, `closure.rs`, `variant.rs`) gets a module-static site id:

```rust
// list.rs
use crate::diag;
static SITE_LIST_NEW: OnceLock<u16> = OnceLock::new();
fn list_new_site() -> u16 {
    *SITE_LIST_NEW.get_or_init(|| diag::register_site("list.rs:airl_list_new"))
}

pub extern "C" fn airl_list_new(...) -> *mut RtValue {
    RtValue::alloc_at(TAG_LIST, RtData::List { ... }, list_new_site())
}
```

For the COW paths (`airl_append`, `airl_map_set`): use site IDs like `"list.rs:append.clone-path"` vs `"list.rs:append.in-place"` to distinguish.

For AOT-generated allocations (AIRL user code calling `rt_list` etc.): the site id is set by the generating Cranelift code to point to the AIRL source location. This is the harder part — requires threading source spans through the AOT emit.

**Phase 1 targets Rust-side sites only.** That covers list/map/string/variant/closure construction from builtins and stdlib-AIRL. User AIRL code's allocation attribution is Phase 2.

### Output

On process exit (or periodically via the existing trace thread), dump the top-N sites:

```
[rt-trace-sites] top allocations (sorted by alive):
  rank  site                                           alive     allocs    freed
  1     list.rs:airl_list_new                          800123   2500001   1699878
  2     variant.rs:airl_variant                        300000    300050       50
  3     list.rs:append.clone-path                      250123   1000000   749877
  4     map.rs:airl_map_set.clone-path                  84000     84500      500
  ...
```

This reveals the top leaking sites with a single run.

## Expected impact

- **Diagnostic time**: turn "I don't know where the 1.6 M lists come from" into a concrete ranked list in a 10-minute re-run.
- **No runtime cost when off**: feature-gated, `#[cfg(feature = "rt_trace_sites")]` compiles to zero.
- **Low cost when on**: 2 extra bytes per RtValue, 1 atomic-add per alloc/free — negligible on the hot path; overall program slowdown <5% even under trace.

## Implementation phases

1. **Phase 1 — Rust-side sites.** Add site_id to RtValue (feature-gated). Add `diag::register_site` + alive-counting. Thread site IDs through every `RtValue::alloc` in airl-rt.
2. **Phase 2 — AIRL-level sites.** AOT emit passes source span as site id when generating rt_int/rt_list/etc. calls. VM does the same. This lets AIRL user code appear as "g3_compiler.airl:313" in the report.
3. **Phase 3 — integration tests.** Add `cargo test --features rt_trace_sites` as a CI gate so the sites feature doesn't bitrot.

## Files to modify

| File | Phase | Change |
|---|---|---|
| `crates/airl-rt/src/value.rs` | 1 | Add site_id field; new `RtValue::alloc_at(tag, data, site)` |
| `crates/airl-rt/src/diag.rs` | 1 | register_site, on_alloc_at_site, dump-on-exit |
| `crates/airl-rt/Cargo.toml` | 1 | Add `rt_trace_sites` feature |
| `crates/airl-rt/src/list.rs, map.rs, string.rs, variant.rs, closure.rs` | 1 | Thread site IDs |
| `crates/airl-runtime/src/bytecode_aot.rs` | 2 | Emit source-span site IDs for rt_* calls |
| `crates/airl-runtime/src/bytecode_vm.rs` | 2 | Same for VM |

## Non-goals

- **Does not become always-on in release builds.** Feature-gated strictly to investigation scenarios.
- **Does not replace `AIRL_RT_TRACE`**. This augments it; the global counts stay.

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Feature-flag matrix bloat | Low | Only one new feature; clearly scoped. |
| 2-byte field adds alignment padding on some platforms | Low | RtValue already has u8 tag + u32 rc; u16 fits in existing padding. |
| Site registry lock contention | Low | First-sight registration behind write-lock; reads on alloc use per-site atomic. |
