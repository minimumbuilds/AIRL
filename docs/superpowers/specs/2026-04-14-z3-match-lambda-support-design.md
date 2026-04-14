# Z3 Match/Lambda Support in Contracts — Design Spec

**Date:** 2026-04-14
**Status:** Draft
**Scope:** Enable `match` and `lambda` expressions in contract clauses to be verified by Z3, removing the `TranslationError` fallback.

## Background

Contract clauses using `match` or `lambda` expressions are rejected by the Z3 translator (`crates/airl-solver/src/translate.rs`) with explicit `UnsupportedExpression` errors:

- `match`: `"match expressions in contracts require explicit encoding — use if/cond instead"` (lines 262, 358, 488)
- `lambda`: `"lambda expressions cannot appear in Z3 contracts"` (lines 258, 354, 484)

These rejections cause `VerifyResult::TranslationError`, which falls back to runtime-only enforcement. Users who write contracts using pattern matching must manually rewrite them as nested `if`/`cond`, which is error-prone and less readable.

## Design

### Match → ITE Chain

AIRL `match` expressions have a fixed structure: a scrutinee and a list of arms, each with a pattern and a body. For Z3, translate each arm as a nested `if-then-else` (ITE) chain:

```lisp
;; AIRL:
(match x
  (Ok v)  (> v 0)
  (Err _) false)

;; Z3 encoding:
(ite (is-Ok x) (> (Ok-value x) 0)
     (ite (is-Err x) false
          false))  ;; unreachable default
```

#### Supported match patterns

| Pattern | Z3 encoding | Notes |
|---------|------------|-------|
| `(VariantName binding)` | `(ite (is-VariantName scrutinee) body[binding := VariantName-value(scrutinee)] ...)` | ADT accessor |
| `_` (wildcard) | Default/else branch | Always last |
| `literal` (int, bool, string) | `(ite (= scrutinee literal) ...)` | Equality test |
| `binding` (bare name) | Bind scrutinee to name in body | Like wildcard but named |

#### Variant type encoding

For match on variant types (`Ok`/`Err`, user-defined ADTs), declare Z3 datatypes:

```smt2
(declare-datatypes ((Result 0))
  ((Ok (Ok-value Int))
   (Err (Err-value String))))
```

This requires:
1. Detecting which variant types appear in contracts
2. Declaring them as Z3 algebraic datatypes at the start of verification
3. Translating variant constructors and destructors

**Limitation:** Only closed, non-recursive variant types are supported. Recursive types (e.g., `List`) fall back to `Unknown`.

### Lambda → Uninterpreted Functions

Lambda expressions in contracts are typically used as predicate arguments (e.g., in `forall`/`exists` with a filter, or in higher-order contract combinators).

For Z3, translate lambdas as inline expansion when the application site is known:

```lisp
;; AIRL contract:
(forall (x : i32) (> ((fn [y] (* y y)) x) 0))

;; Z3 encoding (inline expansion):
(forall ((x Int)) (> (* x x) 0))
```

#### Supported lambda patterns

| Pattern | Z3 encoding | Notes |
|---------|------------|-------|
| Lambda immediately applied | Inline-expand: substitute args into body | Most common case |
| Lambda passed to `forall`/`exists` guard | Inline-expand within quantifier body | |
| Lambda stored in variable | `Unknown` fallback | Too complex for reliable expansion |
| Recursive lambda | `Unknown` fallback | Undecidable in general |

### Implementation

#### Match translation

**File:** `crates/airl-solver/src/translate.rs`

Replace the three `ExprKind::Match` error arms (lines 262, 358, 488) with:

```rust
ExprKind::Match(scrutinee, arms) => {
    self.translate_match_bool(scrutinee, arms)
    // (or translate_match_int / translate_match_real for the other contexts)
}
```

New method:

```rust
fn translate_match_bool<'a>(
    &self,
    scrutinee: &Expr,
    arms: &[MatchArm],
) -> Result<z3::ast::Bool<'a>, TranslateError> {
    // Build ITE chain from arms, innermost-out
    let mut result = self.ctx.bool_val(false); // unreachable default
    for arm in arms.iter().rev() {
        let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;
        let body = self.with_pattern_bindings(scrutinee, &arm.pattern, |t| {
            t.translate_bool(&arm.body)
        })?;
        result = condition.ite(&body, &result);
    }
    Ok(result)
}
```

#### Lambda inline expansion

**File:** `crates/airl-solver/src/translate.rs`

Replace the three `ExprKind::Lambda` error arms with:

```rust
ExprKind::Lambda(params, body) => {
    // Lambda can only be translated when immediately applied or
    // used as a predicate in forall/exists. Store for inline expansion.
    Err(TranslateError::UnsupportedExpression(
        "standalone lambda — must be applied inline or used in quantifier".into()
    ))
}
```

In `translate_bool` for `FnCall`, when the callee is a `Lambda`:

```rust
ExprKind::FnCall(callee, args) if matches!(callee.kind, ExprKind::Lambda(..)) => {
    // Inline-expand: substitute args for params in lambda body
    if let ExprKind::Lambda(params, body) = &callee.kind {
        for (param, arg) in params.iter().zip(args) {
            // Declare arg as Z3 variable, bind to translated arg value
            self.translate_and_bind(&param.name, &param.ty, arg)?;
        }
        self.translate_bool(body)
    } else {
        unreachable!()
    }
}
```

### Variant Type Registration

**File:** `crates/airl-solver/src/translate.rs`

Add a pre-pass that scans contract expressions for variant constructors/destructors and declares Z3 datatypes:

```rust
fn declare_variant_types(&mut self, ctx: &'a Context, exprs: &[Expr]) {
    // Scan for ExprKind::Match with VariantCtor patterns
    // For each unique variant type, create Z3 DatatypeSort
    // Store in self.variant_sorts: HashMap<String, DatatypeSort>
}
```

This runs once per function, before translating any clauses.

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-solver/src/translate.rs` | Replace match/lambda error arms with translation logic; add variant type declaration; add inline lambda expansion |

## Testing

New test cases in `crates/airl-solver/src/prover.rs` `#[cfg(test)]`:

```rust
#[test]
fn prove_match_on_result() {
    // (defn handle [(r : Result) -> i32]
    //   :ensures [(>= result 0)]
    //   :body (match r (Ok v) v (Err _) 0))
    // Z3 should prove result >= 0 given body translation
}

#[test]
fn prove_lambda_inline_application() {
    // :ensures [((fn [x] (>= x 0)) result)]
    // Equivalent to (>= result 0) after inline expansion
}

#[test]
fn match_with_literal_patterns() {
    // (match x 0 "zero" 1 "one" _ "other")
    // Z3 should handle integer literal patterns
}
```

New fixture files:
- `tests/fixtures/valid/match_contract.airl` — function with match in `:ensures`, Z3-provable
- `tests/fixtures/valid/lambda_contract.airl` — function with inline lambda in `:ensures`

## Limitations

- Recursive variant types (e.g., `List`) are not supported — fall to `Unknown`
- Lambdas that are not immediately applied fall to `Unknown`
- Match exhaustiveness is not checked by Z3 (assumed by the type checker)
- Nested match expressions are supported (recursive ITE chain) but may hit Z3 timeout for deep nesting

## Dependencies

None — this is independent of Phase 2A/2B and can be implemented at any time. It improves the set of contracts Z3 can verify, making Phase 2B's opcode elision more effective.
