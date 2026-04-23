# Z3 Strict Enforcement Policy — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Make `:verify proven` the project-wide default; enforce public-API contract coverage in proven modules; track grandfathered exceptions in a committed baseline file that can only be ratcheted down.

## Background

The Z3 verification infrastructure is complete as of commit `4bc02ca`:

- `VerifyLevel::{Checked, Proven, Trusted}` is parsed on modules and threaded through `z3_verify_tops` in `crates/airl-driver/src/pipeline.rs:208`.
- `ContractDisproven` and `ContractUnprovable` are hard errors (`pipeline.rs:284`, `:291`).
- `ProofCache` elides runtime assertion opcodes for Proven clauses.
- `DiskCache` persists Z3 results across compilations (`.airl-z3-cache`).
- `--strict` / `AIRL_STRICT_VERIFY` promotes every `VerifyLevel` to `Proven`.

What is missing is **policy**: a project-level default, enforcement of contract *coverage* (not just provability of the contracts that exist), and a governance mechanism that keeps strict mode from silently decaying. Today the parser default is `VerifyLevel::Checked` (`ast.rs:63-65`), which means authors must opt *in* to strictness per module. This spec inverts the burden.

Related prior specs (all implemented):
- `2026-04-07-z3-enforcement-design.md` — informational → load-bearing Z3
- `2026-04-14-z3-phase2-enforcement-design.md` — hard error on Disproven + opcode elision
- `2026-04-14-verify-level-enforcement-design.md` — `:verify` per-module enforcement
- `2026-04-14-z3-proof-caching-design.md` — DiskCache

## Goals

1. Flip the parser default: unannotated modules and top-level `defn`s are `VerifyLevel::Proven`.
2. Enforce coverage: in a `:verify proven` context, every `:pub defn` must have at least one `:ensures` clause.
3. Grandfather existing code via a committed `.airl-verify-baseline.toml` listing every module currently at `:verify checked` or `:verify trusted`.
4. Provide `airl verify-policy`, a subcommand that is the sole enforcement point for the baseline invariant.
5. Provide `airl verify-policy --init`, a one-shot migration tool that writes grandfather annotations and populates the baseline.

## Non-Goals

- Any change to Z3, the solver, the ProofCache, the DiskCache, or runtime assertion behavior.
- Audit/justification annotations on `:verify trusted` (separate future spec).
- Coverage metrics dashboards or percentage tracking.
- Changes to the G3 bootstrap compiler's contract semantics. Bootstrap modules remain at `:verify checked` indefinitely.
- Enforcement during regular `airl run` / `airl check` / `airl compile`. Those commands' behavior is unchanged except where the parser default flip trivially changes semantics for unannotated modules.

## Architecture

```
┌──────────────────┐    ┌───────────────────────┐    ┌───────────────────┐
│ airl-syntax      │    │ airl-driver           │    │ .airl-verify-     │
│ parser default:  │──▶ │ z3_verify_tops:       │◀───│ baseline.toml     │
│ Proven (was      │    │  + coverage check     │    │ (grandfather      │
│ Checked)         │    │                       │    │  module list)     │
│                  │    │ verify_policy cmd:    │    │                   │
│ :verify allowed  │    │  diff tree vs         │    │                   │
│ on defn too      │    │  baseline → Err       │    │                   │
└──────────────────┘    └───────────────────────┘    └───────────────────┘
```

Three crates touched:

| Crate | Change |
|-------|--------|
| `airl-syntax` | Default flip; `:verify` on `FnDef`. |
| `airl-driver` | Coverage rule in `z3_verify_tops`; new `verify-policy` subcommand; baseline file reader/writer. |
| (no others)   | Solver, runtime, codegen, agent, rt, contracts — untouched. |

## Components

### 1. Parser changes (`crates/airl-syntax/`)

**`src/ast.rs`:**

```rust
impl Default for VerifyLevel {
    fn default() -> Self { VerifyLevel::Proven }  // was Checked
}

pub struct FnDef {
    // ... existing fields ...
    pub verify: Option<VerifyLevel>,  // new; None = inherit from module/default
}
```

**`src/parser.rs`:**

Accept `:verify <level>` on `defn` forms, using the existing `parse_verify_level` helper (`parser.rs:1613`). Resolution precedence (highest wins):

1. `FnDef.verify: Some(level)` — per-function override.
2. Enclosing `ModuleDef.verify` — module annotation.
3. `VerifyLevel::default()` — now `Proven`.

### 2. Coverage rule (`crates/airl-driver/src/pipeline.rs`)

Inside `z3_verify_tops` (line 252 loop), before the existing `match level { ... }`:

```rust
if *level == airl_syntax::ast::VerifyLevel::Proven
    && f.is_pub
    && f.ensures.is_empty()
{
    return Err(PipelineError::ContractCoverageMissing {
        fn_name: f.name.clone(),
        module: module_path_for_fn(&f.name, tops),
    });
}
```

`PipelineError::ContractCoverageMissing { fn_name, module }` is a new variant in `pipeline.rs:1418`-region with a `Display` impl:

```
error: public function `{fn_name}` in module `{module}` has no :ensures clause
  note: `:verify proven` modules require every :pub defn to have at least one :ensures
  help: add an :ensures clause, or mark the module or function :verify checked
```

**Scope of the check:**
- Only triggers for `VerifyLevel::Proven`. Checked and Trusted are exempt.
- Only triggers for `:pub` functions. Private helpers and test fixtures are exempt.
- `f.ensures.is_empty()` is literal — a clause like `[(= result result)]` satisfies coverage even if vacuous. The strictness escalation is social, not syntactic. Authors can add more teeth in a later spec.

### 3. Baseline file (`.airl-verify-baseline.toml`)

Lives at repo root, tracked by git. Format:

```toml
# Managed by `airl verify-policy`. Hand-edits are allowed; CI validates
# consistency on every run.
version = 1

# Modules at :verify checked. Paths are repo-relative. Remove an entry
# to ratchet — requires upgrading the module's :verify to proven and
# adding :ensures to every :pub defn in that module in the same PR.
grandfathered_checked = [
  "bootstrap/lexer.airl",
  "stdlib/list.airl",
  # ...
]

# Modules at :verify trusted. Listed separately because the trusted
# escape hatch is more dangerous than checked and deserves explicit
# audit pressure. Kept small intentionally.
grandfathered_trusted = [
  "crates/airl-rt/src/thread_primitives.airl",
  # ...
]
```

**Semantics:**

| Condition | Result |
|-----------|--------|
| Module is at `:verify checked` AND path is in `grandfathered_checked` | OK |
| Module is at `:verify checked` AND path is NOT in `grandfathered_checked` | Regression — `verify-policy` fails |
| Module is at `:verify trusted` AND path is in `grandfathered_trusted` | OK |
| Module is at `:verify trusted` AND path is NOT in `grandfathered_trusted` | Regression — `verify-policy` fails |
| Path is in `grandfathered_checked` but module is now `:verify proven` (upgraded) | OK — stale entry tolerated; `--prune` removes it |
| Path is in `grandfathered_checked` but file no longer exists | Stale — `--prune` removes it |

Mixed-file case: a file contains multiple modules or bare top-level defns. The baseline key is the file path, optionally with a `#<name>` suffix to disambiguate (e.g., `bootstrap/lexer.airl#lexer`). See Section 6 for the full key grammar and how `--init` chooses plain-path vs. `#`-qualified keys.

**Scope of the scan:** `.airl` files tracked in git, excluding `tests/fixtures/**`. Fixtures deliberately exercise contract edge cases (including disproven and uncovered contracts) and would generate noise. The exclusion is a fixed glob, not configurable — if a fixture needs to be policy-enforced, it doesn't belong in fixtures.

### 4. `airl verify-policy` subcommand (`crates/airl-driver/src/main.rs`)

| Invocation | Behavior |
|------------|----------|
| `airl verify-policy` or `airl verify-policy --check` | Scan tracked `.airl` files. For each, parse and extract `VerifyLevel` per module / per top-level defn. Compare against baseline. Print human-readable diff. Exit 0 if clean, 1 on regression. No Z3 runs. |
| `airl verify-policy --init` | One-shot migration. See Section 7. |
| `airl verify-policy --prune` | Remove entries from baseline where the corresponding module has been upgraded or the file deleted. Writes the baseline file. Exit 0. |
| `airl verify-policy --list-uncovered` | Print every `:pub defn` in `:verify proven` modules that is missing `:ensures`. Read-only; useful in ratcheting PRs. |

**Parsing cost:** each `.airl` file is parsed once using the existing `airl-syntax` parser. For large repos this is O(seconds) but runs in CI only, not on every build.

### 5. `--strict` / `AIRL_STRICT_VERIFY` semantics

Unchanged. The existing flag (`main.rs:77-80`, `pipeline.rs:229`) still promotes every `VerifyLevel` to `Proven`, including grandfathered `:verify checked` modules. With the coverage rule now active in the `Proven` branch, `--strict` also activates coverage globally. This makes it a useful diagnostic: "show me every `:pub defn` that would need an `:ensures` if we deleted the entire baseline."

