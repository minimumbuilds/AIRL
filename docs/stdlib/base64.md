# base64

## b64-char
**Signature:** `(idx : i64) -> Bytes`
**Intent:** Map a 6-bit index to the corresponding Base64 character as a 1-byte Bytes

---

## b64-val
**Signature:** `(b : i64) -> i64`
**Intent:** Map a Base64 character byte to its 6-bit value, or -1 for padding/invalid

---

## b64-encode-triple
**Signature:** `(a : i64) (b : i64) (c : i64) -> List`
**Intent:** Encode 3 bytes into a list of 4 base64 character bytes

---

## b64-encode-loop
**Signature:** `(src : Bytes) (i : i64) (len : i64) -> List`
**Intent:** Process input bytes in groups of 3, producing base64 output bytes

---

## base64-encode
**Signature:** `(s : String) -> String`
**Intent:** Encode a string to Base64 (RFC 4648)

---

## b64-count-padding
**Signature:** `(src : Bytes) (len : i64) -> i64`
**Intent:** Count trailing padding characters in base64 input

---

## b64-decode-quad
**Signature:** `(v0 : i64) (v1 : i64) (v2 : i64) (v3 : i64) (pad : i64) -> List`
**Intent:** Decode 4 base64 values into output bytes, respecting padding count

---

## b64-decode-loop
**Signature:** `(src : Bytes) (i : i64) (len : i64) (pad : i64) -> List`
**Intent:** Process base64 input in groups of 4 characters

---

## base64-decode
**Signature:** `(s : String) -> String`
**Intent:** Decode a Base64 string (RFC 4648)

---

