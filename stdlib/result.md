# AIRL Standard Library: Result Combinators

> Source: `stdlib/result.airl` | 8 functions | Auto-loaded

Combinators for working with `Result` values (`(Ok v)` / `(Err e)`) without verbose `match` expressions. Written in pure AIRL. All functions are available automatically — no imports needed.

## Dependencies

None. This module is self-contained.

## Why Result Combinators?

Without combinators, every Result requires a full match:

```lisp
;; Verbose — nested match for each step
(match (safe-divide 100 5)
  (Ok first) (match (safe-divide first 2)
    (Ok second) (print "Final:" second)
    (Err e) (print "Error:" e))
  (Err e) (print "Error:" e))
```

With combinators:

```lisp
;; Concise — chain operations, handle error once
(let (result : _ (and-then (fn [x] (safe-divide x 2))
                   (safe-divide 100 5)))
  (print "Final:" (unwrap-or result 0)))
```

## Functions

### Inspection: is-ok?, is-err?

```lisp
(is-ok? (Ok 42))          ;; → true
(is-ok? (Err "fail"))     ;; → false

(is-err? (Err "fail"))    ;; → true
(is-err? (Ok 42))         ;; → false
```

### Extraction: unwrap-or

```lisp
(unwrap-or (Ok 42) 0)          ;; → 42 (Ok value extracted)
(unwrap-or (Err "fail") 0)     ;; → 0  (default returned)
```

### Transformation: map-ok, map-err

```lisp
;; Transform the Ok value, leave Err unchanged
(map-ok (fn [x] (* x 2)) (Ok 21))        ;; → (Ok 42)
(map-ok (fn [x] (* x 2)) (Err "fail"))   ;; → (Err "fail")

;; Transform the Err value, leave Ok unchanged
(map-err (fn [e] (+ e "!")) (Err "oops"))  ;; → (Err "oops!")
(map-err (fn [e] (+ e "!")) (Ok 42))       ;; → (Ok 42)
```

### Chaining: and-then, or-else

```lisp
;; and-then — if Ok, apply f (which must return a Result). If Err, propagate.
;; This is monadic bind (>>=) for Results.
(and-then (fn [x] (if (> x 0) (Ok (* x 2)) (Err "negative"))) (Ok 5))
;; → (Ok 10)

(and-then (fn [x] (Ok (* x 2))) (Err "already failed"))
;; → (Err "already failed")

;; or-else — if Err, apply f to try recovery. If Ok, pass through.
(or-else (fn [e] (Ok 0)) (Err "failed"))   ;; → (Ok 0)
(or-else (fn [e] (Ok 0)) (Ok 42))          ;; → (Ok 42)
```

### Conversion: ok-or

```lisp
;; Convert a potentially-nil value to a Result
(ok-or 42 "was nil")     ;; → (Ok 42)
(ok-or nil "was nil")    ;; → (Err "was nil")
```

## Function Reference

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `is-ok?` | `(is-ok? r)` | Bool | True if r is `(Ok ...)` |
| `is-err?` | `(is-err? r)` | Bool | True if r is `(Err ...)` |
| `unwrap-or` | `(unwrap-or r default)` | any | Ok value, or default |
| `map-ok` | `(map-ok f r)` | Result | Apply f to Ok value |
| `map-err` | `(map-err f r)` | Result | Apply f to Err value |
| `and-then` | `(and-then f r)` | Result | Chain: f must return Result |
| `or-else` | `(or-else f r)` | Result | Recover: f receives the error |
| `ok-or` | `(ok-or val err)` | Result | nil → Err, non-nil → Ok |

## Patterns

### Railway-Oriented Error Handling

Chain multiple fallible operations where any failure short-circuits:

```lisp
(defn process-pipeline
  :sig [(input : i64) -> _]
  :requires [(valid input)]
  :ensures [(valid result)]
  :body (and-then (fn [x] (safe-divide x 2))
          (and-then (fn [x] (if (> x 0) (Ok (* x 10)) (Err "non-positive")))
            (safe-divide input 3))))
```

### Default Values with Fallback Chain

```lisp
;; Try primary, fall back to secondary, fall back to default
(let (result : _ (or-else (fn [_] (lookup-cache key))
                   (or-else (fn [_] (lookup-db key))
                     (lookup-memory key))))
  (unwrap-or result "not found"))
```
