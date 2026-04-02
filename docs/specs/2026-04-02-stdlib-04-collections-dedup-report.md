# Spec 04 — Collections Dedup Analysis

> Identify Rust builtins that duplicate AIRL stdlib functions and determine which can be removed.

## Summary

- **7 higher-order list builtins** have verified AIRL equivalents in `stdlib/prelude.airl`
- **8 structural list builtins** have verified AIRL equivalents in `stdlib/prelude.airl`
- **6 map builtins** are already wrapped by AIRL functions in `stdlib/map.airl`; the rest are irreducible primitives
- **Total safe to remove:** 15 Rust builtins (7 higher-order + 8 structural)
- **Must keep:** 17 irreducible builtins (10 list primitives + 7 map primitives)

## 1. Higher-Order List Builtins (Rust → AIRL)

These Rust builtins in `crates/airl-rt/src/list.rs` have exact AIRL equivalents in `stdlib/prelude.airl`:

| Rust Builtin | AIRL Stdlib | Match | Verified | Notes |
|---|---|---|---|---|
| `airl_map` (L294) | `prelude.airl::map` (L4) | Exact | Yes | AIRL uses recursive cons; Rust uses iterative Vec |
| `airl_filter` (L313) | `prelude.airl::filter` (L14) | Exact | Yes | Same semantics |
| `airl_fold` (L336) | `prelude.airl::fold` (L26) | Exact | Yes | Both are left folds |
| `airl_sort` (L357) | `prelude.airl::sort` (L152) | Different algo | Yes | Rust: insertion sort; AIRL: merge sort. AIRL version is *better* (O(n log n) vs O(n^2)) |
| `airl_any` (L393) | `prelude.airl::any` (L102) | Exact | Yes | Both short-circuit |
| `airl_all` (L413) | `prelude.airl::all` (L119) | Exact | Yes | Both short-circuit |
| `airl_find` (L433) | `prelude.airl::find` (L127) | Exact | Yes | Both return nil on miss |

**Status:** All 7 are **NOT registered** in the AOT builtin map (`bytecode_aot.rs`). They exist only as `extern "C"` symbols in `list.rs`. The AIRL stdlib versions are what actually get used by compiled programs. These Rust functions are dead code.

**Recommendation:** Safe to remove all 7. They are unreachable — the compiler links the AIRL stdlib versions, not these Rust implementations.

## 2. Structural List Builtins (Rust → AIRL)

These Rust builtins in `crates/airl-rt/src/misc.rs` are registered in the AOT builtin map AND have AIRL equivalents in `stdlib/prelude.airl`:

| Rust Builtin | AOT Name | AIRL Stdlib | Match | Verified |
|---|---|---|---|---|
| `airl_concat_lists` (misc.rs:278) | `"concat"` | `prelude.airl::concat` (L44) | Exact | Yes |
| `airl_range` (misc.rs:292) | `"range"` | `prelude.airl::range` (L72) | Exact | Yes |
| `airl_reverse_list` (misc.rs:301) | `"reverse"` | `prelude.airl::reverse` (L37) | Exact | Yes |
| `airl_take` (misc.rs:313) | `"take"` | `prelude.airl::take` (L82) | Exact | Yes |
| `airl_drop` (misc.rs:327) | `"drop"` | `prelude.airl::drop` (L97) | Exact | Yes |
| `airl_zip` (misc.rs:341) | `"zip"` | `prelude.airl::zip` (L55) | Exact | Yes |
| `airl_flatten` (misc.rs:361) | `"flatten"` | `prelude.airl::flatten` (L66) | Exact | Yes |
| `airl_enumerate` (misc.rs:381) | `"enumerate"` | `prelude.airl::enumerate` (L179) | Exact | Yes |

**Status:** All 8 are registered in the AOT builtin map (lines 1031-1038 of `bytecode_aot.rs`). Since builtins shadow stdlib definitions of the same name, the Rust versions currently "win" at runtime. The AIRL versions are compiled into the binary but never called.

**Recommendation:** Safe to remove all 8 from the builtin registry. The AIRL stdlib versions will take over. The Rust `extern "C"` functions can also be removed from `misc.rs`. Removal requires:
1. Delete the 8 `m.insert(...)` lines from the builtin map in `bytecode_aot.rs`
2. Delete the corresponding fields from the `RuntimeFunctions` struct
3. Delete the `extern "C"` functions from `misc.rs`
4. Run the AOT test suite to verify AIRL versions produce identical output

## 3. Irreducible List Primitives (MUST KEEP)

These builtins have no AIRL equivalents — they are the foundational operations the stdlib is built on:

| Rust Builtin | AOT Name | Why Irreducible |
|---|---|---|
| `airl_head` (list.rs:31) | `"head"` | Primitive list access — requires pointer dereference |
| `airl_tail` (list.rs:43) | `"tail"` | Creates COW view with offset — cannot be done in AIRL |
| `airl_cons` (list.rs:71) | `"cons"` | Allocates new list node — requires memory allocator |
| `airl_empty` (list.rs:85) | `"empty?"` | Checks tag + length — requires runtime type inspection |
| `airl_length` (list.rs:109) | `"length"` | Polymorphic (List/Str/Map/Bytes) — needs tag dispatch |
| `airl_at` (list.rs:121) | `"at"` | Indexed access — needs pointer arithmetic |
| `airl_append` (list.rs:149) | `"append"` | COW optimization (rc==1 mutate in-place) — perf-critical |
| `airl_list_new` (list.rs:95) | (internal) | List literal construction — compiler intrinsic |
| `airl_at_or` (list.rs:200) | `"at-or"` | Safe indexing with negative check — could be AIRL but relies on `at` + match |
| `airl_set_at` (list.rs:231) | `"set-at"` | Immutable update at index — could be AIRL but O(n) copy is perf-sensitive |
| `airl_list_contains` (list.rs:262) | `"list-contains?"` | Element search using runtime equality — could be AIRL but uses internal `airl_eq` |

