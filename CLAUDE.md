# AIRL — Project Instructions for Claude

## Project Overview

AIRL (AI Intermediate Representation Language) is a programming language designed for AI systems. It's a Rust Cargo workspace with 8 crates, 428 tests, ~13K lines of code.

**Language spec:** `AIRL-Language-Specification-v0.1.0.md`
**Design specs:** `docs/superpowers/specs/`
**Implementation plans:** `docs/superpowers/plans/`

## Build & Test

```bash
cargo build                    # Build all crates
cargo test --workspace         # Run all 428 tests
cargo run -- run <file.airl>   # Execute an AIRL program
cargo run -- check <file.airl> # Type-check and verify
cargo run -- repl              # Interactive REPL
```

**First build note:** Z3 (in `airl-solver`) compiles from C++ source on first build (~5-15 min). Requires CMake, C++ compiler, Python 3.

## Crate Dependency Chain

```
airl-syntax (no deps)
    ↓
airl-types
    ↓
airl-contracts
    ↓
airl-runtime ← airl-codegen (Cranelift)
    ↓
airl-agent
    ↓
airl-driver ← airl-solver (Z3)
```

**Critical:** `airl-runtime` depends on `airl-codegen`, so `airl-codegen` CANNOT depend on `airl-runtime` (circular). Tensor JIT uses raw `&[f64]` slices, not `Value`, for this reason. Same pattern: `airl-agent` depends on `airl-runtime`, so `airl-runtime` duplicates framing protocol in `agent_client.rs` rather than importing from `airl-agent`.

## Key Conventions

- **Zero external deps for core crates.** Only `airl-codegen` (Cranelift) and `airl-solver` (Z3) have external deps.
- **Tests are inline** `#[cfg(test)]` modules in each source file, plus fixture-based E2E tests in `crates/airl-driver/tests/fixtures.rs`.
- **Fixtures live in** `tests/fixtures/valid/`, `tests/fixtures/type_errors/`, `tests/fixtures/contract_errors/`, `tests/fixtures/linearity_errors/`, `tests/fixtures/agent/`.
- **The `orchestrator.airl` fixture** requires the built binary (uses `spawn-agent`) — it's in `tests/fixtures/agent/`, NOT `tests/fixtures/valid/`, so the fixture runner doesn't try to run it.
- **Builtin dispatch pattern:** Builtins that need `&mut self` (spawn-agent, send, tensor JIT) are handled directly in the `FnCall` arm of `eval.rs` BEFORE the generic builtin registry dispatch. The `Option::take()` trick is used for tensor_jit to work around the borrow checker.

---

## Remaining Tasks

### Tier 1 — High Priority

#### 1. ~~Async Agent Builtins: `await`, `parallel`~~ ✅ DONE

**Implemented:** `send-async`, `await`, and `parallel` builtins.

- `send-async` — Same args as `send` but returns immediately with a task ID string. Writes the task frame synchronously, then spawns a background thread to read the response via `mpsc::channel`.
- `await` — Takes a task ID and optional timeout in milliseconds. Blocks on the channel receiver. `(await task-id)` or `(await task-id 5000)`.
- `parallel` — Takes a list of task ID strings (from prior `send-async` calls), awaits all, returns a list of results. Optional timeout as second arg.
- Agent reader/writer changed to `Arc<Mutex<>>` for thread-safe sharing.
- `pending_results: HashMap<String, mpsc::Receiver<Result<Value, String>>>` added to Interpreter.
- Test fixture: `tests/fixtures/agent/async_orchestrator.airl`.

---

#### 2. ~~Z3 Quantifier Support (`forall`/`exists`)~~ ✅ DONE

