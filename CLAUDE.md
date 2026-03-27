# AIRL — Project Instructions for Claude

## Project Overview

AIRL (AI Intermediate Representation Language) is a programming language designed for AI systems. It's a Rust Cargo workspace with 9 crates, ~520 tests, ~19K lines of Rust + ~21K lines of AIRL. Version 0.6.0. **Self-hosted:** the G3 compiler is written in AIRL and produces native binaries.

**Language spec:** `AIRL-Language-Specification-v0.1.0.md`
**LLM header:** `AIRL-Header.md` — **MUST read before writing any AIRL code.** Compressed reference with all traps, syntax, signatures, and patterns (~360 lines, ~3K tokens). Replaces the 7-file pre-flight checklist.
**Full LLM guide:** `AIRL-LLM-Guide.md` — Verbose human-readable reference with examples. Read for deep understanding; `AIRL-Header.md` is sufficient for code generation.
**Stdlib docs:** `stdlib/*.md` — Detailed docs with examples. Read for edge cases beyond what the header covers.
**Design specs:** `docs/superpowers/specs/`
**Implementation plans:** `docs/superpowers/plans/`

## Pre-Flight Checklist (BLOCKING)

**Before writing or modifying ANY `.airl` file, you MUST have read the following in the current conversation. No exceptions.**

1. `AIRL-Header.md` — **entire file**. Contains all traps, syntax rules, function signatures, and patterns. This single file replaces reading 7 separate files.

**If you have not read the file above, STOP and read it now before proceeding.**

*For deeper understanding, the full reference files are also available:*
- `AIRL-LLM-Guide.md` — verbose guide with examples
- `stdlib/*.md` — detailed stdlib docs (collections, math, result, string, map, set)

## Build & Test

```bash
cargo build --features jit,aot          # Build all crates with JIT + AOT (recommended)
cargo build --features jit              # Build with JIT only (no AOT compile command)
cargo build                             # Build without JIT (bytecode-only, no Cranelift)
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver  # Run all ~508 tests
cargo run --features jit -- run <file.airl>            # Execute an AIRL program (JIT default)
cargo run --features jit,aot -- compile <file.airl> -o <binary>  # AOT compile to native executable
cargo run --features jit -- check <file.airl>          # Type-check and verify
cargo run --features jit -- repl                       # Interactive REPL
```

**Execution model (v0.6.0):** Three paths, one runtime. `airl run` JIT-compiles all functions to native x86-64 via Cranelift. `airl compile` AOT-compiles to standalone native executables. `./g3 --` uses the self-hosted AIRL compiler (bootstrap lexer/parser/bc_compiler → Cranelift AOT). All paths call builtins via `extern "C"` functions in `crates/airl-rt/` (Rust). VM-aware builtins provide native `map`/`filter`/`fold`/`sort` with IntList specialization and inline closure compilation. Contract assertions and ownership checks compile to native conditional branches. Thread-per-task concurrency with message-passing channels.

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
- **Builtin dispatch pattern:** Builtins are dispatched via `CallBuiltin` opcode in the bytecode VM, which calls into `Builtins::get()` registry. The tree-walking interpreter (`eval.rs`) is kept for the REPL and agent runtime but is not the default execution path.
- **Multi-binding `let` is the preferred style.** Use `(let (x : T v1) (y : T v2) body)` instead of nested single-binding lets. The parser, bytecode compiler, and JIT all support it.

## Standard Library

**Location:** `stdlib/` directory (embedded in binary via `include_str!`, auto-loaded before user code)

The stdlib is 6 modules (68 functions total) — mostly pure AIRL, with Rust builtins for list destructuring, string character access, file I/O, float math, and HTTP.

### Primitive Builtins (Rust)

**List builtins** (7) in `crates/airl-rt/`:
- `head` — first element of list (errors on empty)
- `tail` — all but first element (errors on empty)
- `empty?` — is list empty? → Bool
- `cons` — prepend element to front of list → List
- `at-or` — `(at-or list idx default)` safe indexing, returns default on out-of-bounds
- `set-at` — `(set-at list idx val)` immutable update at index, returns new list
- `list-contains?` — `(list-contains? list val)` element membership check → Bool

