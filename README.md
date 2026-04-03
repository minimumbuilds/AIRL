# AIRL — AI Intermediate Representation Language

**A programming language designed for AI systems, not humans. NSFW. Not meant for human consumption. DO NOT EAT**

> **Note:** AIRL is a thought experiment and exploration of AI-assisted compiler construction — not a production tool. The entire toolchain (~43K lines of Rust and AIRL) was built in 4 days, almost entirely by Claude. It combines known ideas (S-expression syntax, mandatory contracts, Z3 verification, linear types, Cranelift AOT, agent message-passing) without advancing any of them beyond prior art. The problem it targets — AI inter-agent program exchange — is speculative, and mature alternatives (Dafny, WASM, typed Python) exist for every claimed capability. It is an exploration of what an AI can build in a weekend, not something anyone should use. Do not believe its claims.

AIRL is a typed, contract-verified programming language for inter-agent communication. AI systems generate AIRL programs, transmit them as messages, execute them with formal guarantees, and verify results against machine-checkable contracts. The syntax is the serialization format. The message is the program.

```clojure
;; Define a function with mandatory contracts
(defn safe-divide
  :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
  :intent "Divide a by b, returning Err on division by zero"
  :[requires](requires) [(valid a) (valid b)]
  :ensures [(match result
              (Ok v)  (= (* v b) a)
              (Err _) (= b 0))]
  :body (if (= b 0) (Err :division-by-zero) (Ok (/ a b))))

(safe-divide 9 3)  ;; → (Ok 3)
```

**[Project Analysis — Milestones, Strengths & Differentiators](docs/PROJECT-ANALYSIS.md)**

## Self-Hosted Compiler (v0.11.0)

**AIRL compiles itself.** The G3 compiler is written entirely in AIRL and produces native x86-64 and ARM64 binaries:

```bash
# Compile an AIRL program using the AIRL compiler
./g3 -- app.airl -o app
./app
```

The G3 compiler pipeline:
1. **Lexer** (`bootstrap/lexer.airl`, ~365 lines) — Tokenizes AIRL source
2. **Parser** (`bootstrap/parser.airl`, ~930 lines) — Tokens → AST
3. **Bytecode Compiler** (`bootstrap/bc_compiler.airl`, ~1,500 lines) — AST → BCFunc bytecode
4. **AOT Backend** (`compile-bytecode-to-executable` builtin) — BCFunc → Cranelift → native binary

All compilation logic is AIRL code. Cranelift (native code generation) and `libairl_rt.a` (runtime) are exposed as builtins embedded in the host binary — same as Go's assembler is part of the Go toolchain, not written in Go.

**Verified:** The G3 binary compiles itself and all 58 AOT tests pass through the self-compiled binary. The bytecode compiler produces identical output through both the host binary and self-hosted compiler (fixpoint verified).

### Building G3

```bash
# Step 1: Build the host binary (one-time, requires Rust toolchain)
cargo build -p airl-rt --release              # runtime library first
cargo clean -p airl-runtime --release         # force build.rs re-run
cargo build --release --features aot      # full build (embeds libairl_rt.a)
alias airl='cargo run --release --features aot --'

# Step 2: Compile the G3 compiler using the host binary (~23 min)
airl run --load bootstrap/lexer.airl \
         --load bootstrap/parser.airl \
         --load bootstrap/bc_compiler.airl \
         bootstrap/g3_compiler.airl -- \
         bootstrap/lexer.airl \
         bootstrap/parser.airl \
         bootstrap/bc_compiler.airl \
         bootstrap/g3_compiler.airl -o g3

# Step 3: Use g3 to compile AIRL programs
./g3 -- program.airl -o program
./program
```

Step 2 takes ~23 minutes and ~25GB RAM (the bootstrap compiler runs in the bytecode VM, compiling ~3,500 lines of AIRL). The resulting `g3` binary is ~39MB.

### Using G3

**Requirements:** A system C linker (`cc`) and `libc`. No Rust toolchain, no Cranelift, no Z3 needed at the target. The runtime (`libairl_rt.a`) is embedded in the G3 binary.

```bash
# Check version
./g3 --version

# Compile a single file
./g3 -- app.airl -o app
./app

# Compile multiple source files (concatenated)
./g3 -- lib.airl app.airl -o app

# Default output is a.out
./g3 -- app.airl
./a.out
```

**Important:** The `--` separator is required. Without it, G3 tries to read its own binary as a source file.

