# Z3 Contract Enforcement — Phase 2A/2B Implementation Spec

**Date:** 2026-04-14
**Supersedes:** `2026-04-07-z3-enforcement-design.md` (approved but never implemented)
**Status:** Draft
**Scope:** Make Z3 contract verification load-bearing — disproven contracts block execution, proven contracts elide runtime assertion opcodes.

## Background

Phase 1 (2026-03-21) established Z3 as informational only. A Phase 2 design was approved on 2026-04-07 but **never implemented** — the `ContractDisproven` error variant and `ProofCache` type do not exist in the codebase. Z3 results are still discarded after printing to stderr.

As of 2026-04-14, Z3 verifies `:requires`, `:ensures`, AND `:invariant` clauses. All three clause types need enforcement and elision support.

## Current Behavior (verified 2026-04-14)

| Z3 result | Pipeline action | Runtime |
|-----------|----------------|---------|
| `Proven` | `eprintln!("note: ...")` in Check mode only; discarded in Run | Opcode always emitted and executed |
| `Disproven` | `eprintln!("warning/error: ...")`, execution continues | Opcode always emitted and executed |
| `Unknown` | Warning if first for function | Opcode always emitted and executed |

**Source:** `crates/airl-driver/src/pipeline.rs` lines 172-201 (Run), lines 340-361 (Check).

**Additional issue:** The `result` false-positive guard (`clause.contains("result")`) at line 183 suppresses disproven reports for postconditions referencing `result`, even though body translation (commit `b8cd4c5`) already binds `result` correctly. This guard should be removed.

## Target Behavior

| Z3 result | Pipeline action | Runtime |
|-----------|----------------|---------|
| `Proven` | Note in Check mode | **Opcode NOT emitted** |
| `Disproven` | **Hard error — pipeline returns Err** | N/A (never reached) |
| `Unknown` | Warning | Opcode emitted and executed |
| `TranslationError` | Warning | Opcode emitted and executed |

## Changes Required

### Phase 2A: Hard Error on Disproven

**File:** `crates/airl-driver/src/pipeline.rs`

#### 1. Add error variant

```rust
// In PipelineError enum:
ContractDisproven {
    fn_name: String,
    clause: String,
    counterexample: Vec<(String, String)>,
},
```

Add Display impl: `"contract disproven in `{fn_name}`: {clause} (counterexample: {counterexample:?})"`.

#### 2. Update Run mode Z3 block (lines 172-201)

Replace the `Disproven` arm:

```rust
// Before:
airl_solver::VerifyResult::Disproven { counterexample } => {
    if clause.contains("result") {
        eprintln!("note: postcondition referencing 'result'...");
    } else {
        let msg = format!("contract disproven in `{}`...", f.name);
        match mode {
            PipelineMode::Check => eprintln!("error: {}", msg),
            _ => eprintln!("warning: {}", msg),
        }
    }
}

// After:
airl_solver::VerifyResult::Disproven { counterexample } => {
    return Err(PipelineError::ContractDisproven {
        fn_name: f.name.clone(),
        clause: clause.clone(),
        counterexample: counterexample.clone(),
    });
}
```

Remove the `clause.contains("result")` guard entirely — body translation handles `result` correctly.

#### 3. Update Check mode Z3 block (lines 340-361)

Same change: `Disproven` → `return Err(PipelineError::ContractDisproven { ... })`.

#### 4. Update error formatting

In the driver's error display code, format `ContractDisproven` with source context (function name, clause text, counterexample values).

### Phase 2B: Opcode Elision for Proven Contracts

**File:** `crates/airl-solver/src/lib.rs`

#### 1. Add ProofCache type

