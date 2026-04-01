# AIRL_castle Bytes Type Migration + COW Consumption

**Date:** 2026-04-01
**Status:** Approved design
**Target:** `repos/AIRL_castle` (Kafka SDK)
**Prerequisites:** COW list port merged (ae3299c), g3 rebuilt (2c7b8ae)

## Problem

The Kafka SDK uses `List` (boxed integers, 16+ bytes per element) for all byte buffers despite the runtime having a native `Bytes` type (`Vec<u8>`, 1 byte per element). This causes:

- **6.2x TCP overhead:** `tcp-recv` returns `Bytes`, but SDK code typed as `List` forces `extract_bytes()` to iterate and re-box on every boundary crossing
- **Per-byte boxing:** Every encode function (e.g., `encode-int32`) creates a `Bytes` value via `bytes-from-int32`, but AIRL-level code treats it as `List`, so any `cons`/manual byte construction creates boxed integers
- **50x TCP send penalty:** When `tcp-send` receives `List[Int]`, it iterates all pointers, dereferences, matches `Int`, casts to `u8` — vs zero-copy for `Bytes`
- **Benchmark data:** Full sync produce 369µs vs 135µs Confluent (2.7x slower), root cause: per-byte value boxing

### What's already in place but not consumed

| Runtime feature | Status | SDK consumption |
|----------------|--------|----------------|
| `RtData::Bytes(Vec<u8>)` | Implemented (value.rs:35) | **Not used** — SDK declares `List` |
| `bytes-from-int{16,32,64}` returns `Bytes` | Implemented (misc.rs:1190) | **Already returning Bytes** but SDK ignores it |
| `bytes-concat-all` single-pass allocation | Implemented (misc.rs:1273) | SDK calls it but result treated as `List` |
| `tcp-recv` returns `Bytes` | Implemented (misc.rs:1052) | **Already returning Bytes** — SDK ignores it |
| `tcp-send` Bytes fast path | Implemented (misc.rs:1022-1023) | **Never hit** because SDK concatenates as `List` |
| COW `airl_append` for Bytes | Implemented (list.rs:158) | **Never hit** because SDK uses `List` append |
| COW `airl_tail` O(1) views | Implemented (list.rs) | **Available** for any fold/accumulate on lists |
| `extract_bytes()` dual-type support | Implemented (misc.rs:1207) | Accepts both, but List path is 50-80x slower |
| Compression accepts Bytes | Implemented (misc.rs:1303+) | SDK passes `List`, pays extraction cost |
| `crc32c` accepts Bytes | Implemented | SDK passes `List`, pays extraction cost |

## Key Insight

**The runtime builtins already return `Bytes`.** The SDK's `encode-int32` calls `bytes-from-int32` which calls `rt_bytes()` — the value flowing through AIRL code is already tagged `TAG_BYTES`. But:

1. AIRL type annotations say `List`, which is misleading but not enforced at runtime (duck typing)
2. Any AIRL-level byte construction using `cons` or manual `(list b1 b2 b3)` creates actual `List[Int]`, not `Bytes`
3. Functions that concatenate via `fold` + `append` may mix `Bytes` and `List` — `bytes-concat-all` handles this but `extract_bytes()` on the `List` parts is slow

**The fix is ensuring the SDK never constructs `List[Int]` byte buffers at the AIRL level.** All byte construction must go through `bytes-*` builtins that return native `Bytes`.

## Changes

### Change 1: Update type signatures in binary.airl

Update all encode function signatures from `-> List` to `-> Bytes` and all decode buffer parameters from `(buf : List)` to `(buf : Bytes)`. This is documentation-only at runtime (AIRL uses structural typing), but makes the intent explicit and catches misuse in future type checking.

**Encode functions (14 functions):**
```
encode-int8      : (n : i64) -> Bytes        (was -> List)
encode-int16     : (n : i64) -> Bytes
encode-int32     : (n : i64) -> Bytes
encode-int64     : (n : i64) -> Bytes
encode-unsigned-varint : (n : i64) -> Bytes
encode-signed-varint   : (n : i64) -> Bytes
encode-string    : (s : String) -> Bytes
encode-nullable-string : (s : _) -> Bytes
encode-compact-string  : (s : String) -> Bytes
encode-compact-nullable-string : (s : _) -> Bytes
encode-bytes     : (data : Bytes) -> Bytes    (was data : List)
encode-nullable-bytes  : (data : _) -> Bytes
encode-array     : (encoder-fn : _) (items : List) -> Bytes
encode-compact-array   : (encoder-fn : _) (items : List) -> Bytes
```

