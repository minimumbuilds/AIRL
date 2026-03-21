# AIRL Standard Library: Math

> Source: `stdlib/math.airl` | 13 functions | Auto-loaded

Integer math utilities written in pure AIRL. All functions operate on `i64` values and are available automatically — no imports needed.

## Dependencies

`sum-list` and `product-list` depend on `fold` from the Collections module (loaded first).

## Functions

### Basic: abs, min, max, clamp, sign

```lisp
(abs -5)                ;; → 5
(abs 3)                 ;; → 3
(abs 0)                 ;; → 0

(min 3 7)               ;; → 3
(max 3 7)               ;; → 7

(clamp 5 0 10)          ;; → 5   (within range)
(clamp -3 0 10)         ;; → 0   (below range)
(clamp 15 0 10)         ;; → 10  (above range)

(sign -42)              ;; → -1
(sign 0)                ;; → 0
(sign 7)                ;; → 1
```

### Predicates: even?, odd?

```lisp
(even? 4)               ;; → true
(even? 3)               ;; → false
(odd? 5)                ;; → true
(odd? 0)                ;; → false
```

### Arithmetic: pow, gcd, lcm

```lisp
(pow 2 10)              ;; → 1024
(pow 3 0)               ;; → 1
(pow 5 3)               ;; → 125

(gcd 12 8)              ;; → 4
(gcd 100 75)            ;; → 25
(gcd 7 0)               ;; → 7

(lcm 4 6)               ;; → 12
(lcm 3 5)               ;; → 15
(lcm 0 5)               ;; → 0
```

### Aggregation: sum-list, product-list

```lisp
(sum-list [1 2 3 4 5])         ;; → 15
(sum-list [])                   ;; → 0

(product-list [1 2 3 4 5])     ;; → 120
(product-list [])               ;; → 1
```

## Function Reference

| Function | Signature | Returns | Contracts |
|----------|-----------|---------|-----------|
| `abs` | `(abs x)` | i64 | ensures `(>= result 0)` |
| `min` | `(min a b)` | i64 | ensures result is one of a or b |
| `max` | `(max a b)` | i64 | ensures result is one of a or b |
| `clamp` | `(clamp x lo hi)` | i64 | requires `(<= lo hi)`, ensures `(>= result lo) (<= result hi)` |
| `sign` | `(sign x)` | i64 | ensures result is -1, 0, or 1 |
| `even?` | `(even? x)` | Bool | — |
| `odd?` | `(odd? x)` | Bool | — |
| `pow` | `(pow base exp)` | i64 | requires `(>= exp 0)` |
| `gcd` | `(gcd a b)` | i64 | requires `(>= a 0) (>= b 0)`, ensures `(>= result 0)` |
| `lcm` | `(lcm a b)` | i64 | requires `(>= a 0) (>= b 0)`, ensures `(>= result 0)` |
| `sum-list` | `(sum-list xs)` | i64 | — |
| `product-list` | `(product-list xs)` | i64 | — |

## Notes

- `pow` uses naive recursive multiplication — O(exp). For large exponents, consider using repeated squaring (not yet in stdlib).
- `gcd` implements the Euclidean algorithm. Both arguments must be non-negative.
- `sum-list` and `product-list` use `fold` internally, so they share its recursion depth characteristics.
- All functions are integer-only (`i64`). For float math, use arithmetic builtins directly (`+`, `-`, `*`, `/` on float literals).
