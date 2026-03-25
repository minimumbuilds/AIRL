# AIRL — AI Intermediate Representation Language

## Language Specification

**Version 0.1.0 — Draft**
**March 2026**

*A programming language designed for AI systems, not humans.*
*Communication protocol. Execution format. Verification framework.*

---

## 1. Introduction

### 1.1 Purpose

AIRL (AI Intermediate Representation Language) is a programming language designed exclusively for AI systems to author, transmit, execute, and verify computational programs. Unlike every existing programming language, AIRL does not optimize for human readability, ergonomic syntax, or developer experience. Instead, it optimizes for the needs of AI producers and consumers: deterministic parseability, token efficiency, formal verifiability, and seamless inter-agent communication.

AIRL occupies a unique position in the language design space. It is simultaneously a compilation target (AI systems generate AIRL programs for native execution), an interchange protocol (agents communicate by exchanging AIRL expressions), and a verification framework (every AIRL program carries machine-checkable correctness proofs).

### 1.2 Design Principles

The following principles govern all design decisions in AIRL, listed in priority order:

| Priority | Principle | Rationale |
|----------|-----------|-----------|
| 1 | Agent Interoperability | The primary purpose of AIRL is to serve as a typed, verifiable communication medium between AI agents. Every design decision must preserve or enhance the ability of heterogeneous AI systems to exchange AIRL programs. |
| 2 | Verification Strength | Every AIRL program must carry formal correctness proofs. Contracts are mandatory, not optional. The compiler rejects programs without pre/post conditions. Runtime contract violations are treated as bugs, not expected errors. |
| 3 | Token Efficiency | Every token an LLM generates costs compute. AIRL must be maximally dense in semantic content per token. Redundant syntax, boilerplate, and ceremony are defects. |
| 4 | Hardware Targeting | AIRL programs must compile to efficient native code for CPUs, GPUs, and AI accelerators. The compilation path is AIRL → MLIR dialects → LLVM IR → native binary. |

### 1.3 What AIRL Is Not

- AIRL is not a language for humans to write. There is no IDE, no syntax highlighting, no language server protocol. If a human needs to inspect AIRL, they read the contract blocks, not the implementation bodies.
- AIRL is not a framework or library. It is a complete language with its own grammar, type system, semantics, and compilation pipeline.
- AIRL is not an extension of an existing language. While it draws structural inspiration from Lisp (S-expressions), Rust (linear types), and Idris (dependent types), it is a new language with its own semantics.
- AIRL is not a prompt engineering tool or natural language wrapper. AIRL has formal, mathematical semantics. Every valid program has exactly one meaning.

### 1.4 Relationship to Existing Work

**Intent language:** AIRL shares the contract-first philosophy but goes further by making contracts formally provable (not just runtime assertions) and by targeting multi-agent communication as a primary use case.

**Mojo:** AIRL shares the MLIR compilation target and the goal of high-performance AI workloads, but differs fundamentally in that Mojo is designed for human developers while AIRL is designed for AI producers.

**LLVM IR:** AIRL sits at a higher semantic level than LLVM IR. LLVM IR is a low-level SSA representation; AIRL carries intent, contracts, type-level proofs, and agent routing metadata. AIRL lowers to MLIR, which then lowers to LLVM IR.

**Protocol Buffers / gRPC:** AIRL replaces structured data interchange formats by being both the message format and the execution format. A protobuf message describes data; an AIRL message describes computation with verifiable guarantees.

---

## 2. Lexical Structure

### 2.1 Character Set

AIRL source text is encoded in UTF-8. The language uses a minimal set of syntactic characters to maximize token efficiency:

| Character(s) | Purpose | Notes |
|--------------|---------|-------|
| `( )` | Expression delimiters | Every compound form is parenthesized |
| `[ ]` | Type parameters and collections | Used in signatures, type arguments, and literal lists |
| `:` | Keyword prefix / type annotation | Keywords are atoms prefixed with colon |
| `"` | String delimiter | UTF-8 string literals |
| `;` | Comment to end of line | Stripped during parsing |
| `#\| \|#` | Block comment | Nestable |

### 2.2 Token Types

AIRL has seven token types:

| Token Type | Pattern | Examples |
|------------|---------|----------|
| Integer | Decimal, hex (0x), binary (0b) | `42`, `0xFF`, `0b1010` |
| Float | Decimal with dot or exponent | `3.14`, `1e-7`, `0.5f32` |
| String | Double-quoted, backslash escapes | `"hello"`, `"line\n"` |
| Symbol | Alphanumeric + hyphen + dot | `matrix-multiply`, `tensor.contract` |
| Keyword | Colon-prefixed symbol | `:sig`, `:requires`, `:body` |
| Boolean | Literal truth values | `true`, `false` |
| Nil | Absence of value | `nil` |

