# `:verify` Module Level Enforcement — Design Spec

**Date:** 2026-04-14
**Status:** Draft
**Scope:** Make the `VerifyLevel` enum (`Checked`, `Proven`, `Trusted`) on `ModuleDef` control Z3 and runtime contract behavior per-module.

## Background

The parser recognizes `:verify checked | proven | trusted` on module definitions. The `VerifyLevel` enum is defined in `crates/airl-syntax/src/ast.rs` (lines 56-65) and stored on `ModuleDef.verify` (line 43). Default is `Checked`.

**Current state:** No code in the pipeline reads `ModuleDef.verify`. All functions receive identical Z3 treatment regardless of module annotation. The enum is parsed and discarded.

## Intended Semantics

| Level | Z3 behavior | Runtime behavior | Use case |
|-------|------------|-----------------|----------|
| `Checked` | Z3 runs; `Unknown` → runtime assertion | All contract opcodes emitted | Default — standard verification |
| `Proven` | Z3 runs; `Unknown` → **hard error** (must be provable) | Proven → elided; `Unknown` rejected | High-assurance modules — every contract must be statically proven |
| `Trusted` | Z3 **skipped** entirely | **No contract opcodes emitted** | FFI wrappers, performance-critical inner loops, bootstrap code |

### Key distinction

- `Checked`: Z3 is best-effort. Anything it can't prove falls to runtime. This is the current behavior for all code.
- `Proven`: Z3 is mandatory. If Z3 returns `Unknown` or `TranslationError`, the module fails to compile. This forces authors to write contracts Z3 can verify.
- `Trusted`: All verification is skipped. The module's contracts are assumed correct. Dangerous — should be used sparingly for code whose correctness is established by other means (e.g., TLA+ verified runtime primitives).

## Changes Required

### 1. Thread VerifyLevel through the pipeline

**File:** `crates/airl-driver/src/pipeline.rs`

During the Z3 verification loop, determine the `VerifyLevel` for each function by finding its enclosing module (or defaulting to `Checked` for top-level functions):

```rust
fn verify_level_for_fn(fn_name: &str, tops: &[TopLevel]) -> VerifyLevel {
    for top in tops {
        if let TopLevel::Module(m) = top {
            for item in &m.body {
                if let TopLevel::Defn(f) = item {
                    if f.name == fn_name {
                        return m.verify;
                    }
                }
            }
        }
    }
    VerifyLevel::Checked // default for top-level functions
}
```

### 2. Apply VerifyLevel in Z3 pass

```rust
for top in &tops {
    if let TopLevel::Defn(f) = top {
        let level = verify_level_for_fn(&f.name, &tops);

        match level {
            VerifyLevel::Trusted => continue, // skip Z3 entirely

            VerifyLevel::Proven => {
                let verification = z3_prover.verify_function(f);
                for (clause, result) in verification.ensures_results.iter()
                    .chain(verification.invariants_results.iter())
                {
                    match result {
                        VerifyResult::Proven => { /* ok */ }
                        VerifyResult::Unknown(reason) => {
                            return Err(PipelineError::ContractUnprovable {
                                fn_name: f.name.clone(),
                                clause: clause.clone(),
                                reason: reason.clone(),
                            });
                        }
                        VerifyResult::TranslationError(msg) => {
                            return Err(PipelineError::ContractUnprovable {
                                fn_name: f.name.clone(),
                                clause: clause.clone(),
                                reason: msg.clone(),
                            });
                        }
                        VerifyResult::Disproven { .. } => {
                            // Same as Phase 2A hard error
                            return Err(PipelineError::ContractDisproven { ... });
                        }
                    }
                }
            }

            VerifyLevel::Checked => {
                // Current behavior — Z3 runs, Unknown falls to runtime
                let verification = z3_prover.verify_function(f);
                // ... existing reporting and proof_cache logic ...
            }
        }
    }
}
```

### 3. Apply VerifyLevel in bytecode compilation

For `Trusted` modules, skip emitting `AssertRequires`/`AssertEnsures`/`AssertInvariant` opcodes entirely — even if Z3 was not consulted.

### 4. Add error variant

```rust
// In PipelineError:
ContractUnprovable {
    fn_name: String,
    clause: String,
    reason: String,
},
```

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-driver/src/pipeline.rs` | Thread `VerifyLevel`; differentiate Z3 behavior per level; add `ContractUnprovable` variant |
| `crates/airl-runtime/src/bytecode_compiler.rs` | Skip contract opcodes for `Trusted` modules |
| `crates/airl-driver/src/error.rs` | Format `ContractUnprovable` error |

## Testing

New fixtures:
- `tests/fixtures/valid/module_verified_proven.airl` — module with `:verify proven` and provable contracts → passes
- `tests/fixtures/contract_errors/module_unprovable.airl` — module with `:verify proven` and an `Unknown` contract → compile error
- `tests/fixtures/valid/module_trusted.airl` — module with `:verify trusted` and deliberate contract violations → runs (no checking)

## Dependencies

- Phase 2A (hard error on Disproven) should land first — `:verify proven` builds on the same error reporting infrastructure
- Phase 2B (ProofCache / opcode elision) should land first — `:verify trusted` uses the same "skip opcode" mechanism
