# AIRL — Project Analysis

**Date:** 2026-03-24
**Version:** 0.2.0
**Scope:** ~43K lines (19K Rust + 21K AIRL + 3K misc), 9 crates, ~508 tests. Built in 4 days, almost entirely by Claude.

---

## Major Milestones

### 1. Core Language & Runtime

Hand-written recursive descent parser (LL(1)), dependent type system with tensor dimension unification, algebraic data types, pattern matching, first-class functions/closures. 46-function standard library (collections, math, result combinators, strings, maps) — mostly pure AIRL, auto-loaded as prelude.

### 2. Contract System + Z3 Integration

Mandatory `:requires`/`:ensures`/`:invariant` on every function — the parser rejects functions without them. Z3 SMT solver attempts static proofs of contract clauses. What can't be proven statically is enforced at runtime. Contract violations return the exact failing clause with all variable bindings.

### 3. Linear Ownership Analysis

Rust-inspired `own`/`ref`/`mut`/`copy` annotations. Static linearity checker catches use-after-move and borrow conflicts at compile time. Runtime `MarkMoved`/`CheckNotMoved` opcodes provide a backstop.

### 4. Bootstrap Compiler (Self-Compiling)

~2,500 lines of AIRL implementing the compiler's own front-end: lexer (365 lines, self-parse verified), parser (930 lines), evaluator (616 lines), type checker (715 lines), IR compiler (400 lines). Fixpoint verified — the compiled compiler produces identical output to the interpreted compiler.

### 5. Register-Based Bytecode VM

~34-opcode flat instruction set, linear register allocation, self-TCO. Replaced the tree-walking interpreter as the default execution path. 5-28x faster than the interpreter depending on workload.

### 6. v0.2 Execution Consolidation

Removed the tree-walking interpreter and IR VM entirely. Single execution path: bytecode VM + Cranelift JIT. Contracts compiled to bytecode assertion opcodes — always enforced, no opt-out. Eliminated the "safety vs speed" tradeoff that existed in v0.1.

### 7. Contract-Aware Full JIT

Compiles **all** functions to native x86-64 via Cranelift, using `airl-rt` C-ABI runtime for value operations. Contract assertions are native conditional branches (single predicted-taken branch on happy path — essentially free). fib(30) with contracts: 13ms (23x faster than Python).

### 8. Two-Tier AOT Compilation

`airl compile <file.airl> -o <binary>` produces standalone native executables. Unboxed tier: eligible pure-arithmetic functions compile to raw `i64`/`f64` CPU instructions — no heap allocation. Boxed tier: everything else uses `*mut RtValue` runtime calls. Boundary marshaling handles cross-tier calls automatically. fib(35): 56ms vs Python 2,335ms (**42x faster**).

### 9. Self-Hosting Native Compiler

The native binary can read AIRL source, compile it through the full pipeline, and produce a running native executable. Links against `libairl_rt.a` (analogous to libc).

### 10. Agent Infrastructure

`spawn-agent`, `send`, `send-async`, `await`, `parallel`, `broadcast`, `retry`, `escalate`. Message-passing concurrency where AIRL source text is both the message format and the execution format.

---

## Strengths

### 1. The Correctness Result