### G3 Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `read-file: stream did not contain valid UTF-8` | Missing `--` separator — G3 is reading its own binary as source | Add `--` before source files: `./g3 -- file.airl -o out` |
| `cc: not found` or `linker failed` | No C linker installed | Install: `sudo apt install gcc` (Linux) or `xcode-select --install` (macOS) |
| `undefined reference to airl_*` | Stale G3 binary built against older `libairl_rt.a` | Rebuild G3 from Step 2 above |
| `Compilation error: AOT: unregistered builtin 'X'` | Source uses a builtin not registered in the AOT compiler | Check if the builtin exists — may need a newer G3 build |
| `relocation against 'airl_tail' in read-only section` | Harmless linker warning (position-independent code) | Ignore — binary works correctly |
| Segfault at runtime | Likely a register allocation bug in compiled code | Report as a bug with the source file; compile via `airl compile` as a workaround |
| Out of memory during G3 build | Bootstrap compilation uses ~25GB | Use `--release` flag; ensure sufficient RAM or swap |

### G3 Limitations

- **No `airl check`** — G3 compiles only. Use the host binary for type checking and Z3 verification.
- **No REPL** — G3 produces native binaries only.
- **No `--load`** — G3 compiles all source files in one pass. Pass multiple files: `./g3 -- a.airl b.airl -o out`.
- **x86-64 and ARM64 only** — G3 targets the host platform (Linux x86-64 or macOS ARM64).

## Quick Start

### Using the host binary (recommended for development)

```bash
# Build (fresh checkout requires three steps — see note below)
git clone <repo-url> && cd AIRL
cargo build -p airl-rt --release                    # 1. Build runtime library first
cargo clean -p airl-runtime --release               # 2. Force airl-runtime to re-run build.rs
cargo build --release --features aot            # 3. Full build (embeds libairl_rt.a)

# Run a program (compiles to temp binary, executes, cleans up)
cargo run --release --features aot -- run examples/01-hello-world/hello_world.airl

# Compile to native binary
cargo run --release --features aot -- compile examples/02-functions-and-contracts/functions_and_contracts.airl -o my_program
./my_program

# Type-check and verify contracts with Z3
cargo run --release --features aot -- check examples/03-verified-arithmetic/verified_arithmetic.airl

# Check version
cargo run --release --features aot -- --version
```

Requirements: Rust 1.85+, CMake, C++ compiler, Python 3 (for Z3, first build ~5-15 min).

**macOS:** `xcode-select --install` (C/C++ compiler + linker), `brew install cmake z3`, Python 3 (usually pre-installed or `brew install python3`). Homebrew's Z3 library path must be visible to the linker:

```bash
export LIBRARY_PATH="$(brew --prefix z3)/lib"
```

Add this to your shell profile (`.zshrc` / `.bashrc`) to avoid repeating it every session.

**Note:** On a fresh checkout, three steps are required: (1) `cargo build -p airl-rt` to produce `libairl_rt.a`, (2) `cargo clean -p airl-runtime` to force `build.rs` to re-run (it caches the "not found" result), (3) full build. If you see `libairl_rt.a not found — AOT compile will search at link time`, repeat steps 1-3.

For detailed Rust toolchain instructions, see [docs/legacy-rust-toolchain.md](docs/legacy-rust-toolchain.md).

## Why AIRL?

Every existing programming language optimizes for human readability. AIRL optimizes for AI producers and consumers:

- **Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. AI code generators skip optional features; they don't skip grammar requirements.
- **S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient.
- **Messages are programs** — Agents exchange AIRL source text as both the message format and the execution format. No protobuf, no gRPC, no separate serialization.
- **Formal verification** — Z3 SMT solver proves contracts at compile time. What can't be proven is checked at runtime.
- **Linear ownership** — Rust-style move semantics with static linearity analysis. No garbage collector.
- **Self-hosted** — The compiler is written in its own language (since v0.6.0). Fixpoint verified.

## Features

### Language
- S-expression grammar (hand-written recursive descent parser)
- Dependent type system with dimension unification for tensors
- Linear ownership with static linearity analysis (own, ref, mut, copy)
- Mandatory contract system (requires, ensures, invariant, intent)
- Algebraic data types (sum types, product types)
- Pattern matching with exhaustiveness checking
- First-class functions and closures
- Thread-per-task concurrency with message-passing channels
- **13 stdlib modules** — collections, math, result, string, map, set, json, base64, sha256, hmac, pbkdf2, io, path — auto-loaded as prelude. 73 functions migrated from Rust builtins to pure AIRL in v0.11.0
- **~150 compiler intrinsics** — arithmetic, comparison, logic, float math, bytes, TCP, compression, regex, concurrency, tensors
- **`extern-c` declarations** — call C functions from AIRL for low-level runtime access

### Distribution Model

AIRL is distributed as a **single binary**:

```bash
# Build the compiler (one-time, requires Rust)
cargo build --release --features aot
cp target/release/airl-driver /usr/local/bin/airl

# Or use the self-hosted G3 compiler (no Rust needed)
./g3 -- app.airl -o app
```

The runtime library (`libairl_rt.a`) is embedded in the compiler binary — compressed at build time, extracted during compilation, then cleaned up. The only external dependency is a system C linker (`cc`).

