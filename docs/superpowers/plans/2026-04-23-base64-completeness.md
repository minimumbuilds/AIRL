# base64 Completeness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-register `airl_base64_encode_bytes` and `airl_base64_decode_bytes` Rust builtins, and add `stdlib/base64.airl` to `STDLIB_MODULES`. End result: all four base64 functions (encode/decode, String and Bytes variants) work without explicit import.

**Architecture:** Re-add dispatch entries in `bytecode_vm.rs` + `bytecode_aot.rs` (platform-gated to match the underlying Rust fns), add one `StdlibModule` registry entry in `pipeline.rs`, bump `stdlib_embed_hash_is_stable` anchor, update audit, add fixture.

**Spec:** `docs/superpowers/specs/2026-04-23-base64-completeness-design.md`

---

## Task 1: Re-register base64 `-bytes` Rust builtins

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_vm.rs` (~line 794)
- Modify: `crates/airl-runtime/src/bytecode_aot.rs` (~line 1116)

Each file's dispatch block has a comment saying "base64-encode, base64-decode, base64-encode-bytes, base64-decode-bytes deregistered — AIRL stdlib equivalents in base64.airl take over". Below that is a gap where the `-bytes` entries used to sit.

- [ ] **Step 1: Inspect platform gating on the Rust functions**

```
grep -B2 "fn airl_base64_encode_bytes\|fn airl_base64_decode_bytes" crates/airl-rt/src/misc.rs
```

Note any `#[cfg(...)]` attributes on the Rust functions. The spec says they are `#[cfg(not(target_os = "airlos"))]` — confirm. Your dispatch additions must mirror the same cfg.

- [ ] **Step 2: Re-register in `bytecode_vm.rs`**

Find the `Crypto` dispatch block. After the comment `// base64-encode, base64-decode, base64-encode-bytes, base64-decode-bytes / // deregistered — AIRL stdlib equivalents in base64.airl take over`, and before the `"random-bytes" => ...` line (OR wherever base64 would fit alphabetically — mirror the convention of neighbors like `bytes-xor`), insert:

```rust
#[cfg(not(target_os = "airlos"))]
"base64-encode-bytes" => airl_rt::misc::airl_base64_encode_bytes(a0!()),
#[cfg(not(target_os = "airlos"))]
"base64-decode-bytes" => airl_rt::misc::airl_base64_decode_bytes(a0!()),
```

Also update the surrounding comment block: change "deregistered — AIRL stdlib equivalents take over" to reflect that only the String variants are AIRL-provided now:

```rust
// Crypto
// base64-encode, base64-decode: AIRL stdlib impl in base64.airl (deregistered as Rust builtins).
// base64-encode-bytes, base64-decode-bytes: re-registered as Rust builtins 2026-04-23 after audit
//   found them unreachable (no AIRL impl existed for Bytes→Bytes).
```

And delete the `// base64-decode-bytes, base64-encode-bytes removed above` line elsewhere in the same block.

- [ ] **Step 3: Same change in `bytecode_aot.rs`**

Apply the symmetric edit to `crates/airl-runtime/src/bytecode_aot.rs`'s equivalent Crypto section. The dispatch machinery may differ (lookup into an `HashMap<String, fn>` vs a match statement) — match whatever pattern the surrounding code uses. Grep for how `airl_random_bytes` or `airl_sha256_bytes` is registered there to find the idiom.

- [ ] **Step 4: Build**

```
cargo build --features aot
```

Expected: clean. No warnings about unused functions.

- [ ] **Step 5: Quick smoke**

Compile and run an inline test:

```
echo '(print (bytes-to-string (base64-decode-bytes (base64-encode-bytes (bytes-from-string "hi")))))' | cargo run --release --features aot -- run /dev/stdin
```

Expected output: `hi`. This proves the Rust dispatch is wired.

- [ ] **Step 6: DO NOT commit yet.** We commit once at the end.

---

## Task 2: Add `stdlib/base64.airl` to the STDLIB_MODULES registry

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Add the `BASE64_SOURCE` constant**

Find the `*_SOURCE` include_str! constants (near the top of the file, around line 49-60). Add:

```rust
const BASE64_SOURCE: &str = include_str!("../../../stdlib/base64.airl");
```

Place it alphabetically or in the position implied by the order of the existing constants — look at the existing order and insert consistently.

- [ ] **Step 2: Add the `StdlibModule` registry entry**

Find `STDLIB_MODULES` (should be an ordered const array per the consolidation refactor). Insert a new entry. Position: after `JSON_SOURCE` entry, before `SQLITE_SOURCE` entry (per the spec):

```rust
StdlibModule {
    source: BASE64_SOURCE,
    path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/base64.airl"),
    name: "base64",
    has_extern_c: false,
    on_airlos: true,
},
```

- [ ] **Step 3: Compile**

```
cargo build
```

Expected: clean. If `stdlib/base64.airl` references functions from earlier registry entries (e.g. `at`, `length`, `str`), they should resolve because base64 is placed after collections/string/map — check the registry order if compile fails.

- [ ] **Step 4: Capture the NEW embed hash**

Add a temporary print test:

```rust
#[test]
fn __capture_stdlib_embed_hash_after_base64() {
    eprintln!("STDLIB_EMBED_HASH_AFTER_BASE64 = {}", stdlib_embed_hash());
}
```

Run:

```
cargo test -p airl-driver __capture_stdlib_embed_hash_after_base64 -- --nocapture 2>&1 | grep STDLIB_EMBED_HASH
```