**String builtins** (17) in `crates/airl-rt/`:
- `str` — **variadic string concatenation with auto-coercion**. `(str "count: " 42 " done")` → `"count: 42 done"`. Strings included as-is (no quotes); all other types auto-coerced via Display.
- `char-at`, `substring`, `chars` — character access (Unicode-safe)
- `split`, `join` — split/join strings
- `contains`, `starts-with`, `ends-with`, `index-of` — search
- `trim`, `to-upper`, `to-lower`, `replace` — transformation
- `char-code` — `(char-code "A")` → `65` (Unicode codepoint of first character)
- `char-from-code` — `(char-from-code 65)` → `"A"` (1-char string from codepoint)

**Map builtins** (10) in `crates/airl-rt/`:
- `map-new`, `map-from` — creation
- `map-get`, `map-get-or`, `map-has`, `map-size` — reading
- `map-set`, `map-remove` — mutation (returns new map)
- `map-keys`, `map-values` — enumeration

**File I/O builtins** (11) in `crates/airl-rt/`:
- `read-file`, `write-file`, `append-file` — read/write/append file contents
- `file-exists?`, `is-dir?` — path queries
- `delete-file`, `delete-dir` — removal (delete-file rejects directories)
- `rename-file` — rename/move files and directories
- `read-dir` — list directory entries (sorted) → List[Str]
- `create-dir` — create directory recursively (idempotent)
- `file-size` — size in bytes → Int
- All paths sandbox-validated: no absolute paths, no `..`

**Float math builtins** (15) in `crates/airl-rt/`:
- `sqrt`, `sin`, `cos`, `tan`, `log` (natural), `exp` — transcendentals (accept Int or Float)
- `floor`, `ceil`, `round` — rounding (return Int)
- `float-to-int`, `int-to-float` — explicit numeric conversion
- `infinity`, `nan` — IEEE 754 special values (0-arg constructors)
- `is-nan?`, `is-infinite?` — IEEE 754 predicates → Bool

**Error handling builtins** (2) in `crates/airl-rt/`:
- `panic` — `(panic msg)` abort execution with custom error message
- `assert` — `(assert condition msg)` abort if condition is false, returns `true` if passes

**Type conversion builtins** (5) in `crates/airl-rt/`:
- `int-to-string`, `float-to-string` — numeric to string
- `string-to-int`, `string-to-float` — string to numeric (returns Result)
- `type-of` — returns type name as string

**Network/JSON builtins** (3) in `crates/airl-rt/`:
- `http-request` — `(http-request method url body headers)` → Result[Str, Str]. Supports GET, POST, PUT, DELETE, PATCH, HEAD.
- `json-parse`, `json-stringify` — JSON ↔ AIRL value conversion

**Byte encoding builtins** (11) in `crates/airl-rt/`:
- `bytes-from-int16`, `bytes-from-int32`, `bytes-from-int64` — integer to big-endian byte list (IntList)
- `bytes-to-int16`, `bytes-to-int32`, `bytes-to-int64` — decode integer from byte list at offset
- `bytes-from-string` — UTF-8 encode string to bytes. `bytes-to-string` — UTF-8 decode bytes to string
- `bytes-concat` — concatenate byte lists. `bytes-slice` — extract slice with bounds check
- `bytes-concat-all` — `(bytes-concat-all parts)` concatenate List[IntList] in one O(n) pass
- `crc32c` — CRC32C (Castagnoli) checksum

**TCP socket builtins** (8) in `crates/airl-rt/`:
- `tcp-listen` — `(tcp-listen port backlog)` → Result[Int, Str]. Bind + listen, returns server handle.
- `tcp-accept` — `(tcp-accept handle)` → Result[Int, Str]. Blocking accept, returns connection handle.
- `tcp-connect` — `(tcp-connect host port)` → Result[Int, Str]. Returns handle for connection.
- `tcp-close` — close connection or listener by handle
- `tcp-send` — `(tcp-send handle data)` send IntList bytes, returns bytes sent
- `tcp-recv` — receive up to max-bytes. `tcp-recv-exact` — receive exactly n bytes or error
- `tcp-set-timeout` — set read/write timeout in milliseconds (≤0 = no timeout)

**Thread/channel builtins** (10) in `crates/airl-rt/`:
- `thread-spawn` — `(thread-spawn closure)` → Int. Spawn thread running 0-arg closure, returns handle.
- `thread-join` — `(thread-join handle)` → Result. Block until done. Ok(value) or Err(msg).
- `thread-set-affinity` — `(thread-set-affinity core-id)` → Result[Nil, Str]. Pin calling thread to CPU core (Linux).
- `channel-new` — `(channel-new)` → [sender-handle receiver-handle]. Unbounded mpsc channel.
- `channel-send`, `channel-recv`, `channel-recv-timeout`, `channel-drain`, `channel-close` — message-passing operations.

