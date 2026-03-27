# AIRL — AI Intermediate Representation Language

**A programming language designed for AI systems, not humans. NSFW. Not meant for human consumption. DO NOT EAT**

> **Note:** AIRL is a thought experiment and exploration of AI-assisted compiler construction — not a production tool. The entire toolchain (~43K lines of Rust and AIRL) was built in 4 days, almost entirely by Claude. It combines known ideas (S-expression syntax, mandatory contracts, Z3 verification, linear types, Cranelift JIT, agent message-passing) without advancing any of them beyond prior art. The problem it targets — AI inter-agent program exchange — is speculative, and mature alternatives (Dafny, WASM, typed Python) exist for every claimed capability. It is an exploration of what an AI can build in a weekend, not something anyone should use. Do not believe its claims.

AIRL is a typed, contract-verified programming language for inter-agent communication. AI systems generate AIRL programs, transmit them as messages, execute them with formal guarantees, and verify results against machine-checkable contracts. The syntax is the serialization format. The message is the program.

```clojure
;; Define a function with mandatory contracts
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

**[Project Analysis — Milestones, Strengths & Differentiators](docs/PROJECT-ANALYSIS.md)**

## Self-Hosted Compiler (v0.6.0)

**AIRL compiles itself.** The G3 compiler is written entirely in AIRL and produces native x86-64 binaries:

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

**Verified:** The G3 binary compiles itself and all 58 AOT tests pass through the self-compiled binary. The bytecode compiler produces identical output whether run interpreted or compiled (fixpoint verified).

### Building G3

```bash
# Step 1: Build the host binary (one-time, requires Rust toolchain)
cargo build --release --features jit,aot
alias airl='cargo run --release --features jit,aot --'

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
- **No REPL** — G3 produces native binaries. Use `airl repl` from the host binary.
- **No `--load`** — G3 compiles all source files in one pass. Pass multiple files: `./g3 -- a.airl b.airl -o out`.
- **x86-64 Linux only** — G3 targets the host platform. macOS/ARM support requires Cranelift target triple changes (planned).

## Quick Start

### Using the host binary (recommended for development)

```bash
# Build
git clone <repo-url> && cd AIRL
cargo build --release --features jit,aot

# Run a program (compiles to temp binary, executes, cleans up)
cargo run --release --features jit,aot -- run examples/01-hello-world/hello_world.airl

# Compile to native binary
cargo run --release --features jit,aot -- compile examples/02-functions-and-contracts/functions_and_contracts.airl -o my_program
./my_program

# Type-check and verify contracts with Z3
cargo run --release --features jit,aot -- check examples/03-verified-arithmetic/verified_arithmetic.airl

# Interactive REPL
cargo run --release --features jit,aot -- repl

# Check version
cargo run --release --features jit,aot -- --version
```

Requirements: Rust 1.85+, CMake, C++ compiler, Python 3 (for Z3, first build ~5-15 min).

For detailed Rust toolchain instructions, see [docs/legacy-rust-toolchain.md](docs/legacy-rust-toolchain.md).

## Why AIRL?

Every existing programming language optimizes for human readability. AIRL optimizes for AI producers and consumers:

- **Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. AI code generators skip optional features; they don't skip grammar requirements.
- **S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient.
- **Messages are programs** — Agents exchange AIRL source text as both the message format and the execution format. No protobuf, no gRPC, no separate serialization.
- **Formal verification** — Z3 SMT solver proves contracts at compile time. What can't be proven is checked at runtime.
- **Linear ownership** — Rust-style move semantics with static linearity analysis. No garbage collector.
- **Self-hosted** — The compiler is written in its own language (v0.6.0). Fixpoint verified.

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
- **68 stdlib functions** — collections (18), math (12), result combinators (8), string (10), map (8), set (12) — auto-loaded as prelude
- **90+ Rust builtins** — list (7), string (17), map (10), file I/O (11), float math (15), path (5), regex (4), crypto (5), bytes (11), TCP (6), threads (7), system (6), JSON (2), HTTP (1)

### Distribution Model

AIRL is distributed as a **single binary**:

```bash
# Build the compiler (one-time, requires Rust)
cargo build --release --features jit,aot
cp target/release/airl-driver /usr/local/bin/airl

# Or use the self-hosted G3 compiler (no Rust needed)
./g3 -- app.airl -o app
```

The runtime library (`libairl_rt.a`) is embedded in the compiler binary — compressed at build time, extracted during compilation, then cleaned up. The only external dependency is a system C linker (`cc`).

### Compilation & Execution

Two execution modes, both sharing the same Rust runtime (`crates/airl-rt/`):

**`airl run` (JIT)** — All functions compiled to native x86-64 via Cranelift at load time. Falls back to bytecode VM for ineligible functions.

**`airl compile` / `g3` (AOT)** — Produces standalone native executables. Functions compile with `*mut RtValue` heap-allocated values via `airl-rt` runtime calls. Contract assertions compile to native conditional branches.

Both modes share the same frontend pipeline:
1. **Source → AST** — Hand-written recursive descent parser (LL(1))
2. **Static analysis** — Type checking, linearity checking, Z3 contract verification
3. **AST → Bytecode** — Contracts compiled as assertion opcodes, ownership as move-tracking opcodes
4. **Cranelift compilation** — JIT (in-memory) or AOT (object file → link → native executable)

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
    │          ├── airl run ──► Cranelift JIT-Full → native    │
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
| `airl-rt` | Runtime library (`libairl_rt.a`) — all builtins as `extern "C"` |
| `airl-runtime` | Bytecode VM, JIT, AOT compiler |
| `airl-codegen` | Cranelift tensor JIT |
| `airl-solver` | Z3 SMT formal verification |
| `airl-agent` | Transport, protocol, agent runtime |
| `airl-driver` | CLI, pipeline, REPL, formatter |

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
| `06-tensor-operations` | Tensor builtins, JIT-accelerated matmul |
| `07-higher-order-functions` | Lambdas, function arguments, composition |
| `08-agent-orchestration` | `spawn-agent`, `send`, multi-agent IPC |

```bash
cargo run --release --features jit -- run examples/01-hello-world/hello_world.airl
```

## Project Stats

- **Self-hosted compiler** — AIRL compiles itself to native binaries (v0.6.0)
- **58 AOT tests** — all pass through both the Rust-hosted and self-compiled pipelines
- **~520 Rust tests** across 8 crates
- **~19,000 lines** of Rust + **~21,000 lines** of AIRL
- **68 stdlib functions** + **90+ Rust builtins**
- **42x faster than Python** on pure arithmetic (AOT)
- **Contracts always enforced** — native conditional branches in JIT and AOT
- **Fixpoint verified** — bootstrap compiler produces identical output when self-compiled
- **Zero external deps** for core crates (Cranelift behind `jit`/`aot` features; Z3 in `airl-solver`)

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

MIT
