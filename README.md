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

## Why AIRL?

Every existing programming language optimizes for human readability. AIRL optimizes for AI producers and consumers:

- **Mandatory contracts** — The compiler rejects functions without `:requires`/`:ensures`. AI code generators skip optional features; they don't skip grammar requirements.
- **S-expression syntax** — The AST *is* the syntax. LL(1), zero ambiguity, trivially parseable, maximally token-efficient.
- **Messages are programs** — Agents exchange AIRL source text as both the message format and the execution format. No protobuf, no gRPC, no separate serialization.
- **Formal verification** — Z3 SMT solver proves contracts at compile time. What can't be proven is checked at runtime.
- **Linear ownership** — Rust-style move semantics with static linearity analysis. No garbage collector.

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

**4. AIRL already compiles to native code — the AI doesn't need to.** The toolchain transparently compiles all functions to native x86-64 via Cranelift (JIT at load time, or AOT to standalone executables). Eligible pure-arithmetic functions compile to raw CPU instructions — 42x faster than Python. AI generates high-level, verified AIRL once; the toolchain picks the fastest execution strategy automatically.

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
- Linear ownership with static linearity analysis (own, ref, mut, copy)
- Mandatory contract system (requires, ensures, invariant, intent)
- Algebraic data types (sum types, product types)
- Pattern matching with exhaustiveness checking
- First-class functions and closures
- **Standard library** — 46 pure AIRL functions (collections, math, result combinators, string, map) auto-loaded as a prelude

### Compilation & Execution

AIRL v0.2 supports two execution modes:

**`airl run` (JIT)** — All functions compiled to native x86-64 via Cranelift at load time, using the `airl-rt` C-ABI runtime library for value operations.

**`airl compile` (AOT)** — Produces standalone native executables with a two-tier compilation strategy:
- **Unboxed tier:** Eligible functions (pure arithmetic, no lists/closures/builtins) compile to raw `i64`/`f64` register operations — single CPU instructions, no heap allocation. fib(35): 56ms (42x faster than Python).
- **Boxed tier:** Functions using lists, variants, closures, or builtins compile with `*mut RtValue` heap-allocated values via `airl-rt` runtime calls.
- **Boundary marshaling:** When boxed code calls unboxed code, values are automatically extracted/reboxed at the call boundary.

Both modes share the same frontend pipeline:
1. **Source → AST** — Hand-written recursive descent parser (LL(1))
2. **Static analysis** — Type checking, linearity checking, Z3 contract verification
3. **AST → IR → Bytecode** — Contracts compiled as assertion opcodes, ownership annotations compiled as move-tracking opcodes
4. **Cranelift compilation** — JIT (in-memory) or AOT (object file → link → native executable)

Contracts are **always enforced** — in both JIT and AOT compiled native code, every contract assertion is a single conditional branch (essentially free on the happy path). Ownership is enforced both statically (linearity checker) and at runtime (move-tracking opcodes). There is no "fast but unsafe" mode.

### Performance

| Benchmark | AIRL v0.2 | Python | Ratio |
|-----------|-----------|--------|-------|
| fib(35) AOT native | 56ms | 2,335ms | **42x faster** |
| fact(20) x 100K AOT | 6ms | 248ms | **41x faster** |
| 21/25 trivial tasks (AOT) | 1ms | 40ms | **40x faster** |
| Code size (25 tasks) | 10,519 chars | 18,342 chars | **43% smaller** |

AIRL v0.2 AOT uses a two-tier compilation strategy: eligible pure-arithmetic functions compile to raw CPU instructions (no heap allocation), while functions using lists, closures, or builtins compile with boxed runtime calls. The unboxed fast path achieves C-like performance on compute-heavy workloads, while enforcing contracts on every function call.

### Bootstrap Compiler (Self-Compiling)

AIRL includes a self-compiling bootstrap compiler — the compiler's front-end phases (lexer, parser, evaluator, type checker, IR compiler) are themselves implemented in AIRL. This is ~2,500 lines of AIRL that can lex, parse, type-check, and compile AIRL source code — including itself.

**What it does:**
- **Lexer** (`bootstrap/lexer.airl`, ~365 lines) — Tokenizes AIRL source strings. Self-parse verified: lexes its own source (15,691 chars → 3,400 tokens).
- **Parser** (`bootstrap/parser.airl`, ~930 lines) — Converts token streams to typed AST nodes, including `deftype` sum/product type declarations.
- **Evaluator** (`bootstrap/eval.airl`, ~616 lines) — Interprets AST nodes using tagged value variants and a map-based environment frame stack.
- **Type Checker** (`bootstrap/types.airl` + `bootstrap/typecheck.airl`, ~715 lines) — Two-pass architecture: registration then checking. All bootstrap modules pass cleanly.
- **IR Compiler** (`bootstrap/compiler.airl`, ~400 lines) — Compiles AST to a tree-flattened IR format.

