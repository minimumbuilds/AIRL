# AIRL — AI Intermediate Representation Language

**v0.11.1** — AIRL is a new category of software artifact: an **LLM-native executable language**. It is designed to be synthesized by a language model and compiled directly to native code — with no human authoring step in between. Initial benchmarking shows LLMs generate equivalent programs in AIRL using roughly **half the tokens** they would in Python — a structural property of the language, not the model. While AIRL is not intended to be written by humans, it is deliberately human-readable: a developer can inspect, audit, and reason about synthesized AIRL without tooling.

```clojure
(defn safe-divide
  :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
  :intent "Divide a by b, returning Err on division by zero"
  :requires [(valid a) (valid b)]
  :ensures [(match result
              (Ok v)  (= (* v b) a)
              (Err _) (= b 0))]
  :body (if (= b 0) (Err :division-by-zero) (Ok (/ a b))))

(safe-divide 9 3)  ;; → (Ok 3)
```

## A New Category

Every existing programming language optimizes for human readability and human authorship. AIRL does not. It is **synthesis-first**: the primary author is an LLM, the primary consumer is a compiler.

This makes AIRL something more specific than a programming language:

> **An LLM output format that compiles directly to executable code.**

Think of it as the relationship between protobuf and JSON — not a replacement for general-purpose languages, but a purpose-built format for a specific exchange: *intent → synthesis → execution*. The human expresses intent in natural language. The LLM synthesizes AIRL. The compiler produces a native binary. The human never touches the AIRL source.

The design-by-contract layer (`:requires` / `:ensures`) is not documentation — it is a **verifiable interface** between the synthesis step and the execution step. Contracts the LLM generates are checked by the compiler; those that cannot be proven statically become native conditional branches at runtime.

## Why Not Python

Python is the dominant LLM output format today — by accident, not design. It is human-readable, has a vast ecosystem, and the models know it well. But it optimizes for humans, which has a measurable cost when used as a machine output format.

Benchmarking an **untrained** model (qwen3-coder, zero AIRL exposure) against Python across 19 tasks:

| Metric | AIRL | Python |
|--------|------|--------|
| Completion tokens (avg/task) | 122 | 283 |
| **Token ratio** | **0.43×** | 1.0× |
| Correctness (zero-shot) | 74% | 100% |

AIRL uses **43% of the completion tokens Python does** — a structural property of the language, not the model. The 74% zero-shot correctness is a floor: with a model trained on AIRL, correctness is expected to reach parity or better. The token efficiency advantage is permanent — it comes from the language design, not the model's familiarity with it.

Fewer tokens has three compounding effects:

- **Lower cost** — API pricing is per token. At 0.43× token usage, generating the same program in AIRL costs roughly half as much. At scale across thousands of agentic tasks, that difference is material.
- **Faster generation** — LLMs generate tokens sequentially. Fewer tokens means faster time-to-result, directly — not through any architectural change, just by saying the same thing in less space.
- **Lower energy use** — Token generation is the dominant energy cost of LLM inference. Generating half the tokens to produce equivalent code consumes roughly half the energy per inference call. Across large-scale deployments, AIRL's structural conciseness translates directly to a smaller compute and carbon footprint.

## What It Solves

- **Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. LLMs skip optional features; they cannot skip grammar requirements. Synthesis correctness is structurally enforced.
- **S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient.
- **Messages are programs** — Agents exchange AIRL source as both the message format and the execution format. No protobuf, no gRPC, no separate serialization layer.
- **Formal verification** — Z3 SMT solver proves contracts at compile time. What cannot be proven is checked at runtime as native conditional branches.
- **Compiled to native** — Not interpreted, not sandboxed. AIRL compiles to native x86-64 and ARM64 binaries via Cranelift. 42× faster than Python on pure arithmetic.
- **Linear ownership** — Rust-style move semantics with static linearity analysis. No garbage collector.
- **Self-hosted** — The compiler is written in its own language (since v0.6.0). Fixpoint verified.

