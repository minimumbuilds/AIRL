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

## Standard Library

**Location:** `stdlib/` directory (embedded in binary via `include_str!`, auto-loaded before user code)

The stdlib is 4 modules (46 functions total) — mostly pure AIRL, with Rust builtins for list destructuring and string character access.

### Primitive Builtins (Rust)

**List builtins** (4) in `crates/airl-runtime/src/builtins.rs`:
- `head` — first element of list (errors on empty)
- `tail` — all but first element (errors on empty)
- `empty?` — is list empty? → Bool
- `cons` — prepend element to front of list → List

**String builtins** (13) in `crates/airl-runtime/src/builtins.rs`:
- `char-at`, `substring`, `chars` — character access (Unicode-safe)
- `split`, `join` — split/join strings
- `contains`, `starts-with`, `ends-with`, `index-of` — search
- `trim`, `to-upper`, `to-lower`, `replace` — transformation

**Map builtins** (10) in `crates/airl-runtime/src/builtins.rs`:
- `map-new`, `map-from` — creation
- `map-get`, `map-get-or`, `map-has`, `map-size` — reading
- `map-set`, `map-remove` — mutation (returns new map)
- `map-keys`, `map-values` — enumeration

### Stdlib Modules (Pure AIRL)

**Collections** (`stdlib/prelude.airl`) — 15 functions:

| Function | Signature | Description |
|----------|-----------|-------------|
| `map` | `(map f xs)` | Apply f to each element |
| `filter` | `(filter pred xs)` | Keep elements where pred is true |
| `fold` | `(fold f init xs)` | Left fold with accumulator |
| `reverse` | `(reverse xs)` | Reverse a list |
| `concat` | `(concat xs ys)` | Concatenate two lists |
| `zip` | `(zip xs ys)` | Pair elements from two lists |
| `flatten` | `(flatten xss)` | Flatten list of lists |
| `range` | `(range start end)` | Generate integers [start, end) |
| `take` | `(take n xs)` | First n elements |
| `drop` | `(drop n xs)` | Skip first n elements |
| `any` | `(any pred xs)` | Any element satisfies pred? |
| `all` | `(all pred xs)` | All elements satisfy pred? |
| `find` | `(find pred xs)` | First element satisfying pred, or nil |
| `sort` | `(sort cmp xs)` | Merge sort with comparison fn |
| `merge` | `(merge cmp xs ys)` | Merge two sorted lists |

**Math** (`stdlib/math.airl`) — 13 functions:

| Function | Signature | Description |
|----------|-----------|-------------|
| `abs` | `(abs x)` | Absolute value |
| `min` | `(min a b)` | Minimum of two values |
| `max` | `(max a b)` | Maximum of two values |
| `clamp` | `(clamp x lo hi)` | Clamp value to range [lo, hi] |
| `sign` | `(sign x)` | Returns -1, 0, or 1 |
| `even?` | `(even? x)` | Is integer even? |
| `odd?` | `(odd? x)` | Is integer odd? |
| `pow` | `(pow base exp)` | Integer exponentiation |
| `gcd` | `(gcd a b)` | Greatest common divisor |
| `lcm` | `(lcm a b)` | Least common multiple |
| `sum-list` | `(sum-list xs)` | Sum all elements |
| `product-list` | `(product-list xs)` | Multiply all elements |

**Result Combinators** (`stdlib/result.airl`) — 8 functions:

| Function | Signature | Description |
|----------|-----------|-------------|
| `is-ok?` | `(is-ok? r)` | Check if Result is Ok |
| `is-err?` | `(is-err? r)` | Check if Result is Err |
| `unwrap-or` | `(unwrap-or r default)` | Extract Ok or return default |
| `map-ok` | `(map-ok f r)` | Apply f to Ok value |
| `map-err` | `(map-err f r)` | Apply f to Err value |
| `and-then` | `(and-then f r)` | Chain Result-returning function |
| `or-else` | `(or-else f r)` | Recover from Err |
| `ok-or` | `(ok-or val err)` | Wrap non-nil in Ok, nil becomes Err |

**String** (`stdlib/string.airl`) — 10 AIRL functions + 13 Rust builtins:

| Function | Signature | Description |
|----------|-----------|-------------|
| `words` | `(words s)` | Split by whitespace |
| `unwords` | `(unwords ws)` | Join with spaces |
| `lines` | `(lines s)` | Split by newlines |
| `unlines` | `(unlines ls)` | Join with newlines |
| `repeat-str` | `(repeat-str s n)` | Repeat string n times |
| `pad-left` | `(pad-left s w ch)` | Pad to width on left |
| `pad-right` | `(pad-right s w ch)` | Pad to width on right |
| `is-empty-str` | `(is-empty-str s)` | Is string empty? |
| `reverse-str` | `(reverse-str s)` | Reverse a string |
| `count-occurrences` | `(count-occurrences s sub)` | Count substring occurrences |

