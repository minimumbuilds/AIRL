# Z3 Strict Enforcement Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flip AIRL's module verification default from `Checked` to `Proven`, enforce public-API contract coverage in proven modules, and ship a committed baseline file + `airl verify-policy` subcommand that tracks grandfathered exceptions and prevents regressions.

**Architecture:** Parser change (add per-defn `:verify`, flip default), compile-time coverage gate inside `z3_verify_tops`, new `airl-driver/src/verify_policy.rs` module with hand-rolled TOML reader/writer, new `verify-policy` subcommand. No changes to the solver, runtime, or codegen. Migration lands in two ordered commits: Commit A (mechanical grandfather annotation + baseline file), Commit B (default flip).

**Tech Stack:** Rust (airl-syntax, airl-driver crates), zero new external dependencies (hand-rolled TOML), existing `tests/fixtures/` harness + new `verify_policy.rs` integration test.

**Spec:** `docs/superpowers/specs/2026-04-23-z3-strict-enforcement-policy-design.md`

---

## Phase 0 — Worktree setup

### Task 0.1: Create implementation worktree

**Why:** Per project convention, all work happens in worktrees; main accepts merges only.

- [ ] **Step 1: Create the worktree**

```bash
git worktree add -b strict-enforcement-policy ../AIRL-strict-enforcement main
cd ../AIRL-strict-enforcement
```

- [ ] **Step 2: Verify clean state**

```bash
git status
git log --oneline -3
```

Expected: `On branch strict-enforcement-policy`, clean, recent commits from main visible.

- [ ] **Step 3: Install git hooks in the worktree**

```bash
bash scripts/git-hooks/install.sh
```

Expected: "Hooks installed" or similar. Idempotent.

---

## Phase 1 — Parser: per-defn `:verify`

### Task 1.1: Add `FnDef.verify: Option<VerifyLevel>` field

**Files:**
- Modify: `crates/airl-syntax/src/ast.rs:78-93` (FnDef struct)
- Modify: `crates/airl-syntax/src/ast.rs:487-501` (existing default tests — extend)

- [ ] **Step 1: Write the failing test**

Add to `crates/airl-syntax/src/ast.rs` test module (near the existing `fn_def_has_is_public` test around line 514):

```rust
#[test]
fn fn_def_has_verify_override() {
    use crate::span::Span;
    let f = FnDef {
        name: "foo".to_string(),
        params: vec![],
        return_type: AstType { kind: AstTypeKind::Named("i64".into()), span: Span::default() },
        intent: None,
        requires: vec![],
        ensures: vec![],
        invariants: vec![],
        is_pure: false,
        is_total: false,
        body: Expr { kind: ExprKind::NilLit, span: Span::default() },
        execute_on: None,
        priority: None,
        is_public: false,
        verify: Some(VerifyLevel::Checked),
        span: Span::default(),
    };
    assert_eq!(f.verify, Some(VerifyLevel::Checked));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-syntax fn_def_has_verify_override`
Expected: FAIL with "no field `verify` on type `FnDef`".

- [ ] **Step 3: Add the field**

Edit `crates/airl-syntax/src/ast.rs` — add after `is_public: bool,` (line 91):

```rust
    pub is_public: bool,
    /// Per-function override of the enclosing module's :verify level.
    /// `None` means inherit from module (or the parser default).
    pub verify: Option<VerifyLevel>,
    pub span: Span,
```

- [ ] **Step 4: Fix all FnDef construction sites**

Compile errors will list every site constructing `FnDef`. Add `verify: None,` to each. Search:

```bash
grep -rn "FnDef {" crates/airl-syntax/src/ crates/airl-syntax/tests/ 2>/dev/null
```

The main construction site is `parser.rs:962` (inside `parse_defn`). Add `verify: None,` before `span,`. Other construction sites are test helpers — add `verify: None,` to each.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p airl-syntax fn_def_has_verify_override`
Expected: PASS.

- [ ] **Step 6: Full airl-syntax tests still pass**

Run: `cargo test -p airl-syntax`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/airl-syntax/src/ast.rs crates/airl-syntax/src/parser.rs
git commit -m "$(cat <<'EOF'
feat(syntax): add FnDef.verify per-function :verify override field

Required for the strict enforcement policy spec
(docs/superpowers/specs/2026-04-23-z3-strict-enforcement-policy-design.md).
Field is None by default; parser wiring follows in the next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 1.2: Parse `:verify` on defn

**Files:**
- Modify: `crates/airl-syntax/src/parser.rs:807-978` (parse_defn)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `parser.rs` (near existing `parse_defn_public` around line 1905):

```rust
#[test]
fn parse_defn_with_verify_override() {
    let src = r#"
      (defn foo
        :verify checked
        :sig [(x : i64) -> i64]
        :requires [(>= x 0)]
        :body x)
    "#;
    let parsed = parse(src).expect("parse failed");
    match &parsed.tops[0] {
        TopLevel::Defn(f) => assert_eq!(f.verify, Some(VerifyLevel::Checked)),
        _ => panic!("expected Defn"),
    }
}