No renaming, no semantic change, no CLI churn.

### 6. Migration tool (`airl verify-policy --init`)

One-shot, idempotent. Run this once when the parser default flip lands; never again in normal operation.

**Algorithm:**

```
for each tracked .airl file (excluding tests/fixtures/**):
    parse the file
    for each ModuleDef m:
        match m.verify:
          explicit Checked  -> add path to baseline.grandfathered_checked
          explicit Trusted  -> add path to baseline.grandfathered_trusted
          explicit Proven   -> leave alone, no baseline entry
          implicit (default)-> rewrite header to :verify checked,
                               add path to baseline.grandfathered_checked
    for each top-level FnDef f (not inside a module):
        match f.verify:
          explicit Checked  -> add <path>#<fn_name> to grandfathered_checked
          explicit Trusted  -> add <path>#<fn_name> to grandfathered_trusted
          explicit Proven   -> leave alone, no baseline entry
          implicit (default)-> append :verify checked to the defn,
                               add <path>#<fn_name> to grandfathered_checked

write .airl-verify-baseline.toml (sorted, deduplicated)
```

**Baseline key grammar:** a line in `grandfathered_checked` or `grandfathered_trusted` is either:
- A plain path (`crates/airl-rt/src/thread.airl`) — refers to *all* modules in the file.
- A path with `#<name>` suffix (`bootstrap/lexer.airl#lexer` or `bootstrap/lexer.airl#main`) — refers to a specific module or top-level defn by name. `#` is used instead of `::` to avoid confusion with AIRL's module-qualified identifiers.

The `--init` tool emits plain paths when the whole file is grandfathered (the common case) and `#`-qualified keys only when the file mixes grandfathered and non-grandfathered items.

**Source rewriting:** performed by a minimal textual pass that preserves formatting — insert `:verify checked` after the module/defn name and before other keyword fields. The pass uses the parser's span information to locate the insertion point; no reformatting of the rest of the file.

**Idempotency:** a module that already has `:verify` of any flavor is skipped, so re-running `--init` is a no-op (and useful as a recovery step).

**Order of operations in the PR that ships this spec:**
1. Add per-defn `:verify` to `FnDef` and the parser. Merge.
2. Add `PipelineError::ContractCoverageMissing` and the coverage check, gated behind an env var (e.g., `AIRL_COVERAGE_ENFORCE`) during development so it can be exercised without breaking the tree.
3. Add `airl verify-policy` subcommand scaffolding.
4. In a single PR with two ordered commits:
   - **Commit A (migration):** Run `airl verify-policy --init` on the live tree. Commit the mechanical source rewrite plus the initial `.airl-verify-baseline.toml`. At this commit the parser default is still `Checked`, so the tree is semantically unchanged — the `:verify checked` annotations merely make the implicit default explicit.
   - **Commit B (default flip):** Change `impl Default for VerifyLevel` to `Proven` and remove the dev-gate on the coverage check. `cargo test` and `scripts/build-g3.sh` must pass at this commit because every existing module and top-level defn carries an explicit `:verify checked` from commit A.
5. Add `airl verify-policy` to CI in the same PR.

Commit A is large but mechanical. A reviewer should be able to verify it by diffing the textual rewrite and spot-checking a handful of module headers. Commit B is small and semantically load-bearing.

## Data flow

```
Author writes module              Parser assigns VerifyLevel
       │                                   │
       ▼                                   ▼
  (no annotation)                  default = Proven
       │                                   │
       │                                   ▼
       │                    z3_verify_tops (compile time)
       │                          │
       │                          ▼
       │                   coverage gate: pub fn must have :ensures
       │                          │
       │                          ▼
       │                   Z3 verification (existing flow)
       │
       ▼
  airl verify-policy (CI)
       │
       ▼
  scans tree, compares to .airl-verify-baseline.toml
       │
       ├─ match → exit 0
       └─ mismatch → exit 1 with diff
```

Two enforcement points, two failure modes, one baseline file.

## Error handling

**New `PipelineError` variant:**

```rust
ContractCoverageMissing {
    fn_name: String,
    module: String,
}
```

Added to the `PipelineError` enum in `pipeline.rs:1418`-region, with a `Display` impl that points to the specific function and suggests the three fixes (add `:ensures`, mark module `:verify checked`, mark function `:verify checked`).

**`verify-policy` exit codes:**

- `0` — baseline and tree agree.
- `1` — regression detected; diff printed.
- `2` — malformed baseline file or unparseable `.airl` file; actionable error printed.

**Baseline file parse errors:** fatal. The CLI refuses to proceed with a corrupt baseline rather than silently regenerating. `--init` overwrites, so recovery is an explicit operator action.

