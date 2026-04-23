# stdlib/json.airl Missing from Auto-Include Registry — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Fix the pre-existing AOT linker failure for `json-parse` and `json-stringify` by adding `stdlib/json.airl` to the driver's auto-included stdlib list. Audit and document the other seven stdlib files currently missing from the list.

## Background

Running `tests/aot/round3_builtin_json_full.airl` fails at link time with:

```
/usr/bin/ld: airl_program:(.text+0x9f8): undefined reference to `__airl_fn_json_parse'
/usr/bin/ld: airl_program:(.text+0x130e): undefined reference to `__airl_fn_json_stringify'
collect2: error: ld returned 1 exit status
[g3] err:  linker failed: Some(1)
```

This reproduces on `main` (`7f6a982`) without any other changes applied — it is not caused by the strict enforcement policy migration.

### Root cause

The AIRL driver auto-includes a fixed list of stdlib files at compile time. This list lives in `crates/airl-driver/src/pipeline.rs`:

```rust
const STDLIB_PATHS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/prelude.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/math.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/result.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/string.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/map.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/set.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/io.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/path.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),
    #[cfg(not(target_os = "airlos"))]
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),
];
```

Corresponding `include_str!` constants (`COLLECTIONS_SOURCE`, `MATH_SOURCE`, etc.) exist for each file and feed into `stdlib_embed_hash()`.

`stdlib/json.airl` is a 332-line, fully-defined module providing `json-parse`, `json-stringify`, and their helpers in pure AIRL. It is **not** in `STDLIB_PATHS`, has no `JSON_SOURCE` constant, and does not contribute to the embed hash.

The intent was clearly that these AIRL implementations would replace the Rust builtins — two comments explicitly say so:

```
// crates/airl-runtime/src/bytecode_aot.rs:1084
// json-parse, json-stringify
// deregistered — AIRL stdlib equivalents in json.airl take over
```

```
// crates/airl-runtime/src/bytecode_vm.rs:742
// json-parse, json-stringify
// deregistered — AIRL stdlib equivalents in json.airl take over
```

But the replacement was never wired into the auto-include path. Result: `json-parse` and `json-stringify` have no implementation at link time.

### Audit: other stdlib files in the same state

Comparing `stdlib/*.airl` to the embedded list turns up seven additional files that live in `stdlib/` but are not auto-included:

| File | Likely disposition |
|------|--------------------|
| `stdlib/json.airl` | **Bug — fix in this spec.** Rust builtins deregistered; AIRL implementation exists. |
| `stdlib/aircon.airl` | Probably intentional (Claude/AIR connector — domain-specific, opt-in via explicit import). Needs confirmation. |
| `stdlib/base64.airl` | Rust builtins (`base64-encode`, `base64-decode`) are still registered in `airl-rt` — probably intentional (AIRL wrapper for docs/completeness, Rust is canonical). Needs confirmation. |
| `stdlib/hmac.airl` | Same shape as base64 — AIRL wrapper over Rust builtins. |
| `stdlib/sha256.airl` | Same. Note the explicit comment in `bytecode_aot.rs:1108`: `// sha256-bytes and hmac-sha256-bytes re-registered as C builtins`. |
| `stdlib/pbkdf2.airl` | Same. |
| `stdlib/test.airl` | Almost certainly intentional — test framework, opt-in. |

Of these, **only `json.airl` has the "Rust deregistered, AIRL replaces" pattern documented in code**. The others appear to be intentional AIRL wrappers over registered Rust builtins. This spec treats `json.airl` as the only confirmed bug and recommends a follow-up audit for the remainder.

## Goals

