# random

## rnd-nibble-to-char
**Signature:** `(n : i64) -> String`
**Intent:** Convert 0-15 to a single hex character string

---

## rnd-hex-nibble-val
**Signature:** `(c : String) -> i64`
**Intent:** Parse a single hex character to its integer value 0-15

---

## rnd-hex-pair-to-byte
**Signature:** `(hex : String) (i : i64) -> i64`
**Intent:** Decode one byte (two hex chars) from hex string at pair index i

---

## rnd-hex-to-bytes
**Signature:** `(hex : String) (i : i64) (n : i64) -> List`
**Intent:** Decode all n bytes from a 2n-char hex string into a List of ints

---

## rnd-ts-nibble
**Signature:** `(ts : i64) (k : i64) -> String`
**Intent:** Extract the k-th nibble from the low 48 bits of ts as a hex char

---

## rnd-int-to-hex-12
**Signature:** `(ts : i64) -> String`
**Intent:** Format the low 48 bits of ts as a 12-character lowercase hex string

---

## rnd-b64url-char
**Signature:** `(n : i64) -> String`
**Intent:** Map a 6-bit value (0-63) to its base64url character

---

## rnd-b64url-triple
**Signature:** `(a : i64) (b : i64) (c : i64) -> String`
**Intent:** Encode 3 bytes into 4 base64url characters (no padding)

---

## rnd-b64url-loop
**Signature:** `(bytes : List) (i : i64) (len : i64) -> String`
**Intent:** Encode byte list to base64url string, no padding

---

## random-hex
**Signature:** `(n : i64) -> String`
**Intent:** Return a lowercase hex string of n cryptographically random bytes

---

## random-url-token
**Signature:** `(n : i64) -> String`
**Intent:** Return n cryptographically random bytes encoded as base64url (no padding)

---

## uuid-v4
**Signature:** ` -> String`
**Intent:** Generate a random UUID v4 (RFC 4122) string

---

## uuid-v7
**Signature:** ` -> String`
**Intent:** Generate a time-ordered UUID v7 (RFC 9562) string

---

