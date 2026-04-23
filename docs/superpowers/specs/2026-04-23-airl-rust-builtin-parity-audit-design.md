# AIRL/Rust Builtin Deregistration Parity Audit — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Audit every Rust builtin that has been marked "deregistered — AIRL stdlib equivalent takes over" and verify that the AIRL replacement matches the old Rust signature (especially return type). Produce a tracked findings document; fix any signature drift discovered; add a convention note to AIRL-Header.md that prevents recurrence.

## Background

The `stdlib/json.airl` autoinclude fix (`2026-04-23-stdlib-json-autoinclude-fix.md`) surfaced a latent bug: `json-parse` in `stdlib/json.airl` returned raw values but the test fixture and the former Rust builtin returned `Result` (`Ok v` / `Err e`). Because `json.airl` was not auto-included, the raw-value behavior was never linked and never exercised — the bug was invisible until the linker fix made the AIRL implementation reachable.

This is a class of silent drift: when a Rust builtin is deregistered in favor of an AIRL replacement, **nothing in the codebase checks that the two agree**. The deregistration is performed by commenting out the map insertion in `bytecode_aot.rs` / `bytecode_vm.rs`; the AIRL replacement is just a `defn` somewhere in `stdlib/*.airl`. No test, type check, or CI step compares signatures.

Grepping for `deregistered — AIRL stdlib equivalents` turns up **~15** such cases across two files:

| File | Line | Notes |
|------|------|-------|
| `bytecode_vm.rs` | 647 | prelude.airl |
| `bytecode_vm.rs` | 656 | string.airl |
| `bytecode_vm.rs` | 662 | string.airl |
| `bytecode_vm.rs` | 678 | read-line, read-stdin → io.airl |
| `bytecode_vm.rs` | 692 | map.airl |
| `bytecode_vm.rs` | 695 | read-file, get-args, getenv, exit |
| `bytecode_vm.rs` | 743 | **json.airl** — already audited via this spec's motivating fix |
| `bytecode_vm.rs` | 767 | path.airl |
| `bytecode_vm.rs` | 779 | base64.airl |
| `bytecode_aot.rs` | 1008 | string.airl |
| `bytecode_aot.rs` | 1020 | map.airl |
| `bytecode_aot.rs` | 1085 | json.airl |
| `bytecode_aot.rs` | 1103 | prelude.airl |
| `bytecode_aot.rs` | 1105 | path.airl |
| `bytecode_aot.rs` | 1117 | generic "AIRL stdlib equivalents take over" |

Each is an opportunity for the same class of bug. Many of these overlap (base64, prelude, string, map appear in both files), but each deregistration needs auditing once.

## Goals

1. **Catalog:** Produce a tracked markdown document listing every `(rust_builtin_name, rust_signature, airl_replacement_name, airl_signature, parity_status)` tuple covering every deregistered builtin.
2. **Fix drift:** Any row where `rust_signature` and `airl_signature` don't match (e.g. `Result<T>` vs `T`) becomes a bug to fix in the same PR — either by updating the AIRL implementation to match the deregistered Rust signature, or by explicitly documenting why the replacement is intentionally different and adding that caveat to the commit.
3. **Prevent recurrence:** Add a paragraph to `AIRL-Header.md` (and possibly `CLAUDE.md`) spelling out the deregistration-parity convention, so future deregistrations must run this audit and update the tracked document.

## Non-Goals

- Writing an automated parity checker. Manual audit is sufficient for the current ~15-item list; a tool would take more effort to build than the audit saves.
- Deregistering any additional Rust builtins.
- Changing the deregistration mechanism itself.
- Re-registering any Rust builtins. If the audit finds a deregistration was premature (e.g. the AIRL replacement is significantly slower and users depend on the Rust behavior), filing a separate issue is the right response — not re-registering in this spec's PR.
- Auditing non-deregistered builtins. Only builtins explicitly marked `// deregistered — ...` are in scope.

## Architecture

### The audit document

A new file at `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` with a single table:

| # | Rust builtin | Rust signature (pre-dereg) | AIRL replacement | AIRL signature | Parity | Notes |
|---|--------------|----------------------------|------------------|----------------|--------|-------|
| 1 | `json-parse` | `fn(*mut RtValue) -> *mut RtValue` returning `Result<Value,String>` variant | `stdlib/json.airl: json-parse` | `[-> Result<_>]` wrapping `Ok v` / `Err e` | ✅ Parity (fixed 2026-04-23 in commit `8fd4a23`) | Was the motivating bug |
| 2 | `json-stringify` | ... | ... | ... | ... | ... |
| ... | ... | ... | ... | ... | ... | ... |

Each row is filled by:
1. Looking up the deregistered Rust function's signature in `airl-rt` or `airl-runtime` source (the function may still exist — just unregistered from the builtin map).
2. Locating the corresponding AIRL function in `stdlib/*.airl`.
3. Comparing. Status values:
   - **✅ Parity** — signatures match, no action needed.
   - **❌ Drift** — signatures differ in a way that would break callers relying on the old Rust behavior. File a fix in the same PR.
   - **⚠️ Intentional** — signatures differ, but the difference is a deliberate improvement (e.g. AIRL version uses better error types). Document the rationale in the Notes column.

### Fix scope

For every **❌ Drift** row, either:
- Amend the AIRL implementation in `stdlib/*.airl` to match the pre-deregistration Rust signature, OR
- Mark the row **⚠️ Intentional** with a written rationale and update any downstream callers (tests, other stdlib files, documentation) to match the new AIRL contract.

A single PR carries the audit document + every drift fix. Tests must pass after all changes.

### Convention documentation

Add to `AIRL-Header.md`:

```markdown
## Deregistering a Rust builtin

When replacing a Rust `extern "C"` builtin with an AIRL stdlib implementation:

1. **Before commenting out the Rust registration,** confirm the AIRL implementation has the same visible signature as the Rust one — parameter count, parameter types, and especially return type (raw value vs `Result`).
2. **Update** `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` with the new row.
3. **Test** any code path that previously reached the Rust builtin — the AIRL replacement must pass the same tests.

Deregistrations without matching AIRL signatures cause silent latent bugs: if the stdlib module is not yet auto-included (see pipeline.rs:STDLIB_MODULES), the drift is invisible until the module becomes reachable. See `2026-04-23-stdlib-json-autoinclude-fix.md` for a concrete instance.
```

And a one-line pointer in `CLAUDE.md` under Conventions:

```markdown
- **Deregistering a Rust builtin:** audit AIRL replacement parity; see AIRL-Header.md § "Deregistering a Rust builtin" and the tracked audit at `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`.
```

## Components

### 1. Audit document (new file)

`docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` — markdown with a single table covering all ~15 deregistered builtins.

The document's header includes:
- Date of audit (2026-04-23)
- Audit methodology (grep commands used, signature-comparison criteria)
- Summary count (e.g. "15 deregistrations audited, 14 parity, 1 drift fixed in this PR")
- Table

Living document: future deregistrations add rows.

### 2. Signature fixes (zero or more edits to `stdlib/*.airl`)

Count unknown in advance. The json-parse Result-wrapping was already caught; there may be zero additional drifts, or several. The spec doesn't prescribe a target because it's an audit, not a fix quota.

### 3. Documentation updates

- `AIRL-Header.md`: new "Deregistering a Rust builtin" section.
- `CLAUDE.md`: one-line pointer under Conventions.

## Execution plan (for the implementer)

The audit is conceptually simple but needs careful execution. Steps:

1. **Enumerate** every `// deregistered` comment via `grep -rn "deregistered" crates/airl-rt/src crates/airl-runtime/src`. Deduplicate by (function name) — a builtin appearing in both `bytecode_vm.rs` and `bytecode_aot.rs` is one audit row, not two.