#[test]
fn parse_defn_without_verify_is_none() {
    let src = r#"
      (defn bar
        :sig [(x : i64) -> i64]
        :requires [(>= x 0)]
        :body x)
    "#;
    let parsed = parse(src).expect("parse failed");
    match &parsed.tops[0] {
        TopLevel::Defn(f) => assert_eq!(f.verify, None),
        _ => panic!("expected Defn"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-syntax parse_defn_with_verify_override parse_defn_without_verify_is_none`
Expected: FAIL — `parse_defn_with_verify_override` fails because `:verify` is unknown; `parse_defn_without_verify_is_none` fails because field doesn't yet exist in parser construction.

- [ ] **Step 3: Add verify parsing in parse_defn**

Edit `crates/airl-syntax/src/parser.rs` inside the `while i < items.len()` keyword-match loop in `parse_defn` (around line 837-925). Add a local `let mut verify: Option<VerifyLevel> = None;` declaration alongside the other `let mut` declarations (around line 834, after `priority`):

```rust
    let mut priority = None;
    let mut verify: Option<VerifyLevel> = None;
```

Then add the keyword arm in the match block (insert before the `_ =>` catchall at line 919):

```rust
                "verify" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected verify level after :verify", span));
                    }
                    verify = Some(parse_verify_level(&items[i])?);
                }
```

Then add `verify,` to the `FnDef { ... }` literal at line 962-977 (replacing the `verify: None,` placeholder from Task 1.1):

```rust
    Ok(FnDef {
        name,
        // ...
        is_public,
        verify,
        span,
    })
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p airl-syntax parse_defn_with_verify_override parse_defn_without_verify_is_none`
Expected: PASS.

- [ ] **Step 5: Full airl-syntax tests still pass**

Run: `cargo test -p airl-syntax`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-syntax/src/parser.rs
git commit -m "$(cat <<'EOF'
feat(syntax): parse :verify keyword on defn

Per-function override of the enclosing module's :verify level.
Consumed by z3_verify_tops in a later commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2 — Coverage rule + error variant

### Task 2.1: Add `ContractCoverageMissing` error variant

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs:1418`-region (PipelineError enum), `:1448`-region (Display impl)

- [ ] **Step 1: Write the failing test**

Add to `crates/airl-driver/src/pipeline.rs` test module (at the bottom of the file, below existing `strict_verify_overrides_checked_to_proven` test):

```rust
#[test]
fn contract_coverage_missing_display() {
    let e = PipelineError::ContractCoverageMissing {
        fn_name: "foo".to_string(),
        module: "bar.airl".to_string(),
    };
    let s = format!("{}", e);
    assert!(s.contains("foo"), "expected fn name in message: {}", s);
    assert!(s.contains("bar.airl"), "expected module in message: {}", s);
    assert!(s.contains(":ensures") || s.contains("ensures"), "expected hint about :ensures: {}", s);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-driver contract_coverage_missing_display`
Expected: FAIL — `ContractCoverageMissing` not a variant.

- [ ] **Step 3: Add the enum variant**

Edit `crates/airl-driver/src/pipeline.rs`. Find the `PipelineError` enum (around line 1418, near `ContractDisproven`) and add a variant in alphabetical order:

```rust
    ContractCoverageMissing {
        fn_name: String,
        module: String,
    },
    ContractDisproven {
        // ... existing ...
    },
    ContractUnprovable {
        // ... existing ...
    },
```

- [ ] **Step 4: Add the Display match arm**

Find the `impl Display for PipelineError` block (around line 1448) and add, in the same order:

```rust
            PipelineError::ContractCoverageMissing { fn_name, module } => {
                write!(
                    f,
                    "public function `{}` in module `{}` has no :ensures clause\n  \
                     note: `:verify proven` modules require every :pub defn to have at least one :ensures\n  \
                     help: add an :ensures clause, or mark the module or function :verify checked",
                    fn_name, module
                )
            }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p airl-driver contract_coverage_missing_display`
Expected: PASS.

- [ ] **Step 6: Run full driver tests**

Run: `cargo test -p airl-driver`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs
git commit -m "$(cat <<'EOF'
feat(driver): add ContractCoverageMissing error variant

Emitted when a :pub defn in a :verify proven module lacks :ensures.
Coverage gate wiring follows in the next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2.2: Update `verify_level_for_fn` to honor per-defn override

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs:353-366` (verify_level_for_fn)

- [ ] **Step 1: Write the failing test**

Add to `crates/airl-driver/src/pipeline.rs` test module:

```rust
#[test]
fn per_defn_verify_overrides_module_level() {
    // Module is :verify proven, but one defn overrides to :verify checked.
    let src = r#"
      (module test-mod
        :verify proven
        (defn foo
          :verify checked
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
    "#;
    let tops = airl_syntax::parse(src).expect("parse failed").tops;
    let level = verify_level_for_fn("foo", &tops);
    assert_eq!(level, airl_syntax::ast::VerifyLevel::Checked);
}

#[test]
fn defn_without_override_inherits_module_level() {
    let src = r#"
      (module test-mod
        :verify proven
        (defn bar
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
    "#;
    let tops = airl_syntax::parse(src).expect("parse failed").tops;
    let level = verify_level_for_fn("bar", &tops);
    assert_eq!(level, airl_syntax::ast::VerifyLevel::Proven);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-driver per_defn_verify_overrides_module_level defn_without_override_inherits_module_level`
Expected: FAIL — current `verify_level_for_fn` ignores `FnDef.verify`.

- [ ] **Step 3: Update the function**

Replace `verify_level_for_fn` in `pipeline.rs:353-366` with:

```rust
/// Look up the verify level for a function.
/// Precedence: FnDef.verify > enclosing ModuleDef.verify > VerifyLevel::default().
fn verify_level_for_fn(fn_name: &str, tops: &[airl_syntax::ast::TopLevel]) -> airl_syntax::ast::VerifyLevel {
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Module(m) => {
                for item in &m.body {
                    if let airl_syntax::ast::TopLevel::Defn(f) = item {
                        if f.name == fn_name {
                            return f.verify.unwrap_or(m.verify);
                        }
                    }
                }
            }
            airl_syntax::ast::TopLevel::Defn(f) => {
                if f.name == fn_name {
                    return f.verify.unwrap_or_else(airl_syntax::ast::VerifyLevel::default);
                }
            }
            _ => {}
        }
    }
    airl_syntax::ast::VerifyLevel::default()
}
```

- [ ] **Step 4: Update `z3_verify_tops` fn-collection loop**

The loop at `pipeline.rs:232-250` also hard-codes module-level verify for functions inside modules. Update it to honor per-defn override:

Replace lines 232-250 with:

```rust
    // Collect all function definitions, including those inside modules.
    let mut all_fns: Vec<(&airl_syntax::ast::FnDef, airl_syntax::ast::VerifyLevel)> = Vec::new();
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Defn(f) => {
                let mut level = f.verify.unwrap_or_else(airl_syntax::ast::VerifyLevel::default);
                if strict_verify { level = airl_syntax::ast::VerifyLevel::Proven; }
                all_fns.push((f, level));
            }
            airl_syntax::ast::TopLevel::Module(m) => {
                for item in &m.body {
                    if let airl_syntax::ast::TopLevel::Defn(f) = item {
                        let mut level = f.verify.unwrap_or(m.verify);
                        if strict_verify { level = airl_syntax::ast::VerifyLevel::Proven; }
                        all_fns.push((f, level));
                    }
                }
            }
            _ => {}
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p airl-driver per_defn_verify_overrides_module_level defn_without_override_inherits_module_level`
Expected: PASS.

- [ ] **Step 6: Regression — strict_verify test still passes**

Run: `cargo test -p airl-driver strict_verify_overrides_checked_to_proven`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs
git commit -m "$(cat <<'EOF'
feat(driver): honor per-defn :verify override in z3_verify_tops

Precedence: FnDef.verify > ModuleDef.verify > VerifyLevel::default().
Lets a single defn opt out of an enclosing :verify proven module.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2.3: Coverage gate (gated by env var during dev)

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs` inside `z3_verify_tops`

- [ ] **Step 1: Write the failing test**

Add to `crates/airl-driver/src/pipeline.rs` test module:

```rust
#[test]
fn coverage_gate_fires_for_pub_fn_without_ensures() {
    // Only runs when AIRL_COVERAGE_ENFORCE is set.
    std::env::set_var("AIRL_COVERAGE_ENFORCE", "1");
    let src = r#"
      (module mymod
        :verify proven
        (defn :pub foo
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
      (foo 1)
    "#;
    let result = run_source_with_mode(src, PipelineMode::Check);
    std::env::remove_var("AIRL_COVERAGE_ENFORCE");
    match result {
        Err(PipelineError::ContractCoverageMissing { fn_name, .. }) => {
            assert_eq!(fn_name, "foo");
        }
        other => panic!("expected ContractCoverageMissing, got {:?}", other.err()),
    }
}

#[test]
fn coverage_gate_skips_private_fn() {
    std::env::set_var("AIRL_COVERAGE_ENFORCE", "1");
    let src = r#"
      (module mymod
        :verify proven
        (defn foo
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
      (foo 1)
    "#;
    let result = run_source_with_mode(src, PipelineMode::Check);
    std::env::remove_var("AIRL_COVERAGE_ENFORCE");
    // Private defn without :ensures is exempt; should not hit coverage gate.
    // (It may still fail for other reasons; what we care about is NOT ContractCoverageMissing.)
    if let Err(PipelineError::ContractCoverageMissing { .. }) = result {
        panic!("coverage gate fired for private defn");
    }
}

#[test]
fn coverage_gate_skips_checked_module() {
    std::env::set_var("AIRL_COVERAGE_ENFORCE", "1");
    let src = r#"
      (module mymod
        :verify checked
        (defn :pub foo
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
      (foo 1)
    "#;
    let result = run_source_with_mode(src, PipelineMode::Check);
    std::env::remove_var("AIRL_COVERAGE_ENFORCE");
    if let Err(PipelineError::ContractCoverageMissing { .. }) = result {
        panic!("coverage gate fired for :verify checked module");
    }
}

#[test]
fn coverage_gate_disabled_when_env_not_set() {
    // Explicitly ensure env var is unset
    std::env::remove_var("AIRL_COVERAGE_ENFORCE");
    let src = r#"
      (module mymod
        :verify proven
        (defn :pub foo
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
      (foo 1)
    "#;
    let result = run_source_with_mode(src, PipelineMode::Check);
    if let Err(PipelineError::ContractCoverageMissing { .. }) = result {
        panic!("coverage gate fired without AIRL_COVERAGE_ENFORCE set");
    }
}
```

**Note:** These tests mutate process-wide env vars. If flakiness appears under parallel test execution, mark them `#[ignore]` and run explicitly, or thread a `CoverageConfig` struct instead (deferrable).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-driver coverage_gate`
Expected: all four tests FAIL — coverage gate not implemented.

- [ ] **Step 3: Add the gate in `z3_verify_tops`**

Edit `crates/airl-driver/src/pipeline.rs` inside `z3_verify_tops`. After the `let strict_verify = ...` line (around line 229), add:

```rust
    let coverage_enforce = std::env::var("AIRL_COVERAGE_ENFORCE").is_ok();
```

Then inside the `for (f, level) in &all_fns { ... }` loop (starting around line 252), at the very top of the loop body, insert:

```rust
        if coverage_enforce
            && *level == airl_syntax::ast::VerifyLevel::Proven
            && f.is_public
            && f.ensures.is_empty()
        {
            let module = module_path_for_fn(&f.name, tops);
            return Err(PipelineError::ContractCoverageMissing {
                fn_name: f.name.clone(),
                module,
            });
        }
```

- [ ] **Step 4: Add `module_path_for_fn` helper**

Add below `verify_level_for_fn` in `pipeline.rs`:

```rust
/// Return the containing module name for a function, or "<top-level>" if the
/// function is a bare top-level defn.
fn module_path_for_fn(fn_name: &str, tops: &[airl_syntax::ast::TopLevel]) -> String {
    for top in tops {
        if let airl_syntax::ast::TopLevel::Module(m) = top {
            for item in &m.body {
                if let airl_syntax::ast::TopLevel::Defn(f) = item {
                    if f.name == fn_name {
                        return m.name.clone();
                    }
                }
            }
        }
    }
    "<top-level>".to_string()
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p airl-driver coverage_gate`
Expected: all four PASS.

- [ ] **Step 6: Full test suite regression**

Run: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
Expected: all pass. (Coverage is env-gated; no behavior change without the env var.)

- [ ] **Step 7: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs
git commit -m "$(cat <<'EOF'
feat(driver): coverage gate for :pub defn in :verify proven modules

Gated by AIRL_COVERAGE_ENFORCE env var during development — will become
unconditional once the grandfather baseline lands. Fires only for
:verify proven context and :pub functions missing :ensures.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2.4: Fixture test for coverage gate

**Files:**
- Create: `tests/fixtures/contract_errors/pub_fn_no_ensures.airl`

- [ ] **Step 1: Create the fixture**

Write to `tests/fixtures/contract_errors/pub_fn_no_ensures.airl`:

```clojure
;; ERROR: has no :ensures clause
(module coverage-test
  :verify proven
  (defn :pub foo
    :sig [(x : i64) -> i64]
    :requires [(>= x 0)]
    :body x))

(foo 5)
```

- [ ] **Step 2: Add a dedicated integration test that runs this fixture with the env var set**

`tests/fixtures/contract_errors/` is scanned by the standard harness. But the harness does not set `AIRL_COVERAGE_ENFORCE`, so add a separate test in `crates/airl-driver/tests/fixtures.rs` (append at the bottom):

```rust
#[test]
fn coverage_fixture_pub_fn_no_ensures() {
    std::env::set_var("AIRL_COVERAGE_ENFORCE", "1");
    let path = fixtures_root()
        .join("contract_errors")
        .join("pub_fn_no_ensures.airl");
    let src = std::fs::read_to_string(&path).expect("fixture missing");
    let result = airl_driver::pipeline::run_source(&src);
    std::env::remove_var("AIRL_COVERAGE_ENFORCE");
    let err = result.expect_err("expected ContractCoverageMissing");
    let msg = format!("{}", err);
    assert!(msg.contains("has no :ensures"), "unexpected error: {}", msg);
}
```

- [ ] **Step 3: Run test**

Run: `cargo test -p airl-driver coverage_fixture_pub_fn_no_ensures`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/contract_errors/pub_fn_no_ensures.airl \
        crates/airl-driver/tests/fixtures.rs
git commit -m "$(cat <<'EOF'
test(driver): fixture for coverage gate error on :pub defn

Covers the AIRL_COVERAGE_ENFORCE=1 path. Becomes unconditional once
the default flip lands.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3 — Baseline file: data types + hand-rolled TOML

### Task 3.1: Create `verify_policy` module skeleton

**Files:**
- Create: `crates/airl-driver/src/verify_policy.rs`
- Modify: `crates/airl-driver/src/lib.rs` (add `pub mod verify_policy;`)

- [ ] **Step 1: Create the file with types**

Write to `crates/airl-driver/src/verify_policy.rs`:

```rust
//! Implements the `airl verify-policy` subcommand and the baseline file
//! that tracks grandfathered :verify checked / :verify trusted modules.
//!
//! Baseline file format is a hand-rolled minimal TOML subset:
//!   version = 1
//!   grandfathered_checked = [ "path/a.airl", "path/b.airl#module" ]
//!   grandfathered_trusted = [ "path/c.airl" ]

use std::path::{Path, PathBuf};

/// An entry in the baseline — either a whole file or a file#name suffix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BaselineKey {
    pub path: String,
    /// Optional disambiguator (module name or top-level defn name).
    pub name: Option<String>,
}

impl BaselineKey {
    pub fn whole_file(path: impl Into<String>) -> Self {
        Self { path: path.into(), name: None }
    }

    pub fn qualified(path: impl Into<String>, name: impl Into<String>) -> Self {
        Self { path: path.into(), name: Some(name.into()) }
    }

    /// Format as it appears in the baseline file.
    pub fn to_string(&self) -> String {
        match &self.name {
            Some(n) => format!("{}#{}", self.path, n),
            None => self.path.clone(),
        }
    }

    /// Parse from a line string like "path/a.airl" or "path/b.airl#name".
    pub fn parse(s: &str) -> Self {
        if let Some(idx) = s.find('#') {
            Self {
                path: s[..idx].to_string(),
                name: Some(s[idx + 1..].to_string()),
            }
        } else {
            Self { path: s.to_string(), name: None }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub version: u32,
    pub grandfathered_checked: Vec<BaselineKey>,
    pub grandfathered_trusted: Vec<BaselineKey>,
}

pub const BASELINE_VERSION: u32 = 1;
pub const BASELINE_FILE: &str = ".airl-verify-baseline.toml";

impl Baseline {
    pub fn new() -> Self {
        Self {
            version: BASELINE_VERSION,
            grandfathered_checked: Vec::new(),
            grandfathered_trusted: Vec::new(),
        }
    }
}

// Parser, writer, scanner, and command entry points follow in later tasks.
```

- [ ] **Step 2: Register module**

Edit `crates/airl-driver/src/lib.rs` — add alongside other `pub mod` declarations:

```rust
pub mod verify_policy;
```

- [ ] **Step 3: Compile check**

Run: `cargo build -p airl-driver`
Expected: compiles cleanly.

- [ ] **Step 4: Unit test for BaselineKey round-trip**

Add at the bottom of `verify_policy.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_key_whole_file_roundtrip() {
        let k = BaselineKey::whole_file("crates/foo/bar.airl");
        assert_eq!(k.to_string(), "crates/foo/bar.airl");
        assert_eq!(BaselineKey::parse("crates/foo/bar.airl"), k);
    }

    #[test]
    fn baseline_key_qualified_roundtrip() {
        let k = BaselineKey::qualified("crates/foo/bar.airl", "mymod");
        assert_eq!(k.to_string(), "crates/foo/bar.airl#mymod");
        assert_eq!(BaselineKey::parse("crates/foo/bar.airl#mymod"), k);
    }
}
```

Run: `cargo test -p airl-driver verify_policy::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs crates/airl-driver/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(driver): verify-policy module scaffolding + BaselineKey type

Foundation for the grandfather baseline file. Parser/writer/scanner
added in subsequent commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3.2: Hand-rolled TOML reader

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `verify_policy.rs`:

```rust
    #[test]
    fn parse_baseline_minimal() {
        let src = r#"
version = 1
grandfathered_checked = [
  "crates/a.airl",
  "crates/b.airl#mod2",
]
grandfathered_trusted = [
  "bootstrap/x.airl",
]
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.version, 1);
        assert_eq!(b.grandfathered_checked.len(), 2);
        assert_eq!(b.grandfathered_checked[0], BaselineKey::whole_file("crates/a.airl"));
        assert_eq!(b.grandfathered_checked[1], BaselineKey::qualified("crates/b.airl", "mod2"));
        assert_eq!(b.grandfathered_trusted.len(), 1);
        assert_eq!(b.grandfathered_trusted[0], BaselineKey::whole_file("bootstrap/x.airl"));
    }

    #[test]
    fn parse_baseline_empty_arrays() {
        let src = r#"
version = 1
grandfathered_checked = []
grandfathered_trusted = []
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.version, 1);
        assert!(b.grandfathered_checked.is_empty());
        assert!(b.grandfathered_trusted.is_empty());
    }

    #[test]
    fn parse_baseline_ignores_comments_and_blank_lines() {
        let src = r#"
# a leading comment
version = 1

# another comment
grandfathered_checked = [
  "a.airl",  # inline comment
]
grandfathered_trusted = []
"#;
        let b = Baseline::parse(src).expect("parse failed");
        assert_eq!(b.grandfathered_checked.len(), 1);
        assert_eq!(b.grandfathered_checked[0].path, "a.airl");
    }

    #[test]
    fn parse_baseline_rejects_missing_version() {
        let src = r#"
grandfathered_checked = []
grandfathered_trusted = []
"#;
        assert!(Baseline::parse(src).is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-driver parse_baseline`
Expected: FAIL — `Baseline::parse` not defined.

- [ ] **Step 3: Implement the parser**

Add to `verify_policy.rs`:

```rust
impl Baseline {
    /// Parse the minimal TOML subset used by `.airl-verify-baseline.toml`.
    /// Supported grammar:
    ///   - Line comments starting with '#'
    ///   - `version = <int>`
    ///   - `<name> = [ ]` for empty arrays
    ///   - Multi-line string arrays: `<name> = [\n  "...",\n  "...",\n]`
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut version: Option<u32> = None;
        let mut checked: Vec<BaselineKey> = Vec::new();
        let mut trusted: Vec<BaselineKey> = Vec::new();

        let mut lines = src.lines().peekable();
        while let Some(raw) = lines.next() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("version") {
                let rest = rest.trim_start();
                let rest = rest.strip_prefix('=').ok_or("expected `=` after version")?.trim();
                let n: u32 = rest.parse().map_err(|_| format!("invalid version: {}", rest))?;
                version = Some(n);
                continue;
            }
            if let Some(rest) = line.strip_prefix("grandfathered_checked") {
                let entries = parse_array(rest, &mut lines)?;
                checked = entries.into_iter().map(|s| BaselineKey::parse(&s)).collect();
                continue;
            }
            if let Some(rest) = line.strip_prefix("grandfathered_trusted") {
                let entries = parse_array(rest, &mut lines)?;
                trusted = entries.into_iter().map(|s| BaselineKey::parse(&s)).collect();
                continue;
            }
            return Err(format!("unexpected line: {}", line));
        }

        let version = version.ok_or("baseline missing `version` field")?;
        Ok(Self {
            version,
            grandfathered_checked: checked,
            grandfathered_trusted: trusted,
        })
    }
}

fn strip_comment(line: &str) -> &str {
    // Naive: first '#' starts a comment. Strings don't contain '#' in practice.
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn parse_array<'a, I>(after_name: &str, lines: &mut std::iter::Peekable<I>) -> Result<Vec<String>, String>
where
    I: Iterator<Item = &'a str>,
{
    // after_name is the text after the array name on the same line,
    // e.g. " = [" or " = []" or " = [ \"a\", \"b\" ]"
    let rest = after_name.trim_start().strip_prefix('=').ok_or("expected `=` after array name")?.trim_start();
    // Concatenate all lines until we see the closing `]`.
    let mut buf = String::from(rest);
    if !buf.contains(']') {
        for next in lines.by_ref() {
            buf.push(' ');
            buf.push_str(strip_comment(next).trim());
            if buf.contains(']') {
                break;
            }
        }
    }
    // Strip brackets.
    let inner = buf
        .trim()
        .strip_prefix('[')
        .ok_or("expected `[` starting array")?
        .trim_end()
        .strip_suffix(']')
        .ok_or("expected `]` ending array")?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if p.is_empty() { continue; }
        let s = p
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| format!("array element not quoted: {}", p))?;
        out.push(s.to_string());
    }
    Ok(out)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver parse_baseline`
Expected: all four PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): parser for .airl-verify-baseline.toml

Hand-rolled minimal TOML subset — no new external dependencies.
Supports comments, blank lines, and bracketed string arrays.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3.3: TOML writer with stable output

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests`:

```rust
    #[test]
    fn baseline_writer_roundtrip() {
        let mut b = Baseline::new();
        b.grandfathered_checked = vec![
            BaselineKey::whole_file("crates/a.airl"),
            BaselineKey::qualified("crates/b.airl", "mod2"),
        ];
        b.grandfathered_trusted = vec![
            BaselineKey::whole_file("bootstrap/x.airl"),
        ];
        let rendered = b.render();
        let parsed = Baseline::parse(&rendered).expect("roundtrip parse failed");
        assert_eq!(parsed, b);
    }

    #[test]
    fn baseline_writer_sorts_entries() {
        let mut b = Baseline::new();
        b.grandfathered_checked = vec![
            BaselineKey::whole_file("z.airl"),
            BaselineKey::whole_file("a.airl"),
            BaselineKey::whole_file("m.airl"),
        ];
        let rendered = b.render();
        let a_pos = rendered.find("a.airl").unwrap();
        let m_pos = rendered.find("m.airl").unwrap();
        let z_pos = rendered.find("z.airl").unwrap();
        assert!(a_pos < m_pos && m_pos < z_pos, "entries not sorted:\n{}", rendered);
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver baseline_writer`
Expected: FAIL — `Baseline::render` not defined.

- [ ] **Step 3: Implement render**

Add to `impl Baseline`:

```rust
    /// Render the baseline as a stable, sorted TOML string.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("# Managed by `airl verify-policy`. Hand-edits are allowed; CI validates\n");
        s.push_str("# consistency on every run. Remove an entry to ratchet — requires upgrading\n");
        s.push_str("# the module's :verify to proven and adding :ensures to every :pub defn.\n");
        s.push_str(&format!("version = {}\n", self.version));
        s.push_str("\n");
        s.push_str("grandfathered_checked = [\n");
        let mut checked: Vec<BaselineKey> = self.grandfathered_checked.clone();
        checked.sort();
        checked.dedup();
        for k in &checked {
            s.push_str(&format!("  \"{}\",\n", k.to_string()));
        }
        s.push_str("]\n");
        s.push_str("\n");
        s.push_str("grandfathered_trusted = [\n");
        let mut trusted: Vec<BaselineKey> = self.grandfathered_trusted.clone();
        trusted.sort();
        trusted.dedup();
        for k in &trusted {
            s.push_str(&format!("  \"{}\",\n", k.to_string()));
        }
        s.push_str("]\n");
        s
    }

    /// Read baseline from disk, or return `Ok(Baseline::new())` if missing.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Baseline::new());
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {}", path.display(), e))?;
        Baseline::parse(&src)
    }

    /// Write baseline to disk atomically-ish (write then rename).
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let rendered = self.render();
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, rendered.as_bytes())
            .map_err(|e| format!("writing {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("renaming to {}: {}", path.display(), e))?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver baseline_writer`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): baseline writer with stable sorted output

Render is deterministic (sorted, deduped). `load` returns an empty
baseline if the file is missing. `write` uses temp-and-rename.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 4 — Tree scanner

### Task 4.1: Enumerate `.airl` files (git-aware)

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests` (uses tempfile dev-dep):

```rust
    #[test]
    fn scan_airl_files_excludes_fixtures() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        // Create fake tree
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::create_dir_all(root.join("tests/fixtures/valid")).unwrap();
        std::fs::write(root.join("crates/a/src/lib.airl"), "(module a (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();
        std::fs::write(root.join("tests/fixtures/valid/skip.airl"), "(module skip (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();
        std::fs::write(root.join("crates/a/src/notes.md"), "# not airl").unwrap();

        let files = enumerate_airl_files(root);
        let rel: Vec<String> = files.iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(rel.iter().any(|p| p == "crates/a/src/lib.airl"), "missing lib.airl: {:?}", rel);
        assert!(!rel.iter().any(|p| p.starts_with("tests/fixtures/")), "included fixture: {:?}", rel);
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver scan_airl_files`
Expected: FAIL — `enumerate_airl_files` not defined.

- [ ] **Step 3: Implement enumeration**

Add to `verify_policy.rs`:

```rust
/// Walk the tree rooted at `root`, collecting `.airl` files.
/// Excludes `tests/fixtures/**` and anything under a `target/` directory.
pub fn enumerate_airl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        // Exclusions
        if rel_str.starts_with("tests/fixtures/") || rel_str.starts_with("tests\\fixtures\\") {
            continue;
        }
        if let Some(name) = path.file_name() {
            if name == "target" || name == ".git" {
                continue;
            }
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().map_or(false, |e| e == "airl") {
            out.push(path);
        }
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p airl-driver scan_airl_files`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): enumerate .airl files with fixture exclusion

Walks the repo tree; skips tests/fixtures/**, target/, .git/.
Sorted output for deterministic diffs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4.2: Extract verify levels from a parsed file

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn extract_entries_from_module() {
        let src = r#"(module foo :verify checked (defn x :sig [-> i64] :requires [true] :body 0))"#;
        let tops = airl_syntax::parse(src).unwrap().tops;
        let entries = extract_verify_entries("path/foo.airl", &tops);
        assert_eq!(entries.len(), 1);
        let (key, level) = &entries[0];
        assert_eq!(key, &BaselineKey::whole_file("path/foo.airl"));
        assert_eq!(*level, airl_syntax::ast::VerifyLevel::Checked);
    }

    #[test]
    fn extract_entries_multi_module_file() {
        let src = r#"
          (module foo :verify checked (defn x :sig [-> i64] :requires [true] :body 0))
          (module bar :verify trusted (defn y :sig [-> i64] :requires [true] :body 0))
        "#;
        let tops = airl_syntax::parse(src).unwrap().tops;
        let entries = extract_verify_entries("path/f.airl", &tops);
        assert_eq!(entries.len(), 2);
        let names: Vec<Option<String>> = entries.iter().map(|(k, _)| k.name.clone()).collect();
        assert!(names.contains(&Some("foo".to_string())));
        assert!(names.contains(&Some("bar".to_string())));
    }

    #[test]
    fn extract_entries_top_level_defn() {
        let src = r#"(defn :pub foo :verify checked :sig [-> i64] :requires [true] :body 0)"#;
        let tops = airl_syntax::parse(src).unwrap().tops;
        let entries = extract_verify_entries("path/f.airl", &tops);
        assert_eq!(entries.len(), 1);
        let (key, level) = &entries[0];
        assert_eq!(key.name.as_deref(), Some("foo"));
        assert_eq!(*level, airl_syntax::ast::VerifyLevel::Checked);
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver extract_entries`
Expected: FAIL.

- [ ] **Step 3: Implement extraction**

Add to `verify_policy.rs`:

```rust
/// Extract (key, level) pairs for every module and top-level defn in the file.
///
/// Multi-module files emit qualified keys (path#modname / path#defnname).
/// Single-module or single-top-level-defn files emit a plain-path key only
/// when the result is unambiguous.
pub fn extract_verify_entries(
    path: &str,
    tops: &[airl_syntax::ast::TopLevel],
) -> Vec<(BaselineKey, airl_syntax::ast::VerifyLevel)> {
    let mut modules: Vec<(String, airl_syntax::ast::VerifyLevel)> = Vec::new();
    let mut bare_defns: Vec<(String, airl_syntax::ast::VerifyLevel)> = Vec::new();
    for top in tops {
        match top {
            airl_syntax::ast::TopLevel::Module(m) => {
                modules.push((m.name.clone(), m.verify));
            }
            airl_syntax::ast::TopLevel::Defn(f) => {
                let level = f.verify.unwrap_or_else(airl_syntax::ast::VerifyLevel::default);
                bare_defns.push((f.name.clone(), level));
            }
            _ => {}
        }
    }
    let total = modules.len() + bare_defns.len();
    let use_plain = total == 1;
    let mut out = Vec::new();
    for (name, level) in modules {
        let key = if use_plain {
            BaselineKey::whole_file(path)
        } else {
            BaselineKey::qualified(path, name)
        };
        out.push((key, level));
    }
    for (name, level) in bare_defns {
        let key = if use_plain {
            BaselineKey::whole_file(path)
        } else {
            BaselineKey::qualified(path, name)
        };
        out.push((key, level));
    }
    out
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver extract_entries`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): extract per-module/per-defn verify levels from a parsed file

Multi-module files emit path#name keys; single-entry files use plain
paths. Basis for the tree-vs-baseline diff.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4.3: Compute tree-vs-baseline diff

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn diff_detects_new_checked_module_not_in_baseline() {
        let mut b = Baseline::new();
        // Baseline is empty
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Checked),
        ];
        let diff = compute_diff(&b, &scanned);
        assert_eq!(diff.new_checked.len(), 1);
        assert_eq!(diff.new_checked[0], BaselineKey::whole_file("a.airl"));
        assert!(diff.new_trusted.is_empty());
        assert!(diff.stale_checked.is_empty());
    }

    #[test]
    fn diff_tolerates_upgraded_module_in_baseline() {
        let mut b = Baseline::new();
        b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Proven),
        ];
        let diff = compute_diff(&b, &scanned);
        assert!(diff.new_checked.is_empty(), "should not regress on upgraded module");
        assert_eq!(diff.stale_checked.len(), 1, "should report upgrade as prunable");
    }

    #[test]
    fn diff_clean_when_baseline_matches() {
        let mut b = Baseline::new();
        b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Checked),
        ];
        let diff = compute_diff(&b, &scanned);
        assert!(diff.is_clean(), "expected clean: {:?}", diff);
    }

    #[test]
    fn diff_flags_new_trusted_separately() {
        let b = Baseline::new();
        let scanned = vec![
            (BaselineKey::whole_file("a.airl"), airl_syntax::ast::VerifyLevel::Trusted),
        ];
        let diff = compute_diff(&b, &scanned);
        assert_eq!(diff.new_trusted.len(), 1);
        assert!(diff.new_checked.is_empty());
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver diff_`
Expected: FAIL.

- [ ] **Step 3: Implement diff**

Add to `verify_policy.rs`:

```rust
#[derive(Debug, Default)]
pub struct PolicyDiff {
    /// Keys at :verify checked in the tree but missing from grandfathered_checked.
    pub new_checked: Vec<BaselineKey>,
    /// Keys at :verify trusted in the tree but missing from grandfathered_trusted.
    pub new_trusted: Vec<BaselineKey>,
    /// Keys in grandfathered_checked but no longer at :verify checked in the tree.
    pub stale_checked: Vec<BaselineKey>,
    /// Keys in grandfathered_trusted but no longer at :verify trusted in the tree.
    pub stale_trusted: Vec<BaselineKey>,
}

impl PolicyDiff {
    pub fn is_clean(&self) -> bool {
        // "Clean" means no regressions. Stale entries are tolerated (they just
        // mean the user has ratcheted but not yet run --prune).
        self.new_checked.is_empty() && self.new_trusted.is_empty()
    }

    pub fn is_fully_clean(&self) -> bool {
        self.new_checked.is_empty()
            && self.new_trusted.is_empty()
            && self.stale_checked.is_empty()
            && self.stale_trusted.is_empty()
    }
}

pub fn compute_diff(
    baseline: &Baseline,
    scanned: &[(BaselineKey, airl_syntax::ast::VerifyLevel)],
) -> PolicyDiff {
    use std::collections::HashSet;
    let baseline_checked: HashSet<&BaselineKey> = baseline.grandfathered_checked.iter().collect();
    let baseline_trusted: HashSet<&BaselineKey> = baseline.grandfathered_trusted.iter().collect();

    let mut scanned_checked: HashSet<&BaselineKey> = HashSet::new();
    let mut scanned_trusted: HashSet<&BaselineKey> = HashSet::new();
    for (key, level) in scanned {
        match level {
            airl_syntax::ast::VerifyLevel::Checked => { scanned_checked.insert(key); }
            airl_syntax::ast::VerifyLevel::Trusted => { scanned_trusted.insert(key); }
            airl_syntax::ast::VerifyLevel::Proven => {}
        }
    }

    let mut diff = PolicyDiff::default();
    for k in &scanned_checked {
        if !baseline_checked.contains(k) {
            diff.new_checked.push((*k).clone());
        }
    }
    for k in &scanned_trusted {
        if !baseline_trusted.contains(k) {
            diff.new_trusted.push((*k).clone());
        }
    }
    for k in &baseline_checked {
        if !scanned_checked.contains(k) {
            diff.stale_checked.push((*k).clone());
        }
    }
    for k in &baseline_trusted {
        if !scanned_trusted.contains(k) {
            diff.stale_trusted.push((*k).clone());
        }
    }
    diff.new_checked.sort();
    diff.new_trusted.sort();
    diff.stale_checked.sort();
    diff.stale_trusted.sort();
    diff
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver diff_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): tree-vs-baseline diff with regression detection

new_* entries are regressions (fail the policy check).
stale_* entries indicate upgrades that --prune can remove.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 5 — `verify-policy` subcommand

### Task 5.1: Subcommand dispatch in main.rs

**Files:**
- Modify: `crates/airl-driver/src/main.rs`
- Modify: `crates/airl-driver/src/verify_policy.rs` (add `run_command`)

- [ ] **Step 1: Identify the subcommand dispatch site**

Open `crates/airl-driver/src/main.rs` and find the main dispatch block. Look for where `check`, `compile`, `run` are matched. The dispatch is by `args[1]`. Add a new arm for `"verify-policy"` that calls into `verify_policy::run_command`.

- [ ] **Step 2: Add stub `run_command` entry point**

Add to `verify_policy.rs`:

```rust
/// Entry point for `airl verify-policy [...]`.
/// Returns a process exit code.
pub fn run_command(args: &[String]) -> i32 {
    let mut mode = Mode::Check;
    for arg in args {
        match arg.as_str() {
            "--check" => mode = Mode::Check,
            "--init" => mode = Mode::Init,
            "--prune" => mode = Mode::Prune,
            "--list-uncovered" => mode = Mode::ListUncovered,
            _ => {
                eprintln!("verify-policy: unknown argument `{}`", arg);
                eprintln!("usage: airl verify-policy [--check | --init | --prune | --list-uncovered]");
                return 2;
            }
        }
    }
    match mode {
        Mode::Check => run_check(&std::env::current_dir().unwrap()),
        Mode::Init => run_init(&std::env::current_dir().unwrap()),
        Mode::Prune => run_prune(&std::env::current_dir().unwrap()),
        Mode::ListUncovered => run_list_uncovered(&std::env::current_dir().unwrap()),
    }
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Check,
    Init,
    Prune,
    ListUncovered,
}

fn run_check(root: &Path) -> i32 {
    let baseline_path = root.join(BASELINE_FILE);
    let baseline = match Baseline::load(&baseline_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: reading {}: {}", baseline_path.display(), e);
            return 2;
        }
    };
    let mut scanned: Vec<(BaselineKey, airl_syntax::ast::VerifyLevel)> = Vec::new();
    for file in enumerate_airl_files(root) {
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: reading {}: {}", file.display(), e);
                return 2;
            }
        };
        let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        let parsed = match airl_syntax::parse(&src) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: parsing {}: {}", rel, e);
                return 2;
            }
        };
        scanned.extend(extract_verify_entries(&rel, &parsed.tops));
    }
    let diff = compute_diff(&baseline, &scanned);
    if diff.is_clean() {
        println!("verify-policy: OK ({} checked, {} trusted, {} stale)",
                 baseline.grandfathered_checked.len(),
                 baseline.grandfathered_trusted.len(),
                 diff.stale_checked.len() + diff.stale_trusted.len());
        0
    } else {
        eprintln!("verify-policy: REGRESSION");
        for k in &diff.new_checked {
            eprintln!("  + :verify checked (not in baseline): {}", k.to_string());
        }
        for k in &diff.new_trusted {
            eprintln!("  + :verify trusted (not in baseline): {}", k.to_string());
        }
        eprintln!("fix: upgrade the module's :verify annotation, or add the entry to {}", BASELINE_FILE);
        1
    }
}

fn run_init(_root: &Path) -> i32 {
    eprintln!("verify-policy --init: not yet implemented (see Phase 6)");
    2
}

fn run_prune(_root: &Path) -> i32 {
    eprintln!("verify-policy --prune: not yet implemented (see Phase 6)");
    2
}

fn run_list_uncovered(_root: &Path) -> i32 {
    eprintln!("verify-policy --list-uncovered: not yet implemented (see Phase 6)");
    2
}
```

- [ ] **Step 3: Wire into main.rs dispatch**

Find the main subcommand match in `crates/airl-driver/src/main.rs`. Add a case for `"verify-policy"` that collects remaining args and exits with `verify_policy::run_command`. Insert alongside other subcommand branches. Exact placement depends on the match layout; grep for `"check"` or `"compile"` to locate it:

```bash
grep -n '"check"\|"compile"\|"run"' crates/airl-driver/src/main.rs | head
```

Then add (example shape — adapt to the real dispatch):

```rust
        "verify-policy" => {
            let rest: Vec<String> = args.iter().skip(2).cloned().collect();
            std::process::exit(airl_driver::verify_policy::run_command(&rest));
        }
```

- [ ] **Step 4: Build**

Run: `cargo build -p airl-driver`
Expected: compiles.

- [ ] **Step 5: Smoke test in the worktree**

```bash
cargo run -p airl-driver -- verify-policy --help 2>&1 | head -5 || true
cargo run -p airl-driver -- verify-policy --unknown-arg
```

The second command should exit 2 with a usage message.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-driver/src/main.rs crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): verify-policy subcommand dispatch + --check mode

