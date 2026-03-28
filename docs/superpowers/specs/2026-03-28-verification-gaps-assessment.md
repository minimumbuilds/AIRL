# AIRL Verification Gaps — Assessment

**Date:** 2026-03-28
**Status:** Confirmed — all three claims verified against source code
**Context:** Response to claims about Z3, linearity, and type checking limitations

---

## 1. Z3 Verification Is Informational Only

**Confirmed.** Z3 contract verification never blocks execution.

| Mode | Disproven contract | Effect |
|------|-------------------|--------|
| `check` | Prints `"error: contract disproven..."` to stderr | Continues — no hard error |
| `run` | Prints `"warning: contract disproven..."` to stderr | Continues |
| `repl` | Prints `"warning: ..."` | Continues |

**Source:** `crates/airl-driver/src/pipeline.rs` lines 111-124. The `VerifyResult::Disproven` handler calls `eprintln!()` but never returns `Err`. Execution proceeds unconditionally to compilation (line 138+).

**Runtime contracts are the actual enforcement.** `:requires`/`:ensures`/`:invariant` clauses are compiled to `AssertRequires`/`AssertEnsures`/`AssertInvariant` bytecode opcodes (`crates/airl-runtime/src/bytecode_compiler.rs` lines 867-930) and checked at runtime (`bytecode_vm.rs` lines 1169-1198). These always execute regardless of Z3 proof results.

**Design intent** (from `docs/superpowers/specs/2026-03-21-z3-integration-design.md` line 146): "For Phase 1, runtime assertions always run regardless of proof results. The value of Z3 is catching bugs at compile time, not optimizing runtime checks."

**Note:** Clauses referencing `result` are suppressed entirely (known false positive — Z3 doesn't constrain `result` to the function body's return value).

---

## 2. Linearity Checking Is Opt-In

**Confirmed.** Only parameters with explicit `Own`/`Ref`/`Mut` annotations are tracked.

**Evidence:**

- `crates/airl-driver/src/pipeline.rs` lines 512-527: `build_ownership_map()` only includes functions where at least one parameter has `Ownership::Own`. Functions without explicit annotations are absent from the map entirely.

- `crates/airl-types/src/linearity.rs` lines 189-196: `extract_callee_ownerships()` returns empty for unregistered functions. All arguments default to `Ownership::Default`, which triggers no move/borrow tracking.

- `pipeline.rs` lines 70-97: Linearity errors are fatal only in `Check` mode. In `Run`/`Repl` modes, they print as `"warning (linearity): ..."` and execution continues. Runtime enforcement (`MarkMoved`/`CheckNotMoved` opcodes) is the backstop.

**Implication:** The vast majority of AIRL code (which uses default ownership) has no linearity checking at compile time or runtime.

---

## 3. Type Checking Is Incomplete for Builtins

**Confirmed.** 45+ builtins are registered as `Ty::TypeVar("builtin")` — generic placeholders with no type validation.

**Evidence:** `crates/airl-types/src/checker.rs` lines 68-130:

```rust
for name in &["length", "at", "append", "head", "tail", "empty?", "cons",
    "print", "type-of", ..., "map", "filter", "fold", ...] {
    self.env.bind(name.to_string(), Ty::TypeVar("builtin".to_string()));
}
```

When the type checker encounters a call to any of these (lines 344-350):
1. Recognizes it as `Ty::TypeVar(_)`
2. Evaluates argument expressions (for sub-expression diagnostics)
3. **Does NOT validate argument count or types**
4. Returns `Ty::TypeVar("_")` — type information is lost

**What IS properly typed:** 11 arithmetic/comparison operators (`+`, `-`, `*`, `/`, `%`, `<`, `>`, `<=`, `>=`, `==`, `!=`) have `Ty::Func` signatures with proper parameter and return types (lines 31-66).

**Ratio:** ~11 properly typed vs ~45+ untyped builtins. The most commonly used functions in the language (`map`, `filter`, `fold`, `head`, `tail`, `str`, `print`, `read-file`, `json-parse`, etc.) have no compile-time type checking.

**Implication:** Type errors in builtin calls (wrong argument count, wrong types) are caught at runtime as `airl_*: type mismatch` panics, not at compile time.

---

## Summary

| Feature | Status | Enforcement |
|---------|--------|-------------|
| Z3 verification | Informational — never blocks | Runtime contract assertions are the real check |
| Linearity | Opt-in — only explicit `Own`/`Ref`/`Mut` | Runtime `MarkMoved`/`CheckNotMoved` for annotated params only |
| Type checking | Incomplete — builtins are `TypeVar("builtin")` | Runtime panics on type mismatch |

These are Phase 1 design decisions, not bugs. Z3 and linearity are incremental features. The type checker was built for the core language constructs with builtins deferred. However, they represent real gaps in compile-time safety that users should be aware of.