**Note on `at-or`, `set-at`, `list-contains?`:** These three *could* theoretically be written in AIRL, but they are simple, perf-sensitive, and already well-tested. The cost of converting them is low benefit for meaningful risk.

## 4. Map Builtins Analysis

### Irreducible Map Primitives (MUST KEEP — 7)

| Rust Builtin | AOT Name | Why Irreducible |
|---|---|---|
| `airl_map_new` | `"map-new"` | Allocates HashMap — requires runtime allocator |
| `airl_map_get` | `"map-get"` | HashMap lookup — requires internal data structure access |
| `airl_map_get_or` | `"map-get-or"` | HashMap lookup with default — same reason |
| `airl_map_set` | `"map-set"` | COW HashMap insert — perf-critical path |
| `airl_map_has` | `"map-has"` | HashMap contains_key — internal access |
| `airl_map_remove` | `"map-remove"` | COW HashMap delete — perf-critical |
| `airl_map_keys` | `"map-keys"` | Iterates HashMap keys → List — internal access |

### Map Builtins with AIRL Wrappers (3 already exist)

These are registered as builtins but could be replaced by AIRL wrappers:

| Rust Builtin | AOT Name | AIRL Wrapper Possible? | Notes |
|---|---|---|---|
| `airl_map_values` | `"map-values"` | Yes | `(map (fn [k] (map-get m k)) (map-keys m))` — but existing Rust version is more efficient (avoids N lookups) |
| `airl_map_size` | `"map-size"` | Yes | `(length (map-keys m))` — trivial wrapper |
| `airl_map_from` | `"map-from"` | Yes | Already exists as `map.airl::map-from-entries` but with different calling convention (list of pairs vs flat alternating) |

### Map AIRL Wrappers Already in stdlib/map.airl (6 functions)

These higher-level map operations are already pure AIRL, built on the 7 irreducible primitives:

| AIRL Function | Built From |
|---|---|
| `map-entries` | `map-keys` + `map-get` + `map` |
| `map-from-entries` | `fold` + `map-set` + `map-new` |
| `map-merge` | `fold` + `map-set` + `map-entries` |
| `map-map-values` | `fold` + `map-set` + `map-entries` |
| `map-filter` | `fold` + `map-set` + `map-entries` |
| `map-update` | `map-has` + `map-get` + `map-set` |
| `map-update-or` | `map-get-or` + `map-set` |
| `map-count` | `fold` + `map-entries` |

These are correct and do not need changes.

## 5. Recommended Removal Order

### Phase 1: Remove dead code (zero risk)
Remove the 7 higher-order functions from `list.rs` that are not registered in the builtin map:
- `airl_map`, `airl_filter`, `airl_fold`, `airl_sort`, `airl_any`, `airl_all`, `airl_find`

These are completely unreachable. No behavioral change.

### Phase 2: Swap structural builtins to AIRL (low risk)
Remove the 8 structural builtins from the registry and `misc.rs`:
- `concat`, `range`, `reverse`, `take`, `drop`, `zip`, `flatten`, `enumerate`

**Validation:** The AIRL versions are already compiled into every binary. After removing the builtins from the registry, the compiler will resolve these names to the AIRL stdlib definitions instead. Run the full AOT test suite to confirm output is identical.

**Performance note:** The Rust versions are iterative (O(1) stack) while the AIRL versions are recursive (O(n) stack). For lists under ~10,000 elements this is irrelevant. For larger lists, consider keeping the Rust builtins or adding tail-call optimization.

### Phase 3: Consider map wrapper builtins (optional, low priority)
`map-values` and `map-size` could become AIRL wrappers, but the Rust versions are more efficient. Low benefit.

## 6. Surprises

1. **`airl_sort` uses insertion sort (O(n^2))** while the AIRL `sort` uses merge sort (O(n log n)). The AIRL version is strictly better. This is the strongest argument for Phase 1 removal.

2. **The 7 higher-order Rust builtins are completely dead code.** They are never registered in the AOT builtin map. The stdlib has been providing `map`, `filter`, `fold`, `sort`, `any`, `all`, `find` all along.

3. **The 8 structural builtins shadow the AIRL versions.** Because they ARE registered, the AIRL `concat`, `reverse`, etc. in `prelude.airl` are compiled but never called — the Rust builtins take priority. The AIRL versions have already been verified working (see test output above).

4. **`map-from` calling convention mismatch.** The Rust `airl_map_from` takes a flat alternating list `["k1" v1 "k2" v2]`, while the AIRL `map-from-entries` takes a list of pairs `[["k1" v1] ["k2" v2]]`. These serve different use cases and both should be kept.