--check reads .airl-verify-baseline.toml, scans tracked .airl files,
and emits a regression report. --init/--prune/--list-uncovered are
stubs wired up for Phase 6.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5.2: Integration test with temp dir

**Files:**
- Create: `crates/airl-driver/tests/verify_policy.rs`

- [ ] **Step 1: Write the test file**

Write to `crates/airl-driver/tests/verify_policy.rs`:

```rust
use std::fs;
use tempfile::TempDir;

use airl_driver::verify_policy::{
    Baseline, BaselineKey, compute_diff, enumerate_airl_files, extract_verify_entries,
};

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn clean_tree_matches_baseline() {
    let td = TempDir::new().unwrap();
    let root = td.path();
    write_file(root, "crates/a/a.airl",
        "(module a :verify checked (defn x :sig [-> i64] :requires [true] :body 0))");
    write_file(root, "crates/b/b.airl",
        "(module b :verify proven (defn :pub y :sig [-> i64] :requires [true] :ensures [(= result 0)] :body 0))");

    let mut baseline = Baseline::new();
    baseline.grandfathered_checked.push(BaselineKey::whole_file("crates/a/a.airl"));

    let mut scanned = Vec::new();
    for f in enumerate_airl_files(root) {
        let src = fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let parsed = airl_syntax::parse(&src).unwrap();
        scanned.extend(extract_verify_entries(&rel, &parsed.tops));
    }
    let diff = compute_diff(&baseline, &scanned);
    assert!(diff.is_fully_clean(), "expected clean: {:?}", diff);
}

#[test]
fn unlisted_checked_module_is_regression() {
    let td = TempDir::new().unwrap();
    let root = td.path();
    write_file(root, "crates/a/a.airl",
        "(module a :verify checked (defn x :sig [-> i64] :requires [true] :body 0))");
    // Baseline is empty — unlisted checked module is a regression.
    let baseline = Baseline::new();

    let mut scanned = Vec::new();
    for f in enumerate_airl_files(root) {
        let src = fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let parsed = airl_syntax::parse(&src).unwrap();
        scanned.extend(extract_verify_entries(&rel, &parsed.tops));
    }
    let diff = compute_diff(&baseline, &scanned);
    assert!(!diff.is_clean());
    assert_eq!(diff.new_checked.len(), 1);
    assert_eq!(diff.new_checked[0], BaselineKey::whole_file("crates/a/a.airl"));
}

#[test]
fn baseline_file_roundtrip_on_disk() {
    let td = TempDir::new().unwrap();
    let path = td.path().join(".airl-verify-baseline.toml");
    let mut b = Baseline::new();
    b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
    b.grandfathered_trusted.push(BaselineKey::qualified("b.airl", "mod1"));
    b.write(&path).unwrap();
    let loaded = Baseline::load(&path).unwrap();
    assert_eq!(loaded, b);
}
```

