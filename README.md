# AIRL — AI Intermediate Representation Language

**A programming language designed for AI systems, not humans.**

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

## Why AIRL?

Every existing programming language optimizes for human readability. AIRL optimizes for AI producers and consumers:

- **Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. AI code generators skip optional features; they don't skip grammar requirements.
- **S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient.
- **Messages are programs** — Agents exchange AIRL source text as both the message format and the execution format. No protobuf, no gRPC, no separate serialization.
- **Formal verification** — Z3 SMT solver proves contracts at compile time. What can't be proven is checked at runtime.
- **Linear ownership** — Rust-style move semantics enforced at runtime. No garbage collector.

## Features

### Language
- S-expression grammar (hand-written recursive descent parser)
- Dependent type system with dimension unification for tensors
- Linear ownership with borrow tracking (own, ref, mut, copy)
- Mandatory contract system (requires, ensures, invariant, intent)
- Algebraic data types (sum types, product types)
- Pattern matching with exhaustiveness checking
- First-class functions and closures
- **Standard library** — 15 pure AIRL collection functions (map, filter, fold, sort, etc.) auto-loaded as a prelude

### Compilation
- **Tree-walking interpreter** for all AIRL programs
- **Cranelift JIT** — Functions with primitive signatures transparently compile to native code on first call
- **Tensor JIT** — `tensor.add`, `tensor.mul`, `tensor.matmul` compile to native loops
- **Z3 SMT solver** — Formal verification of integer arithmetic contracts

### Agent Communication
- Inter-agent task exchange over TCP and Unix sockets
- `spawn-agent` builtin launches worker processes via stdio
- `send` builtin dispatches typed, contract-verified tasks
- Length-prefixed AIRL S-expression wire protocol
- Capability-based agent registry

### CLI
```
airl run <file>       Run an AIRL source file
airl check <file>     Type-check and verify contracts
airl repl             Interactive REPL with :env introspection
airl agent <file>     Run as an agent worker (--listen tcp:HOST:PORT | stdio)
airl call <ep> <fn>   Call a remote agent function
airl fmt <file>       Pretty-print an AIRL source file
```

## Quick Start

### Build

```bash
git clone <repo-url> && cd AIRL
cargo build
```

Requirements: Rust 1.85+, CMake, C++ compiler, Python 3 (for Z3 compilation on first build).

### Hello World

```clojure
;; examples/01-hello-world/hello_world.airl

(print "Hello, World!")

(defn greet
  :sig [(name : String) -> String]
  :intent "create a personalized greeting"
  :ensures [(valid result)]
  :body (do
    (print "Greetings from AIRL,")
    (print name)
    name))

(greet "fellow AI")
```

```bash
cargo run -- run examples/01-hello-world/hello_world.airl
# Hello, World!
# Greetings from AIRL,
# fellow AI
```

### Run a Program

```bash
# Function with contracts
cargo run -- run examples/02-functions-and-contracts/functions_and_contracts.airl

# Formally verify contracts with Z3
cargo run -- check examples/03-verified-arithmetic/verified_arithmetic.airl
```

### Type Check

```bash
cargo run -- check math.airl
# OK: math.airl
```

### Interactive REPL

```bash
cargo run -- repl
airl> (+ 1 2)
3
airl> (defn sq :sig [(x : i64) -> i64] :intent "square" :requires [(valid x)] :ensures [(valid result)] :body (* x x))
()
airl> (sq 7)
49
airl> :env
── Functions ──
  sq : (x) -> Named("i64")
airl> :quit
```

### Agent Communication

Terminal 1 — start a worker:
```bash
cargo run -- agent worker.airl --listen tcp:127.0.0.1:9001
```

Terminal 2 — send tasks:
```bash
cargo run -- call tcp:127.0.0.1:9001 add 3 4
# → 7
cargo run -- call tcp:127.0.0.1:9001 multiply 6 7
# → 42
```

### Programmatic Orchestration

```clojure
;; orchestrator.airl — spawn a worker and dispatch tasks
(let (w : String (spawn-agent "worker.airl"))
  (let (a : i64 (send w "add" 10 20))
    (let (b : i64 (send w "multiply" a 3))
      b)))
;; → 90
```

```bash
cargo run -- run orchestrator.airl
```

## Examples

The `examples/` directory contains progressive examples showcasing AIRL's capabilities:

| Example | Demonstrates |
|---------|-------------|
| `01-hello-world` | `print`, basic `defn`, `do` blocks |
| `02-functions-and-contracts` | `:requires`/`:ensures`, function composition |
| `03-verified-arithmetic` | Z3 formal proofs (`cargo run -- check`) |
| `04-safe-error-handling` | `Result`/`Option` variants, `match` |
| `05-ownership-and-borrowing` | `own`, `ref`, ownership transfer |
| `06-tensor-operations` | Tensor builtins, JIT-accelerated matmul, softmax |
| `07-higher-order-functions` | Lambdas, function arguments, composition |
| `08-agent-orchestration` | `spawn-agent`, `send`, multi-agent IPC |

Run any example:
```bash
cargo run -- run examples/01-hello-world/hello_world.airl
```

## Architecture

```
AIRL Source
    │
    ▼
[Parser]              S-expr → AST (hand-written, LL(1))
    │
    ▼
[Type Checker]        Dependent types, dimension unification
    │
    ▼
[Z3 Verifier]         Prove contracts via SMT (negation + UNSAT)
    │
    ▼
[Evaluator]           Tree-walking interpreter
    │                     │
    ├─ Scalar JIT ────► Cranelift (i64/f64/bool functions)
    ├─ Tensor JIT ────► Cranelift (add/mul/matmul loops)
    └─ Agent Ops ─────► spawn-agent, send (TCP/stdio)
```

### Crate Structure

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `airl-syntax` | Lexer, parser, AST, diagnostics | None |
| `airl-types` | Type checker, linearity, exhaustiveness | airl-syntax |
| `airl-contracts` | Contract evaluation, stub prover | airl-syntax, airl-types |
| `airl-runtime` | Interpreter, values, builtins, tensor ops | airl-syntax, airl-types, airl-contracts, airl-codegen |
| `airl-codegen` | Cranelift JIT (scalar + tensor) | airl-syntax, airl-types, cranelift |
| `airl-solver` | Z3 SMT formal verification | airl-syntax, z3 |
| `airl-agent` | Transport, protocol, agent runtime | airl-syntax, airl-runtime |
| `airl-driver` | CLI, pipeline, REPL, formatter | all crates |

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

**Three verification levels:**

| Level | Behavior |
|-------|----------|
| Checked | Contracts compiled as runtime assertions (default) |
| Proven | Z3 attempts static proof; falls back to runtime if Unknown |
| Trusted | Contracts assumed true (for FFI and axioms) |

## Ownership Model

AIRL uses linear ownership with explicit annotations:

```clojure
(defn consume
  :sig [(own x : i32) -> i32]     ;; x is moved — caller can't use it after
  :intent "consume x"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)

(let (v : i32 42)
  (do (consume v)
      v))  ;; ERROR: UseAfterMove — v was moved
```

| Annotation | Meaning |
|------------|---------|
| `own` | Caller transfers ownership. Value is consumed. |
| `&ref` | Immutable borrow. Caller retains ownership. |
| `&mut` | Mutable borrow. Exclusive access. |
| `copy` | Explicit copy. Only for primitive types. |

## Tensor Operations

```clojure
(let (a : tensor (tensor.ones f32 [3 3]))
  (let (b : tensor (tensor.identity f32 3))
    (tensor.matmul a b)))
```

Tensor builtins: `tensor.zeros`, `tensor.ones`, `tensor.rand`, `tensor.identity`, `tensor.add`, `tensor.mul`, `tensor.matmul`, `tensor.reshape`, `tensor.transpose`, `tensor.softmax`, `tensor.sum`, `tensor.max`, `tensor.slice`.

`tensor.add`, `tensor.mul`, and `tensor.matmul` are JIT-compiled to native loops via Cranelift.

## JIT Compilation

Functions with primitive signatures (i32, i64, f32, f64, bool) are transparently JIT-compiled on first call:

```clojure
(defn compute
  :sig [(x : i64) -> i64]
  :intent "polynomial"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body (+ (+ (* x x) (* 3 x)) 7))

(compute 5)  ;; JIT-compiled on first call, native on subsequent calls
;; → 47
```

No annotation needed — the interpreter detects eligible functions and compiles them automatically. Contracts are still checked by the interpreter before and after the native call.

## Project Stats

- **428 tests** across 8 crates
- **~13,000 lines** of Rust
- **89 commits** of incremental development
- **Zero external dependencies** for core crates (Cranelift and Z3 are isolated in `airl-codegen` and `airl-solver`)

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

MIT
