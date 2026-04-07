# Z3 Contract Enforcement Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Z3 contract verification load-bearing: disproven contracts hard-fail in all pipeline modes, proven contracts elide their runtime assertion opcodes, `result` postconditions are actually verified, and `(valid x)` has totality semantics.

**Architecture:** Four independent changes threading through three layers — airl-solver (new ProofCache type, `valid` encoding), airl-driver/pipeline.rs (enforcement, result guard removal), and airl-runtime/bytecode_compiler.rs (opcode elision). Tasks 1 and 2 must land before Task 3 can be tested end-to-end; Tasks 3 and 4 are fully independent.

**Tech Stack:** Rust, Z3 (via z3 crate), existing `VerifyResult`/`FunctionVerification` types in airl-solver, `BytecodeCompiler` in airl-runtime, `PipelineError` in airl-driver.

---

## File Map

| File | Change |
|---|---|
| `crates/airl-solver/src/lib.rs` | Add `ProofCache` type |
| `crates/airl-driver/src/pipeline.rs` | Add `ContractDisproven` variant, populate ProofCache, hard-fail on Disproven, remove result guard, thread ProofCache to bc_compiler |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Accept `&ProofCache`, skip AssertRequires/AssertEnsures for Proven clauses |
| `crates/airl-solver/src/prover.rs` | Encode `(valid x)` as totality predicate |
| `crates/airl-driver/tests/fixtures.rs` | Add `z3_disproven_fixtures_hard_fail` test |
| `tests/fixtures/contract_disproven/bad_abs.airl` | New fixture: disproven postcondition |
| `tests/fixtures/z3_proven/valid_totality.airl` | New fixture: `(valid result)` proven for primitive return type |

---

### Task 1: Phase 2A — Hard-fail on Disproven contracts

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/tests/fixtures.rs`
- Create: `tests/fixtures/contract_disproven/bad_abs.airl`

- [ ] **Step 1: Create the disproven fixture**

Create `tests/fixtures/contract_disproven/bad_abs.airl`:

```clojure
;;Z3-DISPROVEN: bad_abs
(defn bad_abs
  :sig [(n : i64) -> i64]
  :requires [(valid n)]
  :ensures [(> result 0)]
  :body n)
