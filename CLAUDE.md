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

#### 1. Async Agent Builtins: `await`, `parallel`

**Status:** `await` and `parallel` are registered as builtin names in `crates/airl-agent/src/builtins.rs` but have zero implementation. Currently calling them produces `UndefinedSymbol` at runtime.

**What to build:**

`await` — Takes a task ID (from a future async `send`) and a timeout duration. Blocks until the result arrives or timeout expires. For Phase 1, `send` is already synchronous, so `await` only makes sense if `send` gets an async mode. **Recommended approach:** Add `send-async` that returns a task ID immediately (spawns a background thread for the send), then `await` blocks on a channel receiver with timeout.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Add `builtin_await`, `builtin_send_async` methods on Interpreter. Add a `pending_results: HashMap<String, mpsc::Receiver<Value>>` field.
- `crates/airl-agent/src/builtins.rs` — Remove from stub list once implemented.

`parallel` — Takes a list of task expressions, dispatches them concurrently (one thread per task), collects results, applies a merge function. **Recommended approach:** `(parallel [(send w1 "fn" args...) (send w2 "fn" args...)] :merge (fn [results] ...))`. Spawn a thread per task, each does a synchronous send, collect via channels, apply merge.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Add `builtin_parallel`. Needs `std::thread::spawn` + channels.

**Estimated effort:** 300-500 lines total.

---

#### 2. Z3 Quantifier Support (`forall`/`exists`)

**Status:** The language spec (§4.4) defines quantified contracts but they are not parseable or translatable to Z3.

**What to build:**

Step 1 — Parse `forall`/`exists` in contracts. Currently the parser treats them as regular function calls. Add `ExprKind::Forall` and `ExprKind::Exists` to the AST (`crates/airl-syntax/src/ast.rs`), and recognize them in the form parser (`crates/airl-syntax/src/parser.rs`).

Syntax from spec:
```clojure
(forall [i : Nat]
  (where (< i (length result)))
  (>= (at result i) 0))
```

Step 2 — Translate to Z3. The `z3` crate supports quantifiers via `z3::ast::forall_const` and `z3::ast::exists_const`. Add handlers in `crates/airl-solver/src/translate.rs`.

Step 3 — Runtime evaluation. In `checked` mode, quantifiers over collections need to be evaluated by iterating. Add evaluation logic in `crates/airl-runtime/src/eval.rs` — `forall` iterates and short-circuits on first false, `exists` short-circuits on first true. Cap iteration at 10,000 elements.

**Files to modify:**
- `crates/airl-syntax/src/ast.rs` — Add `Forall`/`Exists` to `ExprKind`
- `crates/airl-syntax/src/parser.rs` — Recognize `forall`/`exists` forms
- `crates/airl-solver/src/translate.rs` — Translate to Z3 quantifiers
- `crates/airl-runtime/src/eval.rs` — Runtime evaluation via iteration
- `crates/airl-contracts/src/checked.rs` — May need updates for quantifier evaluation

**Estimated effort:** 300-400 lines total.

---

#### 3. Invariant Checking

**Status:** `:invariant` is parsed and stored on `FnDef` but never evaluated at runtime. The `invariants` field exists in the AST but `call_fn` in `eval.rs` ignores it.

**What to build:** In `eval.rs`'s `call_fn`, after evaluating the body (and before `:ensures`), check invariant clauses. For loops/recursive calls, invariants should be checked at each iteration entry — but since AIRL doesn't have explicit loops (recursion only), check invariants on each recursive re-entry.

**Simpler approach for Phase 1:** Check invariants once after body evaluation, same timing as ensures. This is less powerful than continuous verification but catches the common case.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — In `call_fn`, add invariant checking block between body evaluation and ensures checking.

**Estimated effort:** 30-50 lines.

---

### Tier 2 — Important for Completeness

#### 4. Z3 Float Arithmetic Support

**Status:** The Z3 translator (`crates/airl-solver/src/translate.rs`) only handles integer types. Any function with `f32`/`f64` params returns `VerifyResult::Unknown("unsupported parameter types")`.