- [ ] **Step 2: Ensure public exports**

Check that `Baseline`, `BaselineKey`, `compute_diff`, `enumerate_airl_files`, `extract_verify_entries` are `pub` in `verify_policy.rs`. Adjust as needed.

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p airl-driver --test verify_policy`
Expected: all three tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/airl-driver/tests/verify_policy.rs crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
test(driver): integration tests for verify-policy --check

Exercises temp-dir scans against synthetic baselines: clean match,
regression detection, and baseline file roundtrip.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 6 — Migration tool: `--init`, `--prune`, `--list-uncovered`

### Task 6.1: Textual source rewriter for `:verify checked` insertion

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn rewrite_inserts_verify_checked_into_module_header() {
        let src = "(module foo\n  :version 0.1.0\n  (defn x :sig [-> i64] :requires [true] :body 0))";
        let parsed = airl_syntax::parse(src).unwrap();
        let out = insert_verify_checked_into_modules(src, &parsed.tops);
        assert!(out.contains(":verify checked"), "missing insertion: {}", out);
        // Re-parse must succeed and yield VerifyLevel::Checked
        let reparsed = airl_syntax::parse(&out).unwrap();
        match &reparsed.tops[0] {
            airl_syntax::ast::TopLevel::Module(m) => assert_eq!(m.verify, airl_syntax::ast::VerifyLevel::Checked),
            _ => panic!("expected module"),
        }
    }

    #[test]
    fn rewrite_skips_modules_with_explicit_verify() {
        let src = "(module foo :verify proven (defn x :sig [-> i64] :requires [true] :ensures [(= result 0)] :body 0))";
        let parsed = airl_syntax::parse(src).unwrap();
        let out = insert_verify_checked_into_modules(src, &parsed.tops);
        // Proven should be preserved; no :verify checked injected.
        assert!(!out.contains(":verify checked"));
        assert!(out.contains(":verify proven"));
    }

    #[test]
    fn rewrite_inserts_into_top_level_defn() {
        let src = "(defn :pub foo :sig [-> i64] :requires [true] :body 0)";
        let parsed = airl_syntax::parse(src).unwrap();
        let out = insert_verify_checked_into_top_level_defns(src, &parsed.tops);
        assert!(out.contains(":verify checked"), "missing: {}", out);
        let reparsed = airl_syntax::parse(&out).unwrap();
        match &reparsed.tops[0] {
            airl_syntax::ast::TopLevel::Defn(f) => assert_eq!(f.verify, Some(airl_syntax::ast::VerifyLevel::Checked)),
            _ => panic!("expected defn"),
        }
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver rewrite_`
Expected: FAIL — rewriter functions not defined.

