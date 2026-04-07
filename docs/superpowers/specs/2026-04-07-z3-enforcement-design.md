# Z3 Contract Enforcement — Phase 2 Design

**Date:** 2026-04-07
**Status:** Approved
**Scope:** Make Z3 contract verification load-bearing — disproven contracts block execution, proven contracts elide runtime checks, `result` postconditions are verified, `(valid x)` has totality semantics.

## Background

Phase 1 (2026-03-21-z3-integration-design.md) established Z3 as informational only: `Disproven` prints a warning and execution continues; `Proven` prints a note and is discarded. Runtime `AssertRequires`/`AssertEnsures` opcodes are the actual enforcement, running unconditionally regardless of proof results. The two systems — static Z3 and runtime assertions — do not talk to each other.

This design closes that gap. After Phase 2, Z3 results have real consequences at both ends:
- `Disproven` → hard error, program does not compile or run
- `Proven` → runtime check elided, no assertion opcode emitted
- `Unknown` → runtime assertion runs (intentional fallback, not a gap)

## Goals

1. **2A — Enforcement:** `Disproven` is a hard error in all pipeline modes
2. **2B — Elision:** `Proven` causes the corresponding runtime opcode to be skipped
3. **`result` postconditions:** Remove the skip guard; body translation (already implemented) handles them
4. **`(valid x)` semantics:** Totality predicate — auto-proven for all value and Result types

## Non-Goals

- Recursive function body verification (falls to `Unknown` → runtime; no change needed)
- `--force` or escape-hatch flags for disproven contracts
- Loop invariant synthesis
- Verification of builtin function contracts

## Architecture

### Phase 2A: Pipeline Enforcement

**File:** `crates/airl-driver/src/pipeline.rs`

Change the `Disproven` match arm from `eprintln!` to `return Err(PipelineError::ContractDisproven(...))` in **all** `PipelineMode` variants (`Check`, `Run`, `Repl`).

| Z3 result | Before | After |
|---|---|---|
| `Proven` | note in check mode | unchanged |
| `Disproven` | warning/error eprintln, continues | `Err(ContractDisproven)` always |
| `Unknown` | silent | silent (unchanged) |
| `TranslationError` | silent | silent (unchanged) |

Error message includes: function name, disproven clause text, Z3 counterexample.

**New error variant** in `PipelineError`:
```rust
ContractDisproven {
    fn_name: String,
    clause: String,
    counterexample: Option<String>,
}
```

### Phase 2B: Runtime Check Elision

**New type — `ProofCache`** (`crates/airl-solver/src/lib.rs`):
```rust
pub struct ProofCache {
    // fn_name → clause_text → VerifyResult
    pub results: HashMap<String, HashMap<String, VerifyResult>>,
}
```

Populated during the Z3 pass in `pipeline.rs`, passed into `compile_tops_with_contracts`.

**Bytecode compiler change** (`crates/airl-runtime/src/bytecode_compiler.rs`):

`compile_tops_with_contracts` receives a `&ProofCache`. When emitting `:requires` or `:ensures` clauses, check the cache:
- `Proven` → skip `AssertRequires` / `AssertEnsures` opcode
- `Disproven` → unreachable (blocked by 2A)
- `Unknown` / absent → emit opcode as today

No changes to the VM, no new opcodes, no runtime tracking. Elision is purely compile-time.

**Data flow:**
```
Z3 pass → ProofCache
              ↓
          compile_tops_with_contracts(tops, &proof_cache)
              ↓
          BytecodeCompiler: Proven clauses → no opcode emitted
              ↓
          VM: only Unknown clauses run at runtime
```

### `result` Postcondition Verification

**File:** `crates/airl-driver/src/pipeline.rs:164`

Remove the guard:
```rust
// REMOVE this entire branch:
if clause.contains("result") {
    eprintln!("note: postcondition referencing 'result'...");
    // falls through without checking
}
```

Body translation was implemented in commit `b8cd4c5`. The solver already binds `result` to the function body's return value and checks the postcondition. Removing the guard connects the pipeline to the existing implementation.

Recursive functions time out and return `Unknown` → runtime backstop. No special handling needed.

### `(valid x)` Semantics

**File:** `crates/airl-solver/src/prover.rs`

`(valid x)` is a totality predicate. Z3 encoding by declared type:

| Type | Z3 encoding | Provable |
|---|---|---|
| `i64`, `f64`, `Bool` | `true` | Always |
| `String` | `true` | Always |
| `Result<T>` | `is-Ok(x) ∨ is-Err(x)` | Always |
| `List` | `true` | Always |
| User variant type | disjunction over all constructors | Always for closed types |
| Unknown / recursive | `Unknown` | Never — runtime backstop |

All `:ensures [(valid result)]` and `:requires [(valid param)]` annotations on non-recursive functions with value/Result/List return types become `Proven` automatically, causing their runtime opcodes to be elided (2B).

No new syntax. `(valid x)` is already parsed; this is a change to the Z3 translation layer only.

## Test Plan

**New fixture category:** `tests/fixtures/contract_disproven/`

Each fixture is a `.airl` file with a `:ensures` clause that Z3 can disprove, annotated `;;Z3-DISPROVEN: <fn_name>` on the first line (mirroring `;;Z3-PROVEN: <fn_name>`). The driver test asserts the pipeline returns `Err(ContractDisproven { fn_name, .. })` and exits non-zero, not a runtime panic.

**Existing fixtures:** All `;;Z3-PROVEN:` fixtures must continue to pass. Their runtime opcodes are now elided — verify no behavioral regression by running the AOT test suite.

**`valid` semantics:** Add fixtures for functions with `:ensures [(valid result)]` returning `i64`, `String`, `Result`, and `List` — all should be annotated `;;Z3-PROVEN:` after this change.

## Invariants Preserved

- `Unknown` always falls back to runtime — no regression for contracts Z3 can't decide
- Recursive functions are unaffected — they were `Unknown` before and remain `Unknown`
- The `;;Z3-PROVEN:` fixture annotation (added in commit `854212a`) continues to work; 2B makes these annotations load-bearing rather than informational
