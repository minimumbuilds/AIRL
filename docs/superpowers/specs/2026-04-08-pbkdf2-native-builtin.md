# Re-register pbkdf2-sha256-bytes as a Native Builtin

**Date:** 2026-04-08
**Status:** Proposed
**Scope:** `repos/AIRL` — `crates/airl-rt/src/misc.rs` + g3 builtin registration
**Priority:** Blocker for AirDB connecting to real PostgreSQL

---

## Problem

`pbkdf2-sha256-bytes` is implemented in pure AIRL in `stdlib/pbkdf2.airl`. At PostgreSQL's default SCRAM iteration count (4096), it OOM-kills the process before completing.

**Root cause — per-byte heap allocation in `pbkdf2-xor-loop`:**

```airl
(defn pbkdf2-xor-loop
  :sig [(a : Bytes) (b : Bytes) (i : i64) (len : i64) -> List]
  :body (if (= i len) []
          (cons (bytes-from-int8 (bitwise-xor (at a i) (at b i)))
                (pbkdf2-xor-loop a b (+ i 1) len))))
```

Each call to `bytes-from-int8` allocates a 1-byte `RtValue::Bytes` heap object. For 4096 PBKDF2 iterations × 32 bytes per HMAC output = **131,072 separate heap allocations** per `pbkdf2-f-loop` call, before `bytes-concat-all` reassembles them. The AIRL GC (reference-count based) cannot free intermediate objects fast enough. Result: OOM kill.

**Observed impact:**
- AirWire `test-wire-scram` reduced to 1 iteration to avoid OOM — RFC 7677 test vectors (4096 iterations) cannot be verified
- AirDB SCRAM-SHA-256 auth will OOM against any real PostgreSQL server (default: 4096 iterations; some configs use 600,000)
- AIRL_castle SCRAM auth has the same exposure (though Kafka brokers may be configured differently)

---

## Prior Art in This Codebase

**The Rust native implementation already exists** and has not been removed — it was only deregistered from the g3 builtin table.

`crates/airl-rt/src/misc.rs`:
```rust
pub extern "C" fn airl_pbkdf2_sha256(
    password: *mut RtValue, salt: *mut RtValue,
    iterations: *mut RtValue, key_len: *mut RtValue
) -> *mut RtValue {
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pw.as_bytes(), &salt_bytes, iters, &mut derived);
    // returns hex String
}

pub extern "C" fn airl_pbkdf2_sha512(...) -> *mut RtValue { ... }
```

The deregistration happened during the Spec 05 stdlib migration (`stdlib-crypto` issue). Comment in `stdlib/pbkdf2.airl`:
> `Rust builtins deregistered — these are the canonical implementations.`

The motivation was correctness parity (pure AIRL is auditable). The performance consequence at production iteration counts was not evaluated at the time.

**Similar precedent:** `2026-04-01-castle-bytes-migration.md` — the Kafka SDK suffered a 6–50x slowdown because `List[Int]` was used where native `Bytes` existed. Resolution: adopt the native type. Same pattern here: adopt the native Rust PBKDF2 for the hot path.

---

## Fix

### Step 1 — Add `airl_pbkdf2_sha256_bytes` to `crates/airl-rt/src/misc.rs`

The existing `airl_pbkdf2_sha256` takes `String` password/salt and returns a hex `String`. SCRAM-SHA-256 (used by both AirDB and AIRL_castle) works in `Bytes` throughout — no string conversion needed. Add a bytes-in/bytes-out variant:

```rust
/// pbkdf2-sha256-bytes: (password : Bytes) (salt : Bytes) (iterations : Int) (dk-len : Int) -> Bytes
#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha256_bytes(
    password: *mut RtValue,
    salt: *mut RtValue,
    iterations: *mut RtValue,
    key_len: *mut RtValue,
) -> *mut RtValue {
    let pw = extract_bytes(password);      // accepts Bytes or String
    let salt_bytes = extract_bytes(salt);
    let iters = extract_i64(iterations) as u32;
    let klen = extract_i64(key_len) as usize;
    let mut derived = vec![0u8; klen];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&pw, &salt_bytes, iters, &mut derived);
    rt_bytes(derived)
}

/// pbkdf2-sha512-bytes: same but SHA-512
#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha512_bytes(
    password: *mut RtValue,
    salt: *mut RtValue,
    iterations: *mut RtValue,
    key_len: *mut RtValue,
) -> *mut RtValue {
    // same pattern with sha2::Sha512
}
```

