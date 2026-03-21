# AIRL Phase 1 Hardening Design

**Date:** 2026-03-20
**Status:** Approved
**Depends on:** AIRL Phase 1 Implementation (complete, 347 tests)

---

## Overview

Wire the existing static analysis passes (type checker, exhaustiveness checker) into the compilation pipeline, add runtime linearity enforcement, and improve the REPL experience. The goal is to close the gap between "code that exists" and "code that runs" — the type checker, linearity checker, and exhaustiveness checker were built but never connected to the execution pipeline.

---

## 1. Pipeline Integration — Type Checker

### Current State

`pipeline.rs` goes directly from parsing to evaluation. The `TypeChecker` in `airl-types` is only used in unit tests.

### Changes

Add a `PipelineMode` enum:
```rust
pub enum PipelineMode {
    Check,  // strict: type errors block, exit non-zero
    Run,    // warn: type errors printed to stderr, execution proceeds
    Repl,   // warn: type errors printed to stderr, execution proceeds
}
```

The pipeline becomes:
```
Source → Lex → Parse SExpr → Parse AST → Type Check → [halt if Check + errors] → Evaluate
```

Behavior per mode:

| Command | Type Check | On Error |
|---|---|---|
| `airl check` | Yes | Print errors, exit non-zero |
| `airl run` | Yes | Print warnings to stderr, proceed |
| `airl repl` | Yes (per expr) | Print warnings, proceed |

### Persistent State

In REPL mode, the `TypeChecker` persists alongside the `Interpreter` so that function/type definitions accumulate across expressions. Both are passed through `eval_repl_input`.

### Files Modified

- `crates/airl-driver/src/pipeline.rs` — add `PipelineMode`, wire `TypeChecker` into `run_source`/`check_source`
- `crates/airl-driver/src/main.rs` — pass mode to pipeline functions
- `crates/airl-driver/src/repl.rs` — persistent `TypeChecker`
- `crates/airl-driver/src/lib.rs` — re-export new types if needed

---

## 2. Exhaustiveness — Runtime Fallback

### Current State

The exhaustiveness checker is called by `TypeChecker::check_match_expr()` but since the type checker isn't wired in, it never runs. Additionally, `eval.rs` silently falls through if no match arm matches.

### Changes

1. **Static:** Wiring the type checker (section 1) automatically enables exhaustiveness checking during the type-check pass.

2. **Runtime:** Add `RuntimeError::NonExhaustiveMatch` variant. When `eval` processes a match expression and no arm matches the scrutinee, return this error instead of falling through.

### Files Modified

- `crates/airl-runtime/src/error.rs` — add `NonExhaustiveMatch` variant
- `crates/airl-runtime/src/eval.rs` — return error when no match arm matches

---

## 3. Runtime Linearity Enforcement

### Current State

`Env`'s `Slot` has a `moved` flag and `moved_at` span. `get()` errors on moved slots. But `mark_moved()` is never called by the evaluator — values are cloned out freely.

### Changes

#### Slot Enhancement

```rust
pub struct Slot {
    pub value: Value,
    pub moved: bool,
    pub moved_at: Option<Span>,
    pub immutable_borrows: u32,
    pub mutable_borrow: bool,
}
```

#### Evaluator Changes

When calling a function, check each parameter's ownership annotation:

| Annotation | Evaluator Behavior |
|---|---|
| `Ownership::Own` | Clone value out, mark source binding as moved |
| `Ownership::Ref` | Clone value out (read), increment `immutable_borrows` on source. Error if `mutable_borrow` is true. |
| `Ownership::Mut` | Clone value out, set `mutable_borrow` on source. Error if `immutable_borrows > 0` or `mutable_borrow` already true. |
| `Ownership::Copy` | Clone value out. Error if type is not Copy (tensors, functions, strings are not Copy). |
| `Ownership::Default` | Treated as `Own`. |

**Borrow release:** When a function call returns, release all borrows taken for that call (decrement immutable_borrows, clear mutable_borrow).

**`(copy x)` expression:** When the evaluator encounters a SymbolRef that represents a copy operation, clone without moving. This is handled in `FnCall` when the parameter has `Ownership::Copy`.

#### What This Catches

- Use after move (value consumed by `own` parameter, then accessed again)
- Mutable borrow while immutable borrows exist
- Multiple mutable borrows
- Move while borrowed
- Copy on non-Copy type

#### What This Does NOT Catch (Deferred)

- Branch divergence (if one branch moves and other doesn't)
- Lifetime analysis across scopes
- Static detection of any of the above

### Files Modified

- `crates/airl-runtime/src/env.rs` — add borrow fields to Slot, add borrow tracking methods
- `crates/airl-runtime/src/eval.rs` — enforce ownership in `call_fn`, release borrows after call

---

## 4. REPL Enhancements

### `:env` Command

Display all user-defined bindings and functions, skipping builtins:

```
airl> :env
── Bindings ──
  x : i32 = 42
  name : String = "hello"

── Functions ──
  add-one : (i32) -> i32
  safe-divide : (i32, i32) -> Result[i32, DivError]
```

Walk the interpreter's `Env` frames. For each slot:
- Skip `Value::BuiltinFn` entries
- For `Value::Function(f)`, extract and format the signature from `f.def`
- For other values, show `name = display_value`

### Files Modified

- `crates/airl-runtime/src/env.rs` — add `iter_bindings()` method that yields `(name, &Slot)` pairs
- `crates/airl-driver/src/repl.rs` — implement `:env` handler

---

## 5. New Test Fixtures

### Type Error Fixtures (test `airl check`)

| File | What it tests |
|---|---|
| `type_errors/type_mismatch_arg.airl` | Call function with wrong arg type |
| `type_errors/if_branch_mismatch.airl` | If branches return different types |
| `type_errors/non_exhaustive_match.airl` | Match missing a variant |

### Linearity Error Fixtures (test runtime)

| File | What it tests |
|---|---|
| `linearity_errors/use_after_move.airl` | Pass with own, then reuse |
| `linearity_errors/move_while_borrowed.airl` | Borrow then move |

### Fixture Runner Update

Add a `check_fixtures` test that runs `check_source` (not `run_source`) on type error fixtures, expecting errors. This tests the type checker path specifically.

### Files Modified

- `tests/fixtures/type_errors/` — new fixture files
- `tests/fixtures/linearity_errors/` — new fixture files
- `crates/airl-driver/tests/fixtures.rs` — add `check_fixtures` test function

---

## Not In Scope

- Static linearity analysis (requires control flow analysis)
- `:type` REPL command (type checker in REPL context is complex)
- `--strict` flag for `airl run`
- Agent runtime wiring (separate effort)
- Contract proving improvements (Phase 2 / Z3)
