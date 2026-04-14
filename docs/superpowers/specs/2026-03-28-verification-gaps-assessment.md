# AIRL Verification Gaps — Assessment

**Date:** 2026-03-28 (original), **Updated:** 2026-04-14
**Status:** Revised — verified against source code on 2026-04-14

---

## Resolved Since Original Assessment

### Channel Bugs (TLA+-confirmed) — FIXED

All three TLC-confirmed channel bugs are implemented in `crates/airl-rt/src/thread.rs`:
- **Concurrent recv race (HIGH):** `SharedReceiver = Arc<Mutex<Receiver>>` (line 30); recv uses `.get().cloned()` not `.remove()` (line 211). Test at line 404.
- **Message loss on close (MEDIUM):** `channel_close` drains buffered messages with `try_recv()` + `airl_value_release` (lines 304-315).
- **Send-after-close ambiguity (LOW):** `closed_handles()` HashSet (lines 60-63) checked in all channel operations. Test at line 359.

### TLA+ CI Coverage — FIXED (2026-04-14)

All three TLA+ specs now run in CI (`.github/workflows/ci.yml`):
- `airl_channels.tla` — channel protocol invariants (was already in CI)
- `airl_memory.tla` — retain/release/immortal safety (added 2026-04-14)
- `airl_cow.tla` — COW fast-path atomicity (added 2026-04-14)

### Z3 Invariant Verification — FIXED (2026-04-14)

`:invariant` clauses are now verified by Z3 with the same negate-and-check strategy as `:ensures`. Previously they compiled to `AssertInvariant` bytecode only (runtime enforcement) with no static proof path.

---

## 1. Z3 Verification Is Still Informational Only

**Still confirmed.** Despite the Phase 2 design spec (`2026-04-07-z3-enforcement-design.md`) being marked "Approved," Phase 2A and 2B are **not implemented**:

- `ContractDisproven` error variant does not exist in `crates/airl-driver/src/pipeline.rs`
- `ProofCache` type does not exist anywhere in the codebase
- Disproven contracts print warnings/errors via `eprintln!()` but never return `Err`
- No runtime opcode elision for proven contracts

| Mode | Disproven contract | Effect |
|------|-------------------|--------|
| `check` | Prints `"error: contract disproven..."` to stderr | Continues — no hard error |
| `run` | Prints `"warning: contract disproven..."` to stderr | Continues |

**Source:** `crates/airl-driver/src/pipeline.rs` lines 172-201 (Run mode), lines 340-361 (Check mode).

**Runtime assertions remain the sole enforcement.** `AssertRequires`/`AssertEnsures`/`AssertInvariant` opcodes always execute unconditionally.

**Note:** The `result` false-positive guard (`clause.contains("result")`) is still present (line 183), suppressing postcondition disproven reports even though body translation is implemented.

---

## 2. Linearity Checking: Always-On But Lenient

**Corrected from original "opt-in" claim.** The linearity checker runs on ALL functions, not just those with explicit `Own`/`Ref`/`Mut` annotations.

**Evidence:**
- `crates/airl-driver/src/pipeline.rs`: `LinearityChecker` is created unconditionally for every compile
- `crates/airl-types/src/linearity.rs` line 204-206: Parameters are introduced into the checker without checking ownership annotation

**However:** Default ownership is permissive — the checker tracks but does not error for default-ownership parameters. Functions without explicit annotations pass linearity checking trivially. Linearity errors are fatal only in `Check` mode; in `Run`/`Repl` they're warnings.

**Implication:** The infrastructure is always running, but default-ownership code gets no meaningful linearity enforcement.

---

## 3. Type Checking Is Incomplete for Builtins

**Still confirmed — worse than originally assessed.**

| Category | Count | Type info |
|----------|-------|-----------|
| Properly typed (arithmetic ops) | 11 | `Ty::Func` with param/return types |
| Properly typed (collection ops) | 8 | `head`, `tail`, `cons`, `empty?`, `map`, `filter`, `fold`, `str` |
| Untyped builtins | 100+ | `TypeVar("builtin")` — no validation |

**Source:** `crates/airl-types/src/checker.rs` lines 68-153 (untyped), lines 180-255 (typed).

Original assessment said "45+" untyped; actual count is **100+**. The 8 collection builtins with proper signatures (`register_typed_builtins()`) were added after the original assessment.

**Implication:** Type errors in the vast majority of builtin calls are caught only at runtime.

---

## 4. `:verify` Module Levels Are Inert

**New gap identified 2026-04-14.**

The `VerifyLevel` enum (`Checked`, `Proven`, `Trusted`) is defined in `crates/airl-syntax/src/ast.rs` and parsed into `ModuleDef.verify`, but **no pipeline code branches on it**. All functions receive identical Z3 treatment regardless of module-level annotation.

**Source:** Grep for `VerifyLevel`, `Checked`, `Proven`, `Trusted` in `crates/airl-driver/src/pipeline.rs` returns zero behavioral references.

---

## 5. No Z3 Proof Caching

**New gap identified 2026-04-14.**

Z3 creates a fresh `Solver` instance per function per `verify_function()` call (`crates/airl-solver/src/prover.rs` line 66). No memoization or result caching between compilations. The Phase 2 design spec proposed a `ProofCache` type, but it was never implemented.

---

## 6. match/lambda Unsupported in Z3 Contracts

**New gap identified 2026-04-14.**

Both `match` and `lambda` expressions in contract clauses are explicitly rejected by the Z3 translator (`crates/airl-solver/src/translate.rs`):

- `match`: `"match expressions in contracts require explicit encoding — use if/cond instead"` (3 locations)
- `lambda`: `"lambda expressions cannot appear in Z3 contracts"` (3 locations)

Contracts using these constructs get `TranslationError` and fall back to runtime-only enforcement.

---

## Summary

| # | Feature | Status | Enforcement |
|---|---------|--------|-------------|
| 1 | Z3 enforcement (Phase 2A/2B) | **NOT IMPLEMENTED** — spec approved, code not written | Runtime assertions only |
| 2 | Linearity | Always-on but lenient for default ownership | Runtime `MarkMoved`/`CheckNotMoved` for annotated params |
| 3 | Builtin type checking | 19 typed / 100+ untyped | Runtime panics on type mismatch |
| 4 | `:verify` module levels | Parsed but inert | No differentiation |
| 5 | Proof caching | Not implemented | Fresh solver per function |
| 6 | match/lambda in contracts | Unsupported | TranslationError → runtime only |

### Resolved

| Feature | Resolution | Date |
|---------|-----------|------|
| Channel bugs (3 TLC-confirmed) | All three fixed in `crates/airl-rt/src/thread.rs` | Pre-2026-04-14 |
| TLA+ CI coverage | All 3 specs (channels, memory, COW) in CI | 2026-04-14 |
| Z3 invariant verification | `:invariant` clauses now verified by Z3 | 2026-04-14 |

## Design Specs for Remaining Gaps

| # | Gap | Spec |
|---|-----|------|
| 1 | Z3 Phase 2A/2B enforcement + opcode elision | `2026-04-14-z3-phase2-enforcement-design.md` |
| 2 | Linearity default ownership | `2026-04-14-linearity-default-ownership-design.md` |
| 3 | Builtin type signatures (100+) | `2026-04-14-builtin-type-signatures-design.md` |
| 4 | `:verify` module level enforcement | `2026-04-14-verify-level-enforcement-design.md` |
| 5 | Z3 proof caching | `2026-04-14-z3-proof-caching-design.md` |
| 6 | match/lambda in Z3 contracts | `2026-04-14-z3-match-lambda-support-design.md` |