Record the `u64`. Delete the temporary test. The existing `stdlib_embed_hash_is_stable` test currently expects `16774069352182620680` (from the consolidation refactor). That value is now stale — the new hash differs because we added base64. Update the `stdlib_embed_hash_is_stable` test's expected constant to the new captured value.

Add an inline comment next to the constant noting the lineage:

```rust
const EXPECTED: u64 = <NEW-VALUE>;  // Updated 2026-04-23 after adding base64.airl to STDLIB_MODULES.
```

- [ ] **Step 5: Run tests**

```
cargo test -p airl-driver
```

Expected: all pass, including registry invariant tests and the stability test with the new anchor.

- [ ] **Step 6: DO NOT commit yet.**

---

## Task 3: Add AOT fixture

**Files:**
- Create: `tests/aot/round3_builtin_base64_full.airl`

- [ ] **Step 1: Write the fixture**

File content:

```
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

**Do NOT add a `DEPS:` line** — base64.airl is now auto-included via STDLIB_MODULES.

- [ ] **Step 2: Run the full AOT suite**

```
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh 2>&1 | tail -10
```

Expected: new fixture count is 69 (was 68). The new test passes. Total: `69 passed, 0 failed, 0 compile errors, 0 skipped`.

- [ ] **Step 3: DO NOT commit yet.**

---

## Task 4: Update audit document

**Files:**
- Modify: `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`

- [ ] **Step 1: Update rows 36 and 37 (base64-encode-bytes, base64-decode-bytes)**

Change Status column from `❌ Unreachable` → `✅ Parity`.

Update Notes column to describe the re-registration. Example for row 36:

```
Rust function `airl_base64_decode_bytes` (airl-rt/src/misc.rs:999) re-registered as builtin in bytecode_vm.rs and bytecode_aot.rs on 2026-04-23 after this audit flagged it as unreachable. AIRL stdlib has no Bytes-variant implementation; Rust is the sole provider. Parity: the Bytes-to-Bytes contract of the Rust function is fully preserved by re-registration.
```

- [ ] **Step 2: Update rows 34 and 35 (base64-encode, base64-decode — the String variants)**

These were ✅ Parity but had a "NOT auto-included" note. Update that note to:

```
Auto-included via STDLIB_MODULES entry (added 2026-04-23 — see docs/superpowers/specs/2026-04-23-base64-completeness-design.md).
```

Find rows by content — search for `base64-decode\b` and `base64-encode\b` (without `-bytes`).

- [ ] **Step 3: Update the Summary counts**

The summary table currently reads "29 Parity, 2 Drift (fixed), 4 Intentional, 2 Unreachable". Update to:
- Parity: 29 + 2 = 31
- Drift (fixed): 2
- Intentional: 4
- Unreachable: 0

Total unchanged at 37. Add a summary line: "Unreachable count closed via base64 completeness PR 2026-04-23."

- [ ] **Step 4: Add a Drift-fixes-applied entry**

In the "Drift fixes applied in this PR" section, add:

```
- base64-encode-bytes and base64-decode-bytes Rust builtins re-registered in bytecode_vm.rs and bytecode_aot.rs.
- stdlib/base64.airl added to STDLIB_MODULES in pipeline.rs, making base64-encode and base64-decode auto-available.
```

- [ ] **Step 5: Update the Follow-up section**

Item "5. `json.airl` and `base64.airl` auto-include consideration" — strike the `base64.airl` half or remove the item. `json.airl` is now auto-included via the consolidation consolidation; verify that and adjust the follow-up text accordingly.

---

## Task 5: Commit

**Files:**
- Touched by prior tasks: `crates/airl-runtime/src/bytecode_vm.rs`, `crates/airl-runtime/src/bytecode_aot.rs`, `crates/airl-driver/src/pipeline.rs`, `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`, `tests/aot/round3_builtin_base64_full.airl`

- [ ] **Step 1: Full Rust suite**

```
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: all pass.

- [ ] **Step 2: AOT suite**

```
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh 2>&1 | tail -5
```

Expected: 69 pass, 0 fail, 0 compile errors, 0 skipped.

- [ ] **Step 3: Commit**

```bash
git add crates/airl-runtime/src/bytecode_vm.rs \
        crates/airl-runtime/src/bytecode_aot.rs \
        crates/airl-driver/src/pipeline.rs \
        docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md \
        tests/aot/round3_builtin_base64_full.airl
git commit -m "$(cat <<'EOF'
fix(rt,driver): complete base64 deregistration — 4/4 variants reachable

The parity audit flagged base64-encode-bytes and base64-decode-bytes
as unreachable: all four Rust builtins were deregistered but only the
String variants had AIRL replacements in stdlib/base64.airl. The
Bytes variants were left with no provider and would fail at runtime.

Fix:
1. Re-register airl_base64_encode_bytes and airl_base64_decode_bytes
   in both bytecode_vm.rs and bytecode_aot.rs. The Rust functions
   themselves were never deleted — only unregistered from dispatch.
2. Add stdlib/base64.airl to STDLIB_MODULES in pipeline.rs so the
   String variants (AIRL impl) are auto-available without explicit
   import. Bumped stdlib_embed_hash_is_stable anchor to reflect new
   auto-included source.
3. New fixture tests/aot/round3_builtin_base64_full.airl exercises
   all four functions + known base64 output for "Hello, AIRL!".
4. Audit document updated: unreachable count 2 → 0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Do NOT push. Do NOT merge.
