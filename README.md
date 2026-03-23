# AIRL — AI Intermediate Representation Language

**A programming language designed for AI systems, not humans. NSFW. Not meant for human consumption. DO NOT EAT**

> **Note:** AIRL is a thought experiment and exploration of AI-assisted compiler construction — not a production tool. The entire toolchain (~43K lines of Rust and AIRL, 213 commits) was built in 4 days, almost entirely by Claude. It combines known ideas (S-expression syntax, mandatory contracts, Z3 verification, linear types, Cranelift JIT, agent message-passing) without advancing any of them beyond prior art. The problem it targets — AI inter-agent program exchange — is speculative, and mature alternatives (Dafny, WASM, typed Python) exist for every claimed capability. It is an exploration of what an AI can build in a weekend, not something anyone should use.

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

## Why Not Generate Low-Level IR Directly?

If no human reads the code, why not have AI generate Cranelift IR (or LLVM IR, or WASM) directly and skip the high-level language entirely?

**1. High-level code uses fewer tokens, not more.** Cranelift IR requires explicit SSA variables, typed instructions, named basic blocks, and manual control flow. A 2-line AIRL function becomes 15+ lines of CLIF. S-expression syntax is already maximally token-efficient — it *is* the AST.

```clojure
;; AIRL: 1 expression, ~20 tokens
(if (<= n 1) 1 (* n (factorial (- n 1))))
```
```
;; Cranelift IR: 4 basic blocks, ~60 tokens
block0(v0: i64):
    v1 = iconst.i64 1
    v2 = icmp sle v0, v1
    brnz v2, block1
    jump block2
block1:
    return v1
block2:
    v3 = isub v0, v1
    v4 = call %factorial(v3)
    v5 = imul v0, v4
    return v5
```

**2. LLMs have non-zero error rates — safety layers are load-bearing.** AIRL's type checker, contract verifier, linearity checker, and Z3 solver catch mistakes *before* execution. In low-level IR, a wrong register or a missing bounds check is silent corruption or a segfault. There is no error message, no diagnosis, no recovery. The cost of safety infrastructure is paid once at compile time; the cost of a runtime bug is paid every time the program executes.

**3. The abstraction gap determines the defect rate.** The further generated code is from intent, the more places bugs hide. "Divide safely, return Err on zero" → AIRL is a small step. The same intent → register allocation + SSA + calling conventions is a large step with many more failure modes. Current LLMs are trained on high-level code and generate it with far lower error rates than low-level IR.

**4. AIRL already compiles to native code — the AI doesn't need to.** The runtime transparently JIT-compiles eligible functions via Cranelift, compiles to a bytecode VM (`--bytecode`), or lowers to a tree-flattened IR VM (`--compiled`). AI generates high-level, verified AIRL once; the toolchain picks the fastest execution strategy automatically.

## How Does AIRL's Safety Compare to Rust?

AIRL's ownership model and type system were inspired by Rust, but redesigned for AI producers instead of human experts. The two languages occupy different points on the safety-complexity Pareto frontier.

### What They Share

Both eliminate the same core bug classes at compile time:

| Bug Class | Rust | AIRL |
|-----------|------|------|
| Type errors | Static type system | Static type system |
| Use-after-free/move | Borrow checker | Linearity checker |
| Null pointer derefs | `Option<T>`, no null | `Option`/`Result`, nil-safe |
| Unchecked errors | `Result<T,E>`, must handle | Mandatory `:ensures` contracts |
| Data races | `Send`/`Sync` + ownership | Single-threaded + agent message-passing |

### Where AIRL Goes Further

**Mandatory contracts.** Rust has no built-in contract system — `debug_assert!` and third-party crates are optional, and AI skips optional things. AIRL's parser rejects any `defn` without `:requires`/`:ensures`. Z3 then attempts to *prove* them statically.

```rust
// Rust: nothing prevents this — panics on b=0
fn divide(a: i32, b: i32) -> i32 { a / b }
```

```clojure
;; AIRL: compiler rejects this without contracts
(defn divide
  :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
  :requires [(!= b 0)]
  :ensures [(match result (Ok v) (= (* v b) a) (Err _) true)]
  :body (if (= b 0) (Err :division-by-zero) (Ok (/ a b))))
```