2. **For each** builtin in the deduped list:
   a. Find the Rust function definition. Grep `crates/airl-rt/src -rn "fn airl_<name>\|pub extern \"C\" fn airl_<name>"`. The function may still exist (just unregistered) — inspect its signature. If the Rust function has been deleted, git log / git blame shows what it used to return.
   b. Find the AIRL function definition. Grep `stdlib/ -rn "defn <name>"`. The file should match what the deregistration comment says (e.g. "json.airl take over" → `stdlib/json.airl`).
   c. Compare signatures element by element:
      - Param count and types (AIRL has fewer type distinctions — don't over-compare).
      - **Return type**: critical. Rust `Result<T,E>` typically maps to AIRL `(match ... (Ok x) ... (Err e) ...)`. Raw Rust `T` maps to raw AIRL `T`. Mismatch is drift.
      - Error behavior: does the Rust version panic? Return Err? The AIRL version should match.
   d. Record the row in the audit document.

3. **For each drift row:**
   a. Decide: fix AIRL to match Rust (default), OR mark as intentional with a rationale.
   b. If fixing: edit `stdlib/<module>.airl` accordingly. Run `cargo test` + the full AOT suite to catch regressions.
   c. If intentional: write the rationale in the Notes column.

4. **Finalize** the audit document's summary section with counts.

5. **Update docs** (`AIRL-Header.md`, `CLAUDE.md`) with the deregistration convention.

6. **Commit** with message that says "X deregistrations audited, Y drifts fixed".

## Files modified

| File | Change |
|------|--------|
| `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` | Created (table of ~15 rows + summary). |
| `stdlib/*.airl` | Zero or more signature-parity fixes, depending on what the audit finds. |
| `AIRL-Header.md` | Add "Deregistering a Rust builtin" convention section. |
| `CLAUDE.md` | Add one-line pointer under Conventions. |

## Testing

- **Rust test suite:** any AIRL fixes must not break `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`.
- **AOT suite:** must remain 68/68 (or 68/69, 68/70, etc. if new test fixtures are added to pin the audit's findings).
- **New fixtures:** for each fix made, add a fixture test under `tests/aot/round*.airl` that calls the corrected AIRL function and asserts the expected (Rust-compatible) behavior. One fixture per signature-fix.
- **Verification smoke:** for each audit row marked **✅ Parity**, spot-check that the AIRL function is actually reachable (either auto-included via `STDLIB_MODULES` or via explicit `(import "...")` in a representative test).

## Risks

- **Audit incompleteness.** The grep may miss deregistered builtins whose comments don't contain the exact phrase "deregistered". Mitigation: second grep for `replaced by`, `AIRL stdlib equivalent`, `no longer register`, etc.
- **Signature comparison ambiguity.** AIRL's type system has less granularity than Rust's. Mitigation: only flag drift when the observable behavior differs (return-type shape, error vs success path, panic vs Err). Don't flag cosmetic differences (e.g. `(x : String)` vs `(x : string)`).
- **Audit churn.** Future deregistrations may not update the audit document, leaving it stale. Mitigation: the AIRL-Header.md convention section explicitly tells the author to update the audit. Beyond that, a lint (e.g. a CI script that greps for new `deregistered` comments and checks the audit file was also touched in the same PR) is feasible future work.
- **Breaking change risk.** Fixing an AIRL function's signature changes behavior for any already-in-use callers. Mitigation: run the full test suite after every fix; if tests break, the drift fix itself is a breaking change and needs coordination (update callers in the same PR).

## Out of Scope / Future Work

- **Tooling.** A CI script that flags new deregistrations without audit updates is useful but not in this spec.
- **Automated signature extraction.** A tool that parses both Rust `fn` sigs and AIRL `:sig`s and compares them programmatically would make this audit continuous instead of one-shot. Design in a separate spec if the manual audit becomes frequent.
- **Proactive deregistration of more builtins.** The stdlib could replace more Rust builtins with pure AIRL; that's a separate modernization effort.

## Invariants Preserved

- No public AIRL function is removed.
- No Rust builtin is registered OR deregistered — status quo on that dimension.
- The audit is a documentation + small-fix PR. It does not restructure the stdlib, the driver, or the runtime.
- Test coverage only grows.