```

This is disproven: when `n = 0`, `result = 0` which violates `(> result 0)`.

- [ ] **Step 2: Write the failing fixture test**

In `crates/airl-driver/tests/fixtures.rs`, add after the `extract_z3_proven` function (around line 32):

```rust
fn extract_z3_disproven(source: &str) -> Vec<String> {
    source
        .lines()
        .filter(|l| l.contains(";;Z3-DISPROVEN:"))
        .map(|l| l.split(";;Z3-DISPROVEN:").nth(1).unwrap().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn z3_disproven_fixtures_hard_fail() {
    let dir = fixtures_root().join("contract_disproven");
    if !dir.exists() {
        return;
    }
    let files = collect_airl_files(&dir);
    let mut failures = Vec::new();

    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let disproven_names = extract_z3_disproven(&source);
        if disproven_names.is_empty() {
            continue;
        }

        match airl_driver::pipeline::run_source(&source) {
            Err(airl_driver::pipeline::PipelineError::ContractDisproven { ref fn_name, .. }) => {
                if !disproven_names.contains(fn_name) {
                    failures.push(format!(
                        "{}: expected disproven fn in {:?}, got '{}'",
                        file.display(), disproven_names, fn_name
                    ));
                }
            }
            Ok(_) => failures.push(format!(
                "{}: expected ContractDisproven error, got Ok",
                file.display()
            )),
            Err(e) => failures.push(format!(
                "{}: expected ContractDisproven error, got {:?}",
                file.display(), e
            )),
        }
    }

    if !failures.is_empty() {
        panic!("Z3 disproven fixture failures:\n{}", failures.join("\n"));
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
cargo test -p airl-driver z3_disproven_fixtures_hard_fail 2>&1 | tail -20
```

Expected: FAIL — `ContractDisproven` variant doesn't exist yet.

- [ ] **Step 4: Add `ContractDisproven` variant to `PipelineError`**

In `crates/airl-driver/src/pipeline.rs`, the `PipelineError` enum is at lines 963-970. Add the new variant:

```rust
#[derive(Debug)]
pub enum PipelineError {
    Io(String),
    Syntax(Diagnostic),
    Parse(Diagnostics),
    TypeCheck(Diagnostics),
    Runtime(RuntimeError),
    ContractDisproven {
        fn_name: String,
        clause: String,
        counterexample: Option<String>,
    },
}
```

- [ ] **Step 5: Add Display impl for the new variant**

In the `Display` impl for `PipelineError` (lines 972-991), add the new arm:

```rust
PipelineError::ContractDisproven { fn_name, clause, counterexample } => {
    write!(f, "Contract disproven in `{}`: {}", fn_name, clause)?;
    if let Some(ce) = counterexample {
        write!(f, " (counterexample: {})", ce)?;
    }
    Ok(())
}
```

- [ ] **Step 6: Make the Disproven match arm return Err in all modes**

In `pipeline.rs`, the Z3 verification loop is at lines 148-181. Replace the `Disproven` arm:

**Before:**
```rust
airl_solver::VerifyResult::Disproven { counterexample } => {
    if clause.contains("result") {
        eprintln!("note: postcondition referencing 'result' in `{}` is checked at runtime only (static verification not yet supported)", f.name);
    } else {
        let msg = format!("contract disproven in `{}`: {} (counterexample: {:?})",
            f.name, clause, counterexample);
        match mode {
            PipelineMode::Check => eprintln!("error: {}", msg),
            _ => eprintln!("warning: {}", msg),
        }
    }
}
```

**After:**
```rust
airl_solver::VerifyResult::Disproven { counterexample } => {
    let ce_str = if counterexample.is_empty() {
        None
    } else {
        Some(counterexample.iter()
            .map(|(k, v)| format!("{} = {}", k, v))
            .collect::<Vec<_>>()
            .join(", "))
    };
    return Err(PipelineError::ContractDisproven {
        fn_name: f.name.clone(),
        clause: clause.clone(),
        counterexample: ce_str,
    });
}
```

- [ ] **Step 7: Run the test to verify it passes**

```bash
cargo test -p airl-driver z3_disproven_fixtures_hard_fail 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 8: Run full driver test suite to check for regressions**

```bash
cargo test -p airl-driver 2>&1 | tail -20
```

Expected: all tests pass. If any `z3_proven` fixtures now fail with `ContractDisproven`, those fixtures have genuinely disproven contracts — investigate before proceeding.

- [ ] **Step 9: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs \
        crates/airl-driver/tests/fixtures.rs \
        tests/fixtures/contract_disproven/bad_abs.airl
git commit -m "feat(z3): hard-fail on Disproven contracts in all pipeline modes (Phase 2A)

Adds PipelineError::ContractDisproven and makes Disproven a hard Err
in Run, Check, and Repl modes. Adds z3_disproven fixture harness and
bad_abs fixture.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Phase 2B — ProofCache and runtime check elision

**Files:**
- Modify: `crates/airl-solver/src/lib.rs`
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-runtime/src/bytecode_compiler.rs`

- [ ] **Step 1: Add `ProofCache` to airl-solver**

In `crates/airl-solver/src/lib.rs`, add after the `FunctionVerification` impl block:

```rust
/// Maps function name → clause text → proof result.
/// Built during the Z3 pass and passed to the bytecode compiler
/// so proven clauses can have their runtime assertion opcodes elided.
#[derive(Debug, Clone, Default)]
pub struct ProofCache {
    pub results: std::collections::HashMap<String, std::collections::HashMap<String, VerifyResult>>,
}

impl ProofCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, fn_name: &str, clause: &str, result: VerifyResult) {
        self.results
            .entry(fn_name.to_string())
            .or_default()
            .insert(clause.to_string(), result);
    }

    pub fn is_proven(&self, fn_name: &str, clause: &str) -> bool {
        matches!(
            self.results.get(fn_name).and_then(|m| m.get(clause)),
            Some(VerifyResult::Proven)
        )
    }
}
```

- [ ] **Step 2: Verify airl-solver compiles**

```bash
cargo build -p airl-solver 2>&1 | tail -5
```

Expected: compiles cleanly.

- [ ] **Step 3: Populate ProofCache in pipeline.rs Z3 loop**

In `pipeline.rs`, in `run_source_with_mode` (and the equivalent positions in `run_file_with_preloads` and `run_repl_input`), build the ProofCache during the Z3 loop. The Z3 loop is at lines 148-181. Replace the loop body:

```rust
// Z3 contract verification
let z3_prover = airl_solver::prover::Z3Prover::new();
let mut proof_cache = airl_solver::ProofCache::new();
for top in &tops {
    if let airl_syntax::ast::TopLevel::Defn(f) = top {
        let verification = z3_prover.verify_function(f);
        for (clause, result) in &verification.ensures_results {
            match result {
                airl_solver::VerifyResult::Proven => {
                    proof_cache.insert(&f.name, clause, airl_solver::VerifyResult::Proven);
                    if mode == PipelineMode::Check {
                        eprintln!("note: `{}` contract proven: {}", f.name, clause);
                    }
                }
                airl_solver::VerifyResult::Disproven { counterexample } => {
                    let ce_str = if counterexample.is_empty() {
                        None
                    } else {
                        Some(counterexample.iter()
                            .map(|(k, v)| format!("{} = {}", k, v))
                            .collect::<Vec<_>>()
                            .join(", "))
                    };
                    return Err(PipelineError::ContractDisproven {
                        fn_name: f.name.clone(),
                        clause: clause.clone(),
                        counterexample: ce_str,
                    });
                }
                airl_solver::VerifyResult::Unknown(_)
                | airl_solver::VerifyResult::TranslationError(_) => {
                    // Silent — fall back to runtime checking
                }
            }
        }
    }
}
```

Then pass `proof_cache` to `compile_program_with_contracts` (updated in Step 5). The `compile_tops_with_contracts` call is unchanged — it doesn't need ProofCache.

Update the call at line 190 (and equivalent locations):
```rust
let (funcs, main_func) = bc_compiler.compile_program_with_contracts(&ir_nodes, &contracts, &proof_cache);
```

- [ ] **Step 4: Update `compile_program_with_contracts` signature in bytecode_compiler.rs**

The function is at line 974. Change signature:

```rust
pub fn compile_program_with_contracts(
    &mut self,
    nodes: &[IRNode],
    contracts: &HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
    proof_cache: &airl_solver::ProofCache,
) -> (Vec<BytecodeFunc>, BytecodeFunc)
```

Thread `proof_cache` through to the private `compile_function_with_contracts` call inside this function.

- [ ] **Step 5: Update `compile_function_with_contracts` to elide proven opcodes**

The private helper is at line 910. Change signature:

```rust
fn compile_function_with_contracts(
    compiler: &mut Self,
    fn_name: &str,
    body_nodes: &[IRNode],
    requires_clauses: &[(IRNode, String)],
    ensures_clauses: &[(IRNode, String)],
    invariant_clauses: &[(IRNode, String)],
    proof_cache: &airl_solver::ProofCache,
) -> BytecodeFunc
```

For the AssertRequires emission (around line 928), wrap with proof check:

```rust
for (clause_ir, clause_src) in requires_clauses {
    if proof_cache.is_proven(fn_name, clause_src) {
        continue; // Proven — no runtime check needed
    }
    let bool_reg = compiler.compile_expr(clause_ir);
    let fn_name_idx = compiler.add_constant(Value::Str(fn_name.to_string()));
    let clause_src_idx = compiler.add_constant(Value::Str(clause_src.clone()));
    compiler.emit(Op::AssertRequires, fn_name_idx, bool_reg, clause_src_idx);
}
```

For the AssertEnsures emission (around line 949), same pattern:

```rust
for (clause_ir, clause_src) in ensures_clauses {
    if proof_cache.is_proven(fn_name, clause_src) {
        continue; // Proven — no runtime check needed
    }
    let bool_reg = compiler.compile_expr(clause_ir);
    let fn_name_idx = compiler.add_constant(Value::Str(fn_name.to_string()));
    let clause_src_idx = compiler.add_constant(Value::Str(clause_src.clone()));
    compiler.emit(Op::AssertEnsures, fn_name_idx, bool_reg, clause_src_idx);
}
```

- [ ] **Step 6: Verify the project builds**

```bash
cargo build --features aot 2>&1 | tail -10
```

Expected: clean build. Fix any type errors from the signature changes.

- [ ] **Step 7: Run full driver tests**

```bash
cargo test -p airl-driver 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/airl-solver/src/lib.rs \
        crates/airl-driver/src/pipeline.rs \
        crates/airl-runtime/src/bytecode_compiler.rs
git commit -m "feat(z3): elide runtime assertion opcodes for proven contracts (Phase 2B)

Adds ProofCache to airl-solver, populates it during the Z3 pass, and
threads it into BytecodeCompiler so proven :requires/:ensures clauses
emit no AssertRequires/AssertEnsures opcodes.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Remove `result` postcondition skip guard

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Create: `tests/fixtures/z3_proven/valid_result_postcondition.airl`

The body translation that verifies `result` postconditions was already implemented in commit `b8cd4c5`. The only barrier is a guard in the pipeline that skips clauses referencing `result` before they reach the solver.

- [ ] **Step 1: Create a fixture that exercises result postcondition verification**

Create `tests/fixtures/z3_proven/valid_result_postcondition.airl`:

```clojure
;;Z3-PROVEN: double_positive
(defn double_positive
  :sig [(n : i64) -> i64]
  :requires [(> n 0)]
  :ensures [(> result 0)]
  :body (* n 2))
```

Z3 can prove: if `n > 0` then `n * 2 > 0`. This should become `Proven` once the guard is removed.

- [ ] **Step 2: Run the fixture test to confirm it currently fails**

```bash
cargo test -p airl-driver z3_proven_fixtures_all_pass 2>&1 | grep -A5 "valid_result_postcondition\|FAILED\|failure"
```

Expected: the `double_positive` function is not in the proven list (the guard skips it).

- [ ] **Step 3: Remove the `result` skip guard in pipeline.rs**

In `pipeline.rs`, find and remove this block (around line 164 — it was inside the `Disproven` arm in the original code, but after Task 1 it is now in the `Proven` arm or in a pre-check before the match). Search for the string `"result postconditions require"` or `"checked at runtime only"`:

The guard appears as a pre-check before calling the solver, or inside the `Unknown` return from the solver when `clause_references_result` is true. In `prover.rs` at lines ~147-155:

```rust
// REMOVE this block in prover.rs:
if clause_references_result(ensures_expr) && !body_translated {
    results.push((clause_source, VerifyResult::Unknown(
        "result postconditions require body translation".to_string()
    )));
    continue;
}
```

Also remove from `pipeline.rs` any remaining `clause.contains("result")` check in the `Proven` display arm if it still exists after Task 1.

- [ ] **Step 4: Run the fixture test to verify it now passes**

```bash
cargo test -p airl-driver z3_proven_fixtures_all_pass 2>&1 | tail -15
```

Expected: PASS, including `double_positive`.

- [ ] **Step 5: Run full driver tests**

```bash
cargo test -p airl-driver 2>&1 | tail -20
```

Expected: all pass. If any existing fixtures now produce `ContractDisproven` because their `result` postconditions are genuinely disproven, investigate — those functions have real bugs.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs \
        crates/airl-solver/src/prover.rs \
        tests/fixtures/z3_proven/valid_result_postcondition.airl
git commit -m "feat(z3): connect body translation for result postcondition verification

Removes the guard that skipped :ensures clauses referencing 'result'.
Body translation (b8cd4c5) now fully participates in verification.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 4: `(valid x)` totality semantics

**Files:**
- Modify: `crates/airl-solver/src/prover.rs`
- Create: `tests/fixtures/z3_proven/valid_totality.airl`

Currently `(valid x)` is likely translated as an unknown function call, returning `TranslationError` → silent fallback to runtime. This task gives it precise totality semantics so it returns `Proven` for all value types.

- [ ] **Step 1: Create the fixture**

Create `tests/fixtures/z3_proven/valid_totality.airl`:

```clojure
;;Z3-PROVEN: identity_i64
(defn identity_i64
  :sig [(n : i64) -> i64]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body n)

;;Z3-PROVEN: make_string
(defn make_string
  :sig [(n : i64) -> String]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (int-to-string n))
```

Both should be `Proven` after this task: `(valid result)` for `i64` and `String` return types is always true.

- [ ] **Step 2: Run the fixture test to confirm it currently fails**

```bash
cargo test -p airl-driver z3_proven_fixtures_all_pass 2>&1 | grep -A5 "valid_totality\|FAILED\|not.*verified"
```

Expected: `identity_i64` and `make_string` not in the proven list.

- [ ] **Step 3: Add `(valid x)` encoding in prover.rs**

In `crates/airl-solver/src/prover.rs`, find the `translate_bool` or expression translation function. Add a special case for `(valid x)` calls.

Search for where `FnCall` expressions are translated (look for `"valid"` string or the FnCall match arm). Add:

```rust
// In the expression translator, when handling FnCall with name "valid":
Expr::FnCall { name, args, .. } if name == "valid" => {
    // (valid x) is a totality predicate — always true for value types.
    // For all primitive types (i64, f64, Bool, String, List, Result variants),
    // any well-typed value is valid by construction.
    // Return Z3 `true` — this will be Proven by the solver.
    Ok(ctx.from_bool(true))
}
```

This makes `(valid x)` translate to the Z3 boolean `true`, which the solver will immediately prove (asserting `¬true` is unsatisfiable).

- [ ] **Step 4: Run the fixture test to verify it passes**

```bash
cargo test -p airl-driver z3_proven_fixtures_all_pass 2>&1 | tail -15
```

Expected: PASS, including `identity_i64` and `make_string`.

- [ ] **Step 5: Run the full AOT test suite to verify no behavioral regression**

The ~150 `:ensures [(valid result)]` annotations in bootstrap are now `Proven` → their runtime `AssertEnsures` opcodes are elided. Verify behavior is unchanged:

```bash
rm -rf tests/aot/cache
bash tests/aot/run_aot_tests.sh 2>&1 | tail -10
```

Expected: same pass/fail counts as before. If any tests now fail, the elided assertion was masking a real bug — investigate.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-solver/src/prover.rs \
        tests/fixtures/z3_proven/valid_totality.airl
git commit -m "feat(z3): (valid x) totality semantics — proven for all value types

Encodes (valid x) as Z3 true for all well-typed values. Combined with
Phase 2B elision, the ~150 :ensures [(valid result)] annotations across
the bootstrap become free proofs with no runtime cost.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```