**What to build:** Add `z3::ast::Float` (or `z3::ast::Real`) support to the translator. Z3 has a real arithmetic theory (`QF_LRA`) that handles `+`, `-`, `*`, `/`, comparisons on reals. Map `f32`/`f64` to Z3 Reals (not IEEE floats — Z3's float theory is slow and incomplete). This is an approximation but covers most contract patterns.

**Files to modify:**
- `crates/airl-solver/src/translate.rs` — Add `translate_real`, `declare_real`, `VarSort::Real`. Update comparison operators to handle real operands.
- `crates/airl-solver/src/prover.rs` — Update `sort_from_type_name` to map f32/f64 to Real.

**Estimated effort:** 150-200 lines.

---

#### 5. Better Error Messages with Source Context

**Status:** Error messages show the error type and span (line:col) but not the source code context. The infrastructure exists (`format_diagnostic_with_source` in `pipeline.rs` and `Diagnostic` with notes in `diagnostic.rs`) but is underused.

**What to build:**
- Contract violations should show the contract clause source text and the values that violated it.
- Use-after-move errors should show both the move site and the use site.
- Type errors should show the expected vs actual type with source context.

**Key improvement:** The `ContractViolation` struct's `clause_source` field currently contains `format!("{:?}", expr.kind)` (Rust debug output). Replace with a pretty-printed AIRL S-expression of the clause.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Improve `clause_source` formatting in contract violation construction.
- `crates/airl-driver/src/pipeline.rs` — Use `format_diagnostic_with_source` for all error types, not just syntax errors.
- `crates/airl-syntax/src/diagnostic.rs` — Add helper methods for building rich diagnostics.

**Estimated effort:** 150-250 lines.

---

#### 6. REPL Enhancements

**Status:** REPL has `:quit`, `:env`, and expression evaluation. Missing `:type`, `:load`, `:help`.

**What to build:**
- `:help` — Print available commands. Trivial (10 lines).
- `:load <file>` — Read and evaluate a file in the REPL session. Use `std::fs::read_to_string` + existing `eval_repl_input`.
- `:type <expr>` — Show the type of an expression without evaluating. Requires running the `TypeChecker` on the expression. **Challenge:** TypeChecker's `into_diagnostics()` is consuming — need to add `drain_diagnostics(&mut self)` to `crates/airl-types/src/checker.rs` for REPL persistence.

**Files to modify:**
- `crates/airl-driver/src/repl.rs` — Add command handlers.
- `crates/airl-types/src/checker.rs` — Add `drain_diagnostics` for REPL type checking.

**Estimated effort:** 100-200 lines.

---

#### 7. Agent Builtins: `broadcast`, `retry`, `escalate`, `any-agent`

**Status:** Registered as names in `crates/airl-agent/src/builtins.rs`, zero implementation.

**What to build:**

- `broadcast` — `(broadcast [agent1 agent2 agent3] "fn" args... :merge :first-valid)`. Send the same task to multiple agents, return first successful result. Implement as parallel sends (threads) with first-result channel.

- `retry` — `(retry :max 3 :backoff :exponential (send target "fn" args...))`. Wrap a send in retry logic. Implement as a loop with sleep between retries.

- `escalate` — `(escalate target :reason :timeout :partial-results data)`. Send a structured error notification to a specified agent. Implement as a special task message type.

- `any-agent` — `(any-agent :with [:compute-gpu])`. Look up agents by capability in the registry. Requires the `AgentRegistry` from `airl-agent` to be accessible from the interpreter. Currently the interpreter has a simple `Vec<LiveAgent>` — would need to track capabilities per agent.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Add builtin methods.
- `crates/airl-agent/src/builtins.rs` — Remove from stub list.

**Estimated effort:** 400-600 lines total.

---

### Tier 3 — Nice to Have

#### 8. Static Linearity Analysis

**Status:** The `LinearityChecker` in `crates/airl-types/src/linearity.rs` tracks ownership state at the API level but is not wired into the AST walk. Runtime enforcement (in `eval.rs`) catches use-after-move for explicit `Ownership::Own` params, but doesn't detect branch divergence or scope issues.

**What to build:** A control-flow-sensitive pass that walks function bodies and tracks ownership state through branches. For `if`/`match`, both arms must leave bindings in compatible states. This is essentially a simplified version of Rust's borrow checker.

**Files to modify:**
- `crates/airl-types/src/linearity.rs` — Add `check_fn_body(&mut self, body: &Expr)` that walks the AST.
- `crates/airl-types/src/checker.rs` — Call linearity check after type checking.
- `crates/airl-driver/src/pipeline.rs` — Report linearity errors.

**Estimated effort:** 500-800 lines. This is the hardest remaining task.

---

#### 9. Nested Pattern Matching

**Status:** Pattern matching works for top-level variants (`(Ok v)`, `(Err e)`) but not nested patterns (`(Ok (Pair a b))`).

**What to build:** Extend `try_match` in `crates/airl-runtime/src/pattern.rs` to recursively destructure nested patterns. Also extend the parser to recognize nested pattern syntax.

**Files to modify:**
- `crates/airl-runtime/src/pattern.rs` — Recursive matching.
- `crates/airl-syntax/src/parser.rs` — Parse nested patterns in match arms.

**Estimated effort:** 100-200 lines.

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

## Known Issues

1. **`cargo build` warning:** `unused import: Config` in `crates/airl-solver/src/translate.rs:3`. The `Config` import is only used in tests via `super::*`. Fix: move to `#[cfg(test)]` block or gate with `#[cfg(test)]`.

2. **Type checker warnings are noisy:** The type checker warns about `spawn-agent`, `send`, and other builtins it doesn't know about. These are harmless but clutter output. Fix: register builtin types in the type checker environment.

3. **Z3 "disproven" warnings for valid contracts:** Contracts like `(= result (+ a b))` are "disproven" by Z3 because `result` is a free variable — the prover doesn't encode the function body. This is correct behavior but confusing. Consider suppressing disproven warnings for contracts that only reference `result` without body constraints, or adding a note explaining why.

4. **JIT float handling:** The scalar JIT uses I64 as the uniform ABI type and bitcasts for floats. This works but means float-returning functions store results as I64 bit patterns. The marshaling in `eval.rs` (`raw_to_value`) handles this correctly, but it's fragile.

5. **`spawn-agent` sleep:** `builtin_spawn_agent` sleeps 100ms to let the child process start. This is a race condition — a slow system might need more time. A proper solution would be a handshake protocol (agent sends a "ready" message after loading).