96% first-attempt LLM correctness (vs Python's 68%) with zero AIRL-specific training data. 28-point gap from a 115-line language reference. Driven by zero-ambiguity grammar, mandatory structure, and simpler semantics.

### 2. Safety With Zero Runtime Cost

Every function call is contract-verified in native code. A single CPU branch instruction on the happy path. No "fast but unsafe" mode exists. Ownership is enforced both statically and at runtime. v0.2 eliminated the safety-vs-speed tradeoff entirely — you never have to choose between correctness and performance.

### 3. High-Signal Error Messages for LLM Self-Correction

Contract violations include: function name, which clause failed, the S-expression source of the clause, and all relevant variable bindings. An LLM can read `Requires violated in 'safe-div': (!= b 0) evaluated to false with b = 0` and fix the issue in one iteration. This is better debugging signal than a Python traceback.

### 4. Token Efficiency

AIRL programs are 43% smaller than Python equivalents across 25 benchmark tasks — including contracts, type signatures, and intent declarations. The S-expression syntax is both the AST and the serialization format — no parsing ambiguity, no redundant syntax.

### 5. Native Performance Where It Matters

AOT unboxed path: 42x faster than Python for numeric code. Startup: ~1ms (native binary) vs ~40ms (Python). For the primary use case — an agent generates a function, runs it, returns a result — AIRL is faster at every step of the pipeline.

### 6. Engineering Velocity

43K lines, 9 crates, a self-compiling bootstrap compiler with fixpoint verification, bytecode VM, full JIT, AOT to native executables, Z3 integration, agent protocol — built in 4 days. This is itself a data point about what AI-assisted development can produce.

---

## Differentiators

### vs Python

| Dimension | Python | AIRL |
|---|---|---|
| LLM first-attempt correctness | 68% | **96%** |
| Contracts | Optional (`assert`) | **Mandatory (grammar-level)** |
| Formal verification | None | **Z3 integrated** |
| Code size (25 tasks) | 18,836 chars | **10,768 chars (43% smaller)** |
| Error signal quality | Stack traces | **Clause + variable bindings** |
| Safety-speed tradeoff | N/A | **None — both always on** |
| Startup latency | ~40ms | **~1ms AOT, ~4ms JIT** |
| Numeric compute perf | Baseline | **42x faster (AOT unboxed)** |

### vs Generating Low-Level IR (LLVM/WASM) Directly

| Dimension | Direct IR | AIRL |
|---|---|---|
| Token cost | 3-5x more (SSA, basic blocks) | **43% less than Python** |
| Error on wrong code | Segfault / silent corruption | **Contract violation with diagnostics** |
| LLM error rate | High (abstraction gap) | **4% (mandatory structure constrains output)** |
| Native performance | Near-C | **42x faster than Python (unboxed AOT)** |
| Safety layers | None | **Type checker + linearity + Z3 + contracts** |

### vs Rust

| Dimension | Rust | AIRL |
|---|---|---|
| Target author | Human expert | LLM |
| Ownership model | Lifetimes, NLL, reborrowing | 4 annotations, no lifetimes |
| Contracts | Optional (third-party crates) | **Mandatory (grammar-level)** |
| Formal verification | Not built in | **Z3 SMT solver** |
| LLM generation success rate | Low (lifetime complexity) | **96%** |
| Key tradeoff | Maximum safety, high complexity | Sufficient safety, high AI success rate |

### The Core Differentiator

No individual feature is novel — S-expressions, contracts, Z3, Cranelift JIT all exist elsewhere. The contribution is combining them into a coherent system optimized for the specific error distribution and behavior patterns of LLM code generators:

- LLMs skip optional safety features → make contracts mandatory at the grammar level
- LLMs struggle with lifetime reasoning → simplify to 4 ownership annotations
- LLMs pattern-match rather than verify semantics → add Z3 formal verification
- LLMs need actionable error signals → contract violations show clause + bindings
- LLMs generate small, pure functions → optimize that path to 42x faster than Python

---

## Benchmark Data

**Source:** [`benchmarks/FINDINGS.md`](../benchmarks/FINDINGS.md) (25 tasks, Claude, 2026-03-21)

| Metric | AIRL | Python | Winner |
|---|---|---|---|
| First-attempt correctness | 24/25 (96%) | 17/25 (68%) | **AIRL by 28pp** |
| Total characters | 10,768 | 18,836 | **AIRL (1.75x more compact)** |
| Avg intent recovery score | 4.72/5 | 4.82/5 | Python (marginal) |

**Performance (v0.2):**

| Benchmark | AIRL v0.2 | Python | Ratio |
|---|---|---|---|
| fib(35) AOT unboxed | 56ms | 2,335ms | **42x faster** |
| fib(30) JIT with contracts | 13ms | 302ms | **23x faster** |
| fact(20) x 100K AOT | 6ms | 248ms | **41x faster** |
| 21/25 trivial tasks (AOT) | ~1ms | ~40ms | **40x faster** |

### Benchmark Limitations

- Single LLM (Claude only). Results may differ with GPT-4, Gemini, etc.
- Single run per task. No repeated trials for statistical variance.
- Self-evaluation bias: Claude scores its own output for intent recovery.
- Simple tasks: most are single-function. Multi-module programs not tested.
- S-expression familiarity: Claude has extensive Lisp/Scheme training data, so AIRL's syntax is not truly novel to the model.
- List/closure-heavy performance under the full JIT has not been comprehensively benchmarked. Numeric-heavy and startup-dominated workloads are well-characterized; list-processing workloads are the remaining gap.

---

## Known Gaps

1. **Type checker does not know stdlib signatures.** All 46 stdlib functions are registered as generic `TypeVar("builtin")`. Misuse (wrong arg types, wrong arity) is caught at runtime, not at compile time.

2. **Z3 verification is informational only.** Proven/disproven results are printed but do not block execution. Z3 cannot reason about `result` in `:ensures` clauses (treats it as unconstrained).

3. **Recursive stdlib is a performance ceiling.** `map`/`filter`/`fold` are recursive AIRL functions, not native implementations. Until these are JIT-inlined or replaced with native loops, list-heavy code will be slower than Python's built-in comprehensions.

4. **Non-exhaustive match is runtime, not static.** The exhaustiveness checker exists but is not wired into the type checker for match expressions.

5. **Linearity checker is opt-in.** Only parameters annotated with `own`/`ref`/`mut` are tracked. Unannotated parameters (the default) have no ownership enforcement. Runtime move-tracking opcodes provide a backstop but only for annotated parameters.
