# test

## assert-eq
**Signature:** `(a : Any) (b : Any) (msg : String) -> Unit`
**Intent:** Assert that a equals b, emit JSON result

---

## assert-ne
**Signature:** `(a : Any) (b : Any) (msg : String) -> Unit`
**Intent:** Assert that a does not equal b, emit JSON result

---

## assert-ok
**Signature:** `(r : _) (msg : String) -> Unit`
**Intent:** Assert that a Result is the Ok variant, emit JSON result

---

## assert-err
**Signature:** `(r : _) (msg : String) -> Unit`
**Intent:** Assert that a Result is the Err variant, emit JSON result

---

## assert-contains
**Signature:** `(haystack : String) (needle : String) (msg : String) -> Unit`
**Intent:** Assert that haystack contains needle, emit JSON result

---

## assert-true
**Signature:** `(cond : Bool) (msg : String) -> Unit`
**Intent:** Assert that cond is true, emit JSON result

---