**Implemented:** Full quantifier support across 4 crates:
- **AST:** `ExprKind::Forall` and `ExprKind::Exists` variants with `Param`, optional `where` guard, and body expression.
- **Parser:** `(forall [i : Type] (where guard) body)` and `(exists [i : Type] (where guard) body)` syntax. Where clause is optional.
- **Z3 Translator:** Translates to `z3::ast::forall_const` / `z3::ast::exists_const`. Where clause becomes implication (forall) or conjunction (exists). Temporarily binds quantified variable in translator maps.
- **Runtime:** Iterates integers 0..10,000. `forall` short-circuits on first false, `exists` on first true. Where clause filters domain.
- **Type Checker:** Returns `Bool` type for quantifier expressions.
- Test fixtures: `tests/fixtures/valid/forall_expr.airl`, `tests/fixtures/valid/exists_expr.airl`, `tests/fixtures/valid/forall_contract.airl`.

---

#### 3. ~~Invariant Checking~~ ✅ DONE

**Implemented:** `:invariant` clauses are now evaluated in `call_fn` after body evaluation and before `:ensures` checking (both JIT and interpreter paths). Uses `ContractKind::Invariant` for violation errors. Test fixtures: `tests/fixtures/valid/invariant.airl`, `tests/fixtures/contract_errors/invariant_violation.airl`.

---

### Tier 2 — Important for Completeness

#### 4. ~~Z3 Float Arithmetic Support~~ ✅ DONE

**Implemented:** Z3 Real arithmetic for `f16`, `f32`, `f64`, `bf16` types.
- **Translator:** `VarSort::Real`, `declare_real`, `get_real_var`, `translate_real` (handles `+`, `-`, `*`, `/` on Reals). Float literals converted to Z3 rationals via scaled integers.
- **Comparisons:** `translate_cmp_eq`/`translate_cmp_ord` try Int first, fall back to Real — no type inference needed.
- **Prover:** Declares Real variables for float params/result. Counterexample extraction for Real variables.
- **Quantifiers:** `VarSort::Real` arm added to `translate_quantifier`.
- Test fixture: `tests/fixtures/valid/float_contract.airl`.

---

#### 5. ~~Better Error Messages with Source Context~~ ✅ DONE

**Implemented:**
- **`Expr::to_airl()`** in `ast.rs` — converts any `Expr` AST node back to readable AIRL S-expression syntax. Handles all `ExprKind` variants, `AstType`, and `Pattern`.
- **Contract violation messages** now show `(> x 0)` instead of `FnCall(Expr { kind: SymbolRef(">="), ...})`. All 5 sites in `eval.rs` updated.
- **Z3 disproven warnings** now show `(>= result lo)` instead of debug output. All 3 sites in `prover.rs` updated.
- **`ContractViolation::Display`** improved: shows `"Requires contract violated in \`fn\`: (clause) evaluated to false"` with optional bindings.
- **Bindings captured**: `capture_bindings()` populates the `bindings` field with actual parameter values (filtering out builtins/functions).
- **Runtime errors with source context**: `main.rs` now calls `format_diagnostic_with_source` for `ContractViolation` and `UseAfterMove`, showing the source line and caret.

---

#### 6. ~~REPL Enhancements~~ ✅ DONE

**Implemented:**
- `:help` / `:h` — Lists all commands with descriptions.
- `:load <file>` — Reads and evaluates a file in the REPL session using `eval_repl_input`.
- `:type <expr>` — Shows the type of an expression without evaluating, using a persistent `TypeChecker`.
- `drain_diagnostics(&mut self)` added to `TypeChecker` for non-consuming diagnostic access.
- `Display` impls added for `Ty` and `PrimTy` for readable type output.
- `:env` now shows readable types (e.g., `i32` instead of `Named("i32")`).

---

#### 7. ~~Agent Builtins: `broadcast`, `retry`, `escalate`, `any-agent`~~ ✅ DONE

