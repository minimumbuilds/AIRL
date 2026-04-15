# hmac

## hmac-pad-key
**Signature:** `(key : Bytes) -> Bytes`
**Intent:** Prepare HMAC key: hash if > 64 bytes, zero-pad to 64 bytes

---

## hmac-xor-key
**Signature:** `(key : Bytes) (pad : i64) -> Bytes`
**Intent:** XOR 64-byte key with pad byte, returning 64-byte Bytes

---