- [ ] **Step 3: Implement rewriter**

Approach: the parser already captured spans. Find each target form's opening `(` and insert ` :verify checked` right after the name symbol. Use parsed spans to avoid regex fragility.

Add to `verify_policy.rs`:

```rust
/// Rewrite source to insert `:verify checked` after each module's name symbol,
/// but only for modules that have no explicit :verify annotation.
///
/// Strategy: sort modules by their span's start offset in descending order,
/// then insert at `module_name_span.end`. Working right-to-left preserves
/// earlier offsets.
pub fn insert_verify_checked_into_modules(
    src: &str,
    tops: &[airl_syntax::ast::TopLevel],
) -> String {
    let mut edits: Vec<(usize, &str)> = Vec::new();
    for top in tops {
        if let airl_syntax::ast::TopLevel::Module(m) = top {
            if is_module_verify_explicit_in_source(src, m) {
                continue;
            }
            // Name ends at m.span.start + len("(module NAME") — use the name span
            // if available; otherwise locate by scan.
            let ins_offset = locate_after_module_name(src, m);
            edits.push((ins_offset, " :verify checked"));
        }
    }
    apply_edits(src, edits)
}

pub fn insert_verify_checked_into_top_level_defns(
    src: &str,
    tops: &[airl_syntax::ast::TopLevel],
) -> String {
    let mut edits: Vec<(usize, &str)> = Vec::new();
    for top in tops {
        if let airl_syntax::ast::TopLevel::Defn(f) = top {
            if f.verify.is_some() {
                continue;
            }
            let ins_offset = locate_after_defn_name(src, f);
            edits.push((ins_offset, " :verify checked"));
        }
    }
    apply_edits(src, edits)
}

fn is_module_verify_explicit_in_source(src: &str, m: &airl_syntax::ast::ModuleDef) -> bool {
    // The parser defaults m.verify to Checked if the keyword is absent.
    // Inspect the source for the keyword within the module span.
    let start = m.span.start;
    let end = m.span.end.min(src.len());
    src.get(start..end).map_or(false, |slice| slice.contains(":verify"))
}

fn locate_after_module_name(src: &str, m: &airl_syntax::ast::ModuleDef) -> usize {
    // Find the `module` keyword inside the module's span, then skip whitespace
    // to find the name token, then return its end offset.
    let start = m.span.start;
    let slice = &src[start..];
    let mod_kw = slice.find("module").expect("module keyword present in source");
    let after_kw = start + mod_kw + "module".len();
    // Skip whitespace
    let mut i = after_kw;
    while i < src.len() && src.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    // Consume the name token (until whitespace, ')', or keyword ':').
    while i < src.len() {
        let b = src.as_bytes()[i];
        if b.is_ascii_whitespace() || b == b')' || b == b':' || b == b'(' {
            break;
        }
        i += 1;
    }
    i
}

fn locate_after_defn_name(src: &str, f: &airl_syntax::ast::FnDef) -> usize {
    // Same strategy: find `defn` token within the function's span, then the
    // (optional) :pub keyword, then the name.
    let start = f.span.start;
    let slice = &src[start..];
    let defn_kw = slice.find("defn").expect("defn keyword present in source");
    let after_kw = start + defn_kw + "defn".len();
    let mut i = after_kw;
    // Skip whitespace
    while i < src.len() && src.as_bytes()[i].is_ascii_whitespace() { i += 1; }
    // Optional :pub
    if src[i..].starts_with(":pub") {
        i += ":pub".len();
        while i < src.len() && src.as_bytes()[i].is_ascii_whitespace() { i += 1; }
    }
    // Name token
    while i < src.len() {
        let b = src.as_bytes()[i];
        if b.is_ascii_whitespace() || b == b')' || b == b':' || b == b'(' {
            break;
        }
        i += 1;
    }
    i
}

fn apply_edits(src: &str, mut edits: Vec<(usize, &str)>) -> String {
    // Apply right-to-left so offsets stay valid.
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = src.to_string();
    for (offset, text) in edits {
        out.insert_str(offset, text);
    }
    out
}
```

