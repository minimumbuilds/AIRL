# base64 Completeness — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Resolve two follow-up items from the builtin parity audit (`2026-04-23-builtin-deregistration-parity.md`): (1) `base64-encode-bytes` and `base64-decode-bytes` are unreachable — the Rust builtins are deregistered and no AIRL implementation exists for the Bytes→Bytes variants; (2) `stdlib/base64.airl` is not in the driver's auto-include registry, so even the String→String AIRL implementations require explicit `(import "stdlib/base64.airl")`.

## Background

The parity audit uncovered that when commit `<deregistration-commit-SHA>` deregistered all four base64 Rust builtins (`base64-encode`, `base64-decode`, `base64-encode-bytes`, `base64-decode-bytes`), only the String variants got AIRL replacements in `stdlib/base64.airl`. The Bytes variants were left with no implementation. Any code calling `(base64-encode-bytes some-bytes)` after that commit fails at runtime.

Additionally, `stdlib/base64.airl` itself is not auto-included — a user who calls `(base64-encode "hello")` from a fresh file gets a resolver error unless they `(import "stdlib/base64.airl")` first. The audit flagged this as a reachability gap (base64 deregistered but its AIRL replacement isn't globally available).

Audit rows affected:
- Row 35 `base64-decode` — ✅ Parity on signature, but NOT auto-included.
- Row 36 `base64-decode-bytes` — ❌ Unreachable. Rust deregistered, no AIRL impl.
- Row 37 `base64-encode-bytes` — ❌ Unreachable. Rust deregistered, no AIRL impl.
- (Row for `base64-encode`): ✅ Parity on signature, but NOT auto-included.

The Rust functions `airl_base64_encode_bytes` and `airl_base64_decode_bytes` still exist in `crates/airl-rt/src/misc.rs` (lines 999-1015). They were never deleted — just unregistered from the builtin dispatch in `bytecode_aot.rs` and `bytecode_vm.rs`. Re-registering them is a one-line-each flip.

## Goals

1. Make `base64-encode-bytes` and `base64-decode-bytes` reachable again by re-registering the existing Rust functions in both `bytecode_aot.rs` and `bytecode_vm.rs`.
2. Make `base64-encode` and `base64-decode` (the String AIRL implementations in `stdlib/base64.airl`) auto-available by adding `base64.airl` to `STDLIB_MODULES` in `crates/airl-driver/src/pipeline.rs`.
3. Update audit rows to reflect the resolved status.
4. Add an AOT fixture test that exercises all four base64 functions end-to-end.

## Non-Goals

- Re-registering the String variants as Rust builtins. The AIRL implementations work for typical payload sizes; stay consistent with the "deregister when pure-AIRL performance is OK" pattern already established (e.g. json, path).
- Writing AIRL Bytes→Bytes implementations of base64. Pure-AIRL Bytes crypto hit OOM regressions (per CLAUDE.md and the SHA256 precedent: `// sha256-bytes and hmac-sha256-bytes re-registered as C builtins`). Base64 has the same risk on large payloads. Rust is the correct provider.
- Auditing other crypto-related deregistrations. The audit already covered them (row ~rows for sha256, hmac, pbkdf2).
- Performance tuning the existing AIRL `base64-encode` / `base64-decode`. Out of scope.

## Architecture

Three independent edits, one PR:

### 1. Re-register `-bytes` Rust builtins

**`crates/airl-runtime/src/bytecode_vm.rs`** — around line 794 (currently says `// base64-decode-bytes, base64-encode-bytes removed above`). Replace the comment with active dispatch lines:

```rust
// Remove the "removed above" comment.
// Add just above the bitwise-xor line:
"base64-encode-bytes" => airl_rt::misc::airl_base64_encode_bytes(a0!()),
"base64-decode-bytes" => airl_rt::misc::airl_base64_decode_bytes(a0!()),
```

**`crates/airl-runtime/src/bytecode_aot.rs`** — the analogous block (deregistered around line 1116 per the audit). Same pattern: replace the "deregistered" comment with active dispatch insertions for `base64-encode-bytes` and `base64-decode-bytes`.

The String variants (`base64-encode`, `base64-decode`) STAY deregistered — the `stdlib/base64.airl` AIRL implementations continue to provide those.

### 2. Auto-include `stdlib/base64.airl`

This section assumes the stdlib registry consolidation from `2026-04-23-stdlib-registry-consolidation-design.md` has landed. If this spec is implemented **before** the consolidation merges, the work below becomes six edits across multiple lists instead of one.

**`crates/airl-driver/src/pipeline.rs`:**

- Add `const BASE64_SOURCE: &str = include_str!("../../../stdlib/base64.airl");` alongside the other stdlib `include_str!` declarations.
- Add a row to `STDLIB_MODULES`:
  ```rust
  StdlibModule {
      source: BASE64_SOURCE,
      path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/base64.airl"),
      name: "base64",
      has_extern_c: false,
      on_airlos: true,
  },
  ```
- Position: after `JSON_SOURCE` entry, before `SQLITE_SOURCE` entry. Alphabetical by name is not the convention here — the existing order is by "pure-AIRL first, extern-C last". `base64` is pure-AIRL, so it goes in the pure-AIRL block.

The stability test `stdlib_embed_hash_is_stable` will FAIL after this change because the embed hash changes. Update the anchor value. This is correct and expected — the hash is pinned to catch **unintended** drift, and adding a new module is an intended change. Bump the anchor.

### 3. Update audit document

`docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`:

- Row 35 (`base64-decode`): change `NOT auto-included` → `Auto-included (via STDLIB_MODULES entry added 2026-04-23)`.
- Row 36 (`base64-decode-bytes`): change status from `❌ Unreachable` → `✅ Parity`. Notes: `Re-registered as Rust builtin 2026-04-23 after audit flagged as unreachable. Rust implementation at airl-rt/src/misc.rs:999.`
- Row 37 (`base64-encode-bytes`): same treatment.
- (Row for `base64-encode`): same auto-include note as row 35.

Update the Summary counts: **2 drift (fixed)** becomes **2 drift (fixed) + 2 unreachable (fixed)**, and **2 unreachable** becomes **0**. Add a line to the "Drift fixes applied in this PR" section describing the base64 re-registrations and auto-include.

### 4. Add AOT fixture

New file: `tests/aot/round3_builtin_base64_full.airl`

```clojure
;; EXPECT: str-rt:ok|str-known:ok|bytes-rt:ok|bytes-known:ok
(let (s1 : String (base64-encode "Hello, AIRL!"))
     (rt1 : String (base64-decode s1))
     (known1 : String "SGVsbG8sIEFJUkwh")
     (b1 : Bytes (bytes-from-string "Hello, AIRL!"))
     (s2 : Bytes (base64-encode-bytes b1))
     (rt2 : Bytes (base64-decode-bytes s2))
  (print (str
    "str-rt:" (if (= rt1 "Hello, AIRL!") "ok" (str "bad:" rt1))
    "|str-known:" (if (= s1 known1) "ok" (str "bad:" s1))
    "|bytes-rt:" (if (= (bytes-to-string rt2) "Hello, AIRL!") "ok" (str "bad:" (bytes-to-string rt2)))
    "|bytes-known:" (if (= (bytes-to-string s2) known1) "ok" (str "bad:" (bytes-to-string s2))))))
```

Notes:
- No `DEPS:` line needed because `base64.airl` is now auto-included via `STDLIB_MODULES`.
- Tests both String (AIRL impl) and Bytes (Rust builtin) code paths.
- Uses a known-output fixture (`"SGVsbG8sIEFJUkwh"`) to verify round-trip AND correctness-against-external-baseline.

## Files modified

| File | Change |
|------|--------|
| `crates/airl-runtime/src/bytecode_vm.rs` | Re-register `base64-encode-bytes` / `base64-decode-bytes`; update comment block. |
| `crates/airl-runtime/src/bytecode_aot.rs` | Same re-registration in the AOT dispatch. |
| `crates/airl-driver/src/pipeline.rs` | Add `BASE64_SOURCE` constant + `StdlibModule` registry entry. Bump `stdlib_embed_hash_is_stable` anchor value. |
| `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` | Update rows 35-37 (and base64-encode row) + Summary counts. |
| `tests/aot/round3_builtin_base64_full.airl` | New fixture covering all four functions. |

## Testing

- `cargo test -p airl-runtime -p airl-driver` — both pass. The re-registration is additive dispatch insertion; no prior behavior changes.
- `rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh` — now 69/68 (the new base64 fixture adds one). All pass.
- **`stdlib_embed_hash_is_stable` will initially fail** — this is the anchor test. Update the anchor value to the new hash (the implementer captures it via the same temporary-print technique used in the consolidation plan).
- Manual smoke: `airl run '(base64-encode-bytes (bytes-from-string "test"))'` should print a valid base64-encoded byte string, not error.

## Risks

- **`base64.airl` dependencies on prelude.** `stdlib/base64.airl` uses `at`, `length`, `str`, `bytes-from-string`, `bytes-to-string`, etc. These are core builtins or prelude entries — should be available in every auto-include context. Verify by compiling `base64.airl` in isolation as part of the `STDLIB_MODULES` iteration order: if `base64` comes AFTER `collections`, `string`, any internal callers have their dependencies resolved. Current placement (after json, before sqlite) satisfies this.
- **Platform gating.** The Rust `airl_base64_encode_bytes` and `airl_base64_decode_bytes` are `#[cfg(not(target_os = "airlos"))]`-gated. On airlos builds, re-registering them would fail to compile. Mirror the same cfg gate on the dispatch lines in `bytecode_vm.rs` and `bytecode_aot.rs`. Look at how `airl_random_bytes` or another airlos-gated builtin is dispatched for the idiomatic pattern.
- **Fixture test dependency on auto-include.** The fixture omits a `DEPS:` line because it assumes `base64.airl` is auto-included. If this spec's auto-include step is not applied (perhaps because consolidation hasn't merged), the fixture fails. Correct — but the fixture file itself is only added alongside the auto-include change, so the ordering is enforced.
- **Audit document drift.** If a future PR modifies the audit before this one lands, row numbers may shift. The implementer should use row content (`base64-encode-bytes`) rather than row numbers when finding the rows to edit.

## Invariants Preserved

- No change to existing passing tests. The re-registration is strictly additive.
- No change to the AIRL `base64-encode` / `base64-decode` implementations — they continue to work identically.
- Rust `airl_base64_encode_bytes` / `airl_base64_decode_bytes` functions are untouched. Only their dispatch-map entries are added back.
- `STDLIB_MODULES` order is preserved except for the insertion of the new `base64` entry, which goes in a position that doesn't break the compilation-order dependency chain (base64 depends on prelude/string/map/set, all of which are earlier in the registry).

## Out of Scope / Future Work

- **Complete deregistration of base64.** If future performance analysis shows the AIRL `base64-encode` and `base64-decode` are competitive with Rust, consider re-auditing whether to also port `-bytes` variants to AIRL. For now, keep the split.
- **Other unreachable builtins.** The audit flagged 2 unreachable (this spec fixes both). If any are discovered in a future audit, they warrant their own spec.
- **Performance benchmarks.** Not in scope. This spec is about correctness (reachability + parity), not speed.
