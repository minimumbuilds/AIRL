# Linearity Enforcement for Default Ownership — Design Spec

**Date:** 2026-04-14
**Status:** Draft
**Scope:** Make the linearity checker enforce move semantics for default-ownership parameters, closing the gap where most AIRL code gets no meaningful ownership checking.

## Background

The linearity checker (`crates/airl-types/src/linearity.rs`) runs on ALL functions unconditionally. However, its behavior depends on parameter ownership annotations:

- **`Own`/`Ref`/`Mut`** — explicit annotations trigger full move/borrow tracking. `Own` parameters are tracked as `Moved` after being passed to another `Own` context.
- **`Default`** — no annotation. The checker introduces these parameters as `Owned` (line 61) but the `check_arg()` call path (lines 361-365) falls through to generic recursion without move/borrow tracking.

**Current behavior:** `crates/airl-driver/src/pipeline.rs` creates `LinearityChecker` for every compile. `build_ownership_map()` (lines 512-527) only includes functions where at least one parameter has `Ownership::Own`. Functions without annotations are absent from the map entirely, so all their arguments get `Ownership::Default` in `check_arg()`.

**Impact:** The vast majority of AIRL code uses default ownership and receives no linearity enforcement — no use-after-move detection, no borrow-while-moved errors, nothing. The linearity infrastructure is always running but doing no useful work for most programs.

## Design Options

### Option A: Default-as-Own (Recommended)

Treat `Ownership::Default` as `Ownership::Own` in the linearity checker. All parameters are move-tracked by default. Users must explicitly annotate `Ref` or `Mut` for shared/borrowed access.

**Rationale:** This is the safe default. AIRL's ownership model is designed for safety — making it opt-in undermines the design. Languages like Rust default to move semantics for good reason.

**Risk:** May break existing code that implicitly uses values after "moving" them. Mitigation: roll out with warnings first, then escalate to errors.

### Option B: Default-as-Ref

Treat `Ownership::Default` as `Ownership::Ref` — immutable borrow by default. Parameters can be read freely but not moved or mutated without explicit annotation.

**Rationale:** Less breaking than Option A. Catches mutation-without-annotation bugs while allowing free reads.

**Risk:** Doesn't catch use-after-move for values passed to `Own` parameters of other functions.

### Option C: Phased Rollout

Phase 1: Change `Default` to `Ref` (less breaking). Phase 2: Change `Default` to `Own` after codebase adapts.

## Changes Required (Option A)

### 1. Update `build_ownership_map()`

**File:** `crates/airl-driver/src/pipeline.rs` lines 512-527

Currently only includes functions with at least one `Own` parameter. Change to include ALL `defn` functions:

```rust
fn build_ownership_map(tops: &[TopLevel]) -> HashMap<String, Vec<Ownership>> {
    let mut map = HashMap::new();
    for top in tops {
        if let TopLevel::Defn(f) = top {
            let ownerships: Vec<Ownership> = f.params.iter().map(|p| {
                if p.ownership == Ownership::Default {
                    Ownership::Own  // <-- default to Own
                } else {
                    p.ownership
                }
            }).collect();
            map.insert(f.name.clone(), ownerships);
        }
    }
    map
}
```

### 2. Update `check_arg()` fallthrough

**File:** `crates/airl-types/src/linearity.rs` lines 361-365

The catch-all case for `Ownership::Default` should track moves:

```rust
// Before:
_ => {
    self.check_expr(arg);
}

// After:
Ownership::Default => {
    // Default ownership treated as Own — track moves
    self.track_move(arg);
}
```

### 3. Warning mode for migration

Add a `--linearity-warn` flag (or pipeline config) that downgrades linearity errors from `Error` to `Warning` for a transition period. This lets existing code compile while authors add explicit `Ref`/`Mut` annotations.

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-driver/src/pipeline.rs` | `build_ownership_map()` includes all functions; `Default` → `Own` |
| `crates/airl-types/src/linearity.rs` | `check_arg()` `Default` case tracks moves |

## Testing

1. Existing linearity test fixtures in `tests/fixtures/linearity_errors/` must still fail with proper errors
2. Existing valid fixtures must still pass (most don't use values after implicit moves)
3. Add new fixtures demonstrating default-ownership enforcement:
   - `tests/fixtures/linearity_errors/default_use_after_move.airl` — use a default-ownership value after passing it to an `Own` parameter
   - `tests/fixtures/valid/default_single_use.airl` — single-use of default-ownership values passes

## Migration Impact

Scan the codebase and stdlib for functions that use default-ownership parameters multiple times. These will need explicit `Ref` annotations. Run `airl check` on all `.airl` files in `stdlib/` and `bootstrap/` to measure breakage.
