# map

## map-values-loop
**Signature:** `(m : _) (ks : List) (i : i64) (len : i64) (acc : List) -> List`

---

## map-values
**Signature:** `(m : _) -> List`
**Intent:** Get all values from a map as a list

---

## map-size
**Signature:** `(m : _) -> i64`
**Intent:** Get the number of entries in a map

---

## map-get-or
**Signature:** `(m : _) (key : String) (default : _) -> _`
**Intent:** Get value for key, or return default if key absent

---

## map-from-pairs-loop
**Signature:** `(lst : List) (i : i64) (len : i64) (acc : _) -> _`
**Intent:** Build map from alternating key-value pairs

---

## map-from
**Signature:** `(lst : List) -> _`
**Intent:** Create a map from a flat list of alternating keys and values

---

## map-entries-loop
**Signature:** `(m : _) (ks : List) (i : i64) (len : i64) (acc : List) -> List`

---

## map-entries
**Signature:** `(m : _) -> List`
**Intent:** Get all entries as a list of [key value] pairs

---

## map-from-entries-loop
**Signature:** `(entries : List) (i : i64) (len : i64) (acc : _) -> _`

---

## map-from-entries
**Signature:** `(entries : List) -> _`
**Intent:** Create a map from a list of [key value] pairs

---

## map-merge
**Signature:** `(m1 : _) (m2 : _) -> _`
**Intent:** Merge two maps, m2 values overwrite m1 on key conflict

---

## map-map-values-loop
**Signature:** `(transform : fn) (entries : List) (i : i64) (len : i64) (acc : _) -> _`

---

## map-map-values
**Signature:** `(transform : fn) (m : _) -> _`
**Intent:** Apply transform to every value in the map, keeping keys

---

## map-filter-loop
**Signature:** `(pred-fn : fn) (entries : List) (i : i64) (len : i64) (acc : _) -> _`

---

## map-filter
**Signature:** `(pred-fn : fn) (m : _) -> _`
**Intent:** Keep entries where pred-fn(key, value) returns true

---

## map-update
**Signature:** `(m : _) (key : String) (updater : fn) -> _`
**Intent:** Apply updater to the value at key, or do nothing if key absent

---

## map-update-or
**Signature:** `(m : _) (key : String) (default : _) (updater : fn) -> _`
**Intent:** Apply updater to value at key, or set to (updater default) if absent

---

## map-count-loop
**Signature:** `(pred-fn : fn) (entries : List) (i : i64) (len : i64) (acc : i64) -> i64`

---

## map-count
**Signature:** `(pred-fn : fn) (m : _) -> i64`
**Intent:** Count entries where pred-fn(key, value) returns true

---

