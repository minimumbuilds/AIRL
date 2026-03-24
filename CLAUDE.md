# AIRL — Project Instructions for Claude

## Project Overview

AIRL (AI Intermediate Representation Language) is a programming language designed for AI systems. It's a Rust Cargo workspace with 9 crates, ~479 tests, ~19K lines of Rust + ~21K lines of AIRL. Version 0.2.0.

**Language spec:** `AIRL-Language-Specification-v0.1.0.md`
**LLM guide:** `AIRL-LLM-Guide.md` — **MUST read this file before writing any AIRL code.** It contains critical language idioms, pitfalls, and patterns that prevent common mistakes.
**Stdlib docs:** `stdlib/*.md` — **MUST read before writing AIRL code that uses library functions.** Five modules: `collections.md` (map/filter/fold/sort), `math.md` (abs/pow/gcd), `result.md` (Result combinators), `string.md` (13 Rust builtins + 10 AIRL functions), `map.md` (10 Rust builtins + 8 AIRL functions). Also read the stdlib source (`stdlib/*.airl`) for exact signatures and implementations.
**Design specs:** `docs/superpowers/specs/`
**Implementation plans:** `docs/superpowers/plans/`

## Pre-Flight Checklist (BLOCKING)

**Before writing or modifying ANY `.airl` file, you MUST have read ALL of the following in the current conversation. No exceptions. No rationalizing ("this code doesn't use stdlib" is not an excuse). Complete all reads BEFORE writing a single line of AIRL.**

1. `AIRL-LLM-Guide.md` — **entire file**, not partial. Contains critical pitfalls (eager `and`/`or`, no mixed int/float, etc.)
2. `stdlib/collections.md` — `map`/`filter`/`fold`/`sort` signatures and semantics
3. `stdlib/math.md` — `abs`/`pow`/`gcd` signatures
4. `stdlib/result.md` — Result combinator signatures (`unwrap-or`, `and-then`, etc.)
5. `stdlib/string.md` — 13 Rust builtins + 10 AIRL helper signatures
6. `stdlib/map.md` — 10 Rust builtins + 8 AIRL helper signatures

**If you have not read all 6 files above, STOP and read them now before proceeding.**

## Build & Test

```bash
cargo build --features jit              # Build all crates with JIT (recommended)
cargo build                             # Build without JIT (bytecode-only, no Cranelift)
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver  # Run all ~479 tests
cargo run --features jit -- run <file.airl>   # Execute an AIRL program (JIT default)
cargo run --features jit -- check <file.airl> # Type-check and verify
cargo run --features jit -- repl              # Interactive REPL
```

**v0.2 execution model:** `airl run` JIT-compiles eligible functions to native x86-64 via Cranelift with contract assertions as conditional branches. Ineligible functions (using lists, closures, variants, builtins) fall back to the bytecode VM automatically. No mode flags needed.

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
- **Fixtures live in** `tests/fixtures/valid/`, `tests/fixtures/type_errors/`, `tests/fixtures/contract_errors/`, `tests/fixtures/linearity_errors/`, `tests/fixtures/agent/`, `tests/fixtures/interpreter_only/`.
- **`interpreter_only/` fixtures** require the tree-walking interpreter (quantifier expressions, runtime ownership enforcement). These don't run in the default bytecode/JIT path.
- **The `orchestrator.airl` fixture** requires the built binary (uses `spawn-agent`) — it's in `tests/fixtures/agent/`, NOT `tests/fixtures/valid/`, so the fixture runner doesn't try to run it.
- **Builtin dispatch pattern:** Builtins are dispatched via `CallBuiltin` opcode in the bytecode VM, which calls into `Builtins::get()` registry. The tree-walking interpreter (`eval.rs`) is kept for the REPL and agent runtime but is not the default execution path.

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
- Stdlib is compiled to bytecode and loaded via `compile_and_load_stdlib_bytecode()` before user code
- Called in `run_source_with_mode()`, `run_source_bytecode()`, JIT pipelines, and REPL startup
- **Load order matters:** math depends on collections (`fold`), string depends on collections (`filter`, `reverse`)
- **Recursion depth limit:** 50,000 (in `BytecodeVm.recursion_depth`) to prevent stack overflow on large lists
- **Known issue:** Type checker warns "undefined symbol" for stdlib functions because they are loaded at runtime, not registered in the type checker. Cosmetic only — functions work correctly.