### 2.3 S-Expression Grammar

All AIRL programs are S-expressions. The grammar is defined by a single recursive production:

```
expr     ::= atom | list
atom     ::= integer | float | string | symbol | keyword | bool | nil
list     ::= '(' expr* ')'
type     ::= symbol '[' type-arg (',' type-arg)* ']'
           | symbol
type-arg ::= type | nat-expr
```

This grammar is LL(1) and unambiguous. There are no operator precedence rules, no statement/expression distinction, and no syntactic special forms beyond the parenthesized list. This is the fundamental reason AIRL uses S-expressions: the syntax IS the abstract syntax tree.

---

## 3. Type System

### 3.1 Overview

AIRL employs a dependent type system with linear ownership semantics. Types are first-class values that can appear in expressions, be passed as arguments, and participate in computation. There is no type inference. All types are explicit. This is a deliberate design choice: an AI generating AIRL pays a small token cost for explicit types, but every consumer of the AIRL program (whether another AI or a verification tool) can read the types without reconstruction.

### 3.2 Primitive Types

| Type | Description | Size |
|------|-------------|------|
| `bool` | Boolean truth value | 1 bit logical, 1 byte physical |
| `i8`, `i16`, `i32`, `i64` | Signed integers | 1, 2, 4, 8 bytes |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | 1, 2, 4, 8 bytes |
| `f16`, `f32`, `f64` | IEEE 754 floating point | 2, 4, 8 bytes |
| `bf16` | Brain floating point | 2 bytes |
| `Nat` | Type-level natural number | Erased at runtime |
| `String` | UTF-8 string | Pointer + length |
| `Unit` | Zero-information type | 0 bytes |
| `Never` | Uninhabited type (divergence) | Cannot exist |

**Float type usage notes:**

- Scalar float math builtins (`sqrt`, `sin`, `cos`, `tan`, `log`, `exp`, `floor`, `ceil`, `round`) operate on `f64` values only. Passing an integer argument is accepted (it is promoted to `f64`), but no other float widths are supported for scalar math.
- `f16` and `bf16` are **tensor-element types** and cannot be used with scalar float math builtins. They exist solely as storage formats for tensor data.
- `f32` is used internally by tensor operations (e.g., `tensor.zeros f32 [N]`) and is not directly usable with scalar math builtins.

### 3.3 Compound Types

#### 3.3.1 Tensor Types

Tensors are the fundamental multi-dimensional array type, parameterized by element type and shape:

```clojure
;; Tensor type syntax: tensor[ElementType Shape...]
(let v : tensor[f32 128]           ;; 1D vector of 128 f32s
    (tensor.zeros f32 [128]))

(let m : tensor[f32 64 64]         ;; 2D 64x64 matrix
    (tensor.identity f32 64))

(let t : tensor[bf16 B S H]        ;; 3D with type-level dims
    (tensor.alloc bf16 [B S H]))    ;; B, S, H are Nat parameters
```

#### 3.3.2 Algebraic Data Types

AIRL supports sum types (tagged unions) and product types (structs) with explicit memory layout:

```clojure
;; Sum type (tagged union)
(deftype Result [T : Type, E : Type]
  (| (Ok T)
     (Err E)))

;; Product type (struct)
(deftype AgentMessage
  (& (id      : String)
     (from    : AgentId)
     (to      : AgentId)
     (payload : Expr)
     (ttl     : Nat)))
```

#### 3.3.3 Function Types

Function types use arrow notation within type contexts:

```clojure
;; Function type: (-> InputTypes OutputType)
(let add : (-> [i32 i32] i32)
    (fn [a b] (+ a b)))

;; Higher-order: function taking a function
(let map : (-> [(-> [T] U) List[T]] List[U])
    ...)
```

### 3.4 Linear Ownership Model

Every value in AIRL has exactly one owner. When ownership is transferred (moved), the source binding becomes invalid. This is enforced statically by the compiler. There is no garbage collector, no reference counting, and no runtime memory management overhead.

| Annotation | Meaning | Semantics |
|------------|---------|-----------|
| `(own x : T)` | Owned value | Caller transfers ownership. x is consumed after use. |
| `(&ref x : T)` | Immutable borrow | Caller retains ownership. Callee reads only. No mutation. |
| `(&mut x : T)` | Mutable borrow | Caller retains ownership. Callee may mutate. Exclusive access. |
| `(copy x : T)` | Explicit copy | Creates independent duplicate. Only for Copy-implementing types. |