**System builtins** (7) in `crates/airl-rt/`:
- `cpu-count` — `(cpu-count)` → Int. Available parallelism (logical CPU count).
- `shell-exec` — `(shell-exec cmd args-list)` → Result with stdout/stderr/exit-code
- `time-now` — milliseconds since epoch → Int
- `sleep` — `(sleep ms)` pause execution for N milliseconds → Nil
- `format-time` — `(format-time millis fmt)` format UTC timestamp. Supports `%Y %m %d %H %M %S`.
- `getenv` — `(getenv "VAR")` → Result[Str, Str]
- `get-args` — command-line arguments → List[Str]

### Stdlib Modules (Pure AIRL)

**Collections** (`stdlib/prelude.airl`) — 18 functions:

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
| `unique` | `(unique xs)` | Remove duplicate elements |
| `enumerate` | `(enumerate xs)` | Pair each element with its 0-based index |
| `group-by` | `(group-by f xs)` | Group elements by key function → Map |

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

**Set** (`stdlib/set.airl`) — 11 AIRL functions (implemented over maps):

| Function | Signature | Description |
|----------|-----------|-------------|
| `set-new` | `(set-new)` | Create empty set |
| `set-from` | `(set-from xs)` | Create set from list (dedup) |
| `set-add` | `(set-add s x)` | Add element |
| `set-remove` | `(set-remove s x)` | Remove element |
| `set-contains?` | `(set-contains? s x)` | Check membership |
| `set-size` | `(set-size s)` | Number of elements |
| `set-to-list` | `(set-to-list s)` | Convert to list |
| `set-union` | `(set-union a b)` | Union |
| `set-intersection` | `(set-intersection a b)` | Intersection |
| `set-difference` | `(set-difference a b)` | Difference (a \ b) |
| `set-subset?` | `(set-subset? a b)` | Subset check |
| `set-equal?` | `(set-equal? a b)` | Equality check |

**Note:** Set elements must be strings (AIRL map keys are string-only). See `stdlib/set.md`.

### Prelude Loading