Dependency note: the tests require `airl_syntax::ast::ModuleDef.span` and `FnDef.span` to be `Span { start, end }` with byte offsets. Verify this matches the existing `Span` struct:

```bash
grep -n "pub struct Span" crates/airl-syntax/src/span.rs
```

If `Span` uses a different shape (e.g., line/col), adapt the `locate_*` helpers to compute byte offsets from line/col.

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver rewrite_`
Expected: PASS. If Span layout differs, fix the helpers before re-running.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): textual source rewriter for :verify checked insertion

Uses parsed spans to locate insertion points right after a module or
defn's name symbol. Right-to-left edit application preserves earlier
byte offsets. Skips targets that already have :verify.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.2: `--init` implementation

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn init_populates_baseline_and_rewrites_sources() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::write(root.join("crates/a/src/lib.airl"),
            "(module a (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();

        let code = run_init(root);
        assert_eq!(code, 0, "init should succeed");

        let baseline = Baseline::load(&root.join(BASELINE_FILE)).unwrap();
        assert!(baseline.grandfathered_checked.iter()
            .any(|k| k.path == "crates/a/src/lib.airl"), "missing entry: {:?}", baseline);

        let rewritten = std::fs::read_to_string(root.join("crates/a/src/lib.airl")).unwrap();
        assert!(rewritten.contains(":verify checked"), "module not rewritten: {}", rewritten);
    }

    #[test]
    fn init_is_idempotent() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.airl"),
            "(module a (defn x :sig [-> i64] :requires [true] :body 0))").unwrap();

        assert_eq!(run_init(root), 0);
        let first = std::fs::read_to_string(root.join("src/lib.airl")).unwrap();
        assert_eq!(run_init(root), 0);
        let second = std::fs::read_to_string(root.join("src/lib.airl")).unwrap();
        assert_eq!(first, second, "init not idempotent");
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver init_`
Expected: FAIL — `run_init` still the stub.

- [ ] **Step 3: Implement `run_init`**

Replace the stub `fn run_init` in `verify_policy.rs`:

```rust
fn run_init(root: &Path) -> i32 {
    let files = enumerate_airl_files(root);
    let mut baseline = Baseline::load(&root.join(BASELINE_FILE)).unwrap_or_default();
    // Preserve any explicit hand-edits to the baseline — --init merges, doesn't overwrite.
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: reading {}: {}", file.display(), e);
                return 2;
            }
        };
        let parsed = match airl_syntax::parse(&src) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: parsing {}: {}", file.display(), e);
                return 2;
            }
        };
        let rel = file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/");

        // First pass: collect existing entries to populate baseline for already-explicit
        // Checked/Trusted modules and defns.
        for (key, level) in extract_verify_entries(&rel, &parsed.tops) {
            match level {
                airl_syntax::ast::VerifyLevel::Checked => {
                    if !baseline.grandfathered_checked.contains(&key) {
                        baseline.grandfathered_checked.push(key);
                    }
                }
                airl_syntax::ast::VerifyLevel::Trusted => {
                    if !baseline.grandfathered_trusted.contains(&key) {
                        baseline.grandfathered_trusted.push(key);
                    }
                }
                airl_syntax::ast::VerifyLevel::Proven => {}
            }
        }

        // Second pass: rewrite implicit-default modules to explicit :verify checked.
        let rewritten_mods = insert_verify_checked_into_modules(&src, &parsed.tops);
        let rewritten_all = insert_verify_checked_into_top_level_defns(&rewritten_mods, &parsed.tops);

        if rewritten_all != src {
            if let Err(e) = std::fs::write(file, &rewritten_all) {
                eprintln!("error: writing {}: {}", file.display(), e);
                return 2;
            }
            // Re-parse the rewritten file and re-extract entries so the newly
            // explicit Checked entries land in the baseline too.
            if let Ok(reparsed) = airl_syntax::parse(&rewritten_all) {
                for (key, level) in extract_verify_entries(&rel, &reparsed.tops) {
                    if level == airl_syntax::ast::VerifyLevel::Checked
                        && !baseline.grandfathered_checked.contains(&key)
                    {
                        baseline.grandfathered_checked.push(key);
                    }
                }
            }
        }
    }
    baseline.grandfathered_checked.sort();
    baseline.grandfathered_checked.dedup();
    baseline.grandfathered_trusted.sort();
    baseline.grandfathered_trusted.dedup();
    if let Err(e) = baseline.write(&root.join(BASELINE_FILE)) {
        eprintln!("error: writing baseline: {}", e);
        return 2;
    }
    println!(
        "verify-policy --init: {} files scanned, {} checked + {} trusted in baseline",
        files.len(),
        baseline.grandfathered_checked.len(),
        baseline.grandfathered_trusted.len()
    );
    0
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver init_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): verify-policy --init migration tool

Rewrites every module and bare top-level defn without explicit
:verify to carry :verify checked, and populates
.airl-verify-baseline.toml accordingly. Idempotent — re-running
is a no-op.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.3: `--prune` implementation

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn prune_removes_stale_entries() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        // Module was grandfathered as Checked but has since been upgraded to Proven.
        std::fs::write(root.join("src/lib.airl"),
            "(module a :verify proven (defn :pub x :sig [-> i64] :requires [true] :ensures [(= result 0)] :body 0))").unwrap();
        let mut b = Baseline::new();
        b.grandfathered_checked.push(BaselineKey::whole_file("src/lib.airl"));
        b.write(&root.join(BASELINE_FILE)).unwrap();

        assert_eq!(run_prune(root), 0);
        let updated = Baseline::load(&root.join(BASELINE_FILE)).unwrap();
        assert!(updated.grandfathered_checked.is_empty(), "stale entry should be pruned: {:?}", updated);
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p airl-driver prune_`
Expected: FAIL.

- [ ] **Step 3: Implement `run_prune`**

Replace the stub:

```rust
fn run_prune(root: &Path) -> i32 {
    let baseline_path = root.join(BASELINE_FILE);
    let mut baseline = match Baseline::load(&baseline_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: reading baseline: {}", e);
            return 2;
        }
    };

    let mut scanned = Vec::new();
    for file in enumerate_airl_files(root) {
        let src = std::fs::read_to_string(&file).unwrap_or_default();
        if src.is_empty() { continue; }
        let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        if let Ok(parsed) = airl_syntax::parse(&src) {
            scanned.extend(extract_verify_entries(&rel, &parsed.tops));
        }
    }

    let diff = compute_diff(&baseline, &scanned);
    let before_checked = baseline.grandfathered_checked.len();
    let before_trusted = baseline.grandfathered_trusted.len();

    baseline.grandfathered_checked.retain(|k| !diff.stale_checked.contains(k));
    baseline.grandfathered_trusted.retain(|k| !diff.stale_trusted.contains(k));

    if let Err(e) = baseline.write(&baseline_path) {
        eprintln!("error: writing baseline: {}", e);
        return 2;
    }
    println!(
        "verify-policy --prune: removed {} checked + {} trusted stale entries",
        before_checked - baseline.grandfathered_checked.len(),
        before_trusted - baseline.grandfathered_trusted.len(),
    );
    0
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver prune_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): verify-policy --prune removes stale baseline entries

A baseline entry is stale if its module no longer sits at the
level the baseline claims (e.g. Checked → Proven upgrade).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6.4: `--list-uncovered` implementation

**Files:**
- Modify: `crates/airl-driver/src/verify_policy.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn list_uncovered_reports_missing_ensures() {
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.airl"),
            "(module a :verify proven (defn :pub x :sig [-> i64] :requires [true] :body 0))").unwrap();

        let (code, report) = run_list_uncovered_collect(root);
        assert_eq!(code, 0);
        assert!(report.contains("x"), "missing fn name in report: {}", report);
        assert!(report.contains("src/lib.airl"), "missing path: {}", report);
    }
```

- [ ] **Step 2: Replace the stub**

```rust
fn run_list_uncovered(root: &Path) -> i32 {
    let (code, report) = run_list_uncovered_collect(root);
    print!("{}", report);
    code
}