## Build

### Using the host binary (development)

```bash
# Fresh checkout — three steps required
cargo build -p airl-rt --release                 # 1. Build runtime library first
cargo clean -p airl-runtime --release            # 2. Force build.rs to re-run
cargo build --release --features aot            # 3. Full build (embeds libairl_rt.a)

# Run a program
cargo run --release --features aot -- run examples/01-hello-world/hello_world.airl

# Compile to native binary
cargo run --release --features aot -- compile examples/02-functions-and-contracts/functions_and_contracts.airl -o my_program
./my_program

# Type-check and verify contracts with Z3
cargo run --release --features aot -- check examples/03-verified-arithmetic/verified_arithmetic.airl
```

**Requirements:** Rust 1.85+, CMake, C++ compiler, Python 3 (for Z3; first build ~5-15 min).

**macOS:** `xcode-select --install`, `brew install cmake z3`, then `export LIBRARY_PATH="$(brew --prefix z3)/lib"` (add to your shell profile).

### Building the G3 self-hosted compiler

```bash
# Rebuild G3 (~1 min)
bash scripts/build-g3.sh

# Use G3 to compile AIRL programs (no Rust toolchain needed)
./g3 -- app.airl -o app
./app
```

**Note:** The `--` separator is required. Without it, G3 tries to read its own binary as a source file.

## Self-Hosted Compiler (G3)

AIRL compiles itself. The G3 compiler is written entirely in AIRL and produces native x86-64 and ARM64 binaries via Cranelift.

**Pipeline:**

| Stage | File | Lines |
|-------|------|-------|
| Lexer | `bootstrap/lexer.airl` | ~365 |
| Parser | `bootstrap/parser.airl` | ~930 |
| Bytecode compiler | `bootstrap/bc_compiler.airl` | ~1,500 |
| AOT backend | `compile-bytecode-to-executable` builtin | Cranelift |

Cranelift (native code generation) and `libairl_rt.a` (runtime) are exposed as builtins embedded in the host binary. The resulting `g3` binary is ~39 MB. **Fixpoint verified:** the bootstrap compiler produces identical output when self-compiled.

### G3 Usage

```bash
./g3 --version                          # Check version
./g3 -- app.airl -o app                 # Compile single file
./g3 -- lib.airl app.airl -o app        # Compile multiple files
./g3 -- app.airl                        # Default output: a.out
```

### G3 Limitations

- No `airl check` — G3 compiles only; use the host binary for type checking and Z3 verification.
- No REPL — G3 produces native binaries only.
- No `--load` — pass multiple files directly: `./g3 -- a.airl b.airl -o out`.
- Targets the host platform (Linux x86-64 or macOS ARM64).

### G3 Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `stream did not contain valid UTF-8` | Missing `--` separator | Add `--` before source files |
| `cc: not found` | No C linker | `sudo apt install gcc` or `xcode-select --install` |
| `undefined reference to airl_*` | Stale G3 binary | Rebuild via `bash scripts/build-g3.sh` |
| `Compilation error: unregistered builtin 'X'` | Source uses newer builtin | Rebuild G3 |
| Segfault at runtime | Register allocation bug | Report with source; use `airl compile` as workaround |
| Out of memory during build | Insufficient RAM | Ensure at least 4 GB free |

## Language Features

### Type System & Ownership

- Dependent type system with dimension unification for tensors
- Linear ownership: `own`, `&ref`, `&mut`, `copy` — enforced statically and at runtime

```clojure
(defn consume
  :sig [(own x : i32) -> i32]
  :intent "consume x"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)

(let (v : i32 42)
  (do (consume v)
      v))  ;; ERROR: use-after-move
```

| Annotation | Meaning |
|------------|---------|
| `own` | Caller transfers ownership. Value is consumed. |
| `&ref` | Immutable borrow. Caller retains ownership. |
| `&mut` | Mutable borrow. Exclusive access. |
| `copy` | Explicit copy. Only for primitive types. |