- Embedded via `include_str!()` in `crates/airl-driver/src/pipeline.rs`
- Stdlib is compiled to bytecode and loaded via `compile_and_load_stdlib_bytecode()` before user code
- Called in `run_source_with_mode()`, `run_source_bytecode()`, JIT pipelines, and REPL startup
- **Load order matters:** math depends on collections (`fold`), string depends on collections (`filter`, `reverse`), set depends on map + collections (`fold`, `all`)
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
- **Contract-Aware JIT** — Contract assertion opcodes compile to native conditional branches via Cranelift. Happy path: one branch instruction (predicted taken, ~free). Sad path: calls `airl_jit_contract_fail` runtime helper, stores error in thread-local cell, VM propagates as `ContractViolation`. fib(30) with contracts: 13ms (19x faster than Python).
- **Runtime Ownership Tracking** — `MarkMoved` and `CheckNotMoved` bytecode opcodes for runtime move enforcement. The bytecode compiler emits ownership checks around calls to functions with `Own`-annotated parameters: `CheckNotMoved` before the call (use-after-move detection), `MarkMoved` after (marks register consumed), and conflict detection for borrow+move on the same register. Subsequent `Load` of a moved variable emits `CheckNotMoved`. Pipeline builds ownership map from AST parameter annotations and passes it to the bytecode compiler. Static linearity checker is now non-fatal in Run mode (warns only); runtime enforcement is the backstop. Functions with ownership opcodes are JIT-ineligible (fall back to bytecode VM). Fixtures moved from `interpreter_only/` to `linearity_errors/`.
- **Unboxed AOT Compilation** — Two-tier AOT compiler in `bytecode_aot.rs`. Eligible functions (pure arithmetic, no lists/variants/closures/builtins, arity ≤ 8) compile to raw `i64`/`f64` register operations — arithmetic is single CPU instructions, no heap allocation. Ineligible functions compile with boxed `*mut RtValue` (existing path). `is_eligible()` checks opcodes and recursively validates cross-function calls. `compile_func_unboxed()` ports the JIT's unboxed compilation to `ObjectModule`. Boundary marshaling in `compile_func()` extracts raw values from boxed args via `airl_as_int_raw`, calls the unboxed function, and reboxes the result using `eligible_return_hints`. Added `airl_as_int_raw` and `airl_as_float_raw` to `airl-rt`. Performance: fib(35) AOT unboxed 56ms vs Python 2,335ms (**42x faster**), vs boxed AOT ~16s (**~290x speedup** from unboxing).
- **File I/O Builtins** — 8 new builtins: `append-file`, `delete-file`, `delete-dir`, `rename-file`, `read-dir`, `create-dir`, `file-size`, `is-dir?`. All sandbox-validated (no absolute paths, no `..`). Registered in bytecode VM, JIT-full, and AOT. `extern "C"` counterparts in `airl-rt`. Also backfilled missing `airl_write_file` and `airl_file_exists` in `airl-rt` (were declared in AOT but never defined). Total file I/O builtins: 11 (`read-file`, `write-file`, `append-file`, `file-exists?`, `delete-file`, `delete-dir`, `rename-file`, `read-dir`, `create-dir`, `file-size`, `is-dir?`).
- **Execution Path Consolidation** — Deleted 1,422 lines of dead code: primitive JIT (`bytecode_jit.rs`, 951 lines), orphaned AST-level JIT (`airl-codegen/jit.rs`, 362 lines), dead pipeline functions (`run_file_jit`/`run_source_jit`). Extracted shared contract-error signaling into `jit_contract.rs`. Removed primitive JIT field and dispatch from bytecode VM. v0.2.1.
- **HTTP Request Builtin** — Generic `(http-request method url body headers)` supporting GET, POST, PUT, DELETE, PATCH, HEAD. Returns `Result[Str, Str]`. Uses `ureq` in Rust.
- **Character Code Builtins** — `char-code` (string → Unicode codepoint as Int), `char-from-code` (Int codepoint → 1-char string), `string-to-float` (parse float strings → Result). Unlocks `parse_int` and cipher algorithms that were impossible without character-to-digit conversion.
- **Float Math Builtins** — 15 builtins: transcendentals (`sqrt`, `sin`, `cos`, `tan`, `log`, `exp`), rounding (`floor`, `ceil`, `round` → Int), conversion (`float-to-int`, `int-to-float`), IEEE 754 (`infinity`, `nan`, `is-nan?`, `is-infinite?`). All accept Int or Float via `as_float` auto-coercion. New `crates/airl-rt/src/math.rs` module.
- **Collection Builtins** — 3 Rust builtins: `at-or` (safe indexing with default), `set-at` (immutable update at index), `list-contains?` (element membership). 3 AIRL stdlib functions: `unique` (deduplicate), `enumerate` (zip-with-index), `group-by` (group elements by key function → Map).
- **Error Handling Builtins** — `panic` (abort with custom message) and `assert` (abort if condition false). Provides explicit error paths beyond contract violations.
- **Time/Date Builtins** — `sleep` (pause N milliseconds) and `format-time` (format Unix timestamp with `%Y %m %d %H %M %S` specifiers, UTC, zero external deps — uses Howard Hinnant civil calendar algorithm).
- **Set Data Structure** — 11 AIRL stdlib functions in `stdlib/set.airl`: `set-new`, `set-from`, `set-add`, `set-remove`, `set-contains?`, `set-size`, `set-to-list`, `set-union`, `set-intersection`, `set-difference`, `set-subset?`, `set-equal?`. Implemented over maps (keys with `true` values). Elements must be strings. Auto-loaded as prelude.
- **v0.3.0 Builtins & Performance** — 20 new builtins (path, regex, crypto, read-lines, char-count), expanded unboxed AOT for native list ops, COW tail views + in-place append, VM-aware builtins for native `map`/`filter`/`fold`/`sort` (bypass AIRL stdlib recursion for 10-100x speedup on large lists).
- **v0.3.1 Specialization** — Inline closures compiled to native code (no closure allocation for simple lambdas), IntList specialization (unboxed `Vec<i64>` storage for integer-only lists), 7 new builtins.
- **v0.4.0 Pattern Detection** — Compound predicate detection for filter specialization (e.g., `(fn [x] (and (> x 0) (< x 10)))` compiled to native branch chain). Closure pattern detection for fold/map/filter (recognizes common lambda shapes and emits specialized native loops).
- **JIT-Full Bug Fixes** — All 5 JIT-full bugs resolved: variant tag string corruption (intern strings to stable storage), Cranelift verifier errors (proper block terminators), closure dispatch (compile MakeClosure targets in dependency pass), MakeClosure captures (read capture_count from function metadata), variadic print arity (airl_print_values runtime function).
- **AIRL Header File** — Token-efficient LLM reference (`AIRL-Header.md`, ~360 lines / ~3K tokens) replacing 7-file pre-flight checklist (~2,105 lines / ~15K tokens). 5.4x compression with zero information loss on critical semantics.
- **Byte Encoding Builtins** — 11 builtins for binary data: `bytes-from-int16`/`int32`/`int64` (big-endian encode), `bytes-to-int16`/`int32`/`int64` (decode from offset), `bytes-from-string`/`bytes-to-string` (UTF-8 encode/decode), `bytes-concat`, `bytes-slice`, `crc32c` (CRC32C checksum). Byte sequences represented as `IntList` (list of integers 0-255).
- **TCP Socket Builtins** — 6 builtins for handle-based TCP networking: `tcp-connect` (connect to host:port, returns handle), `tcp-close`, `tcp-send` (send byte list), `tcp-recv` (receive up to N bytes), `tcp-recv-exact` (receive exactly N bytes), `tcp-set-timeout`. All return `Result`. Thread-safe global handle map using `Mutex` + `OnceLock`.
- **Thread-per-Task Concurrency (v0.5.0)** — 7 builtins for OS-level threading with message-passing channels. `thread-spawn` creates a child BytecodeVm (cloned function registry, fresh builtins/call stack, shared JIT via `Arc<BytecodeJitFull>`), spawns it on a new OS thread with 64MB stack. `thread-join` returns `Result[Value, Str]` (propagates runtime errors as Err). Channels use `std::sync::mpsc` (unbounded, single consumer). `channel-new` returns `[sender receiver]` handle pair. `channel-send`/`channel-recv`/`channel-recv-timeout`/`channel-close` for message passing. No shared mutable state — channels are the only inter-thread communication. Handle-based design follows TCP pattern (global `Mutex<HashMap>` registries with `AtomicI64` counters).
- **C Runtime Retired (v0.5.0)** — Deleted `runtime/` directory (~5,148 lines of C). The C runtime (`libairl_rt_c.a`) was a parallel reimplementation of builtins already covered by the Rust `airl-rt` crate (strict superset: 123 shared + 36 Rust-only functions). It existed to support the bootstrap C codegen backend (`bootstrap/codegen_c.airl`), which is superseded by Cranelift AOT (`airl compile`). Two-path architecture established: `builtins.rs` (VM) + `crates/airl-rt/` (AOT). Bootstrap C codegen files marked deprecated.
- **AIRL-Forge Phase 1** — `fn-metadata` builtin for runtime function introspection (`FnDefMetadata` struct threaded through bytecode compilation pipeline, VM-dispatched builtin returns Map with name/intent/param-names/param-types/return-type/requires/ensures). 6 AIRL library modules in `lib/forge/`: codec (JSON marshalling with key validation), schema (AIRL type → JSON Schema conversion), tools (registry with auto-discovery via `:intent` + `fn-metadata`), provider (Anthropic API abstraction with message formatting and response extraction), validate (predicate/key/type validation with retry feedback loop), chain (sequential pipelines via fold + fan-out). Loaded via `--load` flags. Design spec: `docs/superpowers/specs/2026-03-23-airl-forge-design.md`.