1. Add `stdlib/json.airl` to the driver's auto-include list so `json-parse` and `json-stringify` resolve at link time.
2. Add a regression test to the AOT suite (`round3_builtin_json_full` already exists — it'll go from red to green).
3. Document in-code why `json.airl` must be auto-included (the "Rust deregistered" semantics).

## Non-Goals

- Changing the stdlib auto-include mechanism itself.
- Resolving the status of `aircon.airl`, `base64.airl`, `hmac.airl`, `sha256.airl`, `pbkdf2.airl`, `test.airl` — those need an audit spec of their own if anything's wrong.
- Deregistering any remaining Rust builtins, or adding new AIRL wrappers.
- Optimizing json-parse/json-stringify performance.

## Design

### Changes to `crates/airl-driver/src/pipeline.rs`

Three additions, all in the existing stdlib block at lines 48-60 and 1057-1087:

**1. Add the `include_str!` constant** alongside the existing ones (around line 58):

```rust
const JSON_SOURCE: &str = include_str!("../../../stdlib/json.airl");
```

**2. Add the path to `STDLIB_PATHS`** (around line 1066):

```rust
const STDLIB_PATHS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/prelude.airl"),
    // ... existing entries ...
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/json.airl"),
    #[cfg(not(target_os = "airlos"))]
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),
];
```

Placement: before the `sqlite.airl` cfg-gated entry. `json.airl` has no platform gate because it's pure AIRL with no FFI dependencies.

**3. Hash it in `stdlib_embed_hash()`** (around line 1079):

```rust
fn stdlib_embed_hash() -> u64 {
    // ...
    RANDOM_SOURCE.hash(&mut hasher);
    JSON_SOURCE.hash(&mut hasher);
    #[cfg(not(target_os = "airlos"))]
    SQLITE_SOURCE.hash(&mut hasher);
    // ...
}
```

### Inline comment

Add a comment above the `JSON_SOURCE` constant explaining why json.airl is auto-included while base64.airl etc. are not:

```rust
// json.airl is auto-included (like prelude.airl) because the Rust builtins
// `json-parse` and `json-stringify` were deregistered in bytecode_aot.rs and
// bytecode_vm.rs; the AIRL implementations in stdlib/json.airl are now the
// only providers of those symbols. See also the "deregistered — AIRL stdlib
// equivalents in json.airl take over" comments in those files.
const JSON_SOURCE: &str = include_str!("../../../stdlib/json.airl");
```

### Use-site wiring

`STDLIB_PATHS` is consumed in places that iterate over it (for AOT embedding, for change detection). Callers of the individual `*_SOURCE` constants (if any beyond `stdlib_embed_hash`) need `JSON_SOURCE` too. A grep for `PRELUDE_SOURCE`, `MATH_SOURCE`, etc. will reveal whether a bespoke list appears elsewhere. Any mirroring list needs `JSON_SOURCE` inserted in the same relative position.

## Test Plan

### Fixture-level regression

`tests/aot/round3_builtin_json_full.airl` already exists and exercises `json-parse` (int, string, bool, null, round-trip) and `json-stringify` (int, string, list). With the fix, this test transitions from `COMPILE_FAIL` to `PASS`. No new fixture needed.

Verification:

```bash
rm -rf tests/aot/cache/round3_builtin_json_full
bash tests/aot/run_aot_tests.sh 2>&1 | grep "round3_builtin_json_full"
# Expected: PASS: round3_builtin_json_full
```

### Embed-hash regression

`stdlib_embed_hash()` is used to detect stdlib changes across compile sessions. Adding `json.airl` to the hash input is a one-time cache-invalidation event — existing caches will miss once after the fix lands, which is the intended behavior (all stdlib caches regenerate with json.airl included).

### Full Rust suite

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: no regressions. The only functional change is that the compiler now includes ~15KB of additional AIRL source in every compile, increasing AOT binary size by a small amount.

### AOT suite

```bash
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh
```

Expected: 68/68 PASS (currently 67/68).

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-driver/src/pipeline.rs` | Add `JSON_SOURCE` constant, add to `STDLIB_PATHS`, hash in `stdlib_embed_hash()`. |
| (none) | No changes to `stdlib/json.airl`, to Rust builtin deregistration, or to the test fixture. |

Exactly one file touched.

## Risks

- **Minimal.** `stdlib/json.airl` has existed for months; it has been parseable by the existing Rust pipeline in every code path that imports it explicitly. The fix just makes it implicit everywhere (matching the pattern of the other stdlib files).
- **Compile-time cost:** negligible — 332 lines × ~190 driver invocations during a full test run. Already embedded at Rust build time via `include_str!`.
- **Binary size:** `json.airl` compiles to roughly the same size as `string.airl` (both pure-AIRL stdlib modules). Adds a few KB to every AOT binary.
- **Symbol collisions:** none. `json-parse` and `json-stringify` are explicitly not registered as Rust builtins (code comments confirm). The AIRL implementations are unique providers of those names.

## Out of Scope / Follow-Up

Audit spec: document the intended disposition of the remaining seven stdlib files not in `STDLIB_PATHS` (`aircon.airl`, `base64.airl`, `hmac.airl`, `sha256.airl`, `pbkdf2.airl`, `test.airl`, and whether `json.airl` was a one-off or a pattern). Expected answer: most are AIRL wrappers over registered Rust builtins and are opt-in via `(import "stdlib/...")`, not bugs. But the pattern of implicit-vs-explicit inclusion deserves an explicit rule documented in `AIRL-Header.md`.

## Invariants Preserved

- Bootstrap compiler (`bootstrap/g3_compiler.airl`) builds with or without the fix — it does not use `json-parse`/`json-stringify`.
- All 533 existing Rust unit tests continue to pass.
- `airl verify-policy` continues to report OK (json.airl is already grandfathered as `:verify checked` after commit `b4169e3`).
- No external dependencies added.
