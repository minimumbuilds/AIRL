# Add bytes-xor Builtin to Fix PBKDF2 Allocation Pressure

**Date:** 2026-04-08
**Status:** Proposed
**Scope:** `repos/AIRL` — `crates/airl-rt/src/misc.rs` + g3 builtin registration + `stdlib/pbkdf2.airl` + `repos/AirWire/src/wire-scram.airl`
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

Each `bytes-from-int8` allocates a 1-byte `RtValue::Bytes` heap object. `cons` builds a linked list of them. `bytes-concat-all` later reassembles them. For 4096 PBKDF2 iterations × 32 bytes per HMAC output = **131,072 separate heap allocations** just for the XOR step. The reference-count-based runtime cannot free intermediate objects fast enough under this pressure. Result: OOM kill.

The HMAC calls are not the problem — `hmac-sha256-bytes` is already a native Rust builtin and runs fast. The allocation explosion is entirely in the byte-by-byte XOR accumulation.

The same pattern exists in `repos/AirWire/src/wire-scram.airl`, which has its own `xor-loop`/`xor-bytes` with identical structure.

**Observed impact:**
- AirWire `test-wire-scram` reduced to 1 iteration — RFC 7677 test vectors (4096 iterations) cannot be verified
- AirDB SCRAM-SHA-256 auth will OOM against any real PostgreSQL server (default: 4096; some configs: 600,000)
- AIRL_castle SCRAM auth has the same exposure for Kafka brokers using SCRAM

---

## Approach: Add `bytes-xor` Native Builtin

The root cause is the absence of an element-wise byte XOR operation on `Bytes` values. AIRL has no in-place mutation, so there is no way to XOR two `Bytes` buffers without either:
- Allocating one `RtValue` per byte (current approach — OOMs), or
- A single native call that produces a new `Bytes` value in one allocation

Adding `bytes-xor` as a native Rust builtin eliminates the 131,072-allocation XOR pass entirely. The pure AIRL PBKDF2 remains correct, auditable, and testable against RFC vectors. No algorithm is replaced — only the byte-level XOR primitive is made native.

**Why not re-register `pbkdf2-sha256-bytes` as native instead?**

That was the original spec. It is ~5–10x faster (1–3ms vs 5–20ms at 4096 iterations) but makes the algorithm a black box. `bytes-xor` is the right fix because:
1. PBKDF2 stays in pure AIRL — readable, auditable, testable at full iteration counts
2. The fix is at the correct abstraction level (the primitive that was missing, not the algorithm that uses it)
3. `bytes-xor` is useful beyond PBKDF2 — any code that needs element-wise byte operations benefits
4. 5–20ms per connection auth is acceptable for all realistic pooled workloads

The native `pbkdf2-sha256-bytes` Rust implementation remains available in `airl-rt/src/misc.rs` as a future escape hatch if extreme connection churn ever requires it. It should not be registered yet.

---

## Fix

### Step 1 — Add `airl_bytes_xor` to `crates/airl-rt/src/misc.rs`

```rust
/// bytes-xor: (a : Bytes) (b : Bytes) -> Bytes
/// Element-wise XOR of two equal-length Bytes values. Panics if lengths differ.
#[no_mangle]
pub extern "C" fn airl_bytes_xor(
    a: *mut RtValue,
    b: *mut RtValue,
) -> *mut RtValue {
    let a_bytes = extract_bytes(a);
    let b_bytes = extract_bytes(b);
    if a_bytes.len() != b_bytes.len() {
        return rt_error("bytes-xor: length mismatch");
    }
    let result: Vec<u8> = a_bytes.iter().zip(b_bytes.iter()).map(|(x, y)| x ^ y).collect();
    rt_bytes(result)
}
```

`extract_bytes` already accepts both `RtValue::Bytes` and `RtValue::Str`. `rt_bytes` allocates a single `RtValue::Bytes`. Total: one heap allocation per call regardless of buffer size.

### Step 2 — Register in g3 builtin table

Locate the g3 builtin registration file by grepping for `hmac-sha256-bytes` in the bootstrap source. Add:

```
"bytes-xor"  →  airl_bytes_xor   arity=2
```

### Step 3 — Update `stdlib/pbkdf2.airl`

Replace `pbkdf2-xor-loop` and `pbkdf2-xor-bytes` with a single call to the new builtin:

```airl
;; ── XOR two equal-length Bytes ──
;; Uses bytes-xor builtin (airl-rt/src/misc.rs) — single native call, one allocation.
;; The pure loop below is retained as a reference for non-AOT paths.

(defn pbkdf2-xor-bytes
  :sig [(a : Bytes) (b : Bytes) -> Bytes]
  :intent "XOR two equal-length Bytes values"
  :requires [(= (length a) (length b))]
  :ensures [(= (length result) (length a))]
  :body (bytes-xor a b))

;; Reference implementation (not called under g3 — bytes-xor is registered as builtin):
;;
;; (defn pbkdf2-xor-loop ...)  ;; 131,072 allocations at 4096 iterations — OOM
```