`extract_bytes` already accepts both `RtValue::Bytes` and `RtValue::Str` (see existing `airl_hmac_sha256_bytes`). No new extraction logic needed.

### Step 2 — Register in g3 builtin table

In whatever source file registers g3 builtins (the self-hosted compiler's builtin table — locate by grepping for `hmac-sha256-bytes` registration), add:

```
"pbkdf2-sha256-bytes"  →  airl_pbkdf2_sha256_bytes   arity=4
"pbkdf2-sha512-bytes"  →  airl_pbkdf2_sha512_bytes   arity=4
```

These shadow the stdlib definitions when running under g3 AOT. The stdlib AIRL implementations remain as fallback for any non-g3 evaluation path.

### Step 3 — Update `stdlib/pbkdf2.airl`

Update the comment to reflect that native builtins are re-registered:

```airl
;; ── AIRL Standard Library: PBKDF2-HMAC-SHA-256 / SHA-512 ────────────────
;; RFC 2898 PBKDF2 using HMAC-SHA-256 / SHA-512.
;;
;; pbkdf2-sha256-bytes and pbkdf2-sha512-bytes are registered as native Rust
;; builtins in the g3 AOT compiler (airl-rt/src/misc.rs).
;; The pure AIRL implementations below are retained as reference and for
;; non-AOT evaluation paths. Under g3, the builtin is called directly.
;;
;; Performance: native builtin handles 4096+ iterations without heap pressure.
;; Pure AIRL version OOMs at >~10 iterations due to per-byte RtValue allocation
;; in pbkdf2-xor-loop.
```

Do NOT remove the pure AIRL implementation — it documents the algorithm and serves as a correctness reference.

### Step 4 — Rebuild g3

```bash
cd ~/repos/AIRL
cargo build --features jit,aot > /tmp/cargo-build.log 2>&1 && echo "Stage 1 OK" || { tail -30 /tmp/cargo-build.log; exit 1; }
bash scripts/build-g3.sh > /tmp/g3-build.log 2>&1 && echo "Stage 2 OK" || { tail -30 /tmp/g3-build.log; exit 1; }
```

---

## Verification

**Test 1 — correctness against RFC 7677 known vectors:**
```
password: "pencil"
salt:     W22ZaJ0SNY7soEsUEjb6gQ== (base64-decoded)
iterations: 4096
dk-len: 32
expected SaltedPassword (hex): <from RFC 7677>
```

Run via AirWire's test suite with iterations restored to 4096:
```bash
cd ~/repos/AirWire
make test-wire-scram  # should pass at 4096 iterations without OOM
```

**Test 2 — performance sanity:**
4096 iterations of `pbkdf2-sha256-bytes` should complete in < 50ms. Native Rust via the `pbkdf2` crate: ~5–10ms on modern hardware.

**Test 3 — AirDB SCRAM auth integration (once AirDB is merged):**
```bash
cd ~/repos/AirDB
make postgres-up
make test-integration  # connect with SCRAM-SHA-256
make postgres-down
```

---

## Similar Existing Issues

| Issue | Pattern | Resolution |
|-------|---------|------------|
| `castle-bytes-migration` | Pure AIRL `List[Int]` byte buffers slow (6–50x). Native `Bytes` type existed but unused. | Adopt native type in SDK. |
| `stdlib-crypto` (Spec 05) | Moved SHA-256/HMAC/PBKDF2 from Rust builtins to pure AIRL for auditability. | (This issue reverses the PBKDF2 part of that decision.) |
| `rt-alloc-reduction` | Runtime allocation pressure from excess `RtValue` boxing. | Reduce allocations at callsites. |

The PBKDF2 case is the inverse of `castle-bytes-migration`: instead of AIRL code ignoring a faster native type, the compiler was configured to bypass a faster native implementation in favour of a pure AIRL one that is correct but allocation-heavy at production scale.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/airl-rt/src/misc.rs` | Add `airl_pbkdf2_sha256_bytes`, `airl_pbkdf2_sha512_bytes` |
| `<g3 builtin registration file>` | Register both new builtins at arity 4 |
| `stdlib/pbkdf2.airl` | Update comment; restore 4096-iteration tests |
| `repos/AirWire/tests/test-wire-scram.airl` | Restore iterations to 4096; verify RFC 7677 test vectors |

---

## Out of Scope

- Removing the pure AIRL PBKDF2 — keep it as reference implementation
- `hmac-sha256-bytes` re-registration — already registered as a native builtin; not affected
- `sha256` re-registration — already a native builtin; not affected
- PBKDF2-SHA-512 is included here (trivial to add alongside SHA-256) even though no current consumer uses it
