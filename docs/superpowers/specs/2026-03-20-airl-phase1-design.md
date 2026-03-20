# AIRL Phase 1 — Tree-Walking Interpreter Design

**Date:** 2026-03-20
**Status:** Approved
**Spec Version:** AIRL Language Specification v0.1.0

---

## Overview

Phase 1 implements a complete tree-walking interpreter for AIRL in Rust, covering the full language as specified: S-expression parsing, dependent type checking with linear ownership, mandatory contract verification (three levels), a tree-walking evaluator, and an inter-agent communication runtime over stdio/TCP/Unix sockets.

**Design principles:**
- Zero external dependencies (std only)
- Hand-written recursive descent parser (LL(1) grammar)
- Extensive testing at all levels

---

## Crate Structure

```
airl/
├── Cargo.toml              # workspace root
├── crates/
│   ├── airl-syntax/        # Lexer, parser, AST, diagnostics
│   ├── airl-types/         # Type checker, dependent types, linearity
│   ├── airl-contracts/     # Contract evaluation, stub prover
│   ├── airl-runtime/       # Tree-walking interpreter, values, builtins
│   ├── airl-agent/         # Agent identity, transport, task dispatch
│   └── airl-driver/        # CLI, REPL, pipeline orchestration
├── tests/
│   ├── fixtures/           # .airl test programs
│   │   ├── valid/
│   │   ├── type_errors/
│   │   ├── contract_errors/
│   │   ├── linearity_errors/
│   │   └── agent/
│   └── e2e/
└── docs/
```

### Dependency Graph

```
airl-syntax
    ↓
airl-types
    ↓
airl-contracts
    ↓
airl-runtime
    ↓
airl-agent
    ↓
airl-driver
```

Each crate depends only on those above it. No circular dependencies.

---

## 1. `airl-syntax` — Lexer, Parser, AST

### Lexer

Hand-written scanner producing a flat token stream. Token types from spec §2.2:

- `Integer` — decimal, hex (0x), binary (0b)
- `Float` — decimal with dot or exponent, optional suffix (f32)
- `String` — double-quoted, backslash escapes
- `Symbol` — alphanumeric + hyphen + dot
- `Keyword` — colon-prefixed symbol
- `Bool` — `true`, `false`
- `Nil`
- `LParen`, `RParen`, `LBracket`, `RBracket`
- Comments (`;` line, `#| |#` block) are skipped

Every token carries a `Span` (byte offset, line, column) for error reporting.

### Parser

Two-layer design:

1. **S-expression parser** — Produces generic `SExpr` tree (atoms and lists). ~100 lines, handles the entire grammar.
2. **Form parser** — Walks `SExpr` and recognizes AIRL forms (`defn`, `deftype`, `module`, `task`, `let`, `if`, `match`, etc.), producing typed AST nodes.

### AST

```rust
enum TopLevel { Module, Defn, DefType, Task, UseDecl }
enum Expr { Atom, List, If, Match, Let, Do, FnCall, Lambda, ... }
enum Type { Primitive, Tensor, Function, Named, TypeApp }
struct Contract { requires, ensures, invariant, intent }
```

Every node carries a `Span`. Ownership annotations (`own`, `&ref`, `&mut`, `copy`) are part of parameter representation.

### Error Reporting

Structured `Diagnostic` type with severity, span, message, and optional notes. Errors are collected (not fatal on first error).

---

## 2. `airl-types` — Type System and Linearity Checker

### Type Representation

```rust
enum Ty {
    Prim(PrimTy),
    Tensor { elem: Box<Ty>, shape: Vec<DimExpr> },
    Func { params: Vec<Ty>, ret: Box<Ty> },
    Named { name: Symbol, args: Vec<TyArg> },
    Sum(Vec<Variant>),
    Product(Vec<Field>),
    TypeVar(Symbol),
    Nat(NatExpr),
    Unit, Never,
}

enum DimExpr {
    Lit(u64),
    Var(Symbol),
    BinOp(Op, Box<DimExpr>, Box<DimExpr>),
}
```