**Formal verification.** Z3 can prove semantic properties like `result >= lo && result <= hi` for a clamp function. Rust's type system cannot express this without dependent types.

**Intent tracking.** Every AIRL function carries a mandatory `:intent` string — a natural-language anchor for what the code *should* do. This gives both AI verifiers and human auditors a machine-checkable statement of purpose.

### Where Rust Goes Further

**Lifetime precision.** Rust tracks exact borrow lifetimes (NLL, Polonius); AIRL's linearity checker is coarser — it catches use-after-move but not complex reborrowing scenarios.

**Thread safety.** Rust's `Send`/`Sync` trait system deeply integrates with the type system. AIRL is single-threaded by design, using async agent message-passing for concurrency instead.

**Unsafe escape hatch.** Rust has `unsafe {}` for bypassing the borrow checker when necessary. AIRL has no equivalent — the Rust runtime *is* the unsafe layer.

### Why the Producer Being an LLM Changes Everything

The design differences above aren't arbitrary — they follow from the fact that humans and LLMs have fundamentally different error distributions.

**LLMs and humans make different mistakes.**

| Mistake type | Humans | LLMs |
|---|---|---|
| Null/bounds checks | Occasional | Rare (pattern-matched from training data) |
| Lifetime/borrow errors | Frequent (Rust's #1 complaint) | Very frequent (requires cross-scope control flow reasoning) |
| Semantic logic errors | Rare (humans understand intent) | Common (LLMs pattern-match, don't truly verify constraints) |
| Skipping optional best practices | Common | Near-certain (if it compiles without it, LLMs omit it) |

Rust's borrow checker protects against what *humans* get wrong most. But LLMs rarely make simple use-after-free errors — they've seen millions of correct examples. Where LLMs fail is complex lifetime reasoning (multi-scope borrows, self-referential structs) and semantic correctness (code compiles and runs but does the wrong thing). AIRL simplified the first and added verification for the second.

**You cannot rely on optional safety with AI producers.** A human developer can be told "always write tests" or "use `clippy`." Culture, code review, and CI enforce compliance. LLMs have no culture. They optimize for the shortest path to syntactically valid output. If contracts are optional, they'll be omitted. If error handling is optional, you'll get `unwrap()` everywhere.

This is why AIRL makes contracts **grammar-level mandatory** — the parser literally won't accept a function without `:requires`/`:ensures`. It's not a lint, not a CI check — it's the same as requiring a function to have a body. The LLM can't skip it because the code doesn't parse without it. That single design choice — mandatory vs optional safety — is arguably more impactful than any specific type system feature.

### Summary

| | Rust | AIRL |
|--|------|------|
| **Target author** | Human expert | LLM |
| **Ownership model** | Lifetimes, NLL, reborrowing | 4 annotations (`own`, `&ref`, `&mut`, `copy`), no lifetimes |
| **Contracts** | Optional (third-party crates) | Mandatory (grammar-level) |
| **Formal verification** | Not built in | Z3 SMT solver integrated |
| **Borrow checker power** | Turing-complete analysis | Simplified linearity (higher AI success rate) |
| **Concurrency safety** | `Send`/`Sync` + ownership | Single-threaded + message-passing agents |
| **Key insight** | Maximum safety, high complexity | Sufficient safety, high AI generation success rate |

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
- **IR VM** — Compiled execution mode (`--compiled`) via tree-flattened IR with self-TCO, 5-10x faster than interpreted
- **Bytecode VM** — Register-based bytecode compiler and VM (`--bytecode`), ~10x faster than interpreted
- **Bytecode→Cranelift JIT** — Eligible bytecode functions JIT-compiled to native x86-64 (`--jit`), fib(30) 67x faster than bytecode
- **Tensor JIT** — `tensor.add`, `tensor.mul`, `tensor.matmul` compile to native loops via Cranelift
- **Z3 SMT solver** — Formal verification of integer arithmetic contracts

### Self-Hosting
- **Bootstrap compiler** — Lexer, parser, type checker, and IR compiler written in AIRL itself (~2,500 lines)
- **Compiler fixpoint** — The compiled compiler produces identical IR to the interpreted compiler (verified via automated fixpoint test)
- **Self-parse verified** — The bootstrap lexer tokenizes its own source (15,691 chars → 3,400 tokens)

### Agent Communication
- Inter-agent task exchange over TCP and Unix sockets
- `spawn-agent` builtin launches worker processes via stdio
- `send` builtin dispatches typed, contract-verified tasks
- Length-prefixed AIRL S-expression wire protocol
- Capability-based agent registry

### CLI
```
airl run <file>              Run an AIRL source file (interpreted)
airl run --compiled <file>   Run via IR VM (faster, no contracts)
airl run --bytecode <file>   Run via bytecode VM (~10x faster than interpreted)
airl run --jit <file>        Run via bytecode VM + Cranelift JIT (requires --features jit)
airl check <file>            Type-check and verify contracts
airl repl                    Interactive REPL with :env introspection
airl agent <file>            Run as an agent worker (--listen tcp:HOST:PORT | stdio)
airl call <ep> <fn>          Call a remote agent function
airl fmt <file>              Pretty-print an AIRL source file
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
    ├─ Interpreted ──► [Evaluator]     Tree-walking interpreter
    │                     │
    │                     ├─ Tensor JIT ────► Cranelift (matmul loops)
    │                     └─ Agent Ops ─────► spawn-agent, send (TCP/stdio)
    │
    ├─ Compiled ─────► [IR Compiler]   AST → IR nodes (no contracts/spans)
    │                     │
    │                     ▼
    │                  [IR VM]         Self-TCO, pattern matching, closures
    │
    └─ Bytecode ─────► [IR Compiler]   AST → IR → flat bytecode
                          │
                          ▼
                       [Bytecode VM]   Register-based, self-TCO
                          │
                          ▼ (--jit)
                       [Cranelift JIT] Eligible functions → native x86-64
```

### Crate Structure

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `airl-syntax` | Lexer, parser, AST, diagnostics | None |
| `airl-types` | Type checker, linearity, exhaustiveness | airl-syntax |
| `airl-contracts` | Contract evaluation, stub prover | airl-syntax, airl-types |
| `airl-runtime` | Interpreter, IR VM, bytecode VM, bytecode JIT, values, builtins, tensor ops | airl-syntax, airl-types, airl-contracts, airl-codegen, cranelift (optional, `jit` feature) |
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

AIRL has two JIT paths:

### Bytecode→Cranelift JIT (`--jit`)

The bytecode VM can JIT-compile eligible functions to native x86-64 via Cranelift. A function is eligible if it uses only primitive operations (arithmetic, comparisons, control flow, calls to other eligible functions) — no lists, closures, variants, or builtins.

```bash
# Build with JIT support
cargo build --release --features jit

# Run with JIT
cargo run --features jit -- run --jit program.airl

# Debug output shows what gets JIT'd
AIRL_JIT_DEBUG=1 cargo run --features jit -- run --jit program.airl
```

**Performance:**

| Benchmark | Bytecode | JIT | Speedup |
|-----------|----------|-----|---------|
| fib(30) | 4,559ms | 68ms | **67x** |
| fact(12)x10K | 158ms | 68ms | **2.3x** |

Ineligible functions fall back to bytecode transparently — all existing programs work unchanged.

### Tensor JIT

Tensor operations (`tensor.add`, `tensor.mul`, `tensor.matmul`) are JIT-compiled to native loops via Cranelift in the interpreter path. No feature flag required.

### Scalar JIT (interpreter path)

The tree-walking interpreter also detects functions with primitive signatures and JIT-compiles them via the `airl-codegen` crate:

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

No annotation needed — the interpreter detects eligible functions automatically. Contracts are still checked before and after the native call.

## Project Stats

- **517 tests** across 8 crates
- **~19,000 lines** of Rust + **~21,000 lines** of AIRL (bootstrap compiler + tests)
- **Self-hosting** — lexer, parser, type checker, and IR compiler written in AIRL
- **Compiler fixpoint verified** — the compiled compiler reproduces itself
- **Zero external dependencies** for core crates by default (Cranelift in `airl-codegen` and optionally `airl-runtime` behind `jit` feature; Z3 in `airl-solver`)

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

MIT