---

## Completed Tasks

- **Z3 Quantifier Support (`forall`/`exists`)** — `ExprKind::Forall`/`Exists` in AST, parser support via `parse_quantifier_expr`, Z3 translation via `forall_const`/`exists_const`, runtime evaluation via `eval_quantifier`.
- **Invariant Checking** — `:invariant` clauses evaluated after body execution in both JIT and interpreted paths of `call_fn`, using `ContractKind::Invariant`.
- **Z3 Float Arithmetic Support** — `VarSort::Real`, `declare_real()`, `translate_real()` in translator. Maps f16/f32/f64/bf16 to Z3 Reals.
- **Nested Pattern Matching** — `try_match` in `pattern.rs` recursively destructures nested patterns like `(Ok (Ok x))`.
- **GPU Compilation via MLIR** — `crates/airl-mlir/` crate (~1,750 lines) with tensor lowering, GPU kernel generation, JIT execution, and optimization passes. **Build issue:** requires `libzstd-dev` for LLVM/melior linking; currently excluded from `cargo test --workspace`.
- **Async Agent Builtins** — `send-async` (background dispatch returning task ID), `await` (block on task ID with optional timeout), `parallel` (collect multiple async results). Uses `Arc<Mutex<...>>` on agent I/O handles for thread-safe sharing.
- **Agent Coordination Builtins** — `broadcast` (fan-out to multiple agents, return first success), `retry` (exponential backoff wrapper), `escalate` (structured error notification), `any-agent` (return first spawned agent).
- **MLIR Runtime Integration** — `exec_target: Option<ExecTarget>` on Interpreter, `call_fn`/`call_fn_inner` split for scoped `:execute-on` annotations, GPU → MLIR CPU → Cranelift → interpreted dispatch chain, `try_mlir_tensor_jit` behind `#[cfg(feature = "mlir")]`.
- **Better Error Messages** — Contract violations use `contract.to_airl()` for readable S-expression clause display and `capture_bindings()` to show relevant variable values.
- **REPL Enhancements** — `:help` (list commands), `:load <file>` (evaluate a file in session), `:type <expr>` (show inferred type without evaluating). `drain_diagnostics()` on `TypeChecker` for REPL persistence.
- **Static Linearity Analysis** — `LinearityChecker` wired into `pipeline.rs` after type checking. Detects use-after-move, borrow conflicts, and branch divergence at compile time for `Own`/`Ref`/`Mut` annotated params. Default ownership is not tracked, avoiding false positives. Runs in both `run` and `check` modes.
- **Trampoline Eval + Self-TCO** — `eval()` split into trampoline driver loop + `eval_inner()` single-step evaluator. Tail-position expressions (`if` branches, `do` last expr, `match` arms, `let` bodies) return `Continue(Expr)` instead of recursing on Rust stack. Self-recursive function calls detected by `current_fn_name` and looped in `call_fn_inner` via `eval_body()`. `in_tail_context` flag prevents TailCall from leaking into nested sub-expression evaluation. Eliminates stack overflow for tail-recursive AIRL functions (bootstrap lexer/parser loops, fold, map). Thread stack is 64MB.
- **String `length` Fix** — `builtin_length` for strings changed from `s.len()` (byte count) to `s.chars().count()` (character count), aligning with `char-at`'s character-based indexing. Fixes out-of-bounds crashes on non-ASCII strings.
- **Lexer UTF-8 String Support** — `lex_string` in `crates/airl-syntax/src/lexer.rs` now properly decodes multi-byte UTF-8 characters in string literals, instead of treating each byte as a separate `char`. Detects UTF-8 sequence length from leading byte and uses `std::str::from_utf8` to decode.
- **TCO Through Match/Let Arms** — `match` and `let` body evaluation in `eval.rs` now uses inline trampolines that preserve `in_tail_context`, enabling tail-call optimization for recursive functions that recurse inside `match` arms (e.g., `lex-loop`). Previously, these called `eval()` which cleared the tail context flag.
- **Bootstrap Self-Parse Milestone** — The bootstrap lexer can lex its own source (`bootstrap/lexer.airl`, 15,691 chars → 3,400 tokens). Timing: ~56s release, ~100s debug.
- **Bootstrap Type Checker** — Type checker implemented in AIRL (`bootstrap/types.airl` ~215 lines, `bootstrap/typecheck.airl` ~500 lines). Two-pass architecture: registration (deftype → constructor registry, defn → function signatures) then checking (expressions, functions, patterns). Eliminates all `Any` usage from bootstrap code (95 in eval, 24 in parser, 1 in lexer). Lexer type-checks cleanly via the bootstrap type checker.
- **`deftype` Parsing** — Bootstrap parser handles `(deftype Name [Params] (| ...))` sum types and `(deftype Name (& ...))` product types. Includes `parse-variant`, `parse-field`, `parse-sum-body`, `parse-product-body`, `parse-type-params`, `parse-deftype`. Bootstrap lexer updated to include `|` in symbol characters.
- **IR VM** — Tree-flattened IR format (`crates/airl-runtime/src/ir.rs`), Rust VM (`ir_vm.rs`) with self-TCO, value-to-IR marshalling (`ir_marshal.rs`), `run-ir` builtin. Bootstrap AIRL compiler (`bootstrap/compiler.airl`) transforms AST to IR. Rust-side compiler in `pipeline.rs` for native-speed compilation. `--compiled` flag on `cargo run -- run` for compiled execution mode. `IRClosure`/`IRFuncRef` value variants for first-class functions in compiled code.
- **Bootstrap Fixpoint Verification** — Functional equivalence test (`bootstrap/equivalence_test.airl`, 32 tests) proves interpreted eval and compiled run-ir produce identical results across literals, arithmetic, control flow, functions, recursion, pattern matching, closures, higher-order functions, and lists. Compiler fixpoint test (`bootstrap/fixpoint_test.airl`) proves the compiled compiler produces identical IR to the interpreted compiler — Tier 1 (small program) and Tier 2 (compiler self-compilation). IR serializer (`ir-to-string`) for deterministic comparison. The compiler has reached fixpoint: compiler₁ compiling compiler.airl = compiler₂ compiling compiler.airl.
- **Register-Based Bytecode VM** — Flat bytecode instruction set (~34 opcodes), register-based compiler (`bytecode_compiler.rs`) with linear register allocation, bytecode VM (`bytecode_vm.rs`) with tight execution loop and self-TCO. `--bytecode` flag on `cargo run -- run`. Pipeline integration in `pipeline.rs` with `run_source_bytecode()`.
- **Bytecode→Cranelift JIT** — JIT compilation of eligible bytecode functions to native x86-64 via Cranelift (`bytecode_jit.rs`). Primitive-typed functions (no lists/variants/closures) are compiled eagerly at load time. Transparent fallback to bytecode for ineligible functions. Behind `#[cfg(feature = "jit")]` — zero overhead when disabled.
- **v0.2 Execution Consolidation** — Bytecode VM is now the default execution path (was tree-walking interpreter). Contracts (`:requires`/`:ensures`/`:invariant`) compiled to bytecode assertion opcodes (`AssertRequires`/`AssertEnsures`/`AssertInvariant`) — always enforced, no opt-out. IR VM and `--compiled` flag removed. `--bytecode` flag removed (bytecode is the default). JIT is the default when built with `--features jit`.
- **Contract-Aware JIT** — Contract assertion opcodes compile to native conditional branches via Cranelift. Happy path: one branch instruction (predicted taken, ~free). Sad path: calls `airl_jit_contract_fail` runtime helper, stores error in thread-local cell, VM propagates as `ContractViolation`. fib(30) with contracts: 13ms (19x faster than Python). Linearity errors now fatal in Run mode (bytecode VM doesn't enforce ownership at runtime).

---

## Remaining Tasks

### Tier 1 — Long-term

#### 1. Bootstrap Compiler (not yet self-hosting)

**Status:** Compiler front-end phases implemented in AIRL, running on the Rust runtime. The bootstrap lexer (`bootstrap/lexer.airl`, ~365 lines) tokenizes AIRL source strings. The bootstrap parser (`bootstrap/parser.airl`, ~930 lines) converts token streams to typed AST nodes, including `deftype` declarations. The bootstrap evaluator (`bootstrap/eval.airl`, ~616 lines) interprets AST nodes using tagged value variants (`ValInt`, `ValStr`, etc.), a map-based environment frame stack, and builtin delegation to the Rust runtime. The bootstrap type checker (`bootstrap/types.airl` + `bootstrap/typecheck.airl`, ~715 lines) enforces the type system with a two-pass architecture. The full lex→parse→eval pipeline is tested by `bootstrap/pipeline_test.airl`.

**Self-parse verified:** The bootstrap lexer successfully lexes its own source (15,691 chars → 3,400 tokens, ~56s release). TCO through `match`/`let` arms is required for this to work — without it, `lex-loop`'s recursion overflows the stack.

**Type-check verified:** All three bootstrap modules (lexer, parser, eval) pass the bootstrap type checker cleanly. All `Any` annotations have been eliminated from the bootstrap codebase (~120 replacements). The integration tests parse each module through the bootstrap parser and type-check the AST — slow (~5-10 min total in release mode) but comprehensive.

**Not self-hosting:** The Rust runtime is still required — ~48 builtins (arithmetic, string ops, map ops, list primitives, I/O) are implemented in Rust and called by the bootstrap code. True self-hosting would require a native code generator backend that can emit machine code or link against a minimal runtime.

**Next steps:** Potential future work includes a native code generator (e.g., extending the bytecode→Cranelift JIT to AOT compilation with runtime helper calls for non-primitive operations).

---

## Bootstrap Compiler

The bootstrap compiler (AIRL compiler phases implemented in AIRL, running on the Rust runtime) lives in `bootstrap/`. Run tests with:
```bash
cargo run --release --features jit -- run bootstrap/lexer_test.airl       # Lexer tests
cargo run --release --features jit -- run bootstrap/parser_test.airl      # Parser unit tests
cargo run --release --features jit -- run bootstrap/integration_test.airl # Parser integration tests
cargo run --release --features jit -- run bootstrap/eval_test.airl        # Evaluator unit tests
cargo run --release --features jit -- run bootstrap/pipeline_test.airl    # Full lex→parse→eval pipeline tests
cargo run --release --features jit -- run bootstrap/deftype_test.airl     # Deftype parsing tests
cargo run --release --features jit -- run bootstrap/types_test.airl       # Type representation tests
cargo run --release --features jit -- run bootstrap/typecheck_test.airl   # Type checker tests (slow)
cargo run --release --features jit -- run bootstrap/compiler_test.airl    # IR compiler unit tests
cargo run --release --features jit -- run bootstrap/compiler_integration_test.airl  # IR compiler integration tests
cargo run --release --features jit -- run bootstrap/equivalence_test.airl           # Interpreted vs compiled equivalence (32 tests)
cargo run --release --features jit -- run bootstrap/fixpoint_test.airl              # Compiler fixpoint test (slow, ~60min release)
```

**Important AIRL constraints for bootstrap code:**
- `and`/`or` are **eager** (not short-circuit) — use nested `if` for bounds-safe lookahead
- No mixed int/float arithmetic — use `int-to-float` and `digit-value-f` helpers
- No import system — test files must contain all function definitions
- Self-TCO works through `match`/`let` arms — tail-recursive loops like `lex-loop` and `parse-loop` won't overflow the stack

---

## Known Issues

1. **`airl-mlir` requires system libraries:** `melior-macro` needs `libzstd-dev` and LLVM 19+ installed. Use `cargo test --workspace --exclude airl-mlir` if not available. See comment in workspace `Cargo.toml`.
2. **Quantifier expressions (`forall`/`exists`) are interpreter-only.** The bytecode compiler maps them to `nil`. Z3 verification of quantifier contracts still works at check time; runtime evaluation requires `--interpreted` flag.
3. **Runtime ownership enforcement is static-only in v0.2.** The linearity checker catches use-after-move at compile time, but the bytecode VM does not track ownership at runtime. Linearity errors are fatal in Run mode to compensate.
4. **JIT ineligibility for non-primitive functions.** Functions using lists, closures, variants, or builtin calls fall back to the bytecode VM. This is transparent but significantly slower for compute-heavy code (~23x slower than Python on fib(30) scale).