/// Testable variant — returns (exit_code, human_report).
fn run_list_uncovered_collect(root: &Path) -> (i32, String) {
    let mut report = String::new();
    let mut total = 0;
    for file in enumerate_airl_files(root) {
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        let parsed = match airl_syntax::parse(&src) {
            Ok(p) => p,
            Err(_) => continue,
        };
        for top in &parsed.tops {
            match top {
                airl_syntax::ast::TopLevel::Module(m) => {
                    for item in &m.body {
                        if let airl_syntax::ast::TopLevel::Defn(f) = item {
                            let level = f.verify.unwrap_or(m.verify);
                            if level == airl_syntax::ast::VerifyLevel::Proven
                                && f.is_public
                                && f.ensures.is_empty()
                            {
                                report.push_str(&format!("{}: :pub defn `{}` missing :ensures\n", rel, f.name));
                                total += 1;
                            }
                        }
                    }
                }
                airl_syntax::ast::TopLevel::Defn(f) => {
                    let level = f.verify.unwrap_or_else(airl_syntax::ast::VerifyLevel::default);
                    if level == airl_syntax::ast::VerifyLevel::Proven
                        && f.is_public
                        && f.ensures.is_empty()
                    {
                        report.push_str(&format!("{}: :pub defn `{}` missing :ensures\n", rel, f.name));
                        total += 1;
                    }
                }
                _ => {}
            }
        }
    }
    report.push_str(&format!("{} uncovered :pub defns\n", total));
    (0, report)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-driver list_uncovered`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/airl-driver/src/verify_policy.rs
git commit -m "$(cat <<'EOF'
feat(driver): verify-policy --list-uncovered diagnostic

Prints every :pub defn in :verify proven context that lacks an
:ensures clause. Read-only; useful in ratcheting PRs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 7 — Run the migration (Commit A)

### Task 7.1: Dry-run `--init` on the live tree

- [ ] **Step 1: Build the driver**

```bash
cargo build -p airl-driver
```

- [ ] **Step 2: Run `--init`**

From the repo root:

```bash
cargo run -p airl-driver -- verify-policy --init
```

Expected output: `verify-policy --init: N files scanned, M checked + K trusted in baseline` with M roughly matching the number of `.airl` files in the tree (excluding fixtures).

- [ ] **Step 3: Inspect the diff**

```bash
git status
git diff --stat
git diff .airl-verify-baseline.toml
```

Spot-check a few representative files:

```bash
git diff bootstrap/lexer.airl | head -20
git diff stdlib/list.airl | head -20
```

Each should show only `:verify checked` inserted into module or defn headers, no other changes.

- [ ] **Step 4: Parse-check the rewritten tree**

```bash
cargo test -p airl-syntax
```

Expected: all tests pass. Parse errors here mean the rewriter inserted `:verify checked` in a wrong location for some edge case — fix by revisiting `locate_after_module_name` / `locate_after_defn_name` before proceeding.

- [ ] **Step 5: Full test suite regression**

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: all pass. At this point the parser default is still `Checked`, so explicit `:verify checked` is semantically identical to the implicit default.

- [ ] **Step 6: G3 bootstrap still builds**

```bash
bash scripts/build-g3.sh
```

Expected: completes successfully (can take up to ~23 minutes). G3 is the self-hosted compiler; its bootstrap AIRL code is now explicitly `:verify checked`.

- [ ] **Step 7: Commit A (migration)**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore(migration): grandfather :verify checked on all existing modules

Mechanical rewrite by `airl verify-policy --init`. Every module and
bare top-level defn that did not already carry an explicit :verify
annotation now has `:verify checked` on its header. The repo-root
`.airl-verify-baseline.toml` lists all grandfathered entries.

This commit is semantically a no-op at the current parser default
(VerifyLevel::Checked). The default flip to Proven follows in the
next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 8 — Default flip + docs (Commit B)

### Task 8.1: Flip `VerifyLevel::default()` to `Proven`

**Files:**
- Modify: `crates/airl-syntax/src/ast.rs:63-65`
- Modify: `crates/airl-syntax/src/ast.rs:487` (existing default test)

- [ ] **Step 1: Update the default test to expect Proven**

Edit `crates/airl-syntax/src/ast.rs` — find the test asserting `VerifyLevel::default() == VerifyLevel::Checked` (around line 487) and change to:

```rust
    fn verify_level_default_is_proven() {
        assert_eq!(VerifyLevel::default(), VerifyLevel::Proven);
    }
```

- [ ] **Step 2: Run the test (expect FAIL at the new expectation)**

Run: `cargo test -p airl-syntax verify_level_default`
Expected: FAIL — default still returns `Checked`.

- [ ] **Step 3: Flip the default**

Edit `crates/airl-syntax/src/ast.rs:63-65`:

```rust
impl Default for VerifyLevel {
    fn default() -> Self { Self::Proven }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p airl-syntax verify_level_default`
Expected: PASS.

---

### Task 8.2: Remove `AIRL_COVERAGE_ENFORCE` env gate

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs` (coverage gate in `z3_verify_tops`)

- [ ] **Step 1: Delete the env gate**

Find the line `let coverage_enforce = std::env::var("AIRL_COVERAGE_ENFORCE").is_ok();` added in Task 2.3. Delete it.

In the `for (f, level) in &all_fns` loop, change the coverage gate condition from:

```rust
        if coverage_enforce
            && *level == airl_syntax::ast::VerifyLevel::Proven
            && f.is_public
            && f.ensures.is_empty()
```

to:

```rust
        if *level == airl_syntax::ast::VerifyLevel::Proven
            && f.is_public
            && f.ensures.is_empty()
```

- [ ] **Step 2: Update the env-gated tests**

The four `coverage_gate_*` tests added in Task 2.3 set/remove `AIRL_COVERAGE_ENFORCE`. Update them to drop the env-var dance:

```rust
#[test]
fn coverage_gate_fires_for_pub_fn_without_ensures() {
    let src = r#"
      (module mymod
        :verify proven
        (defn :pub foo
          :sig [(x : i64) -> i64]
          :requires [(>= x 0)]
          :body x))
      (foo 1)
    "#;
    let result = run_source_with_mode(src, PipelineMode::Check);
    match result {
        Err(PipelineError::ContractCoverageMissing { fn_name, .. }) => {
            assert_eq!(fn_name, "foo");
        }
        other => panic!("expected ContractCoverageMissing, got {:?}", other.err()),
    }
}
```

Apply the same simplification to the other three `coverage_gate_*` tests.

- [ ] **Step 3: Also update the fixture test**

Simplify `coverage_fixture_pub_fn_no_ensures` in `crates/airl-driver/tests/fixtures.rs` to drop the env-var set/remove.

- [ ] **Step 4: Run the driver tests**

Run: `cargo test -p airl-driver coverage`
Expected: all pass.

---

### Task 8.3: Full test suite + G3 bootstrap

- [ ] **Step 1: Full suite**

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: all pass. If something fails, the most likely cause is a test fixture missing explicit `:verify` that now defaults to `Proven` and fails the coverage gate. Add `:verify checked` to the fixture or mark its module explicitly.

- [ ] **Step 2: AOT tests**

```bash
rm -rf tests/aot/cache
bash tests/aot/run_aot_tests.sh
```

Expected: all 68 tests pass. (Per memory: retry individually on transient COMPILE_FAIL.)

- [ ] **Step 3: G3 bootstrap**

```bash
bash scripts/build-g3.sh
```

Expected: builds. If a bootstrap file was missed by `--init`, `run_init` has an idempotency bug — investigate before proceeding.

---

### Task 8.4: Update documentation

**Files:**
- Modify: `CLAUDE.md` (Project Instructions, around the "Conventions" block)
- Modify: `AIRL-Header.md` (LLM reference)
- Modify: `AIRL-LLM-Guide.md` (verbose guide)

- [ ] **Step 1: Update `CLAUDE.md`**

Add under "Conventions" (after "Multi-binding `let` preferred"):

```markdown
- **Default verification level is `:verify proven`.** Modules without an explicit `:verify` annotation must have provable contracts on every `:pub defn`. Grandfathered modules are listed in `.airl-verify-baseline.toml`; run `airl verify-policy` to check the baseline, `airl verify-policy --list-uncovered` to see which `:pub defn`s need `:ensures`.
```

- [ ] **Step 2: Update `AIRL-Header.md`**

Locate the `:verify` section (grep for `:verify`). Add:

```markdown
Default: `:verify proven`. Grandfather legacy modules with `:verify checked`;
list them in `.airl-verify-baseline.toml`. Use `:verify` on individual `defn`s
to override the enclosing module.
```

- [ ] **Step 3: Update `AIRL-LLM-Guide.md`**

Same substance, longer form. Add a section after the existing verification discussion explaining:
- The default is `:verify proven`.
- `:pub defn` in a proven module requires at least one `:ensures`.
- Grandfathering via baseline file.
- `airl verify-policy` subcommand.

- [ ] **Step 4: Commit B (default flip + docs)**

```bash
git add crates/airl-syntax/src/ast.rs \
        crates/airl-driver/src/pipeline.rs \
        crates/airl-driver/tests/fixtures.rs \
        CLAUDE.md AIRL-Header.md AIRL-LLM-Guide.md
git commit -m "$(cat <<'EOF'
feat(syntax): flip :verify default to proven + enable coverage gate

Parser default VerifyLevel::Checked → VerifyLevel::Proven.
Coverage gate (:pub defn must have :ensures in proven modules) is
now unconditional; removes the AIRL_COVERAGE_ENFORCE dev gate.

All existing modules carry an explicit :verify checked annotation
from the previous commit, so this change is load-bearing only for
new code authored without explicit :verify.

Docs updated.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 9 — CI integration

### Task 9.1: Add `verify-policy` step to CI

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add the step**

Find an existing job that already builds the driver. After its build step, append:

```yaml
      - name: Verify policy
        run: cargo run -p airl-driver -- verify-policy
```

- [ ] **Step 2: Local verification**

```bash
cargo run -p airl-driver -- verify-policy
```

Expected: exits 0 with "OK (N checked, M trusted, 0 stale)".

- [ ] **Step 3: Regression test (manual)**

Make a temporary change: add a new `.airl` file with `:verify checked` at an unlisted path, re-run `verify-policy`. Expected: exits 1 with a "REGRESSION" report. Revert the temp file before committing.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: enforce :verify policy via `airl verify-policy` step

CI now fails when a :verify checked or :verify trusted module is
added without a corresponding entry in .airl-verify-baseline.toml.
Ratcheting happens by removing entries from the baseline.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 10 — Finalize

### Task 10.1: Merge preparation

- [ ] **Step 1: Rebase on main**

```bash
git fetch origin
git rebase origin/main
```

- [ ] **Step 2: Full build + test one last time**

```bash
cargo build
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
rm -rf tests/aot/cache
bash tests/aot/run_aot_tests.sh
bash scripts/build-g3.sh
```

All must pass.

- [ ] **Step 3: Push the branch (DO NOT push to main)**

```bash
git push -u origin strict-enforcement-policy
```

Per project convention: user will review and merge via PR. Do not push to main.

- [ ] **Step 4: Open PR via `gh pr create`**

```bash
gh pr create --title "feat: z3 strict enforcement policy" --body "$(cat <<'EOF'
## Summary
- Flips AIRL's module verification default from `:verify checked` to `:verify proven`.
- Adds a compile-time coverage gate: `:pub defn` in `:verify proven` modules must have at least one `:ensures`.
- Introduces `.airl-verify-baseline.toml` and the `airl verify-policy` subcommand to track grandfathered modules and prevent regressions.

## Spec
`docs/superpowers/specs/2026-04-23-z3-strict-enforcement-policy-design.md`

## Test plan
- [x] `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`
- [x] `bash tests/aot/run_aot_tests.sh`
- [x] `bash scripts/build-g3.sh`
- [x] `cargo run -p airl-driver -- verify-policy` (exits 0)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review notes

**Spec coverage:**

| Spec section | Tasks |
|--------------|-------|
| Section 1 Architecture | Phases 1–9 overall |
| Section 2 Parser changes | Tasks 1.1, 1.2, 8.1 |
| Section 3 Coverage rule | Tasks 2.1–2.4, 8.2 |
| Section 4 Baseline file | Tasks 3.1–3.3 |
| Section 5 `verify-policy` subcommand | Tasks 5.1, 5.2, 6.1–6.4 |
| Section 6 `--strict` semantics | Unchanged — task 2.2 preserves the existing `strict_verify` path |
| Section 7 Migration flow | Phase 7 (Commit A), Phase 8 (Commit B), Phase 9 (CI) |
| Section 8 Testing | Tasks 2.4, 5.2, plus unit tests in every task |

**Potential gotchas:**
- Rewriter correctness depends on `Span` storing byte offsets. If `Span` is line/col, Task 6.1 needs a conversion helper before the tests pass.
- The `coverage_gate_*` tests mutate process-wide env vars. If they flake under parallel test execution, mark `#[ignore]` temporarily and surface this in review.
- G3 rebuild after Phase 7 is ~23 minutes — budget time accordingly. Per memory, `libairl_rt.a` rebuild dance applies if anything in `airl-rt/` changes (it shouldn't here).
- The `is_public` field name (not `is_pub`) is load-bearing in every test snippet. Don't typo it.
