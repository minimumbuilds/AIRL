# AIRL/Rust Builtin Deregistration Parity Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Catalog every Rust builtin marked "deregistered — AIRL stdlib equivalent takes over", verify each AIRL replacement matches the old Rust signature (especially return type), fix any drift found, and add a preventive convention to `AIRL-Header.md`.

**Architecture:** Three artifacts: (1) `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` — a tracked table; (2) zero or more fixes to `stdlib/*.airl` modules for any drift discovered; (3) docs update in `AIRL-Header.md` + `CLAUDE.md`.

**Tech Stack:** Mostly grep + reading. Any fixes are small edits to existing AIRL files with smoke tests via the AOT suite.

**Spec:** `docs/superpowers/specs/2026-04-23-airl-rust-builtin-parity-audit-design.md`

---

## Task 1: Enumerate deregistered builtins

**Files:**
- Read: `crates/airl-runtime/src/bytecode_vm.rs`, `crates/airl-runtime/src/bytecode_aot.rs`

- [ ] **Step 1: Grep for all deregistration markers**

```bash
grep -n "deregistered\|replaced by AIRL\|AIRL stdlib equivalent\|no longer register" \
  crates/airl-rt/src/ crates/airl-runtime/src/ 2>/dev/null
```

- [ ] **Step 2: Read each hit in context**

For every file:line from Step 1, read ~20 lines around it. Note:
- Which builtin name(s) are commented out in the adjacent lines
- Which AIRL stdlib file the comment references (e.g. "json.airl take over")

Build a working list like:

```
bytecode_aot.rs:1085  json-parse, json-stringify → stdlib/json.airl
bytecode_vm.rs:743    json-parse, json-stringify → stdlib/json.airl  (duplicate of above)
bytecode_aot.rs:1008  ... → stdlib/string.airl
...
```

- [ ] **Step 3: Deduplicate by builtin name**

A builtin appearing in both `bytecode_vm.rs` and `bytecode_aot.rs` is one audit row. Collapse duplicates.

Target: a bullet list of unique `(rust_builtin_name, target_airl_file)` pairs, roughly 10-20 entries.

---

## Task 2: Audit each builtin's parity

**Files:**
- Read: `crates/airl-rt/src/*.rs`, `stdlib/*.airl`

For each deduplicated entry from Task 1:

- [ ] **Step 1: Find the Rust function definition**

```bash
grep -rn "pub extern \"C\" fn airl_<name_with_underscores>" crates/airl-rt/src/ crates/airl-runtime/src/
```

Where `<name_with_underscores>` is the AIRL builtin name with `-` replaced by `_` (e.g. `json-parse` → `json_parse`). The function likely still exists — deregistration only removes the builtin-map entry, not the function.

Capture the function's signature: return type (raw pointer? `Result`-variant?), parameter count, parameter types.

If the function no longer exists, `git log -S<name> --all` will show when it was deleted and from what state. In that case the audit row uses "removed" as the Rust signature and relies on the prior git history.

- [ ] **Step 2: Find the AIRL replacement in `stdlib/`**

```bash
grep -n "defn <builtin-name>" stdlib/*.airl
```

Capture the AIRL `:sig`:
- Parameter count and types (`:sig [(x : i64) (y : String) -> Bool]`)
- Return type — especially whether wrapped in `Result` via an `Ok`/`Err` pair

- [ ] **Step 3: Compare**

Parity status:
- **✅ Parity** — same param count, compatible types, same return-type shape (both raw OR both `Result`-wrapped).
- **❌ Drift** — return type mismatch (especially `Result` vs raw), or param count differs.
- **⚠️ Intentional** — differs but is a deliberate improvement. Document rationale.

Be conservative: only flag drift where observable behavior differs. AIRL's type system is less granular than Rust's; don't flag purely cosmetic differences like `:sig` missing where the Rust type was generic.

- [ ] **Step 4: Check reachability**

For every ✅ row, verify the AIRL replacement is actually reachable in at least one code path. Check:
- Is the AIRL stdlib file in `STDLIB_MODULES`? (See `crates/airl-driver/src/pipeline.rs`.) If yes, it's auto-included — reachable everywhere.
- If not auto-included, is it imported by any test fixture or bootstrap file? (`grep -rn "(import \"stdlib/<file>\"" stdlib/ bootstrap/ tests/`)
- If NEITHER auto-included NOR imported anywhere, the deregistration has created an unreachable function. This is a separate class of bug; note it in the audit row's Notes column.

---

## Task 3: Fill in the audit document

**Files:**
- Create: `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`

- [ ] **Step 1: Create the directory if missing**

```bash
mkdir -p docs/superpowers/audits
```

- [ ] **Step 2: Write the audit document**

Template to fill in:

```markdown
# AIRL/Rust Builtin Deregistration Parity Audit

**Date:** 2026-04-23
**Auditor:** (agent)
**Methodology:**
  - Grep `crates/{airl-rt,airl-runtime}/src/` for `deregistered|replaced by AIRL|AIRL stdlib equivalent`
  - For each unique builtin, compare the Rust `extern "C"` signature (in `airl-rt`) to the AIRL `defn`'s `:sig` in `stdlib/*.airl`.
  - Flag drift where observable behavior differs (return-type shape, param count, error semantics).

## Summary

| Metric | Count |
|--------|-------|
| Total deregistered builtins audited | N |
| ✅ Parity | X |
| ❌ Drift (fixed in this PR) | Y |
| ⚠️ Intentional | Z |
| Unreachable (neither auto-included nor imported) | W |

## Findings

| # | Rust builtin | Rust signature | AIRL module | AIRL signature | Status | Notes |
|---|--------------|----------------|-------------|----------------|--------|-------|
| 1 | `json-parse` | `fn(*mut RtValue) -> *mut RtValue` returning a Result variant (`Ok v` / `Err e`) | `stdlib/json.airl` | `:sig [(s : String) -> Result<_>]` returning `(Ok value)` / `(Err "...")` | ✅ Parity | Fixed 2026-04-23 in commit `8fd4a23` after initial audit showed drift (raw value vs Result). |
| 2 | `json-stringify` | `fn(*mut RtValue) -> *mut RtValue` returning `String` | `stdlib/json.airl` | `:sig [(val : Any) -> String]` returning `String` | ✅ Parity | |
| 3 | ... | ... | ... | ... | ... | ... |

## Drift fixes applied in this PR

(list any ❌ rows with a one-line description of what was changed in which file; if no drift found, write "None.")

## Follow-up

(any unreachable functions, missing auto-include entries, or process observations that should be handled in a separate PR)
```

Fill in the actual findings from Task 2. Do not invent rows; every row must correspond to a real deregistration in the codebase.

---

## Task 4: Fix any drift found

For each ❌ row in the audit:

- [ ] **Step 1: Decide fix direction**

Either:
- **Fix AIRL to match Rust** (default — preserve the interface callers expect).
- **Mark as intentional** (only if the AIRL version is a deliberate improvement; downgrade the row to ⚠️ and add the rationale in Notes).

- [ ] **Step 2: Apply the fix**

Edit `stdlib/<module>.airl` to adjust the AIRL function's return shape or signature.

- [ ] **Step 3: Add a test**

For each fix, add a fixture test under `tests/aot/round*_<builtin>_parity.airl` that calls the corrected AIRL function and asserts the Rust-compatible return shape. Annotate with the expected output per the existing fixture harness convention (`;; EXPECT: <text>`).

- [ ] **Step 4: Run affected tests**

```bash
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh 2>&1 | tail -10
```

Expected: 68/68 (or 68 + the new fixture tests you added). Any regression in unrelated tests means the fix altered callers downstream — STOP and report.

**If no drift found, skip this task entirely.** The audit is still valuable as documentation.

---

## Task 5: Add the deregistration convention to docs

**Files:**
- Modify: `AIRL-Header.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Append to `AIRL-Header.md`**

Find an appropriate place (near the builtins reference, or after it). Add this section:

```markdown
## Deregistering a Rust builtin

When replacing a Rust `extern "C"` builtin with an AIRL stdlib implementation:

1. **Before commenting out the Rust registration,** confirm the AIRL implementation has the same visible signature as the Rust one — parameter count, types, and especially return type (raw value vs `Result`).
2. **Add the AIRL stdlib file to `STDLIB_MODULES` in `crates/airl-driver/src/pipeline.rs`** if it's not already there. Otherwise the AIRL replacement will be unreachable and the deregistration produces silent latent bugs.
3. **Add a row to** `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` — the tracked audit of every deregistered builtin.
4. **Run the AOT suite** (`bash tests/aot/run_aot_tests.sh`) with a fixture that exercises the deregistered function; the AIRL replacement must pass the same test.

Background: see `docs/superpowers/specs/2026-04-23-stdlib-json-autoinclude-fix.md` for a concrete instance where a missing auto-include + signature drift combined to produce a silent bug.
```

- [ ] **Step 2: Append to `CLAUDE.md` under Conventions**

Add a single bullet:

```markdown
- **Deregistering a Rust builtin:** audit AIRL replacement for signature parity. See `AIRL-Header.md` § "Deregistering a Rust builtin" and the tracked audit at `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`.
```

---

## Task 6: Full regression + commit

- [ ] **Step 1: Rust suite**

```
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: all pass.

- [ ] **Step 2: AOT suite**

```
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh 2>&1 | tail -5
```

Expected: all pass. If you added new fixtures in Task 4, test count may have increased.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md \
        AIRL-Header.md CLAUDE.md \
        stdlib/  # only if Task 4 edited any stdlib file \
        tests/   # only if Task 4 added any new fixture

git commit -m "$(cat <<'EOF'
docs(audits): catalog deregistered Rust builtins + AIRL parity

Audits every Rust builtin marked "deregistered — AIRL stdlib
equivalent takes over" (~N items across bytecode_vm.rs and
bytecode_aot.rs) and compares signatures to the AIRL replacement.

Findings: X parity, Y drift fixed in this PR, Z intentional.

Convention section added to AIRL-Header.md + CLAUDE.md so future
deregistrations run this audit and update the tracked document.

(Replace N/X/Y/Z with actual counts from Task 3 summary.)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Fill in the real numbers before committing; do NOT commit with "N/X/Y/Z" placeholders.

---

## Self-Review

**Spec coverage:**
- Catalog every deregistered builtin → Task 1 + Task 2
- Fix drift → Task 4
- Audit document → Task 3
- Convention docs → Task 5
- Tests pass → Task 6

**Placeholder scan:** The commit message has `N/X/Y/Z` placeholders with explicit instruction to substitute. No other placeholders.

**Scope:** Audit + small-fix PR. No refactoring, no new features, no re-registrations.

**Risks called out:** Unreachable deregistered functions (neither auto-included nor imported) flagged in Notes column but not fixed — those are a separate class of bug worth raising but not expanding scope for.
