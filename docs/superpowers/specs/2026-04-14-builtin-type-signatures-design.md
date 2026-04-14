# Builtin Type Signatures — Design Spec

**Date:** 2026-04-14
**Status:** Draft
**Scope:** Replace `TypeVar("builtin")` with proper `Ty::Func` signatures for 100+ builtins so the type checker validates argument count and types at compile time.

## Background

The type checker (`crates/airl-types/src/checker.rs`) registers 100+ builtins as `Ty::TypeVar("builtin")` (lines 105-172). When called, the checker:
1. Evaluates argument sub-expressions
2. Does NOT validate argument count or types
3. Returns `Ty::TypeVar("_")` — type information is lost

Only 19 builtins have proper signatures: 11 arithmetic/comparison operators and 8 collection operations (`head`, `tail`, `cons`, `empty?`, `map`, `filter`, `fold`, `str`) added in `register_typed_builtins()` (lines 180-255).

**Impact:** Type errors in 100+ builtin calls are caught only at runtime as panics. This is the largest type-checking hole in AIRL and undermines contract verification — contracts on functions calling untyped builtins are partially opaque to Z3.

## Design

### Approach: Incremental Typing by Category

Add `Ty::Func` signatures to `register_typed_builtins()` in batches, ordered by usage frequency and safety impact. Each builtin gets:
- Parameter types (exact or generic `TypeVar("_")` wildcard)
- Return type
- Arity enforcement (min args, max args)

Builtins that genuinely accept any type (e.g., `print`, `type-of`) keep `TypeVar("_")` parameters but gain arity checking.

### Type Signature Categories

#### Tier 1: String operations (23 builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `char-at` | `(String, Int) -> String` | |
| `substring` | `(String, Int, Int) -> String` | |
| `split` | `(String, String) -> List` | |
| `join` | `(List, String) -> String` | |
| `replace` | `(String, String, String) -> String` | |
| `chars` | `(String) -> List` | |
| `words` | `(String) -> List` | |
| `unwords` | `(List) -> String` | |
| `lines` | `(String) -> List` | |
| `unlines` | `(List) -> String` | |
| `repeat-str` | `(String, Int) -> String` | |
| `pad-left` | `(String, Int, String) -> String` | |
| `pad-right` | `(String, Int, String) -> String` | |
| `is-empty-str` | `(String) -> Bool` | |
| `reverse-str` | `(String) -> String` | |
| `count-occurrences` | `(String, String) -> Int` | |

#### Tier 2: Map operations (10 builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `map-new` | `() -> Map` | |
| `map-get` | `(Map, T) -> T` | Key/value generic |
| `map-set` | `(Map, T, T) -> Map` | |
| `map-has` | `(Map, T) -> Bool` | |
| `map-remove` | `(Map, T) -> Map` | |
| `map-keys` | `(Map) -> List` | |
| `map-entries` | `(Map) -> List` | |
| `map-from-entries` | `(List) -> Map` | |
| `map-merge` | `(Map, Map) -> Map` | |
| `map-count` | `(Map) -> Int` | |

#### Tier 3: List operations (15 builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `length` | `(T) -> Int` | Works on List, String, Map |
| `at` | `(List, Int) -> T` | |
| `at-or` | `(List, Int, T) -> T` | |
| `set-at` | `(List, Int, T) -> List` | |
| `list-contains?` | `(List, T) -> Bool` | |
| `append` | `(List, T) -> List` | |
| `reverse` | `(List) -> List` | |
| `concat` | `(List, List) -> List` | |
| `zip` | `(List, List) -> List` | |
| `flatten` | `(List) -> List` | |
| `range` | `(Int, Int) -> List` | |
| `take` | `(List, Int) -> List` | |
| `drop` | `(List, Int) -> List` | |
| `sort` | `(List) -> List` | |
| `find` | `((T -> Bool), List) -> T` | |

#### Tier 4: Math operations (15 builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `abs` | `(Int) -> Int` | Stdlib |
| `min` | `(Int, Int) -> Int` | |
| `max` | `(Int, Int) -> Int` | |
| `clamp` | `(Int, Int, Int) -> Int` | |
| `sqrt` | `(Float) -> Float` | |
| `sin` | `(Float) -> Float` | |
| `cos` | `(Float) -> Float` | |
| `tan` | `(Float) -> Float` | |
| `log` | `(Float) -> Float` | |
| `exp` | `(Float) -> Float` | |
| `floor` | `(Float) -> Int` | |
| `ceil` | `(Float) -> Int` | |
| `round` | `(Float) -> Int` | |
| `int-to-float` | `(Int) -> Float` | |
| `float-to-int` | `(Float) -> Int` | |

#### Tier 5: I/O, System, Conversion (30+ builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `read-file` | `(String) -> String` | |
| `write-file` | `(String, String) -> Bool` | |
| `file-exists?` | `(String) -> Bool` | |
| `shell-exec` | `(String) -> String` | |
| `time-now` | `() -> Int` | |
| `sleep` | `(Int) -> Nil` | |
| `getenv` | `(String) -> String` | |
| `get-args` | `() -> List` | |
| `int-to-string` | `(Int) -> String` | |
| `string-to-int` | `(String) -> Int` | |
| `print` | `(T) -> Nil` | Arity 1, any type |
| `type-of` | `(T) -> String` | Arity 1, any type |
| ... | ... | Remaining I/O, bytes, TCP, thread, crypto builtins |

#### Tier 6: Result operations (8 builtins)

| Builtin | Signature | Notes |
|---------|-----------|-------|
| `is-ok?` | `(Result) -> Bool` | |
| `is-err?` | `(Result) -> Bool` | |
| `unwrap-or` | `(Result, T) -> T` | |
| `map-ok` | `((T -> U), Result) -> Result` | |
| `map-err` | `((T -> U), Result) -> Result` | |
| `and-then` | `((T -> Result), Result) -> Result` | |
| `or-else` | `((T -> Result), Result) -> Result` | |
| `ok-or` | `(T, T) -> Result` | |

### Implementation Pattern

Each builtin moves from `register_builtins()` to `register_typed_builtins()`:

```rust
// Before (in register_builtins):
self.env.bind_str("char-at", Ty::TypeVar(builtin_sym));

// After (in register_typed_builtins):
self.bind_typed("char-at", &[
    Ty::Named("String".into()),
    Ty::Named("Int".into()),
], Ty::Named("String".into()));
```

A helper method `bind_typed` wraps the `Ty::Func` construction:

```rust
fn bind_typed(&mut self, name: &str, params: &[Ty], ret: Ty) {
    self.env.bind_str(name, Ty::Func {
        params: params.to_vec(),
        ret: Box::new(ret),
    });
}
```

### Invariant

From `checker.rs` lines 100-104: any builtin with a typed signature in `register_typed_builtins()` must NOT appear in the `register_builtins()` list. Each migration removes the name from the untyped list and adds it to the typed function.

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-types/src/checker.rs` | Move builtins from `register_builtins()` to `register_typed_builtins()`; add `bind_typed` helper |

## Testing

For each tier, add test cases in `crates/airl-types/src/checker.rs` `#[cfg(test)]`:
- Correct call compiles without error
- Wrong argument count produces type error
- Wrong argument type produces type error

Run: `cargo test -p airl-types`

Regression: full fixture suite must still pass — some fixtures may need adjustment if they were relying on untyped builtins accepting wrong arguments (these would be latent bugs, not regressions).

## Rollout

Tiers can be implemented independently and merged separately. Each tier is a self-contained PR. Priority order: Tier 1 (strings) and Tier 3 (lists) first — these are the most commonly used in AIRL programs.