## Testing

Three layers.

### Unit tests

- `airl-syntax/src/parser.rs`: `:verify` on `defn` parses correctly at all precedence levels.
- `airl-syntax/src/ast.rs`: `VerifyLevel::default() == Proven` (update the existing test at `ast.rs:487`).
- `airl-driver/src/pipeline.rs`: coverage gate triggers for `:pub defn` with no `:ensures` in Proven modules; does not trigger for private defns; does not trigger under `:verify checked`.

### Fixture tests

- `tests/fixtures/valid/no_verify_annotation.airl` — module with no `:verify`, provable `:ensures` on its pub defn. Passes under new default.
- `tests/fixtures/contract_errors/pub_fn_no_ensures.airl` — Proven module, `:pub defn` without `:ensures`. Expect `ContractCoverageMissing`.
- `tests/fixtures/valid/per_defn_verify_override.airl` — Proven module with one `(:verify checked :pub defn foo ...)` and no `:ensures`. Passes (per-defn override).
- `tests/fixtures/valid/grandfathered_checked.airl` — explicit `:verify checked` module with no contracts; passes.

### Integration test

New file: `crates/airl-driver/tests/verify_policy.rs`. Uses a temp dir with a small fixture tree (two modules: one Proven, one grandfathered Checked, one unlisted Checked):

1. Run `verify-policy --init` on the fixture tree; assert baseline file is written and contains exactly the expected entries.
2. Run `verify-policy` on the clean tree; assert exit 0.
3. Add a new unlisted `:verify checked` module; run `verify-policy`; assert exit 1 with a diff mentioning the new file.
4. Upgrade a grandfathered module to `:verify proven` (and add `:ensures`); run `verify-policy --prune`; assert the baseline is updated.

### Regression

- Full `cargo test` after running `--init` on the live tree. Any failure is a migration bug.
- `bash scripts/build-g3.sh` — G3 must still build. Bootstrap modules should all end up in `grandfathered_checked`.
- `bash tests/aot/run_aot_tests.sh` — AOT test suite; the existing `;;Z3-PROVEN:` annotations must continue to elide opcodes.

## Files modified

| File | Change |
|------|--------|
| `crates/airl-syntax/src/ast.rs` | Flip `VerifyLevel::default()` to `Proven`; add `FnDef.verify: Option<VerifyLevel>`; update default test. |
| `crates/airl-syntax/src/parser.rs` | Accept `:verify` on `defn`. |
| `crates/airl-driver/src/pipeline.rs` | Add `ContractCoverageMissing` variant and coverage gate in `z3_verify_tops`; update `verify_level_for_fn` to honor per-defn override. |
| `crates/airl-driver/src/main.rs` | Add `verify-policy` subcommand dispatch. |
| `crates/airl-driver/src/verify_policy.rs` (new) | Baseline file reader/writer, tree scanner, `--init` / `--prune` / `--list-uncovered` implementations. |
| `.airl-verify-baseline.toml` (new, committed) | Initial grandfather list produced by `--init`. |
| `.github/workflows/ci.yml` | New step: `cargo run -- verify-policy`. |
| `tests/fixtures/{valid,contract_errors}/` | New fixtures per Testing section. |
| `crates/airl-driver/tests/verify_policy.rs` (new) | Integration tests. |
| `CLAUDE.md`, `AIRL-Header.md`, `AIRL-LLM-Guide.md` | Document the default flip and `:verify` precedence. |

## Invariants preserved

- Z3 verification flow is untouched. `ContractDisproven`, `ContractUnprovable`, `ProofCache`, `DiskCache` all behave identically.
- `Unknown` in `:verify checked` still falls back to runtime assertions. No new runtime work.
- `--strict` / `AIRL_STRICT_VERIFY` still does what it does today; coverage rule rides along because it's gated on effective `VerifyLevel::Proven`.
- Fixture semantics unchanged — `tests/fixtures/**` is excluded from the policy scan.
- G3 bootstrap builds. Bootstrap paths are grandfathered and stay at `:verify checked`.

## Out of scope / future work

- Justification annotations on `:verify trusted` (e.g., `:verify trusted :reason "TLA+-verified primitive"`).
- Coverage metrics / dashboards / per-module scorecards.
- Stricter coverage rules (non-trivial `:ensures`, banned tautologies).
- Per-directory policy overrides (`airl-policy.toml` with glob rules).
- Machine-readable `verify-policy --json` output for editor plugins.
- Automatic PR comments from CI showing the baseline diff.

Each of these is a clean follow-on spec if and when needed.
