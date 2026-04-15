# AIRL — AI Intermediate Representation Language

**v0.15.0** — AIRL is a new category of software artifact: an **LLM-native executable language**. It is designed to be synthesized by a language model and compiled directly to native code — with no human authoring step in between. While AIRL is not intended to be written by humans, it is deliberately human-readable: a developer can inspect, audit, and reason about synthesized AIRL without tooling.

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

## Token Efficiency

AIRL programs require **~43% of the completion tokens** equivalent Python programs do — a structural property of the language, not the model.

Benchmarking an untrained model (qwen3-coder, zero AIRL exposure) against Python across 19 tasks:

| Metric | AIRL | Python |
|--------|------|--------|
| Completion tokens (avg/task) | 122 | 283 |
| **Token ratio** | **0.43×** | 1.0× |

This isn't a vanity metric. It compounds in three ways:

- **Lower cost** — API pricing is per token. At 0.43× token usage, generating the same program in AIRL costs less than half as much. Across thousands of agentic tasks, the savings are structural.
- **Faster generation** — LLMs generate tokens sequentially. Fewer tokens means faster time-to-result, directly.
- **Less hallucination surface** — A 70-token AIRL function has less room for the model to go off-track than a 290-token Python function. Combined with mandatory contracts, errors are caught at compile time rather than in production.

Combined with mandatory contracts that enforce correctness at compile time and unambiguous S-expression syntax that eliminates an entire class of generation errors, this produces a measurable compound effect: one developer with LLM assistance built the entire AIRL ecosystem — 26 projects, ~168K lines of code, from a microkernel to a Kafka SDK — in three weeks.

## The Ecosystem

The ecosystem is the proof that token efficiency translates to development velocity. 26 projects, ~168,000 LOC, 1,061 commits — built in approximately three weeks by one person working with LLM synthesis.

### Infrastructure

| Project | Description | Scale |
|---------|-------------|-------|
| **AIRL** | Compiler and runtime (self-hosted, fixpoint verified) | 76K LOC, 595 commits |
| **AIRLOS** | Capability-based x86 microkernel (containers, network namespaces, GUI) | 36K LOC, 190 commits |
| **airshell** | POSIX shell with job control, scripting, identity management | 4.6K LOC |

### Networking & Protocols

| Project | Description | Scale |
|---------|-------------|-------|
| **AIRL_castle** | Kafka SDK — binary wire protocol, 4 SASL mechanisms, 4 compression formats | 8.8K LOC |
| **AirDB** | PostgreSQL client — SCRAM-SHA-256 auth, extended query protocol | 1K LOC |
| **AIReqL** | HTTP client — redirects, retries, keep-alive pooling | 2.7K LOC |
| **AirLock** | SSH client — Curve25519 key exchange, Ed25519 host keys | 2.6K LOC |
| **AirWire** | Wire protocol primitives — binary codecs, SCRAM shared library | 600 LOC |

### Web

| Project | Description | Scale |
|---------|-------------|-------|
| **airlhttp** | HTTP/1.1 server with TLS | 2.2K LOC |
| **AirGate** | Web framework — routing, middleware, WebSocket, CSRF, sessions, compression | 1.9K LOC |

### Tooling

| Project | Description | Scale |
|---------|-------------|-------|
| **CairLI** | CLI argument parser | 2.2K LOC |
| **airtools** | Static analysis linter (14 rules, LSP server) | 6K LOC |
| **airlDelivery** | Package manager | 4.2K LOC |
| **airtest** | Test runner | 891 LOC |
| **AIRLchart** | Code visualization (Graphviz DOT call graphs) | 1.3K LOC |
| **AirParse** | Multi-format parser (JSON, YAML, TOML, HTML) | 1.8K LOC |
| **AirLog** | Structured logging framework | 649 LOC |

### UI & Terminal

| Project | Description | Scale |
|---------|-------------|-------|
| **Canopy** | Algebraic TUI framework — pure functions, no mutable state | 1.4K LOC |
| **AirMux** | Terminal multiplexer | 1.1K LOC |

### AI & Integration

| Project | Description | Scale |
|---------|-------------|-------|
| **AirNexus** | Multi-provider AI agent framework (OpenAI, Anthropic, Gemini, Ollama) | 1.6K LOC |
| **AirTraffic** | MCP server framework | 1.4K LOC |
| **mynameisAIRL** | MCP prompt server + code indexer | 2K LOC |

### Testing & Benchmarks

| Project | Description | Scale |
|---------|-------------|-------|
| **AIRL_bench** | Code generation benchmark (100 tasks) | 847 LOC |
| **kafka_sdk_bench** | Kafka SDK performance benchmark | 1K LOC |

### Async

| Project | Description | Scale |
|---------|-------------|-------|
| **airline** | Share-nothing async framework (reactor, futures, work stealing) | 1.2K LOC |

## Why It Works

Three design choices produce the token efficiency and correctness properties:

**S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient. No indentation errors, no operator precedence bugs, no bracket matching failures. The LLM cannot produce syntactically ambiguous code because the grammar does not allow it.

**Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. LLMs skip optional features; they cannot skip grammar requirements. Synthesis correctness is structurally enforced.

**Linear ownership** — Rust-style move semantics with static linearity analysis. Memory safety without a garbage collector. No class of use-after-free or double-free bugs for the model to introduce.

## Correctness

AIRL achieves **100% correctness on 100 benchmark tasks** (both Sonnet 4.6 and qwen3-coder). This is not luck — it is structural:

- The compiler **rejects** functions without contracts, so the LLM cannot skip verification
- Z3 SMT solver **proves** contracts at compile time; what cannot be proven becomes native runtime checks
- S-expression syntax eliminates the entire class of generation errors caused by ambiguous grammar

Progression on untrained models: 44% (no guide) → 68% (+ guide) → 80% (+ few-shot) → 100% (stdlib improvements in v0.6.0+).

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

### Self-Hosted Compiler (G3)

AIRL compiles itself. The G3 compiler is written entirely in AIRL and produces native x86-64 and ARM64 binaries via Cranelift. **Fixpoint verified:** the bootstrap compiler produces identical output when self-compiled.

| Stage | File | Lines |
|-------|------|-------|
| Lexer | `bootstrap/lexer.airl` | ~365 |
| Parser | `bootstrap/parser.airl` | ~930 |
| Bytecode compiler | `bootstrap/bc_compiler.airl` | ~1,500 |
| AOT backend | `compile-bytecode-to-executable` builtin | Cranelift |

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

## Performance

AIRL compiles to native x86-64 and ARM64 binaries via Cranelift. Current AOT compilation uses boxed value representation. The Kafka SDK (AIRL_castle) achieves 75% of librdkafka's throughput on sync produce, with per-value boxing identified as the primary bottleneck. Unboxed native integer compilation is designed and partially implemented; performance benchmarks will be published when it ships.

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

## Project Stats

- **v0.15.0** — self-hosted since v0.6.0, fixpoint verified
- **75 AOT tests** — all pass through both the Rust-hosted and self-compiled pipelines
- **~478 Rust tests** across 10 crates
- **15 stdlib modules** + **~150 compiler intrinsics**
- **Cross-platform** — Linux x86-64 and macOS ARM64
- **0.43× completion tokens vs Python** — structural token efficiency, model-independent
- **100% correctness** on 100 benchmark tasks (Sonnet 4.6 and qwen3-coder)
- **Contracts always enforced** — native conditional branches in AOT

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

PolyForm Noncommercial 1.0.0 — free for personal, academic, research, and nonprofit use. See [LICENSE](LICENSE) for details.