See `stdlib/string.md` for full documentation including the 13 Rust builtins.

**Map** (`stdlib/map.airl`) — 8 AIRL functions + 10 Rust builtins:

| Function | Signature | Description |
|----------|-----------|-------------|
| `map-entries` | `(map-entries m)` | All entries as `[[k v] ...]` pairs |
| `map-from-entries` | `(map-from-entries pairs)` | Create from `[[k v] ...]` pairs |
| `map-merge` | `(map-merge m1 m2)` | Merge maps (m2 wins on conflict) |
| `map-map-values` | `(map-map-values f m)` | Apply f to every value |
| `map-filter` | `(map-filter pred m)` | Keep entries where pred(k,v) is true |
| `map-update` | `(map-update m key f)` | Apply f to value at key |
| `map-update-or` | `(map-update-or m key default f)` | Update with default for missing keys |
| `map-count` | `(map-count pred m)` | Count matching entries |

See `stdlib/map.md` for full documentation including the 10 Rust builtins.

### Prelude Loading

- Embedded via `include_str!()` in `crates/airl-driver/src/pipeline.rs`
- `eval_prelude()` parses and evaluates all five modules in order: collections → math → result → string → map
- Called in both `run_source_with_mode()` and REPL startup
- **Load order matters:** math depends on collections (`fold`), string depends on collections (`filter`, `reverse`)
- **Recursion depth limit:** 50,000 (in `Interpreter.recursion_depth`) to prevent stack overflow on large lists
- **Known issue:** Type checker warns "undefined symbol" for stdlib functions because they are loaded at runtime, not registered in the type checker. Cosmetic only — functions work correctly.

---

## Completed Tasks

The following tasks have been implemented. Some were partially regressed during the stdlib addition (commit `09924bc`) and need re-integration — see "Remaining Tasks" below.

- **Z3 Quantifier Support (`forall`/`exists`)** — `ExprKind::Forall`/`Exists` in AST, parser support via `parse_quantifier_expr`, Z3 translation via `forall_const`/`exists_const`, runtime evaluation via `eval_quantifier`.
- **Invariant Checking** — `:invariant` clauses evaluated after body execution in both JIT and interpreted paths of `call_fn`, using `ContractKind::Invariant`.
- **Z3 Float Arithmetic Support** — `VarSort::Real`, `declare_real()`, `translate_real()` in translator. Maps f16/f32/f64/bf16 to Z3 Reals.
- **Nested Pattern Matching** — `try_match` in `pattern.rs` recursively destructures nested patterns like `(Ok (Ok x))`.
- **GPU Compilation via MLIR** — `crates/airl-mlir/` crate (~1,750 lines) with tensor lowering, GPU kernel generation, JIT execution, and optimization passes. **Build issue:** requires `libzstd-dev` for LLVM/melior linking; currently excluded from `cargo test --workspace`.

---

## Remaining Tasks

### Tier 1 — Re-integrate Regressed Features

These features were implemented in commit `bdb00f0` but removed during the stdlib addition in `09924bc`. The previous implementations can be referenced via `git show bdb00f0 -- crates/airl-runtime/src/eval.rs`.

#### 1. Async Agent Builtins: `send-async`, `await`, `parallel`

**Status:** Previously implemented, then removed. `await` and `parallel` are still registered as builtin names in `crates/airl-agent/src/builtins.rs` but have no backing code in `eval.rs`.

**What to restore/build:**

`send-async` — Returns a task ID immediately, spawns a background thread for the send. Requires re-adding `pending_results: HashMap<String, mpsc::Receiver<Result<Value, String>>>` field to Interpreter, plus `Arc<Mutex<...>>` wrappers on agent reader/writer.

`await` — Takes a task ID and timeout. Blocks on the channel receiver from `pending_results`.

`parallel` — Takes a list of task expressions, dispatches concurrently (one thread per task), collects results via channels, applies a merge function.

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Re-add `builtin_send_async`, `builtin_await`, `builtin_parallel` methods and `pending_results` field. Restore `Arc<Mutex<...>>` on agent I/O handles.
- `crates/airl-agent/src/builtins.rs` — Remove from stub list once implemented.

**Estimated effort:** 200-400 lines (re-integration from prior commit).

---

#### 2. Agent Builtins: `broadcast`, `retry`, `escalate`, `any-agent`

**Status:** Registered as names in `crates/airl-agent/src/builtins.rs`, zero implementation. These were never fully implemented even in `bdb00f0`.

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

#### 3. MLIR Runtime Integration