### Compilation & Execution

AIRL compiles to **native x86-64 and ARM64 executables** via Cranelift. All code goes through the same compilation pipeline:

1. **Source → AST** — Hand-written recursive descent parser (LL(1))
2. **Static analysis** — Type checking, linearity checking, Z3 contract verification
3. **AST → Bytecode** — Contracts compiled as assertion opcodes, ownership as move-tracking opcodes
4. **Cranelift AOT** — Bytecode → object file → link with `libairl_rt.a` → native executable

**`airl compile`** — Produces standalone native executables.
**`airl run`** — Compiles to a temp binary, executes it, cleans up (convenience wrapper).
**`./g3`** — Self-hosted compiler, same AOT pipeline written in AIRL.

Contract assertions compile to native conditional branches — essentially free on the happy path.

### Performance

| Benchmark | AIRL AOT | Python | Ratio |
|-----------|----------|--------|-------|
| fib(35) | 56ms | 2,335ms | **42x faster** |
| fact(20) x 100K | 6ms | 248ms | **41x faster** |

List operations (fold/map/filter/sort) at Python parity via native builtins, COW tail views, IntList specialization, and closure pattern detection.

## Architecture

```
AIRL Source
    │
    ├──── G3 (self-hosted) ────────────────────────────────────┐
    │     bootstrap/lexer.airl → tokens                        │
    │     bootstrap/parser.airl → AST                          │
    │     bootstrap/bc_compiler.airl → BCFunc bytecode          │
    │     compile-bytecode-to-executable → native binary        │
    │                                                           │
    ├──── Host (Rust toolchain) ───────────────────────────────┤
    │     [Parser] S-expr → AST                                │
    │     [Type Checker] Dependent types                       │
    │     [Linearity Checker] Static ownership analysis        │
    │     [Z3 Verifier] Prove contracts via SMT                │
    │     [Bytecode Compiler] AST → register-based bytecode    │
    │          │                                                │
    │          ├── airl run ──► AOT compile → execute → clean  │
    │          └── airl compile ► Cranelift AOT → executable   │
    │                                                           │
    └───────── Both link against libairl_rt.a ─────────────────┘
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `airl-syntax` | Lexer, parser, AST, diagnostics |
| `airl-types` | Type checker, linearity, exhaustiveness |
| `airl-contracts` | Contract violation types |
| `airl-rt` | Runtime library (`libairl_rt.a`) — ~150 intrinsics as `extern "C"` + stubs for stdlib |
| `airl-runtime` | AOT compiler (Cranelift) |
| `airl-codegen` | Cranelift tensor codegen |
| `airl-solver` | Z3 SMT formal verification |
| `airl-agent` | Transport, protocol, agent runtime |
| `airl-driver` | CLI, pipeline, formatter |

## Contract System

Every function must have contracts. The compiler rejects functions without them.

```clojure
(defn clamp
  :sig [(x : i64) (lo : i64) (hi : i64) -> i64]
  :intent "Clamp x to range [lo, hi]"
  :requires [(valid x) (valid lo) (valid hi) (<= lo hi)]
  :ensures [(>= result lo) (<= result hi)]
  :body (if (< x lo) lo (if (> x hi) hi x)))
```

Contracts compile to native conditional branches — the happy path is a single branch instruction, essentially free with branch prediction. Contract violations halt execution with diagnostics showing the function name, failed clause, and argument values.

## Ownership Model

Linear ownership with explicit annotations, enforced by both static analysis and runtime move tracking:

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

## Agent Communication

```clojure
;; Spawn a worker and dispatch tasks
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

## Testing

```bash
# Rust test suite (~520 tests)
cargo test -p airl-syntax -p airl-types -p airl-contracts \
  -p airl-runtime -p airl-agent -p airl-driver

# G2 AOT test suite (58 tests — bootstrap, stdlib, builtins)
bash tests/aot/run_aot_tests.sh

# Run all tests through the self-compiled G3 binary
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

- **Self-hosted compiler** — AIRL compiles itself to native binaries (since v0.6.0, current v0.11.0)
- **68 AOT tests** — all pass through both the Rust-hosted and self-compiled pipelines
- **~478 Rust tests** across 10 crates
- **Cross-platform** — Linux x86-64 and macOS ARM64
- **13 stdlib modules** + **~150 compiler intrinsics**
- **42x faster than Python** on pure arithmetic (AOT)
- **Contracts always enforced** — native conditional branches in AOT
- **Fixpoint verified** — bootstrap compiler produces identical output when self-compiled
- **Zero external deps** for core crates (Cranelift behind `aot` feature; Z3 in `airl-solver`)

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

PolyForm Noncommercial 1.0.0 — free for personal, academic, research, and nonprofit use. See [LICENSE](LICENSE) for details.