The borrowing rules are:

- At any point, a value may have either one mutable borrow OR any number of immutable borrows, but not both.
- All borrows must end before the owner can be moved or dropped.
- The compiler statically verifies these rules. Violation is a compile-time error, never a runtime error.

### 3.5 Dependent Types

Type-level natural numbers (`Nat`) can appear in type parameters, enabling compile-time verification of dimensional constraints:

```clojure
;; M, K, N are Nat parameters inferred from calling context
(defn matrix-multiply
  :sig [(&ref a : tensor[f32 M K])
        (&ref b : tensor[f32 K N])
        -> tensor[f32 M N]]
  ;; The shared dimension K ensures compatibility
  ;; at compile time. No runtime shape-mismatch possible.
  :requires [(> M 0) (> K 0) (> N 0)]
  :ensures  [(= (shape result) [M N])]
  :body (tensor.contract a b :over K))
```

---

## 4. Contract System

### 4.1 Mandatory Contracts

Every function in AIRL MUST include contract blocks. The compiler rejects functions without them. This is the single most important design decision in the language: contracts are structural, not optional. AI code generators skip optional features. They do not skip grammar requirements.

A complete function has four contract components:

| Block | Keyword | Purpose | Enforcement |
|-------|---------|---------|-------------|
| Intent | `:intent` | Natural language description linking English to formal properties | Metadata (not verified) |
| Preconditions | `:requires` | What must be true before execution | Compile-time proof or runtime assertion |
| Postconditions | `:ensures` | What will be true after execution | Compile-time proof or runtime assertion |
| Invariants | `:invariant` | What remains true throughout execution | Continuous verification |

### 4.2 Contract Syntax

```clojure
(defn safe-divide
  :sig [(a : i32) (b : i32) -> Result[i32 DivError]]

  :intent "Divide a by b, returning Err on division by zero"

  :requires
    [(valid a)    ;; a is initialized and not moved
     (valid b)]   ;; b is initialized and not moved

  :ensures
    [(match result
       (Ok v)  (= (* v b) a)        ;; correct quotient
       (Err _) (= b 0))             ;; error iff b is zero
     (pure)                          ;; no side effects
     (terminates)]                   ;; always halts

  :body
    (if (= b 0)
      (Err :division-by-zero)
      (Ok (/ a b))))
```

### 4.3 Verification Levels

AIRL supports three verification levels, selectable per-module:

| Level | Keyword | Behavior | Use Case |
|-------|---------|----------|----------|
| Checked | `:verify checked` | Contracts compiled as runtime assertions. Violation causes immediate panic with structured error. | Development, testing |
| Proven | `:verify proven` | Contracts statically verified by SMT solver (Z3) at compile time. Compilation fails if any contract cannot be proven. | Production, safety-critical |
| Trusted | `:verify trusted` | Contracts are assumed true without checking. Used for axioms and foreign function interfaces. | Interop with unverified systems |

### 4.4 Quantifier Contracts

Contracts support universal and existential quantification for expressing properties over collections:

```clojure
:ensures
  [(forall [i : Nat]
     (where (< i (length result)))
     (>= (at result i) 0))          ;; all elements non-negative

   (exists [j : Nat]
     (where (< j (length result)))
     (= (at result j) (max result)))] ;; at least one element is the max
```

---

## 5. Agent Communication Protocol

### 5.1 Overview

The agent communication protocol is the primary design center of AIRL. In this model, AI agents exchange AIRL expressions as both messages and executable programs. A message from an orchestrator to a sub-agent is not a description of work to be done — it IS the work, expressed as a verifiable, executable program with formal guarantees about expected inputs, outputs, and behavior.

This unification of message and program has profound implications: every inter-agent communication is type-checked, every delegation carries machine-verifiable contracts, and every result can be validated against its specification without trusting the producing agent.

### 5.2 Agent Identity

```clojure
;; Agent identity is a first-class type
(deftype AgentId
  (& (name        : String)        ;; human-readable name
     (capability  : Set[Cap])       ;; declared capabilities
     (trust-level : TrustLevel)     ;; none | verified | proven
     (endpoint    : Endpoint)))     ;; how to reach this agent

;; Capability declarations
(deftype Cap
  (| :compute-gpu
     :compute-cpu
     :web-search
     :code-execution
     :file-access
     :agent-spawn
     (Custom String)))              ;; extensible
```