**Status:** The `airl-mlir` crate exists with ~1,750 lines of code, but the runtime integration (execution target dispatch, `exec_target` field on Interpreter) was removed in `09924bc`.

**What to restore:**
- Re-add `exec_target: Option<ExecTarget>` field to Interpreter.
- Re-add `mlir_jit: Option<airl_mlir::MlirTensorJit>` field (behind `#[cfg(feature = "mlir")]`).
- Restore the GPU → MLIR CPU → Cranelift → interpreted fallback chain in tensor op dispatch.
- Fix the `libzstd` linker issue (install `libzstd-dev` or make `airl-mlir` a default-off workspace member).

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Re-add MLIR dispatch fields and logic.
- `crates/airl-runtime/Cargo.toml` — Ensure `mlir` feature flag is properly gated.
- `Cargo.toml` (workspace) — Consider making `airl-mlir` a non-default member.

**Estimated effort:** 100-200 lines (re-integration from prior commit).

---

### Tier 2 — New Features

#### 4. Better Error Messages with Source Context

**Status:** Error messages show the error type and span (line:col) but not the source code context. The `clause_source` field in contract violations uses `format!("{:?}", expr.kind)` (Rust debug output) instead of pretty-printed AIRL. A previous implementation used `contract.to_airl()` but was reverted.

**What to build:**
- Contract violations should show the contract clause as readable AIRL S-expressions and the values that violated it.
- Use-after-move errors should show both the move site and the use site.
- Type errors should show the expected vs actual type with source context.
- Re-add binding capture in contract violations (currently `bindings: vec![]`).

**Files to modify:**
- `crates/airl-runtime/src/eval.rs` — Improve `clause_source` formatting, restore binding capture.
- `crates/airl-driver/src/pipeline.rs` — Use `format_diagnostic_with_source` for all error types.
- `crates/airl-syntax/src/diagnostic.rs` — Add helper methods for building rich diagnostics.

**Estimated effort:** 150-250 lines.

---

#### 5. REPL Enhancements

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

#### 6. Static Linearity Analysis

**Status:** The `LinearityChecker` in `crates/airl-types/src/linearity.rs` (~566 lines) tracks ownership state at the API level but is not wired into the AST walk. Runtime enforcement (in `eval.rs`) catches use-after-move for explicit `Ownership::Own` params, but doesn't detect branch divergence or scope issues.

**What to build:** A control-flow-sensitive pass that walks function bodies and tracks ownership state through branches. For `if`/`match`, both arms must leave bindings in compatible states. This is essentially a simplified version of Rust's borrow checker.

**Files to modify:**
- `crates/airl-types/src/linearity.rs` — Add `check_fn_body(&mut self, body: &Expr)` that walks the AST.
- `crates/airl-types/src/checker.rs` — Call linearity check after type checking.
- `crates/airl-driver/src/pipeline.rs` — Report linearity errors.

**Estimated effort:** 500-800 lines. This is the hardest remaining task.

---

### Tier 3 — Long-term

#### 7. Self-Hosting (Phase 3)

**Status:** Not started. The spec's Phase 3 goal is to write the AIRL compiler in AIRL itself.

**Prerequisite:** The language needs file I/O builtins and sufficient expressiveness to implement a parser and evaluator. Currently AIRL has no file I/O builtins. String manipulation was added with the stdlib.

---

## Known Issues

1. **`cargo build` warning:** `unused import: Config` in `crates/airl-solver/src/translate.rs:3`. The `Config` import is only used in tests via `super::*`. Fix: move to `#[cfg(test)]` block or gate with `#[cfg(test)]`.

2. **Type checker warnings are noisy:** The type checker warns about `spawn-agent`, `send`, and other builtins it doesn't know about. These are harmless but clutter output. Fix: register builtin types in the type checker environment.

3. **Z3 "disproven" warnings for valid contracts:** Contracts like `(= result (+ a b))` are "disproven" by Z3 because `result` is a free variable — the prover doesn't encode the function body. This is correct behavior but confusing. Consider suppressing disproven warnings for contracts that only reference `result` without body constraints, or adding a note explaining why.

4. **JIT float handling:** The scalar JIT uses I64 as the uniform ABI type and bitcasts for floats. This works but means float-returning functions store results as I64 bit patterns. The marshaling in `eval.rs` (`raw_to_value`) handles this correctly, but it's fragile.

5. **`spawn-agent` sleep:** `builtin_spawn_agent` sleeps 100ms to let the child process start. This is a race condition — a slow system might need more time. A proper solution would be a handshake protocol (agent sends a "ready" message after loading).

6. **`airl-mlir` linker failure:** `melior-macro` fails to link due to missing `libzstd`. Install `libzstd-dev` or exclude `airl-mlir` from default workspace members.