**Decode functions (12 functions):**
```
decode-int8      : (buf : Bytes) (offset : i64) -> Map   (was buf : List)
decode-int16     : (buf : Bytes) (offset : i64) -> Map
decode-int32     : (buf : Bytes) (offset : i64) -> Map
decode-int64     : (buf : Bytes) (offset : i64) -> Map
decode-unsigned-varint : (buf : Bytes) (offset : i64) -> Map
decode-signed-varint   : (buf : Bytes) (offset : i64) -> Map
decode-string    : (buf : Bytes) (offset : i64) -> Map
decode-nullable-string : (buf : Bytes) (offset : i64) -> Map
decode-compact-string  : (buf : Bytes) (offset : i64) -> Map
decode-bytes     : (buf : Bytes) (offset : i64) -> Map
decode-array     : (decoder-fn : _) (buf : Bytes) (offset : i64) -> Map
decode-compact-array   : (decoder-fn : _) (buf : Bytes) (offset : i64) -> Map
```

### Change 2: Fix encode-int8 to use Bytes builtin

Currently `encode-int8` manually constructs a single-element list:
```clojure
;; CURRENT (binary.airl line 11-16):
(defn encode-int8
  :sig [(n : i64) -> List]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (list (bit-and n 255)))
```

This creates a `List[Int]` — one boxed integer. Change to:
```clojure
(defn encode-int8
  :sig [(n : i64) -> Bytes]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (bytes-from-int8 n))
```

**Runtime support needed:** Add `airl_bytes_from_int8` builtin to `misc.rs` if it doesn't exist. It should return `rt_bytes(vec![(n as u8)])`.

### Change 3: Fix encode-unsigned-varint to use Bytes

Currently builds byte list via `cons` recursion:
```clojure
;; CURRENT (binary.airl lines 38-54):
(defn encode-unsigned-varint-acc
  :sig [(n : i64) (acc : List) -> List]
  ...
  :body (if (<= n 127)
          (reverse (cons (bit-and n 127) acc))
          (encode-unsigned-varint-acc
            (bit-shr n 7)
            (cons (bit-or (bit-and n 127) 128) acc))))
```

This creates a `List[Int]`. Change to use `bytes-append` (COW-optimized) or accumulate into a `Bytes` buffer:
```clojure
(defn encode-unsigned-varint-acc
  :sig [(n : i64) (acc : Bytes) -> Bytes]
  ...
  :body (if (<= n 127)
          (append acc (bit-and n 127))
          (encode-unsigned-varint-acc
            (bit-shr n 7)
            (append acc (bit-or (bit-and n 127) 128)))))

(defn encode-unsigned-varint
  :sig [(n : i64) -> Bytes]
  ...
  :body (encode-unsigned-varint-acc n (bytes-new)))
```

**Key:** `append` on `Bytes` with `rc == 1` hits the COW fast path (in-place `push`). No boxing, no allocation per byte. The accumulator is sole owner throughout recursion, so every `append` is O(1).

**Note:** This eliminates the `reverse` call — varint bytes are now appended in forward order. The original used `cons` (prepend) + `reverse` because `cons` on List is O(1) but `append` on List is O(N). With `Bytes` COW append, forward accumulation is O(1) amortized.

### Change 4: Fix encode-signed-varint

Same pattern as unsigned — change `cons` + `reverse` to `append` on Bytes accumulator.

### Change 5: Fix encode-string byte construction

Currently:
```clojure
(defn encode-string
  :sig [(s : String) -> List]
  ...
  :body (let (raw : List (bytes-from-string s))
          (bytes-concat-all (list (encode-int16 (length raw)) raw))))
```

`bytes-from-string` already returns `Bytes`. `encode-int16` returns `Bytes`. `bytes-concat-all` handles mixed types. Update signature to `-> Bytes`. Also update `length raw` — `length` works on both `Bytes` and `List`.

Similar changes for: `encode-nullable-string`, `encode-compact-string`, `encode-compact-nullable-string`, `encode-bytes`, `encode-nullable-bytes`.

### Change 6: Fix encode-array fold accumulator

Currently:
```clojure
(defn encode-array
  :sig [(encoder-fn : _) (items : List) -> List]
  ...
  :body (let (encoded-items : List (fold (fn [acc item]
            (bytes-concat acc (encoder-fn item))) [] items))
          (bytes-concat-all (list (encode-int32 (length items)) encoded-items))))
```

The `fold` starts with `[]` (empty List) and uses `bytes-concat` to accumulate. This works but creates intermediate `Bytes` values at each step. Better:
```clojure
(defn encode-array
  :sig [(encoder-fn : _) (items : List) -> Bytes]
  ...
  :body (let (parts : List (map encoder-fn items))
          (bytes-concat-all (cons (encode-int32 (length items)) parts))))
```