### 5.3 Task Expression

The core inter-agent communication primitive is the task expression. A task is simultaneously a message, a specification, and an executable program:

```clojure
(task <task-id>
  :from    <agent-id>
  :to      <agent-id>
  :deadline <time-expr>

  :intent  <string>

  :input   [(<name> : <type> <default?>)...]

  :expected-output
    [(<name> : <type>)
     :ensures [<contract>...]]

  :constraints
    [<resource-constraint>...]

  :on-success  <expr>
  :on-failure  <failure-handler>
  :on-timeout  <timeout-handler>)
```

### 5.4 Complete Task Example

```clojure
(task "research-kv-cache-2026-0321"
  :from agent:orchestrator
  :to   agent:research-qwen3
  :deadline (+ (now) (seconds 30))

  :intent "Find recent papers on KV-cache optimization"

  :input
    [(query       : String "KV cache optimization 2025-2026")
     (max-results : Nat 10)
     (min-score   : f32 0.7)]

  :expected-output
    [(papers : List[Paper])
     :ensures
       [(<= (length papers) max-results)
        (forall [p] (in p papers)
          (>= (relevance-score p query) min-score))
        (sorted-by papers :descending relevance-score)]]

  :constraints
    [(max-memory  (megabytes 512))
     (max-tokens  4096)
     (no-network  false)]

  :on-success
    (send agent:orchestrator
      (TaskResult :id "research-kv-cache-2026-0321"
                  :status :complete
                  :payload papers))

  :on-failure
    (retry :max 2 :backoff :exponential
      :fallback
        (escalate agent:orchestrator
          :reason :research-timeout
          :partial-results papers)))
```

### 5.5 Trust and Verification Between Agents

AIRL enforces a trust model where agents declare their trust level and the system verifies accordingly:

| Trust Level | Meaning | Verification Behavior |
|-------------|---------|----------------------|
| `trust:none` | Untrusted agent | All outputs are runtime-checked against contracts. Results treated as potentially adversarial. |
| `trust:verified` | Verified agent | Outputs are spot-checked. Agent has passed prior verification challenges. |
| `trust:proven` | Proven agent | Agent carries compile-time proofs with its outputs. Proofs are machine-checked, not outputs. |

When an orchestrator receives a result from a sub-agent, the AIRL runtime validates the result against the task's `:ensures` contracts. For `trust:none` agents, every contract is checked. For `trust:proven` agents, the accompanying proof object is verified instead (cheaper than re-executing the contracts).

### 5.6 Message Routing

AIRL supports capability-based routing, where an orchestrator specifies required capabilities rather than a specific target agent:

```clojure
;; Route to any agent with GPU compute capability
(task "matmul-batch-47"
  :from agent:orchestrator
  :to   (any-agent :with [:compute-gpu]
                   :prefer :lowest-latency)
  ...)

;; Broadcast to all agents with a capability
(broadcast
  :from agent:orchestrator
  :to   (all-agents :with [:web-search])
  :merge :first-valid    ;; take first result passing contracts
  ...)
```

---

## 6. Execution Model

### 6.1 Dual-Mode Execution

AIRL programs execute in one of two modes, determined by the top-level form:

| Mode | Trigger | Behavior | Use Case |
|------|---------|----------|----------|
| Compiled | `(module ...)` or `(defn ...)` | Full compilation: parse → typecheck → verify contracts → lower to MLIR → LLVM IR → native binary | Performance-critical compute: tensor ops, GPU kernels, data pipelines |
| Interpreted | `(task ...)` or `(send ...)` | Parsed and executed by the agent runtime. Task routing, message passing, and coordination happen without compilation. | Agent orchestration, message routing, workflow coordination |

The runtime automatically selects the appropriate mode. A task expression that contains a compute-heavy `:body` block may trigger JIT compilation of that block while interpreting the surrounding coordination logic.

### 6.2 Compilation Pipeline

```
AIRL Source
    │
    ▼
[Parser]             ;; S-expr → AST (LL(1), zero ambiguity)
    │
    ▼
[Type Checker]       ;; Dependent types, linearity check
    │
    ▼
[Contract Verifier]  ;; Z3 SMT solver for :verify proven
    │                ;; Runtime assertions for :verify checked
    ▼
[MLIR Lowering]      ;; AST → MLIR dialects (tensor, linalg, gpu)
    │
    ▼
[MLIR Optimizer]     ;; Fusion, tiling, vectorization
    │
    ▼
[LLVM Backend]       ;; MLIR → LLVM IR → native code
    │
    ▼
Native Binary / GPU Kernel
```