### Type Checking Pass

Single-pass AST walk:
1. Build scoped type environment (push/pop on let, fn, match arms)
2. Assign types to every expression
3. Verify function call argument types against signatures
4. Unify dependent dimensions (shared `K` in matrix multiply)
5. Resolve type applications (substitute type parameters)
6. Validate match exhaustiveness

### Linearity Checker

Flow-sensitive ownership tracking:

```rust
enum OwnershipState { Owned, Borrowed(BorrowKind, usize), Moved, Dropped }
```

Rules enforced:
- Use after move → error
- `&mut` while `&ref` exists → error
- Multiple `&mut` → error
- Move while borrowed → error
- `copy` on non-Copy type → error

For branches (`if`/`match`), both arms must leave bindings in compatible states.

---

## 3. `airl-contracts` — Contract Evaluation and Stub Prover

### Verification Level: `checked`

Runtime assertions. On function call:
1. Evaluate `:requires` against arguments. Violation → `ContractViolation` with clause, bindings, evaluation trace.
2. Execute body.
3. Bind `result`, evaluate `:ensures`. Same violation reporting.
4. `:invariant` checked at loop/recursion boundaries.

Quantifiers (`forall`, `exists`) evaluated by iteration over collections.

### Verification Level: `proven` (Stub Prover)

Simple symbolic evaluator. Can prove:
- Constant arithmetic: `(= (+ 2 3) 5)`
- Identity: `(= x x)`
- Inequalities from `:requires` context
- Shape propagation
- Boolean tautologies

Returns `Proven`, `Disproven(Counterexample)`, or `Unknown(reason)`. Unknown falls back to runtime assertion with a warning.

### Verification Level: `trusted`

Contracts recorded but never evaluated. Compiler emits a note.

### ContractViolation

Matches spec §9.2:
```rust
struct ContractViolation {
    function: Symbol,
    contract_kind: ContractKind,
    clause_source: String,
    bindings: Vec<(Symbol, Value)>,
    evaluated: String,
    span: Span,
}
```

---

## 4. `airl-runtime` — Tree-Walking Interpreter

### Value Representation

```rust
enum Value {
    Int(i64), UInt(u64), Float(f64), Bool(bool), Str(String), Nil, Unit,
    Tensor(TensorValue),
    List(Vec<Value>), Tuple(Vec<Value>),
    Variant(Symbol, Box<Value>),
    Struct(BTreeMap<Symbol, Value>),
    Function(FnDef), Lambda(LambdaDef), BuiltinFn(BuiltinFnId),
    AgentId(AgentIdValue), TaskResult(TaskResultValue),
}
```

### TensorValue

Flat `Vec<f32/f64/etc.>` with shape metadata. CPU-only for Phase 1.

### Environment

Stack of frames, each with bindings mapping symbols to `Slot { value, ty, ownership }`. Runtime enforces linearity as a double-check on the static analysis.

### Evaluation

`eval(expr, env) -> Result<Value, RuntimeError>` handles: atoms, if, let, match, do, function call (with contract checking), lambda, tensor ops.

### Pattern Matching

```rust
enum Pattern { Wildcard, Binding(Symbol), Literal(Value), Variant(Symbol, Box<Pattern>), Struct(Vec<(Symbol, Pattern)>) }
```

### Builtins

- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `=`, `!=`, `<`, `>`, `<=`, `>=`
- Logic: `and`, `or`, `not`, `xor`
- Tensor: `tensor.zeros`, `tensor.ones`, `tensor.rand`, `tensor.identity`, `tensor.add`, `tensor.mul`, `tensor.matmul`, `tensor.contract`, `tensor.reshape`, `tensor.transpose`, `tensor.slice`, `tensor.sum`, `tensor.max`, `tensor.softmax`
- Collections: `length`, `at`, `append`, `map`, `filter`, `fold`
- Utility: `print`, `assert`, `type-of`, `shape`

