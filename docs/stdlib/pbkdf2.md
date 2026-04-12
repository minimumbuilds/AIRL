# pbkdf2

## pbkdf2-xor-bytes
**Signature:** `(a : Bytes) (b : Bytes) -> Bytes`
**Intent:** XOR two equal-length Bytes values

---

## pbkdf2-f-loop
**Signature:** `(key : Bytes) (u-prev : Bytes) (acc : Bytes) (iter : i64) -> Bytes`
**Intent:** Iterate HMAC and XOR results for PBKDF2 F function

---

## pbkdf2-f
**Signature:** `(key : Bytes) (salt : Bytes) (iterations : i64) (block-idx : i64) -> Bytes`
**Intent:** PBKDF2 F(Password, Salt, c, i) = U1 ^ U2 ^ ... ^ Uc

---

## pbkdf2-blocks
**Signature:** `(key : Bytes) (salt : Bytes) (iterations : i64) (block-idx : i64) (blocks-needed : i64) -> List`
**Intent:** Generate PBKDF2 output blocks T1..Tn

---

## pbkdf2-sha256-bytes
**Signature:** `(password : Bytes) (salt : Bytes) (iterations : i64) (dk-len : i64) -> Bytes`
**Intent:** PBKDF2-HMAC-SHA-256: derive dk-len bytes from password and salt

---

## pbkdf2-sha256
**Signature:** `(password : String) (salt : String) (iterations : i64) (dk-len : i64) -> String`
**Intent:** PBKDF2-HMAC-SHA-256 hex output

---

