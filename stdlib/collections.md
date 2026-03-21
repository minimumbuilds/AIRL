# AIRL Standard Library: Collections

> Source: `stdlib/prelude.airl` | 15 functions | Auto-loaded

Provides higher-order collection operations built in pure AIRL using recursive patterns. All functions are available automatically — no imports needed.

## Dependencies

Relies on 4 Rust builtins for list destructuring:

| Builtin | Signature | Description |
|---------|-----------|-------------|
| `head` | `(head xs)` → element | First element (errors on empty) |
| `tail` | `(tail xs)` → List | All but first (errors on empty) |
| `empty?` | `(empty? xs)` → Bool | Is list empty? |
| `cons` | `(cons x xs)` → List | Prepend element to front |

## Functions

### Core: map, filter, fold

```lisp
;; map — apply function to each element, preserves length
(map (fn [x] (* x 2)) [1 2 3 4 5])       ;; → [2 4 6 8 10]
(map (fn [x] (+ x "!")) ["a" "b" "c"])    ;; → ["a!" "b!" "c!"]
(map (fn [x] (* x 2)) [])                 ;; → []

;; filter — keep elements where predicate returns true
(filter (fn [x] (> x 3)) [1 2 3 4 5])     ;; → [4 5]
(filter (fn [x] (even? x)) [1 2 3 4 5 6]) ;; → [2 4 6]

;; fold — left fold: f(f(f(init, x1), x2), x3)
(fold (fn [acc x] (+ acc x)) 0 [1 2 3 4 5])   ;; → 15 (sum)
(fold (fn [acc x] (* acc x)) 1 [1 2 3 4 5])   ;; → 120 (product)
(fold (fn [acc x] (+ acc 1)) 0 [1 2 3])       ;; → 3 (count)
```

### Structural: reverse, concat, zip, flatten

```lisp
(reverse [1 2 3 4 5])               ;; → [5 4 3 2 1]
(reverse [])                         ;; → []

(concat [1 2] [3 4 5])              ;; → [1 2 3 4 5]
(concat [] [1 2])                    ;; → [1 2]

(zip [1 2 3] [4 5 6])               ;; → [[1 4] [2 5] [3 6]]
(zip [1 2 3] [4 5])                 ;; → [[1 4] [2 5]]  (stops at shorter)

(flatten [[1 2] [3] [4 5]])         ;; → [1 2 3 4 5]
(flatten [[] [1] []])                ;; → [1]
```

### Slicing: range, take, drop

```lisp
(range 1 6)                          ;; → [1 2 3 4 5]
(range 0 0)                          ;; → []
(range -2 3)                         ;; → [-2 -1 0 1 2]

(take 3 [10 20 30 40 50])           ;; → [10 20 30]
(take 0 [1 2 3])                    ;; → []
(take 10 [1 2])                     ;; → [1 2]  (takes what's available)

(drop 2 [10 20 30 40 50])           ;; → [30 40 50]
(drop 0 [1 2 3])                    ;; → [1 2 3]
```

### Searching: any, all, find

```lisp
(any (fn [x] (> x 3)) [1 2 3 4 5])   ;; → true
(any (fn [x] (> x 10)) [1 2 3])      ;; → false
(any (fn [x] true) [])                ;; → false (vacuously)

(all (fn [x] (> x 0)) [1 2 3])       ;; → true
(all (fn [x] (> x 2)) [1 2 3])       ;; → false
(all (fn [x] true) [])                ;; → true (vacuously)

(find (fn [x] (> x 3)) [1 2 3 4 5])  ;; → 4 (first match)
(find (fn [x] (> x 100)) [1 2 3])    ;; → nil
```

### Sorting: sort, merge

```lisp
;; sort — merge sort, takes a comparison function
(sort (fn [a b] (< a b)) [5 3 1 4 2])   ;; → [1 2 3 4 5]
(sort (fn [a b] (> a b)) [5 3 1 4 2])   ;; → [5 4 3 2 1]
(sort (fn [a b] (< a b)) [])             ;; → []
(sort (fn [a b] (< a b)) [42])           ;; → [42]

;; merge — merge two pre-sorted lists (used internally by sort)
(merge (fn [a b] (< a b)) [1 3 5] [2 4 6])  ;; → [1 2 3 4 5 6]
```

## Function Reference

| Function | Signature | Returns | Contracts |
|----------|-----------|---------|-----------|
| `map` | `(map f xs)` | List | ensures `(= (length result) (length xs))` |
| `filter` | `(filter pred xs)` | List | ensures `(<= (length result) (length xs))` |
| `fold` | `(fold f init xs)` | any | — |
| `reverse` | `(reverse xs)` | List | ensures `(= (length result) (length xs))` |
| `concat` | `(concat xs ys)` | List | ensures `(= (length result) (+ (length xs) (length ys)))` |
| `zip` | `(zip xs ys)` | List | stops at shorter list |
| `flatten` | `(flatten xss)` | List | — |
| `range` | `(range start end)` | List | requires `(<= start end)`, ensures `(= (length result) (- end start))` |
| `take` | `(take n xs)` | List | requires `(>= n 0)` |
| `drop` | `(drop n xs)` | List | requires `(>= n 0)` |
| `any` | `(any pred xs)` | Bool | short-circuits on first true |
| `all` | `(all pred xs)` | Bool | short-circuits on first false |
| `find` | `(find pred xs)` | any/nil | returns nil if not found |
| `sort` | `(sort cmp xs)` | List | ensures `(= (length result) (length xs))` |
| `merge` | `(merge cmp xs ys)` | List | ensures `(= (length result) (+ (length xs) (length ys)))` |

## Performance Notes

All functions are implemented via recursion. Processing a list of N elements uses N stack frames. The interpreter has a recursion depth limit of 50,000, so lists up to ~10,000 elements are safe (some functions like `sort` use `O(n log n)` depth). For larger datasets, use tensor operations instead.
