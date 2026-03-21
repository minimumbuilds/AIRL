# AIRL Z3 SMT Solver Integration Design

**Date:** 2026-03-21
**Status:** Approved
**Depends on:** Phase 1 + Hardening + Cranelift JIT (398 tests)

---

## Overview

Add formal contract verification via the Z3 SMT solver. A new `airl-solver` crate isolates the Z3 C++ dependency from the rest of the workspace. The prover translates AIRL contract expressions (`:requires`, `:ensures`) to Z3 integer/boolean assertions, negates each `:ensures` clause, and checks for UNSAT (proven) or SAT (disproven with counterexample).

Runs in the pipeline alongside the type checker: strict in `check` mode, warnings in `run` mode.

---

## 1. New Crate: `airl-solver`

Isolates the Z3 dependency. Only this crate has C++ build requirements (CMake, C++ compiler for static Z3 compilation).

Dependencies:
- `airl-syntax` — AST types (Expr, ExprKind, FnDef, AstType)
- `z3 = { version = "0.12", features = ["static-link-z3"] }`

Does NOT depend on `airl-contracts`, `airl-runtime`, or `airl-codegen`.

Position in workspace:
```
airl-solver (depends on: airl-syntax, z3)
    ↓
airl-driver (depends on: everything including airl-solver)
```

---

## 2. AIRL Expression → Z3 Translation

### Variable Setup

For a function `(defn f :sig [(a : i32) (b : i32) -> i32] ...)`:
- Create Z3 `Int` constants for each integer parameter (`a`, `b`)
- Create Z3 `Bool` constants for each boolean parameter
- Create Z3 `Int`/`Bool` constant for `result` based on return type
- Parameters with non-integer/non-boolean types → translation fails, return Unknown

### Expression Translation

| AIRL Expression | Z3 API |
|---|---|
| `IntLit(v)` | `ast::Int::from_i64(&ctx, v)` |
| `BoolLit(v)` | `ast::Bool::from_bool(&ctx, v)` |
| `SymbolRef(name)` | Look up in variable map → Z3 constant |
| `(+ a b)` | `ast::Int::add(&ctx, &[&a, &b])` |
| `(- a b)` | `ast::Int::sub(&ctx, &[&a, &b])` |
| `(* a b)` | `ast::Int::mul(&ctx, &[&a, &b])` |
| `(/ a b)` | `a.div(&b)` |
| `(% a b)` | `a.modulo(&b)` |
| `(= a b)` | `a._eq(&b)` |
| `(!= a b)` | `a._eq(&b).not()` |
| `(< a b)` | `a.lt(&b)` |
| `(> a b)` | `a.gt(&b)` |
| `(<= a b)` | `a.le(&b)` |
| `(>= a b)` | `a.ge(&b)` |
| `(and p q)` | `ast::Bool::and(&ctx, &[&p, &q])` |
| `(or p q)` | `ast::Bool::or(&ctx, &[&p, &q])` |
| `(not p)` | `p.not()` |
| `(valid x)` | `ast::Bool::from_bool(&ctx, true)` (no-op) |

### Unsupported (returns Unknown)

- Float operations (Z3 float theory is slow and incomplete)
- Collection operations (`length`, `at`, `forall`, `exists`)
- Non-builtin function calls in contracts
- `match` expressions in contracts
- Any expression involving non-integer/non-boolean types

When the translator encounters an unsupported form, it returns `TranslateError::Unsupported` and the prover returns `Unknown` for that contract clause.

---

## 3. Proof Strategy

To verify a function's contracts:

```
1. Create Z3 Context and Solver
2. Create Z3 constants for all params + "result"
3. Assert all :requires clauses as assumptions
4. For each :ensures clause:
   a. solver.push()  (create checkpoint)
   b. Translate clause to Z3 Bool
   c. Assert the NEGATION of the clause
   d. solver.check()
      - Unsat → clause is PROVEN (no counterexample exists)
      - Sat → clause is DISPROVEN (extract counterexample from model)
      - Unknown → UNKNOWN (fall back to runtime)
   e. solver.pop()  (restore checkpoint)
5. Return results for all clauses
```