This collects all encoded parts into a list, then does a single `bytes-concat-all` (pre-measured allocation, no O(n²) accumulation). Same change for `encode-compact-array`.

### Change 7: Add bytes-new and bytes-from-int8 builtins (AIRL repo)

Add to `crates/airl-rt/src/misc.rs`:

```rust
#[no_mangle]
pub extern "C" fn airl_bytes_new() -> *mut RtValue {
    rt_bytes(Vec::new())
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int8(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as u8, _ => 0 };
    rt_bytes(vec![val])
}
```

Register these in the builtin table so they're available as `(bytes-new)` and `(bytes-from-int8 n)`.

Also check if `bytes-new` already exists — if `(bytes-new)` doesn't exist as a builtin, an alternative is `(bytes-from-string "")` which returns empty Bytes.

### Change 8: Update protocol.airl

`encode-request-header-v1` and `encode-request-header-v2` use `bytes-concat-all` — already correct. Update type comments if any.

`kafka-recv-response` and `kafka-recv-response-v1` call `tcp-recv-exact` which returns `Bytes`. The received buffer flows into decode functions. With Change 1's type updates, this is now explicitly `Bytes` throughout — no List conversion anywhere in the receive path.

### Change 9: Update record-batch.airl

`encode-record`, `encode-record-batch`, `encode-record-batch-compressed`, `encode-record-batch-idempotent` — all use `bytes-concat-all` with encoded parts. These already work correctly since the parts are `Bytes` from the builtins. Update type annotations.

`decode-record-batch` passes `buf` from `tcp-recv` (already `Bytes`) through decode functions. No changes needed beyond type annotations.

**Compression paths:** `compress-records` calls `gzip-compress`, `snappy-compress`, etc. These accept both types via `extract_bytes()` but the `Bytes` path is zero-copy. With the SDK now producing `Bytes` throughout, compression hits the fast path.

### Change 10: Update all 26 protocol modules

Every module in `kafka/` that calls `encode-*` or `decode-*` functions inherits the type changes. Most need no code changes — just type annotation updates in `:sig` where byte buffers are passed.

**Files to update (function signatures only):**
- `kafka/produce.airl` — `produce-request-payload`, `parse-produce-response`
- `kafka/fetch.airl` — `fetch-request-payload`, `parse-fetch-response`
- `kafka/api-versions.airl` — request/response functions
- `kafka/metadata.airl` — request/response functions
- `kafka/find-coordinator.airl`
- `kafka/heartbeat.airl`
- `kafka/leave-group.airl`
- `kafka/join-group.airl`
- `kafka/sync-group.airl`
- `kafka/offset-commit.airl`
- `kafka/offset-fetch.airl`
- `kafka/init-producer-id.airl`
- `kafka/consumer-protocol.airl`
- `kafka/sasl-handshake.airl`
- `kafka/sasl-authenticate.airl`
- `kafka/sasl-plain.airl`
- `kafka/sasl-scram.airl`
- `kafka/sasl-oauthbearer.airl`
- `kafka/client.airl`
- `kafka/cluster.airl`
- `kafka/broker-pool.airl`
- `kafka/murmur2.airl`
- `kafka/producer.airl`
- `kafka/consumer.airl`
- `kafka/idempotent-producer.airl`
- `kafka/group-coordinator.airl`
- `kafka/group-consumer.airl`
- `kafka/tls.airl`

### Change 11: Update decode-array accumulation pattern

Currently `decode-array` and `decode-compact-array` use recursive helpers like `decode-array-acc` that accumulate results into a `List` using `append`. The COW list optimization makes this O(1) amortized when the accumulator has `rc == 1`. Verify the pattern is:

```clojure
(defn decode-array-acc
  :sig [(decoder-fn : _) (buf : Bytes) (offset : i64) (count : i64) (acc : List) -> Map]
  ...
  :body (if (<= count 0)
          (make-decoded acc offset)
          (let (decoded : Map (decoder-fn buf offset))
            (decode-array-acc decoder-fn buf
              (map-get decoded "offset")
              (- count 1)
              (append acc (map-get decoded "value"))))))
```

The `acc` list has `rc == 1` throughout recursion (sole owner), so `append` hits the COW fast path. **This is the COW list optimization being consumed.** Verify this pattern exists and is not broken by any sharing.

### Change 12: Update tests

All test files in `tests/` that construct byte buffers manually need updating:

**`tests/test-binary.airl`** — Core encoding/decoding tests. Change assertions from comparing against `List[Int]` to comparing against `Bytes` or using content equality (which works for both types via `extract_bytes` in comparison).