---

## 5. `airl-agent` — Agent Communication Runtime

### Agent Identity

```rust
struct AgentId {
    name: String,
    capabilities: HashSet<Capability>,
    trust_level: TrustLevel,
    endpoint: Endpoint,
}

enum Capability { ComputeGpu, ComputeCpu, WebSearch, CodeExecution, FileAccess, AgentSpawn, Custom(String) }
enum TrustLevel { None, Verified, Proven }
enum Endpoint { Tcp(SocketAddr), Unix(PathBuf), Stdio }
```

### Transport

Trait-based abstraction with three implementations:
- `StdioTransport` — stdin/stdout of a child process
- `TcpTransport` — TCP sockets
- `UnixTransport` — Unix domain sockets

Wire format: `[u32 big-endian length][UTF-8 AIRL S-expression payload]`

### Agent Runtime

Manages agent lifecycle: identity, registry of known agents, pending tasks, interpreter instance, listener.

### Task Lifecycle

Sender serializes `(task ...)` as AIRL text → transport → receiver parses, typechecks, validates `:requires`, executes body, validates `:ensures` → sends `(TaskResult ...)` back → sender validates against `:expected-output` (rigor depends on trust level).

### Trust Verification

- `trust:none` — all contracts re-evaluated
- `trust:verified` — random subset spot-checked
- `trust:proven` — proof object checked (stub prover's `Proven` tag)

### Agent Operations (Builtins)

- `send` — dispatch task over transport
- `await` — block with timeout for result
- `spawn-agent` — launch child process, connect via stdio
- `parallel` — fan-out, collect, merge
- `broadcast` — send to all matching capability filter

---

## 6. `airl-driver` — CLI, REPL, Pipeline

### CLI Modes

```
airl run <file.airl>     # full pipeline + execute
airl check <file.airl>   # parse + typecheck only
airl repl                # interactive REPL
airl agent <file.airl>   # start as listening agent
airl fmt <file.airl>     # pretty-print S-expressions
```

### Pipeline

Source → Lexer → Parser → Type checker → Linearity checker → Contract verification → Execution/Agent listen

Shared `Diagnostics` collector. Errors in any phase skip subsequent phases.

### REPL

Reads one top-level S-expression at a time (paren-balanced multi-line). Persistent state across expressions. Special commands: `:quit`, `:type <expr>`, `:env`. Raw stdin/stdout, no dependencies.

### Error Output

```
error[E0042]: use of moved value `x`
  --> example.airl:12:5
   |
12 |     (+ x y)
   |        ^ value used after move
```

---

## 7. Testing Strategy

### Test Tiers

| Tier | Location | Speed | What |
|---|---|---|---|
| 1. Unit | `crates/*/src/` | Fast | Single function/type |
| 2. Crate integration | `crates/*/tests/` | Fast | Crate public API |
| 3. Fixture E2E | `tests/e2e/` | Medium | Full pipeline on .airl files |
| 4. Multi-agent E2E | `tests/e2e/` | Slow | Multiple agent processes |

### Fixture Organization

```
tests/fixtures/
├── valid/              # programs that pass all phases
├── type_errors/        # expected type check failures
├── contract_errors/    # expected contract violations
├── linearity_errors/   # ownership violations
└── agent/              # multi-agent scenarios
```

### Spec Coverage

Every code example from the AIRL spec (§3-§9) becomes a fixture test. Error fixtures carry `;; ERROR:` annotations specifying expected diagnostics.

### Invariants

- Every public function has at least one unit test
- Every error path has a negative test
- No `#[allow(unused)]` in production code
- Multi-agent tests cover happy path, timeout, and contract failure

---

## Not In Scope

- Z3 / real SMT integration (Phase 2)
- MLIR lowering / compilation (Phase 2)
- GPU execution (Phase 2)
- IDE tooling / LSP (out of scope per spec)
- Self-hosting (Phase 3)