### Result Type

```rust
pub enum VerifyResult {
    Proven,
    Disproven { counterexample: Vec<(String, String)> },  // [(var, value)]
    Unknown(String),  // reason
    TranslationError(String),  // couldn't translate to Z3
}
```

### Function-Level Result

```rust
pub struct FunctionVerification {
    pub function_name: String,
    pub requires_ok: bool,  // all requires translated successfully
    pub ensures: Vec<(String, VerifyResult)>,  // (clause_source, result)
}
```

---

## 4. Pipeline Integration

### Where it runs

After type checking, before evaluation. Same slot in the pipeline as the type checker.

```
Source → Lex → Parse → Type Check → Z3 Verify → [halt/warn] → Evaluate
```

### Mode-dependent behavior

| Mode | Proven | Disproven | Unknown |
|---|---|---|---|
| `check` | note | **error** (with counterexample) | warning |
| `run` | note | warning | silent |
| `repl` | note | warning | silent |

### What to prove

Try to prove every `defn`'s contracts, regardless of `:verify` level. The `:verify proven` annotation could later be used to make proof failure an error even in `run` mode, but for now we keep it simple.

### Runtime interaction

For Phase 1, runtime assertions always run regardless of proof results. The value of Z3 is catching bugs at compile time, not optimizing runtime checks. A future optimization: skip runtime assertions for proven clauses.

---

## 5. Testing

### Unit tests (airl-solver)

Translation tests:
- Translate `(+ a b)` → verify Z3 Int::add produced
- Translate `(= result (+ a b))` → verify Z3 equality
- Translate `(valid x)` → verify returns `true`
- Translate unsupported expression → TranslateError

Proof tests:
- Prove `(= result (+ a b))` for `add(a, b)` → Proven
- Prove `(>= result 0)` given `:requires [(>= a 0) (>= b 0)]` for `(+ a b)` → Proven
- Disprove `(= result (+ a b))` for `(* a b)` → Disproven with counterexample
- `(valid result)` as the only ensures clause → Proven (trivially true)
- Function with String params → Unknown (unsupported type)

### Integration tests

- `airl check` on a module with provable contracts → prints proof notes
- `airl check` on a module with disprovable contracts → prints error with counterexample
- Existing 398 tests all pass unchanged

### Fixture

`tests/fixtures/valid/proven_contracts.airl`:
```clojure
;; EXPECT: 7
;; Contracts should be provable by Z3
(defn add
  :sig [(a : i32) (b : i32) -> i32]
  :intent "add two integers"
  :requires [(valid a) (valid b)]
  :ensures [(= result (+ a b))]
  :body (+ a b))
(add 3 4)
```

---

## 6. Files

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `airl-solver` to members |
| `crates/airl-solver/Cargo.toml` | New — z3 + airl-syntax deps |
| `crates/airl-solver/src/lib.rs` | Module exports, VerifyResult, FunctionVerification |
| `crates/airl-solver/src/translate.rs` | AIRL Expr → Z3 AST translation |
| `crates/airl-solver/src/prover.rs` | Z3Prover with verify_function |
| `crates/airl-driver/Cargo.toml` | Add airl-solver dependency |
| `crates/airl-driver/src/pipeline.rs` | Wire Z3Prover after type checking |
| `crates/airl-driver/src/main.rs` | Handle VerifyResult in error printing |

---

## 7. Build Requirements

The `static-link-z3` feature compiles Z3 from vendored C++ source. Requires:
- C++ compiler (`g++` or `clang++`)
- CMake
- Python 3 (Z3's build system)
- First build takes 5-15 minutes (Z3 compilation)
- Subsequent builds are fast (Z3 is cached)

All other crates remain unaffected — only `airl-solver` has the C++ build dependency.

---

## 8. Not In Scope

- Float contract verification (Z3 float theory)
- Quantifier support (`forall`/`exists`)
- Skipping runtime assertions for proven contracts
- `:verify proven` vs `:verify checked` differentiation (all functions attempted)
- Proof caching to disk
- Interactive proof exploration