**`tests/test-record-batch.airl`** — RecordBatch encoding tests. May construct test data as lists — change to use `bytes-concat-all` or `bytes-from-*` builtins.

**All other test files** — grep for `(list N N N ...)` patterns that represent byte data and convert to `bytes-from-*` or `bytes-concat-all`.

### Change 13: Rebuild and benchmark

After all changes:
```bash
cd /home/jbarnes/repos/AIRL

# Rebuild runtime (for bytes-new, bytes-from-int8 if added)
export CARGO_TARGET_DIR=/tmp/airl-rebuild
cargo build --release -p airl-rt
mkdir -p target/release
ln -sf $CARGO_TARGET_DIR/release/libairl_rt.a target/release/libairl_rt.a
ln -sf $CARGO_TARGET_DIR/release/libairl_runtime.a target/release/libairl_runtime.a

# Rebuild g3
bash scripts/build-g3.sh

# Rebuild SDK
cd /home/jbarnes/repos/AIRL
AIRL_STDLIB=./stdlib ./g3 -- ../AIRL_castle/kafka/*.airl -o ../AIRL_castle/build/airl-kafka-sdk

# Run SDK tests
cd /home/jbarnes/repos/AIRL_castle
# Run each test via g3:
cd /home/jbarnes/repos/AIRL
AIRL_STDLIB=./stdlib ./g3 -- ../AIRL_castle/kafka/binary.airl ../AIRL_castle/tests/test-binary.airl -o /tmp/test-binary && /tmp/test-binary
AIRL_STDLIB=./stdlib ./g3 -- ../AIRL_castle/kafka/binary.airl ../AIRL_castle/kafka/record-batch.airl ../AIRL_castle/tests/test-record-batch.airl -o /tmp/test-record-batch && /tmp/test-record-batch
```

## Expected Performance Impact

Based on `kafka_sdk_bench` analysis:

| Operation | Before (List) | After (Bytes) | Improvement |
|-----------|--------------|---------------|-------------|
| TCP send 100 bytes | ~50 µs | ~1 µs | ~50x |
| TCP recv 200 bytes | ~80 µs | ~1 µs | ~80x |
| bytes-concat-all 10 parts | ~20 µs | ~2 µs | ~10x |
| crc32c 200 bytes | ~30 µs | ~1 µs | ~30x |
| encode-int32 | ~5 µs | ~0.1 µs | ~50x |
| Varint encode (5 bytes) | ~10 µs | ~0.5 µs | ~20x |
| **Full sync produce** | **369 µs** | **~150 µs** | **~2.5x** |
| **Full async produce batch** | **~2ms** | **~0.8ms** | **~2.5x** |

The primary win is eliminating per-byte boxing on encode and per-byte extraction on decode/send/compress/CRC. The COW list optimization provides secondary wins on decode-array accumulation patterns.

## Files Changed Summary

| File | Changes |
|------|---------|
| `crates/airl-rt/src/misc.rs` (AIRL repo) | Add `bytes-new`, `bytes-from-int8` builtins |
| `kafka/binary.airl` | All 26 type signatures + encode-int8, varint, string construction |
| `kafka/protocol.airl` | Type annotations |
| `kafka/record-batch.airl` | Type annotations |
| `kafka/*.airl` (26 modules) | Type annotations on byte buffer parameters |
| `tests/test-binary.airl` | Update test data construction and assertions |
| `tests/test-record-batch.airl` | Update test data construction |
| `tests/*.airl` (remaining) | Update any manual byte list construction |

## Risks

1. **Equality comparison**: `(= bytes-value list-value)` returns `false` — they're different types. Tests comparing encoded output against manually constructed `(list 0 0 0 4)` will fail. Must update test expectations to use `Bytes` or use a `bytes-equal` helper.

2. **`at` on Bytes returns Int**: `(at bytes-buf 0)` returns an `Int` (the byte value). This is the same behavior as `(at list-buf 0)` when the list contains ints. No issue.

3. **`length` on Bytes**: Returns byte count. Same as `length` on `List[Int]` when each element is one byte. No issue.

4. **Mixed types in `bytes-concat-all`**: `extract_bytes()` handles both `Bytes` and `List[Int]` inputs. During migration, some parts may still be `List` — this is safe but slower for those parts. Full migration eliminates all `List` byte buffers.

5. **Varint `append` ordering**: Current code uses `cons` (prepend) + `reverse` — changing to `append` (forward) eliminates the reverse step but changes accumulation order. Verify varint byte order is correct (MSB continuation bits, LSB first in output).