**Implemented:**
- **`broadcast`** — `(broadcast [agent1 agent2 ...] "fn" args...)`. Sends the same task to all agents concurrently via threads, returns the first successful result.
- **`retry`** — `(retry target "fn" args... :max N)`. Wraps a synchronous `send` in retry logic with exponential backoff (100ms, 200ms, 400ms...). Default 3 retries.
- **`escalate`** — `(escalate target :reason "msg" :data value)`. Sends a structured escalation to an agent via `__escalate__` function. Falls back to returning an `(Escalation ...)` variant if the agent doesn't handle it.
- **`any-agent`** — `(any-agent)`. Returns the name of the first spawned agent. Simple Phase 1 implementation without capability filtering.

---

### Tier 3 — Nice to Have

#### 8. ~~Static Linearity Analysis~~ ✅ DONE

**Implemented:** Control-flow-sensitive AST walk tracking ownership through branches.
- **`check_fn(def)`** — Registers function param ownerships, introduces params, walks body.
- **`check_expr(expr)`** — Recursively walks all `ExprKind` variants. For `FnCall`, looks up callee's parameter ownership annotations via `fn_ownerships` registry and calls `track_move`/`track_borrow` on symbol arguments.
- **Branch divergence:** `If` and `Match` use `snapshot()`/`restore()` to check each branch independently, then `merge_branch_states()` verifies all branches agree on ownership state.
- **Pattern bindings:** `introduce_pattern()` handles nested pattern binding introduction in match arms.
- **Pipeline integration:** Linearity analysis runs after type checking in both `run_source_with_mode` and `check_source`. Errors shown as warnings in Run mode, errors in Check mode.
- Detects: use-after-move, move-while-borrowed, branch ownership divergence.

---

#### 9. ~~Nested Pattern Matching~~ ✅ ALREADY WORKING

**Status:** Nested patterns already work. Both the parser (`parse_pattern` recursively calls itself on sub-items) and the runtime (`try_match` recursively matches sub-patterns) support arbitrary nesting. Patterns like `(Ok (Ok v))`, `(Some (Err e))`, and `(Pair a b)` all work correctly. The original task description was outdated. Added test fixture: `tests/fixtures/valid/nested_match.airl`.

---

#### 10. GPU Compilation via MLIR

**Status:** Not started. The spec's Phase 2 calls for MLIR lowering to target GPUs. Research found `melior` (Rust MLIR bindings, alpha, requires LLVM 21 system install).

**What to build:** A new `airl-mlir` crate (optional feature) that lowers tensor operations to MLIR's tensor/linalg/gpu dialects, runs MLIR optimization passes, and compiles to GPU kernels via CUDA/ROCm.

**Prerequisite:** LLVM 21 installed. This is a major effort (1000+ lines) and should be its own sub-project.

**Files:** New crate `crates/airl-mlir/`.

---

#### 11. Self-Hosting (Phase 3)

**Status:** Not started. The spec's Phase 3 goal is to write the AIRL compiler in AIRL itself.

**Prerequisite:** The language needs string manipulation, file I/O, and sufficient expressiveness to implement a parser and evaluator. Currently AIRL has no file I/O builtins.

---

## Known Issues — All Resolved

1. ~~**`cargo build` warning:**~~ ✅ Moved `Config` import to `#[cfg(test)]` block.

2. ~~**Type checker warnings are noisy:**~~ ✅ Registered all builtins (tensor, agent, utility, collection) in `TypeChecker::register_builtins()` using `TypeVar` for polymorphic builtins. FnCall handler now treats `TypeVar` callees as wildcard-typed.

3. ~~**Z3 "disproven" warnings for valid contracts:**~~ ✅ Suppressed disproven warnings when the contract references `result` and `result` is not constrained in `:requires` (since the prover doesn't encode function bodies).

4. ~~**JIT float handling:**~~ ✅ Added safety documentation to `RawValue` explaining the bitcast invariant. Added tests for cross-type confusion and special float values (NaN, infinity, negative zero).

5. ~~**`spawn-agent` sleep:**~~ ✅ Replaced 100ms sleep with a proper handshake protocol. Agent sends a `"ready"` frame on stdout after initialization. Spawner blocks on `read_frame` until ready signal arrives.