### 6.3 Memory Management

AIRL uses a linear type system for all memory management. There is no garbage collector. Every allocation has a statically-known lifetime. The rules are:

- **Every value has exactly one owner.** When the owner goes out of scope, the value is deallocated.
- **Move semantics are the default.** Passing a value to a function transfers ownership. The caller can no longer use the value.
- **Borrows provide temporary access.** Immutable borrows (`&ref`) allow read access. Mutable borrows (`&mut`) allow write access. The borrow checker enforces exclusivity.
- **Explicit copy when needed.** Types that implement the Copy trait can be duplicated with `(copy x)`. This is explicit, never implicit.

For agent communication, message payloads are serialized when crossing agent boundaries (since agents may run on different machines). The serialization format is the AIRL S-expression itself — messages are sent as AIRL source text, which the receiving agent parses. This eliminates the need for a separate serialization protocol.

---

## 7. Core Operations

### 7.1 Arithmetic and Logic

```clojure
;; Arithmetic (prefix notation, no precedence rules)
(+ a b)          ;; addition
(- a b)          ;; subtraction
(* a b)          ;; multiplication
(/ a b)          ;; division (returns Result on integer types)
(% a b)          ;; modulus

;; Comparison (returns bool)
(= a b)  (!= a b)  (< a b)  (> a b)  (<= a b)  (>= a b)

;; Logic
(and a b)  (or a b)  (not a)  (xor a b)
```

### 7.2 Control Flow

```clojure
;; Conditional
(if condition then-expr else-expr)

;; Pattern matching (exhaustive, compiler-verified)
(match expr
  (Ok value)   (use value)
  (Err reason) (handle reason))

;; Let binding (introduces scope, linear)
(let (x : i32 42)
     (y : i32 (+ x 1))
  (+ x y))

;; Sequential execution
(do
  (step1)
  (step2)
  (step3))    ;; value of last expression is returned
```

### 7.3 Tensor Operations

```clojure
;; Creation
(tensor.zeros f32 [M N])        ;; zero-filled
(tensor.ones bf16 [B S H])      ;; one-filled
(tensor.rand f32 [M N])         ;; random uniform [0,1)
(tensor.identity f32 N)         ;; NxN identity matrix

;; Arithmetic (element-wise, broadcasting)
(tensor.add a b)
(tensor.mul a b)
(tensor.matmul a b)             ;; matrix multiplication
(tensor.contract a b :over K)   ;; generalized contraction

;; Shape manipulation
(tensor.reshape t [new-shape...])
(tensor.transpose t [perm...])
(tensor.slice t :dim 0 :start 0 :end 64)

;; Reduction
(tensor.sum t :dim 0)
(tensor.max t :dim -1)
(tensor.softmax t :dim -1)
```

### 7.4 Agent Operations

```clojure
;; Send a task to an agent
(send agent:target (task ...))

;; Wait for a result with timeout
(await task-id :timeout (seconds 30)
  :on-result  (fn [r] (process r))
  :on-timeout (fn [] (fallback)))

;; Spawn a new agent
(spawn-agent
  :name "worker-17"
  :capabilities [:compute-gpu :code-execution]
  :model "qwen3-30b"
  :endpoint (local :port 8081))

;; Parallel task fan-out
(parallel
  [(task "sub-1" ...) (task "sub-2" ...) (task "sub-3" ...)]
  :merge (fn [results] (aggregate results))
  :require-all false  ;; succeed if any task succeeds
)
```

---

## 8. Module System

### 8.1 Module Declaration

```clojure
(module my-service
  :version 0.1.0
  :requires [tensor contracts agent]   ;; dependency modules
  :provides [public-fn-1 public-fn-2]  ;; exported symbols
  :verify proven                        ;; verification level
  :execute-on gpu                       ;; default execution target

  ;; Module body: definitions
  (defn public-fn-1 ...)
  (defn public-fn-2 ...)
  (defn private-helper ...))            ;; not in :provides = private
```

### 8.2 Import and Use

```clojure
;; Import specific symbols
(use tensor [matmul transpose reshape])

;; Import with prefix
(use agent :as ag)
;; then: (ag/send ...) (ag/spawn-agent ...)

;; Import everything (discouraged, pollutes namespace)
(use math :all)
```

---

## 9. Standard Library

AIRL includes a standard library of pure AIRL functions, auto-loaded as a prelude before user code. No imports are needed — all stdlib functions are available in every program.

