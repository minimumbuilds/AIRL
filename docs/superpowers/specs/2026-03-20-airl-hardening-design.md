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

`pipeline.rs` goes directly from parsing to evaluation. The `TypeChecker` in `airl-types` is only used in unit tests. The existing `check_source` function only parses — it does not run the type checker despite the `check` name.

### Changes

Add a `PipelineMode` enum and a `TypeCheck` variant to `PipelineError`:
```rust
pub enum PipelineMode {
    Check,  // strict: type errors block, exit non-zero
    Run,    // warn: type errors printed to stderr, execution proceeds
    Repl,   // warn: type errors printed to stderr, execution proceeds
}

// Add to PipelineError:
pub enum PipelineError {
    Io(String),
    Syntax(Diagnostic),
    Parse(Diagnostics),
    TypeCheck(Diagnostics),  // NEW: type-check errors
    Runtime(RuntimeError),
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

### TypeChecker API Usage

The `TypeChecker` stores diagnostics internally and returns `Result<_, ()>`. To integrate:
1. Create `TypeChecker`, call `check_top_level` for each parsed form
2. After all forms are checked, call `checker.has_errors()` to test for failures
3. Extract diagnostics via `checker.into_diagnostics()` (consuming) or iterate via the checker's error accessors
4. In `Check` mode: if errors exist, return `PipelineError::TypeCheck(diags)`
5. In `Run`/`Repl` mode: if errors exist, print them to stderr as warnings, then proceed to evaluation regardless

**Note:** `check_source` must be substantially rewritten — it currently only parses. After this change it will run the full type-check pass.

### Persistent State

In REPL mode, the `TypeChecker` persists alongside the `Interpreter` so that function/type definitions accumulate across expressions. Both are passed through `eval_repl_input`.

### Files Modified

- `crates/airl-driver/src/pipeline.rs` — add `PipelineMode`, `PipelineError::TypeCheck`, rewrite `check_source`, wire `TypeChecker` into `run_source`
- `crates/airl-driver/src/main.rs` — pass mode to pipeline functions
- `crates/airl-driver/src/repl.rs` — persistent `TypeChecker`
- `crates/airl-driver/src/lib.rs` — re-export new types if needed

---

## 2. Exhaustiveness — Runtime Fallback

### Current State

The exhaustiveness checker is called by `TypeChecker::check_match_expr()` but since the type checker isn't wired in, it never runs. The evaluator already returns an error (`RuntimeError::Custom("no match arm matched value: ...")`) when no arm matches — this is functional but uses a generic error type.

### Changes

1. **Static:** Wiring the type checker (section 1) automatically enables exhaustiveness checking during the type-check pass.

2. **Runtime:** Replace the generic `Custom` error with a dedicated `RuntimeError::NonExhaustiveMatch { value: String }` variant for better error messages and programmatic matching. The existing runtime behavior (error on no match) is preserved — this is a refinement, not a new feature.

### Files Modified

- `crates/airl-runtime/src/error.rs` — add `NonExhaustiveMatch` variant
- `crates/airl-runtime/src/eval.rs` — replace `Custom("no match arm...")` with `NonExhaustiveMatch`

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

**Borrow release:** Each function call maintains a per-call borrow ledger — a `Vec<(String, BorrowKind)>` tracking which slots were borrowed for that specific call. When the call returns, only the borrows in the ledger are released (decrement `immutable_borrows` by 1 per immutable borrow, clear `mutable_borrow` per mutable borrow). This correctly handles nested calls where the same slot is borrowed at multiple call levels.

**`(copy x)` in function parameters:** When a function parameter has `Ownership::Copy`, the evaluator clones the value without marking the source as moved. It checks that the value's type supports Copy (primitives except String are Copy; tensors, functions, strings are not). There is no separate `ExprKind::Copy` AST node — copy semantics are triggered by the parameter annotation in the callee's signature, not by an expression form in the caller.

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
- For moved slots, append `[moved]` to indicate the binding has been consumed

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
