# prelude

## map
**Signature:** `(f : fn) (xs : List) -> List`
**Intent:** Apply function to each element of a list

---

## filter
**Signature:** `(pred : fn) (xs : List) -> List`
**Intent:** Keep elements where predicate returns true

---

## fold
**Signature:** `(f : fn) (init : _) (xs : List) -> _`
**Intent:** Reduce a list to a single value using accumulator

---

## reverse
**Signature:** `(xs : List) -> List`
**Intent:** Reverse a list

---

## concat
**Signature:** `(xs : List) (ys : List) -> List`
**Intent:** Concatenate two lists

---

## zip
**Signature:** `(xs : List) (ys : List) -> List`
**Intent:** Pair corresponding elements from two lists

---

## flatten
**Signature:** `(xss : List) -> List`
**Intent:** Flatten a list of lists into a single list

---

## range
**Signature:** `(start : i64) (end : i64) -> List`
**Intent:** Generate a list of integers from start to end (exclusive)

---

## take
**Signature:** `(n : i64) (xs : List) -> List`
**Intent:** Take the first n elements of a list

---

## drop
**Signature:** `(n : i64) (xs : List) -> List`
**Intent:** Drop the first n elements of a list

---

## any
**Signature:** `(pred : fn) (xs : List) -> bool`
**Intent:** Check if any element satisfies the predicate

---

## all
**Signature:** `(pred : fn) (xs : List) -> bool`
**Intent:** Check if all elements satisfy the predicate

---

## find
**Signature:** `(pred : fn) (xs : List) -> _`
**Intent:** Find first element satisfying predicate, or nil

---

## merge
**Signature:** `(cmp : fn) (xs : List) (ys : List) -> List`
**Intent:** Merge two sorted lists using comparison function

---

## sort
**Signature:** `(cmp : fn) (xs : List) -> List`
**Intent:** Sort a list using merge sort with comparison function

---

## unique
**Signature:** `(xs : List) -> List`
**Intent:** Remove duplicate elements from a list, preserving first occurrence

---

## enumerate
**Signature:** `(xs : List) -> List`
**Intent:** Pair each element with its 0-based index

---

## group-by
**Signature:** `(f : fn) (xs : List) -> Map`
**Intent:** Group elements by key function into a map of lists

---

