# AIRL Standard Library: Set

> Source: `stdlib/set.airl` | 11 functions | Auto-loaded

Set data structure implemented as maps with `true` values. All functions operate on Map values where keys are the set elements.

**Limitation:** Set elements must be strings (AIRL map keys are strings only). Use `int-to-string` for numeric sets.

## Dependencies

Relies on Map builtins (`map-new`, `map-set`, `map-has`, `map-remove`, `map-keys`, `map-size`) and Collections (`fold`, `all`).

## Functions

### Creation

```lisp
(set-new)                    ;; → empty set
(set-from ["a" "b" "c"])     ;; → set with 3 elements
```

### Mutation (returns new set)

```lisp
(set-add s "x")              ;; → set with "x" added
(set-remove s "x")           ;; → set with "x" removed
```

### Queries

```lisp
(set-contains? s "x")        ;; → true/false
(set-size s)                  ;; → Int
(set-to-list s)               ;; → List[Str]
```

### Set Operations

```lisp
(set-union a b)               ;; → elements in a or b
(set-intersection a b)        ;; → elements in both a and b
(set-difference a b)          ;; → elements in a but not b
(set-subset? a b)             ;; → true if all elements of a are in b
(set-equal? a b)              ;; → true if same elements
```

## Quick Reference

| Function | Signature | Returns | Notes |
|----------|-----------|---------|-------|
| `set-new` | `(set-new)` | Map | empty set |
| `set-from` | `(set-from xs)` | Map | deduplicated |
| `set-add` | `(set-add s x)` | Map | idempotent |
| `set-remove` | `(set-remove s x)` | Map | no error if missing |
| `set-contains?` | `(set-contains? s x)` | Bool | O(1) lookup |
| `set-size` | `(set-size s)` | Int | — |
| `set-to-list` | `(set-to-list s)` | List | arbitrary order |
| `set-union` | `(set-union a b)` | Map | — |
| `set-intersection` | `(set-intersection a b)` | Map | — |
| `set-difference` | `(set-difference a b)` | Map | a \ b |
| `set-subset?` | `(set-subset? a b)` | Bool | — |
| `set-equal?` | `(set-equal? a b)` | Bool | — |