**Fixpoint verified:** The compiled compiler produces identical IR to the interpreted compiler — the compiler can compile itself and the output is self-consistent.

**Runtime dependency:** The compiled output links against `libairl_rt.a`, which provides ~48 primitive builtins (`+`, `head`, `char-at`, `map-get`, `print`, etc.) as `extern "C"` functions. This is analogous to a C compiler that needs `libc` — the compiler is self-compiling, the output just needs a runtime library to execute. The AOT mode emits object files via Cranelift `ObjectModule` and links with `libairl_rt.a` to produce standalone native executables.

To build and run the bootstrap compiler tests:

```bash
# Build the AIRL binary with JIT support (recommended)
cargo build --release --features jit

# Lexer tests
cargo run --release --features jit -- run bootstrap/lexer_test.airl

# Parser tests
cargo run --release --features jit -- run bootstrap/parser_test.airl

# Full lex→parse→eval pipeline
cargo run --release --features jit -- run bootstrap/pipeline_test.airl

# Type checker tests (slow, use --release)
cargo run --release --features jit -- run bootstrap/typecheck_test.airl

# IR compiler tests
cargo run --release --features jit -- run bootstrap/compiler_test.airl

# Equivalence: interpreted vs compiled produce identical results (32 tests)
cargo run --release --features jit -- run bootstrap/equivalence_test.airl

# Compiler fixpoint: compiled compiler reproduces itself (slow, ~60min)
cargo run --release --features jit -- run bootstrap/fixpoint_test.airl
```

### Agent Communication
- Inter-agent task exchange over TCP and Unix sockets
- `spawn-agent` builtin launches worker processes via stdio
- `send` builtin dispatches typed, contract-verified tasks
- Length-prefixed AIRL S-expression wire protocol
- Capability-based agent registry

### CLI
```
airl run <file>              Run an AIRL source file (JIT-compiled with contracts)
airl compile <file> -o <out> AOT compile to standalone native executable
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
cargo build --release --features jit,aot
```

Requirements: Rust 1.85+, CMake, C++ compiler, Python 3 (for Z3 compilation on first build, ~5-15 min).

Building without `--features jit,aot` produces a bytecode-only binary (no Cranelift dependency, no native code compilation). All features work, but compute-heavy functions will be slower.

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
cargo run --release --features jit -- run examples/01-hello-world/hello_world.airl
# Hello, World!
# Greetings from AIRL,
# fellow AI
```

### Run a Program

```bash
# Function with contracts
cargo run --release --features jit -- run examples/02-functions-and-contracts/functions_and_contracts.airl

# Formally verify contracts with Z3
cargo run --release --features jit -- check examples/03-verified-arithmetic/verified_arithmetic.airl
```

### Compile to Native Binary

```bash
cargo run --release --features jit,aot -- compile examples/02-functions-and-contracts/functions_and_contracts.airl -o my_program
./my_program   # standalone native executable — no AIRL toolchain needed
```

### Type Check

```bash
cargo run --release --features jit -- check math.airl
# OK: math.airl
```

### Interactive REPL

```bash
cargo run --release --features jit -- repl
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
cargo run --release --features jit -- agent worker.airl --listen tcp:127.0.0.1:9001
```

Terminal 2 — send tasks:
```bash
cargo run --release --features jit -- call tcp:127.0.0.1:9001 add 3 4
# → 7
cargo run --release --features jit -- call tcp:127.0.0.1:9001 multiply 6 7
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
cargo run --release --features jit -- run orchestrator.airl
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
cargo run --release --features jit -- run examples/01-hello-world/hello_world.airl
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
[Linearity Checker]   Static ownership analysis (use-after-move, borrow conflicts)
    │
    ▼
[Z3 Verifier]         Prove contracts via SMT (negation + UNSAT)
    │
    ▼
[IR Compiler]         AST → IR (intermediate representation)
    │
    ▼
[Bytecode Compiler]   IR → register-based bytecode (contracts as assertion opcodes)
    │
    ├──── airl run ────► [Cranelift JIT-Full]  ALL functions → native x86-64
    │                     Contract assertions → native conditional branches
    │                     Ownership checks → native move tracking
    │
    └──── airl compile ► [Cranelift AOT]  Two-tier native compilation
                          Eligible (pure arithmetic) → raw i64/f64 register ops
                          Ineligible (lists/closures) → boxed *mut RtValue calls
                          Boundary marshaling at cross-tier call sites
                          Links with libairl_rt.a → standalone native executable