The rest of `pbkdf2.airl` (`pbkdf2-f-loop`, `pbkdf2-f`, `pbkdf2-blocks`, `pbkdf2-sha256-bytes`, `pbkdf2-sha256`) is unchanged. The entire algorithm remains in pure AIRL.

Update the file header comment:

```airl
;; ── AIRL Standard Library: PBKDF2-HMAC-SHA-256 ──────────────────
;; RFC 2898 PBKDF2 using HMAC-SHA-256. Depends on sha256.airl and hmac.airl.
;;
;; pbkdf2-xor-bytes uses the bytes-xor builtin (registered in g3).
;; This eliminates the per-byte allocation pressure that caused OOM at
;; 4096 iterations. The algorithm itself remains pure AIRL.
;;
;; Performance at 4096 iterations: ~5-20ms (4096 native HMAC calls +
;; 4096 single-allocation XOR calls). Acceptable for connection-pool auth.
```

### Step 4 — Update `repos/AirWire/src/wire-scram.airl`

`xor-bytes` in AirWire has the same structure as `pbkdf2-xor-loop`. Replace it:

```airl
(defn xor-bytes
  :sig [(a : Bytes) (b : Bytes) -> Bytes]
  :requires [(= (length a) (length b))]
  :ensures [(= (length result) (length a))]
  :body (bytes-xor a b))
```

Remove `xor-loop` (it is now unused). Update AirWire's Makefile to ensure `bytes-xor` is available (it will be, since `bytes-xor` is a registered g3 builtin — no source import needed).

### Step 5 — Restore RFC 7677 test vectors in AirWire

In `repos/AirWire/tests/test-wire-scram.airl`, restore the PBKDF2 iteration count to 4096 and verify the full RFC 7677 known-answer test:

```
username:    "user"
password:    "pencil"
client-nonce: "rOprNGfwEbeRWgbNEkqO"
server-first: "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"
```

Expected `SaltedPassword` (hex from RFC 7677):
`9c1e2b3a4f5d6e7f...` *(worker must look up the exact expected value from RFC 7677 §3)*

The test passes when `scram-compute-proof` at `i=4096` completes without OOM and produces the correct proof string.

### Step 6 — Rebuild g3

```bash
cd ~/repos/AIRL
cargo build --features jit,aot > /tmp/cargo-build.log 2>&1 && echo "Stage 1 OK" || { tail -30 /tmp/cargo-build.log; exit 1; }
bash scripts/build-g3.sh > /tmp/g3-build.log 2>&1 && echo "Stage 2 OK" || { tail -30 /tmp/g3-build.log; exit 1; }
```

---

## Performance Characterisation

At 4096 iterations with `bytes-xor`:

| Step | Cost |
|------|------|
| 4096 × `hmac-sha256-bytes` (native Rust) | ~5–15ms total |
| 4096 × `bytes-xor` (native Rust, one alloc each) | ~0.1ms total |
| 4096 × AIRL TCO loop iterations | ~1–2ms overhead |
| RtValue allocations | ~8,192 (vs 131,072 before) |
| **Total** | **~5–20ms** |

Connection pool of 10: ~50–200ms one-time auth cost at startup. Per-reconnect with PBKDF2 cache: ~0ms (derived key reused). Acceptable for all realistic pooled workloads.

If extreme connection churn ever requires faster auth, `airl_pbkdf2_sha256_bytes` already exists in `airl-rt/src/misc.rs` and can be registered as a builtin in a future issue (~1–3ms, zero AIRL overhead). That is not needed now.

---

## Similar Existing Issues

| Issue | Pattern |
|-------|---------|
| `castle-bytes-migration` | Pure AIRL `List[Int]` byte ops slow due to per-element boxing. Fix: adopt native `Bytes` type. |
| `rt-alloc-reduction` | Runtime allocation pressure from excess `RtValue` boxing. |
| `stdlib-crypto` (Spec 05) | Deregistered SHA-256/HMAC/PBKDF2 Rust builtins in favour of pure AIRL. This issue adds one targeted primitive (`bytes-xor`) that makes the pure AIRL implementations viable at production scale — rather than reversing the deregistration. |

---

## Files Changed

| File | Change |
|------|--------|
| `crates/airl-rt/src/misc.rs` | Add `airl_bytes_xor` (2 args, element-wise XOR, one allocation) |
| `<g3 builtin registration file>` | Register `bytes-xor` at arity 2 |
| `stdlib/pbkdf2.airl` | Replace `pbkdf2-xor-loop` + `pbkdf2-xor-bytes` with `(bytes-xor a b)`; update header comment |
| `repos/AirWire/src/wire-scram.airl` | Replace `xor-loop` + `xor-bytes` with `(bytes-xor a b)`; remove `xor-loop` |
| `repos/AirWire/tests/test-wire-scram.airl` | Restore iterations to 4096; verify RFC 7677 known-answer test |

---

## Out of Scope

- Registering `pbkdf2-sha256-bytes` as a native builtin — deferred; not needed for correctness or typical workloads
- Removing the pure AIRL PBKDF2 algorithm — keep it; it is now correct at production iteration counts
- `bytes-xor` error handling for mismatched lengths — `rt_error` is sufficient; callers (SCRAM) always pass equal-length buffers by construction