The stdlib is organized into 5 modules, loaded in dependency order: Collections → Math → Result → String → Map.

### 9.1 Primitive Builtins

The stdlib relies on a small set of Rust builtins for list destructuring, string character access, and map operations:

**List primitives:**
```clojure
(head [1 2 3])        ;; → 1 (first element, errors on empty)
(tail [1 2 3])        ;; → [2 3] (all but first, errors on empty)
(empty? [])           ;; → true
(cons 0 [1 2 3])      ;; → [0 1 2 3] (prepend)
```

**String primitives:**
```clojure
(char-at "hello" 0)       ;; → "h" (Unicode-safe)
(substring "hello" 0 3)   ;; → "hel"
(chars "abc")             ;; → ["a" "b" "c"]
(split "a,b,c" ",")       ;; → ["a" "b" "c"]
(join ["a" "b"] "-")      ;; → "a-b"
(contains "hello" "ell")  ;; → true
(starts-with "hello" "hel") ;; → true
(ends-with "hello" "llo")   ;; → true
(index-of "hello" "ll")     ;; → 2 (char index, or -1)
(trim "  hi  ")            ;; → "hi"
(to-upper "hello")         ;; → "HELLO"
(to-lower "HELLO")         ;; → "hello"
(replace "hello" "l" "r")  ;; → "herro"
```

**Map primitives:**
```clojure
(map-new)                          ;; → {} (empty map)
(map-from ["a" 1 "b" 2])          ;; → {a: 1, b: 2}
(map-get m "key")                  ;; → value or nil
(map-get-or m "key" default)       ;; → value or default
(map-set m "key" value)            ;; → new map with key set
(map-has m "key")                  ;; → bool
(map-remove m "key")               ;; → new map without key
(map-keys m)                       ;; → sorted list of keys
(map-values m)                     ;; → values in key-sorted order
(map-size m)                       ;; → number of entries
```

### 9.2 Collections Module

Source: `stdlib/prelude.airl` — 15 functions for list processing.

```clojure
;; Core
(map (fn [x] (* x 2)) [1 2 3])         ;; → [2 4 6]
(filter (fn [x] (> x 2)) [1 2 3 4])    ;; → [3 4]
(fold (fn [acc x] (+ acc x)) 0 [1 2 3]) ;; → 6

;; Structural
(reverse [1 2 3])                ;; → [3 2 1]
(concat [1 2] [3 4])            ;; → [1 2 3 4]
(zip [1 2] [3 4])               ;; → [[1 3] [2 4]]
(flatten [[1 2] [3] [4 5]])     ;; → [1 2 3 4 5]

;; Slicing
(range 1 5)                      ;; → [1 2 3 4]
(take 2 [1 2 3 4])              ;; → [1 2]
(drop 2 [1 2 3 4])              ;; → [3 4]

;; Searching
(any (fn [x] (> x 3)) [1 2 4])  ;; → true
(all (fn [x] (> x 0)) [1 2 3])  ;; → true
(find (fn [x] (> x 3)) [1 2 4]) ;; → 4 (or nil)

;; Sorting (merge sort)
(sort (fn [a b] (< a b)) [3 1 2])  ;; → [1 2 3]
(merge (fn [a b] (< a b)) [1 3] [2 4]) ;; → [1 2 3 4]
```

### 9.3 Math Module

Source: `stdlib/math.airl` — 13 integer math functions.

```clojure
(abs -5)             ;; → 5
(min 3 7)            ;; → 3
(max 3 7)            ;; → 7
(clamp 15 0 10)      ;; → 10
(sign -3)            ;; → -1
(even? 4)            ;; → true
(odd? 3)             ;; → true
(pow 2 10)           ;; → 1024
(gcd 12 8)           ;; → 4
(lcm 4 6)            ;; → 12
(sum-list [1 2 3])   ;; → 6
(product-list [1 2 3]) ;; → 6
```

### 9.4 Result Combinators Module

Source: `stdlib/result.airl` — 8 functions for working with `Result` values.

```clojure
(is-ok? (Ok 42))                ;; → true
(is-err? (Err "fail"))          ;; → true
(unwrap-or (Err "fail") 0)      ;; → 0
(map-ok (fn [x] (* x 2)) (Ok 21))  ;; → (Ok 42)
(map-err (fn [e] (+ e "!")) (Err "oops")) ;; → (Err "oops!")
(and-then (fn [x] (Ok (* x 2))) (Ok 5))   ;; → (Ok 10)
(or-else (fn [e] (Ok 0)) (Err "fail"))    ;; → (Ok 0)
(ok-or 42 "was nil")            ;; → (Ok 42)
(ok-or nil "was nil")           ;; → (Err "was nil")
```