```

### Crate Structure

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `airl-syntax` | Lexer, parser, AST, diagnostics | None |
| `airl-types` | Type checker, linearity, exhaustiveness | airl-syntax |
| `airl-contracts` | Contract violation types | airl-syntax, airl-types |
| `airl-runtime` | Bytecode VM, bytecode JIT, values, builtins, tensor ops | airl-syntax, airl-types, airl-contracts, airl-codegen, cranelift (optional, `jit` feature) |
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

Contracts are compiled to bytecode assertion opcodes and, for JIT-eligible functions, to native conditional branches. The happy path (contract passes) is a single branch instruction — essentially free on modern CPUs with branch prediction. Contract violations halt execution immediately with a diagnostic showing the function name, failed clause, and argument values.

**Three verification levels:**

| Level | Behavior |
|-------|----------|
| Checked | Contracts compiled as runtime assertions (default) |
| Proven | Z3 attempts static proof; falls back to runtime if Unknown |
| Trusted | Contracts assumed true (for FFI and axioms) |

## Ownership Model

AIRL uses linear ownership with explicit annotations, enforced by both static analysis and runtime move tracking:

```clojure
(defn consume
  :sig [(own x : i32) -> i32]     ;; x is moved — caller can't use it after
  :intent "consume x"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)

(let (v : i32 42)
  (do (consume v)
      v))  ;; ERROR: use-after-move — v was moved
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

## JIT & AOT Compilation

AIRL has two native compilation backends, both using Cranelift:

### JIT (`airl run`)

Compiles **every function** to native x86-64 at load time. Every AIRL value is a `*mut RtValue` pointer — a boxed, ref-counted heap allocation. All operations go through C-ABI runtime helper calls in `airl-rt`.

### AOT (`airl compile`)

Produces **standalone native executables** with a two-tier compilation strategy:

- **Unboxed tier:** Eligible functions (pure arithmetic, no lists/variants/closures/builtins, arity ≤ 8) compile to raw `i64`/`f64` register operations. Arithmetic is a single CPU instruction — no malloc, no function calls. fib(35): **56ms** (42x faster than Python, ~290x faster than boxed).
- **Boxed tier:** Everything else compiles with `*mut RtValue` runtime calls, same as JIT.
- **Boundary marshaling:** When boxed code calls an unboxed function, args are extracted via `airl_as_int_raw`, the unboxed function is called with raw values, and the result is reboxed using the callee's return type hint.

**Contract-aware compilation:** Contract assertions compile to native conditional branches in both tiers. The happy path is a single branch instruction — essentially free with branch prediction. On failure, the runtime signals a `ContractViolation`.

**Ownership-aware compilation:** Move-tracking opcodes (`MarkMoved`, `CheckNotMoved`) enforce use-after-move detection at runtime for `own`-annotated parameters, complementing the static linearity checker.

```bash
# Build with JIT + AOT support
cargo build --release --features jit,aot

# JIT — all functions compile to native code at load time
cargo run --release --features jit -- run program.airl

# AOT — produce standalone native executable
cargo run --release --features jit,aot -- compile program.airl -o program
./program   # runs without cargo or the AIRL toolchain

# Debug output
AIRL_JIT_DEBUG=1 cargo run --release --features jit -- run program.airl
AIRL_AOT_DEBUG=1 cargo run --release --features jit,aot -- compile program.airl -o program
```

## Project Stats

- **~508 tests** across 8 crates
- **~19,000 lines** of Rust + **~21,000 lines** of AIRL (bootstrap compiler + tests)
- **All functions JIT-compiled** — every function compiles to native x86-64 via Cranelift (jit-full)
- **AOT produces standalone native executables** — `airl compile` with two-tier unboxed/boxed compilation
- **Unboxed AOT: 42x faster than Python** — eligible pure-arithmetic functions compile to raw CPU instructions
- **Contracts always enforced** — native conditional branches in both JIT and AOT, assertion opcodes in bytecode
- **Ownership enforced** — static linearity analysis + runtime move tracking
- **Quantifiers work everywhere** — `forall`/`exists` desugared to `fold`+`range`, no interpreter-only restrictions
- **Bootstrap compiler** — lexer, parser, type checker, and IR compiler implemented in AIRL (~2,500 lines), running on Rust runtime
- **Compiler fixpoint verified** — the AIRL compiler produces identical IR whether run interpreted or compiled
- **Zero external dependencies** for core crates by default (Cranelift behind `jit`/`aot` features; Z3 in `airl-solver`)
- **GPU support available** — `cargo build --features mlir` for MLIR/GPU compilation (requires LLVM 19+, Dockerfile provided)

## Specification

The complete language specification is in [`AIRL-Language-Specification-v0.1.0.md`](AIRL-Language-Specification-v0.1.0.md).

## License

MIT