### Contract System

Every function must have contracts. The compiler rejects functions without them.

```clojure
(defn clamp
  :sig [(x : i64) (lo : i64) (hi : i64) -> i64]
  :intent "Clamp x to range [lo, hi]"
  :requires [(valid x) (valid lo) (valid hi) (<= lo hi)]
  :ensures [(>= result lo) (<= result hi)]
  :body (if (< x lo) lo (if (> x hi) hi x)))
```

Contracts compile to native conditional branches — essentially free on the happy path with branch prediction.

### Standard Library

**15 stdlib modules** (written in AIRL, auto-loaded as prelude):

`math`, `result`, `string`, `map`, `set`, `json`, `base64`, `sha256`, `hmac`, `pbkdf2`, `io`, `path`, `collections`, `aircon`, `test`

### Builtins

**~150 compiler intrinsics** covering:

| Category | Examples |
|----------|---------|
| List (7) | `head`, `tail`, `empty?`, `cons`, `at-or`, `set-at` |
| String (23) | `str`, `split`, `join`, `contains`, `replace`, `char-alpha?` |
| Map (10) | `map-new`, `map-from`, `map-get`, `map-set`, `map-keys` |
| File I/O (14) | `read-file`, `write-file`, `file-exists?`, `read-dir` |
| Float math (15) | `sqrt`, `sin`, `cos`, `floor`, `ceil`, `int-to-float` |
| TCP (9) | `tcp-listen`, `tcp-accept`, `tcp-connect`, `tcp-send`, `tcp-recv` |
| Threads (10) | `thread-spawn`, `thread-join`, `channel-new`, `channel-send` |
| Crypto (13) | `sha256`, `sha512`, `hmac-*`, `pbkdf2-*`, `base64-*` |
| Compression (8) | `gzip-*`, `snappy-*`, `lz4-*`, `zstd-*` |
| System (10) | `shell-exec`, `time-now`, `sleep`, `getenv`, `get-args` |

### Agent Communication

```clojure
(let (w : String (spawn-agent "worker.airl"))
  (let (a : i64 (send w "add" 10 20))
    (let (b : i64 (send w "multiply" a 3))
      b)))
;; → 90
```

```bash
# Start a worker
airl agent worker.airl --listen tcp:127.0.0.1:9001

# Send tasks
airl call tcp:127.0.0.1:9001 add 3 4    # → 7
```

### Module System

```clojure
(import "lib/math.airl")              ;; prefix: (math.abs -5)
(import "lib/math.airl" :as m)        ;; alias: (m.abs -5)
(import "lib/math.airl" :only [abs])  ;; bare: (abs -5)
```

## Testing

```bash
# Rust test suite (~478 tests across 10 crates)
cargo test -p airl-syntax -p airl-types -p airl-contracts \
  -p airl-runtime -p airl-agent -p airl-driver

# AOT test suite (75 tests — bootstrap, stdlib, builtins)
bash tests/aot/run_aot_tests.sh

# Run AOT tests through the self-compiled G3 binary
for f in tests/aot/round*.airl; do ./g3 -- "$f" -o /tmp/t && /tmp/t; done
```

## Performance

| Benchmark | AIRL AOT | Python | Ratio |
|-----------|----------|--------|-------|
| fib(35) | 56 ms | 2,335 ms | **42x faster** |
| fact(20) × 100K | 6 ms | 248 ms | **41x faster** |

## Architecture