- **Embedded Runtime (v0.5.2)** — `libairl_rt.a` is gzip-compressed at build time via `build.rs` and embedded in the `airl` binary via `include_bytes!`. At `airl compile` time, extracted to temp file for linking. Enables self-contained compiler: build once with `cargo build --features jit,aot`, then `airl compile` works anywhere with just `cc`. No Rust toolchain needed at the target.
- **Builtin Safety Net (v0.5.2)** — JIT-full and AOT silent nil fallback for unregistered builtins replaced with hard errors (`return Err(...)` instead of emitting nil). Catches missing builtin registrations as compile-time errors instead of silent wrong answers.

- **G3 Self-Hosted Compiler (v0.5.2)** — `bootstrap/g3_compiler.airl` (124 lines) is an AIRL compiler written entirely in AIRL. Pipeline: source → bootstrap lexer → parser → bc_compiler → BCFunc → `compile-bytecode-to-executable` (Cranelift AOT + embedded runtime) → native binary. Includes stdlib compilation (6 modules, 86 functions). New `compile-bytecode-to-executable` builtin takes BCFunc values + output path and produces linked native executables. Cranelift is a builtin, not reimplemented in AIRL (like Go's assembler is part of the Go toolchain). Usage: `airl run --load bootstrap/lexer.airl --load bootstrap/parser.airl --load bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl -- input.airl -o output`.

---

## Milestones

### G3 Self-Hosted Compiler (v0.5.2) — ACHIEVED

**The AIRL compiler is self-hosted.** The G3 compiler (`bootstrap/g3_compiler.airl`) is written entirely in AIRL and produces native binaries. All compilation logic — lexing, parsing, bytecode compilation, stdlib compilation — is AIRL code. Cranelift (native code generation) and `libairl_rt.a` (runtime) are exposed as builtins embedded in the `airl` binary, same as Go's assembler is part of the Go toolchain.

**Pipeline:** AIRL source → bootstrap lexer (AIRL) → parser (AIRL) → bc_compiler (AIRL) → BCFunc bytecode → `compile-bytecode-to-executable` builtin (Cranelift AOT) → native binary.

**What works:** Contracts, closures, pattern matching, all stdlib functions (sum-list, product-list, abs, map, filter, fold, sort, etc.), cross-module function resolution.

**External dependency:** System C linker (`cc`) for final linking. Present on all Linux/macOS systems.

### Self-Compilation — ACHIEVED (v0.6.0)

**The AIRL compiler compiles itself.** The G3 binary (39MB, ~23 min compile time) correctly compiles and runs all 58 AOT test programs covering bootstrap compiler, 68 stdlib functions, and 90+ Rust builtins.

**Bug that blocked self-compilation (fixed in v0.6.0):** `bc-compile-if` in `bootstrap/bc_compiler.airl` freed the condition register back to `dst+1` instead of `cond-reg`, allowing subsequent register allocations to overwrite let-bound variables. This caused `airl_add: type mismatch` when string concat and int arithmetic coexisted in let bodies under if branches. Single-line fix.

**Earlier bugs fixed (v0.5.2):**
- `compile_bytecode_to_executable` silently dropped AOT compilation errors — functions could be declared but never defined (segfault). Fixed: errors now propagate.
- Variadic `print` arity conflict — 1-arg call registered wrong Cranelift signature for multi-arg calls. Fixed: variadic check no longer depends on `call_targets` state.

**AOT unification (v0.6.0):** Deleted `builtins.rs` (-5,472 lines). VM now uses `RtValue` registers and calls the same `extern "C"` functions as AOT. Single builtin dispatch path eliminates the representation divergence that caused G3 crashes.

### Remaining — Next Steps

1. **Eliminate `cc` dependency** — Replace system linker with Cranelift native ELF emission or bundled linker. Zero external dependencies.
2. **macOS/ARM support** — Make Cranelift target triple dynamic (currently hardcoded to x86-64-linux). ~50 lines of changes + testing on Apple Silicon.
3. **Z3 verification depth** — Extend Z3 to prove list and ADT properties.
4. **G3₂ fixpoint** — Verify G3₁ compiles G3 → G3₂, G3₂ compiles G3 → G3₃, G3₂ == G3₃ (binary-level fixpoint).

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

# G2 AOT test suite (58 tests — compile via bootstrap compiler, run as native binaries)
bash tests/aot/run_aot_tests.sh
```

**Important AIRL constraints for bootstrap code:**
- `and`/`or` are **eager** (not short-circuit) — use nested `if` for bounds-safe lookahead
- No mixed int/float arithmetic — use `int-to-float` and `digit-value-f` helpers
- No import system — test files must contain all function definitions
- Self-TCO works through `match`/`let` arms — tail-recursive loops like `lex-loop` and `parse-loop` won't overflow the stack

---

## Known Issues

1. **`airl-mlir` requires system libraries:** `melior` needs `libzstd-dev` and LLVM 19+ (`llvm-19-dev libmlir-19-dev`). The crate is excluded from `default-members` so plain `cargo build` / `cargo test` skip it automatically. To build with GPU/MLIR support: `cargo build -p airl-driver --features mlir` (set `MLIR_SYS_190_PREFIX=/usr/lib/llvm-19` if needed). A `Dockerfile` at the workspace root provides a fully reproducible build environment with all dependencies pre-installed (`docker build -t airl .`). The `build.rs` in `crates/airl-mlir/` detects missing LLVM and prints actionable install instructions before the melior linker error fires.
2. **Runtime errors in JIT'd `extern "C"` functions are non-recoverable.** Runtime errors (e.g., type mismatches in `airl_mul`) in `extern "C"` JIT helper functions call `process::exit(1)`. These are prevented at compile time by the type checker, but a future improvement would use `extern "C-unwind"` ABI for recovery.