### 9.5 String Module

Source: `stdlib/string.airl` — 10 higher-level string functions (built on string builtins).

```clojure
(words "hello  world")     ;; → ["hello" "world"]
(unwords ["hello" "world"]) ;; → "hello world"
(lines "a\nb\nc")          ;; → ["a" "b" "c"]
(unlines ["a" "b"])        ;; → "a\nb"
(repeat-str "ab" 3)        ;; → "ababab"
(pad-left "42" 5 "0")      ;; → "00042"
(pad-right "hi" 5 ".")     ;; → "hi..."
(is-empty-str "")           ;; → true
(reverse-str "hello")       ;; → "olleh"
(count-occurrences "abcabc" "abc") ;; → 2
```

### 9.6 Map Module

Source: `stdlib/map.airl` — 8 higher-level map functions (built on map builtins).

Maps use string keys and arbitrary values. All mutation operations return new maps.

```clojure
(map-entries m)              ;; → [["k1" v1] ["k2" v2] ...]
(map-from-entries [["a" 1] ["b" 2]]) ;; → {a: 1, b: 2}
(map-merge m1 m2)            ;; → merged map (m2 wins on conflict)
(map-map-values (fn [v] (* v 2)) m)  ;; → map with all values doubled
(map-filter (fn [k v] (> v 10)) m)   ;; → filtered map
(map-update m "key" (fn [v] (+ v 1))) ;; → map with key updated
(map-update-or m "key" 0 (fn [v] (+ v 1))) ;; → update with default
(map-count (fn [k v] (> v 0)) m)     ;; → count of matching entries
```

### 9.7 Implementation Notes

- The stdlib is embedded in the binary via `include_str!()` and parsed/evaluated before user code.
- All collection functions are recursive. A recursion depth limit of 50,000 prevents stack overflow on large lists.
- Map keys are always strings. Maps are backed by `HashMap<String, Value>` for O(1) operations.
- All character indexing in string builtins is Unicode-safe (char-based, not byte-based).
- See `stdlib/*.md` for detailed documentation per module.

---

## 10. Error Handling

### 10.1 Result Types

AIRL uses Result types for expected errors and contract violations for bugs. There are no exceptions.

```clojure
;; Result type is built-in
(deftype Result [T : Type, E : Type]
  (| (Ok T) (Err E)))

;; The try operator propagates errors
(defn process-data
  :sig [(data : String) -> Result[Output ParseError]]
  :requires [(not-empty data)]
  :ensures [(match result
              (Ok o)  (valid o)
              (Err e) (meaningful-error e))]
  :body
    (let (parsed : AST (try (parse data)))  ;; early return on Err
         (validated : AST (try (validate parsed)))
      (Ok (transform validated))))
```

### 10.2 Contract Violations

When a contract violation occurs at runtime (in `:verify checked` mode), the system produces a structured error:

```clojure
(ContractViolation
  :function  "safe-divide"
  :contract  :ensures
  :clause    "(match result (Ok v) (= (* v b) a))"
  :values    {a: 7, b: 2, result: (Ok 3)}
  :expected  "(= (* 3 2) 7) = (= 6 7) = false"
  :trace     [...])
```

---

## 11. Implementation Roadmap

### 11.1 Phase 1: Interpreter (Rust)

Build a tree-walking interpreter in Rust that validates the language design:

- S-expression parser (using nom or pest)
- Type checker with dependent type support
- Linear ownership verification (borrow checker)
- Runtime contract assertion engine
- Agent communication runtime (task dispatch, message routing)
- REPL for testing and language iteration

**Target:** Wire into an existing multi-agent system (e.g., Claude + Qwen3 via LiteLLM) as the inter-agent message format. Validate that agents can generate, parse, and execute AIRL task expressions.

### 11.2 Phase 2: MLIR Compilation

Add a compilation path for performance-critical operations:

- AIRL AST to MLIR lowering (tensor dialect, linalg dialect, gpu dialect)
- Integration with MLIR optimization passes (fusion, tiling, vectorization)
- LLVM backend for native code generation
- GPU kernel compilation targeting CUDA and ROCm
- Z3 SMT solver integration for `:verify proven` mode

### 11.3 Phase 3: Self-Hosting

Write the AIRL compiler in AIRL itself:

- Bootstrap compiler: AIRL source compiled by the Rust implementation
- Self-compiling compiler: AIRL compiler compiles itself
- At this point, an AI system can modify and improve the language toolchain

### 11.4 First Compiler: Rust

The Phase 1 implementation language is Rust, chosen for the following reasons:

- **Type safety:** Rust's type system catches compiler bugs at compile time, which is critical for a language whose entire value proposition is correctness.
- **Parsing ecosystem:** nom (parser combinators), pest (PEG parser), and lalrpop (LR parser generator) are mature, well-documented, and battle-tested.
- **Native performance:** The interpreter and compiler need to be fast. Rust compiles to the same native targets as the AIRL output.
- **LLVM/MLIR interop:** Rust has established bindings for LLVM (inkwell, llvm-sys) and emerging MLIR bindings (melior), making the Phase 2 transition natural.
- **Operational fit:** Rust runs natively in Kubernetes environments and integrates with the same infrastructure stack (vLLM, container runtimes) that AIRL targets.

---

## Appendix A: Complete Grammar (EBNF)

```ebnf
program     ::= top-level*
top-level   ::= module | defn | deftype | task | use-decl

module      ::= '(' 'module' symbol module-attr* top-level* ')'
module-attr ::= ':version' version
             |  ':requires' '[' symbol* ']'
             |  ':provides' '[' symbol* ']'
             |  ':verify' verify-level
             |  ':execute-on' exec-target

defn        ::= '(' 'defn' symbol fn-attr* ')'
fn-attr     ::= ':sig' '[' param* '->' type ']'
             |  ':intent' string
             |  ':requires' '[' contract* ']'
             |  ':ensures' '[' contract* ']'
             |  ':invariant' '[' contract* ']'
             |  ':body' expr
             |  ':execute-on' exec-target
             |  ':priority' priority-level

deftype     ::= '(' 'deftype' symbol type-params? type-body ')'
type-params ::= '[' (symbol ':' type)* ']'
type-body   ::= '(' '|' variant* ')'         ;; sum type
             |  '(' '&' field* ')'            ;; product type
             |  type                           ;; type alias

task        ::= '(' 'task' string task-attr* ')'
task-attr   ::= ':from' expr | ':to' expr | ':deadline' expr
             |  ':intent' string
             |  ':input' '[' param* ']'
             |  ':expected-output' '[' param* contract-block? ']'
             |  ':constraints' '[' constraint* ']'
             |  ':on-success' expr
             |  ':on-failure' expr
             |  ':on-timeout' expr

expr        ::= atom | '(' expr+ ')'
atom        ::= integer | float | string | symbol | keyword
             |  'true' | 'false' | 'nil'

contract    ::= expr    ;; boolean expression
param       ::= '(' symbol ':' type expr? ')'
             |  '(' ownership symbol ':' type ')'
ownership   ::= 'own' | '&ref' | '&mut' | 'copy'

type        ::= symbol
             |  symbol '[' type-arg (',' type-arg)* ']'
             |  '(' '->' '[' type* ']' type ')'
type-arg    ::= type | nat-expr
nat-expr    ::= integer | symbol | '(' op nat-expr+ ')'

verify-level   ::= 'checked' | 'proven' | 'trusted'
exec-target    ::= 'cpu' | 'gpu' | 'any' | agent-ref
agent-ref      ::= 'agent:' symbol
priority-level ::= 'low' | 'normal' | 'high' | 'critical'
version        ::= integer '.' integer '.' integer
```

---

## Appendix B: Comparison with Existing Approaches

| Feature | AIRL | Intent | Mojo | LLVM IR | Protobuf |
|---------|------|--------|------|---------|----------|
| Primary consumer | AI systems | AI + Human audit | Human developers | Compilers | Software systems |
| Contracts | Mandatory, provable | Mandatory, runtime | None (uses Rust-style) | None | Schema validation |
| Agent communication | First-class | None | None | None | Data only |
| Compilation target | MLIR → LLVM | Rust (via Go) | MLIR → LLVM | Native | N/A |
| Type system | Dependent + linear | Simple + contracts | Gradual + ownership | SSA typed | Schema types |
| Syntax | S-expressions | C-like | Pythonic | Assembly-like | IDL |
| Token efficiency | High (dense syntax) | Medium | Medium | Low (verbose) | N/A |
| Memory model | Linear ownership | Rust-like | Ownership + GC | Manual | N/A |
| GPU support | Via MLIR | None | Native MLIR | Via backends | N/A |
| Formal verification | Z3 integration | Planned | None | None | None |

---

*— End of Specification —*