```
AIRL Source
    │
    ├──── G3 (self-hosted) ───────────────────────────────────────┐
    │     bootstrap/lexer.airl → tokens                           │
    │     bootstrap/parser.airl → AST                             │
    │     bootstrap/bc_compiler.airl → BCFunc bytecode            │
    │     compile-bytecode-to-executable → native binary          │
    │                                                             │
    ├──── Host (Rust toolchain) ──────────────────────────────────┤
    │     [Parser] S-expr → AST                                   │
    │     [Type Checker] Dependent types + linearity              │
    │     [Z3 Verifier] Prove contracts via SMT                   │
    │     [Bytecode Compiler] AST → register-based bytecode       │
    │          │                                                   │
    │          ├── airl run ──► AOT compile → execute → clean     │
    │          └── airl compile ► Cranelift AOT → executable      │
    │                                                             │
    └───────── Both link against libairl_rt.a ────────────────────┘
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `airl-syntax` | Lexer, parser, AST, diagnostics |
| `airl-types` | Type checker, linearity, exhaustiveness |
| `airl-contracts` | Contract violation types |
| `airl-rt` | Runtime library (`libairl_rt.a`) — ~150 intrinsics as `extern "C"` |
| `airl-runtime` | AOT compiler (Cranelift) |
| `airl-codegen` | Cranelift tensor codegen |
| `airl-solver` | Z3 SMT formal verification |
| `airl-agent` | Transport, protocol, agent runtime |
| `airl-driver` | CLI, pipeline, formatter |

## Examples

| Example | Demonstrates |
|---------|-------------|
| `01-hello-world` | `print`, basic `defn`, `do` blocks |
| `02-functions-and-contracts` | `:requires`/`:ensures`, function composition |
| `03-verified-arithmetic` | Z3 formal proofs (`airl check`) |
| `04-safe-error-handling` | `Result`/`Option` variants, `match` |
| `05-ownership-and-borrowing` | `own`, `ref`, ownership transfer |
| `06-tensor-operations` | Tensor builtins, accelerated matmul |
| `07-higher-order-functions` | Lambdas, function arguments, composition |
| `08-agent-orchestration` | `spawn-agent`, `send`, multi-agent IPC |

```bash
cargo run --release --features aot -- run examples/01-hello-world/hello_world.airl
```

## Ecosystem

AIRL is the core language in a growing ecosystem of LLM-native libraries:

| Repo | Role |
|------|------|
| `CairLI` | CLI framework written in AIRL |
| `airline` | Async framework written in AIRL |
| `AIRL_castle` | Kafka SDK written in AIRL |
| `airl_kafka_cli` | Kafka CLI client written in AIRL |
| `AirTraffic` | MCP server framework written in AIRL |
| `canopy` | Algebraic TUI framework written in AIRL |
| `AIRLOS` | Capability-based microkernel that hosts AIRL evaluation |
| `airshell` | Interactive shell for AIRLOS written in AIRL |
| `AirLock` | SSH client for AIRLOS written in AIRL |

### mynameisAIRL — MCP Prompt Server

`servers/mynameisairl/` contains an MCP server that teaches AIRL to LLMs, built on the AirTraffic framework. It serves the AIRL Language Guide as an MCP prompt called `teach_airl`.

```bash
# Build (requires g3 and AirTraffic source)
AIRL_STDLIB=$AIRL_DIR/stdlib bash servers/mynameisairl/build.sh ./mynameisairl

# Run
./mynameisairl --guide $AIRL_DIR/servers/mynameisairl/AIRL-LLM-Guide.md
```

## Project Stats

- **v0.11.1** — self-hosted since v0.6.0, fixpoint verified
- **75 AOT tests** — all pass through both the Rust-hosted and self-compiled pipelines
- **~478 Rust tests** across 10 crates
- **15 stdlib modules** + **~150 compiler intrinsics**
- **Cross-platform** — Linux x86-64 and macOS ARM64
- **42x faster than Python** on pure arithmetic (AOT native)
- **0.43× completion tokens vs Python** — structural token efficiency, model-independent
- **Contracts always enforced** — native conditional branches in AOT

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

PolyForm Noncommercial 1.0.0 — free for personal, academic, research, and nonprofit use. See [LICENSE](LICENSE) for details.
