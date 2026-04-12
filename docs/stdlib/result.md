# result

## is-ok?
**Signature:** `(r : _) -> bool`
**Intent:** Check if a Result is an Ok variant

---

## is-err?
**Signature:** `(r : _) -> bool`
**Intent:** Check if a Result is an Err variant

---

## unwrap-or
**Signature:** `(r : _) (default : _) -> _`
**Intent:** Extract Ok value, or return default if Err

---

## map-ok
**Signature:** `(f : fn) (r : _) -> _`
**Intent:** Apply f to the Ok value, leave Err unchanged

---

## map-err
**Signature:** `(f : fn) (r : _) -> _`
**Intent:** Apply f to the Err value, leave Ok unchanged

---

## and-then
**Signature:** `(f : fn) (r : _) -> _`
**Intent:** If Ok, apply f (which returns a Result). If Err, propagate.

---

## or-else
**Signature:** `(f : fn) (r : _) -> _`
**Intent:** If Err, apply f to try recovery. If Ok, return as-is.

---

## ok-or
**Signature:** `(val : _) (err : _) -> _`
**Intent:** Wrap non-nil value in Ok, or return Err if nil

---