```rust
use std::collections::HashMap;

/// Cache of Z3 verification results, keyed by function name then clause source text.
/// Passed to the bytecode compiler so proven contracts skip opcode emission.
pub struct ProofCache {
    results: HashMap<String, HashMap<String, VerifyResult>>,
}

impl ProofCache {
    pub fn new() -> Self {
        Self { results: HashMap::new() }
    }

    pub fn insert(&mut self, fn_name: &str, clause: &str, result: VerifyResult) {
        self.results
            .entry(fn_name.to_string())
            .or_default()
            .insert(clause.to_string(), result);
    }

    pub fn is_proven(&self, fn_name: &str, clause: &str) -> bool {
        self.results.get(fn_name)
            .and_then(|m| m.get(clause))
            .map_or(false, |r| matches!(r, VerifyResult::Proven))
    }
}
```

**File:** `crates/airl-driver/src/pipeline.rs`

#### 2. Populate ProofCache during Z3 pass

After the Z3 verification loop, build a `ProofCache` from all results:

```rust
let mut proof_cache = airl_solver::ProofCache::new();
for top in &tops {
    if let TopLevel::Defn(f) = top {
        let verification = z3_prover.verify_function(f);
        for (clause, result) in &verification.ensures_results {
            proof_cache.insert(&f.name, clause, result.clone());
            // ... existing reporting logic ...
        }
        for (clause, result) in &verification.invariants_results {
            proof_cache.insert(&f.name, clause, result.clone());
            // ... existing reporting logic ...
        }
    }
}
```

#### 3. Pass ProofCache to bytecode compiler

`compile_tops_with_contracts()` receives `&ProofCache`. Thread it to `BytecodeCompiler`.

**File:** `crates/airl-runtime/src/bytecode_compiler.rs`

#### 4. Skip opcode emission for proven clauses

In the requires/ensures/invariant compilation sections (lines ~900-950), before emitting `Op::AssertRequires`/`Op::AssertEnsures`/`Op::AssertInvariant`:

```rust
// Check proof cache — skip opcode if Z3 proved this clause
if proof_cache.is_proven(&fn_name, &clause_source) {
    continue; // proven statically, no runtime check needed
}
compiler.emit(Op::AssertRequires, fn_name_idx, bool_reg, clause_src_idx);
```

### Remove `result` Guard

**File:** `crates/airl-driver/src/pipeline.rs` line 183

Delete the `if clause.contains("result")` branch entirely. Body translation (commit `b8cd4c5`) correctly binds `result` to the function body's return value. The guard was a Phase 1 workaround that is no longer needed.

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-driver/src/pipeline.rs` | Add `ContractDisproven` variant; hard-error on Disproven; build and pass `ProofCache`; remove `result` guard |
| `crates/airl-solver/src/lib.rs` | Add `ProofCache` type |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Accept `&ProofCache`; skip opcode emission for proven clauses |
| `crates/airl-driver/src/error.rs` | Format `ContractDisproven` error with source context |

## Testing

### Phase 2A

**New fixture category:** `tests/fixtures/contract_disproven/`

Each `.airl` file has a contract Z3 can disprove, annotated `;;EXPECT-ERROR: ContractDisproven` on line 1. The fixture harness asserts `run_source()` returns `Err(PipelineError::ContractDisproven { .. })`.

Example:
```clojure
;;EXPECT-ERROR: ContractDisproven
(defn bad-add
  :sig [(a : i32) (b : i32) -> i32]
  :ensures [(= result (* a b))]
  :body (+ a b))
(bad-add 1 2)
```

### Phase 2B

Existing `;;Z3-PROVEN:` fixtures verify elision doesn't cause behavioral regressions. After elision, these functions should produce identical results but with fewer bytecode instructions.

### Regression

Run full test suite: `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`

Run AOT tests: `rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh`

## Invariants Preserved

- `Unknown` always falls back to runtime — no regression
- Recursive functions unaffected (remain `Unknown`)
- Existing `;;Z3-PROVEN:` fixture annotations become load-bearing (elision)
- `TranslationError` treated same as `Unknown` (runtime backstop)
