# set

## set-new
**Signature:** ` -> Map`
**Intent:** Create an empty set

---

## set-from
**Signature:** `(xs : List) -> Map`
**Intent:** Create a set from a list of elements

---

## set-add
**Signature:** `(s : Map) (x : _) -> Map`
**Intent:** Add an element to the set

---

## set-remove
**Signature:** `(s : Map) (x : _) -> Map`
**Intent:** Remove an element from the set

---

## set-contains?
**Signature:** `(s : Map) (x : _) -> Bool`
**Intent:** Check if element is in the set

---

## set-size
**Signature:** `(s : Map) -> i64`
**Intent:** Number of elements in the set

---

## set-to-list
**Signature:** `(s : Map) -> List`
**Intent:** Convert set to list of elements

---

## set-union
**Signature:** `(a : Map) (b : Map) -> Map`
**Intent:** Union of two sets

---

## set-intersection
**Signature:** `(a : Map) (b : Map) -> Map`
**Intent:** Intersection of two sets

---

## set-difference
**Signature:** `(a : Map) (b : Map) -> Map`
**Intent:** Elements in first set but not second

---

## set-subset?
**Signature:** `(a : Map) (b : Map) -> Bool`
**Intent:** Check if first set is a subset of second

---

## set-equal?
**Signature:** `(a : Map) (b : Map) -> Bool`
**Intent:** Check if two sets have the same elements

---

