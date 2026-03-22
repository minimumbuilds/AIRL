# IR VM Design

**Date:** 2026-03-22
**Status:** Draft
**Scope:** Tree-flattened IR + Rust VM + self-hosted AIRL compiler, targeting the bootstrap subset

## Overview

A two-part compilation system for AIRL: a Rust VM that executes a simplified IR, and a self-hosted AIRL compiler that transforms AST nodes into that IR. The goal is 10-30x speedup over the current tree-walking interpreter, making further self-hosted development practical.

The IR is a tree of variant nodes (not flat bytecode) — the AIRL compiler emits `(IRInt 42)`, `(IRIf cond then else)`, etc. The Rust VM tree-walks this IR, which is dramatically faster than the current interpreter because the IR is pre-resolved: no name lookups at dispatch time, no contract checking, no AST span/type metadata overhead.

## Goals

1. **Speed up the bootstrap compiler** — Lexer self-parse from ~56s to ~2-5s in release mode
2. **Self-hosted compiler in AIRL** — The AST-to-IR compiler is written in AIRL, extending the self-hosting story
3. **Incremental path to native codegen** — The IR is a stable intermediate format. A future pass can lower it to flat bytecode or native code without changing the AIRL compiler
4. **Validate by comparison** — The existing tree-walking eval.airl remains as the reference implementation for correctness checking

## Non-Goals (v1)

- Contracts (requires/ensures/invariant checking)
- Tensors and tensor JIT
- Agent system (spawn-agent, send, etc.)
- Ownership/linearity annotations
- Quantifiers (forall/exists)
- Flat bytecode encoding (future optimization)
- Register allocation or native code emission

## Architecture

### Pipeline Position

```
source → lex → parse → [type check] → compile → run-ir
                                         ↑           ↑
                                    AIRL compiler  Rust VM
                                  (compiler.airl)  (ir_vm.rs)
```

The compile step replaces the tree-walking eval step. Type checking remains optional (same as today).

### Components

| Component | Language | Location | Lines (est.) |
|-----------|----------|----------|-------------|
| IR node enum | Rust | `crates/airl-runtime/src/ir.rs` | ~80 |
| Rust VM | Rust | `crates/airl-runtime/src/ir_vm.rs` | ~500 |
| Value-to-IR marshalling | Rust | `crates/airl-runtime/src/ir_marshal.rs` | ~150 |
| `run-ir` builtin | Rust | `crates/airl-runtime/src/builtins.rs` | ~30 |
| Self-hosted compiler | AIRL | `bootstrap/compiler.airl` | ~400 |
| Compiler tests | AIRL | `bootstrap/compiler_test.airl` | ~300 |

## IR Node Format

The IR is a set of tagged variants, simpler than the AST. Key simplifications: no source positions, no type annotations, no contract clauses.

### Literals and Variables

```
(IRInt value)           ;; integer literal
(IRFloat value)         ;; float literal
(IRStr value)           ;; string literal
(IRBool value)          ;; boolean literal
(IRNil)                 ;; nil/unit

(IRLoad name)           ;; load variable by name from environment
```

### Control Flow

```
(IRIf cond then else)   ;; conditional — both branches required
(IRDo exprs)            ;; sequence of expressions, return last
(IRLet bindings body)   ;; bindings = [(IRBinding name expr) ...], scoped
```

### Functions

```
(IRFunc name params body)          ;; named function definition
(IRLambda params body)             ;; closure (captures resolved at runtime by VM)
(IRCall name args)                 ;; call named function or builtin
(IRCallExpr callee-expr args)      ;; call computed callee (lambda, higher-order)
```

### Data

```
(IRList items)                     ;; list literal [a b c]
(IRVariant tag args)               ;; variant constructor, e.g., (IRVariant "Ok" [expr])
```

### Pattern Matching

```
(IRMatch scrutinee arms)           ;; arms = [(IRArm pattern body) ...]
```

Patterns use IR-level pattern nodes that mirror but simplify the AST patterns:

```
(IRPatWild)                        ;; match anything, no binding
(IRPatBind name)                   ;; match anything, bind to name
(IRPatLit value)                   ;; match literal value
(IRPatVariant tag sub-patterns)    ;; match variant constructor
```

### Error Handling

```
(IRTry expr)                       ;; unwrap Ok, propagate Err as runtime error
```

### Bindings

```
(IRBinding name expr)              ;; used inside IRLet
```

### Summary

~18 node types vs ~20+ AST node types. The critical simplification is that `ASTDefn` with its 8 fields (name, sig, intent, requires, ensures, body, line, col) becomes `IRFunc` with 3 fields (name, params, body). Patterns are simplified to 4 types without source positions.

### Rust Enum Definition

```rust
/// IR nodes — the simplified instruction set for the VM.
pub enum IRNode {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,

    // Variables
    Load(String),

    // Control flow
    If(Box<IRNode>, Box<IRNode>, Box<IRNode>),
    Do(Vec<IRNode>),
    Let(Vec<IRBinding>, Box<IRNode>),

    // Functions
    Func(String, Vec<String>, Box<IRNode>),       // name, params, body
    Lambda(Vec<String>, Box<IRNode>),              // params, body
    Call(String, Vec<IRNode>),                     // named call
    CallExpr(Box<IRNode>, Vec<IRNode>),            // computed callee

    // Data
    List(Vec<IRNode>),
    Variant(String, Vec<IRNode>),                  // tag, args

    // Pattern matching
    Match(Box<IRNode>, Vec<IRArm>),

    // Error handling
    Try(Box<IRNode>),
}

pub struct IRBinding {
    pub name: String,
    pub expr: IRNode,
}

pub struct IRArm {
    pub pattern: IRPattern,
    pub body: IRNode,
}

pub enum IRPattern {
    Wild,
    Bind(String),
    Lit(Value),
    Variant(String, Vec<IRPattern>),
}
```

## Rust VM

### Location

New module in `crates/airl-runtime/src/ir_vm.rs`. No new crate needed — the VM reuses the existing `Value` enum, `Builtins` registry, and error types.

### State

```rust
pub struct IrVm {
    env: Vec<HashMap<String, Value>>,    // frame stack (innermost first)
    functions: HashMap<String, IRFunc>,   // compiled function table
    builtins: Builtins,                   // reuses existing builtin registry
    recursion_depth: usize,               // stack overflow guard
}
```

### Core Execution

```rust
fn exec(&mut self, node: &IRNode) -> Result<Value, RuntimeError> {
    match node {
        IRNode::Int(v)    => Ok(Value::Int(*v)),
        IRNode::Float(v)  => Ok(Value::Float(*v)),
        IRNode::Str(s)    => Ok(Value::Str(s.clone())),
        IRNode::Bool(b)   => Ok(Value::Bool(*b)),
        IRNode::Nil       => Ok(Value::Nil),

        IRNode::Load(name) => self.env_lookup(name),

        IRNode::If(cond, then_, else_) => {
            match self.exec(cond)? {
                Value::Bool(true) => self.exec(then_),
                Value::Bool(false) => self.exec(else_),
                _ => Err(RuntimeError::TypeError("if: condition not bool")),
            }
        }

        IRNode::Call(name, args) => {
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| self.exec(a))
                .collect::<Result<_, _>>()?;
            self.call_function(name, arg_vals)
        }

        IRNode::Let(bindings, body) => {
            self.push_frame();
            for binding in bindings {
                let val = self.exec(&binding.expr)?;
                self.env_bind(&binding.name, val);
            }
            let result = self.exec(body);
            self.pop_frame();
            result
        }

        // ... Match, Lambda, List, Variant, Do, etc.
    }
}
```

### Why This Is Faster

Compared to the current `eval_inner()` in `eval.rs`:

1. **No span metadata** — IR nodes carry no `Span`. The current AST carries source positions on every node, which bloats memory and cache.
2. **No contract dispatch** — The current interpreter evaluates `:requires` and `:ensures` on every `call_fn`. The IR has no contracts.
3. **No AST pattern matching overhead** — `ExprKind` has 20+ variants with nested structures. `IRNode` has 15 flat variants.
4. **Reduced builtin dispatch overhead** — The current interpreter does multi-level pattern matching in the `FnCall` arm (special builtins → JIT → generic builtins → user functions). The VM does a single hashmap lookup.
5. **Simpler TCO** — The current interpreter maintains `current_fn_name`, `in_tail_context`, and a full trampoline with `EvalResult::Continue`. The IR VM uses a simpler loop-based self-TCO in `call_function`.
6. **Smaller nodes** — `IRNode::Int(42)` is 16 bytes. `Expr { kind: ExprKind::IntLit(42), span: Span { start, end, line, col } }` is 48+ bytes.

### Closure and Function Values

The existing `Value` enum cannot hold IR closures (it stores AST `Expr` bodies). Two new value variants are needed:

```rust
// In value.rs or ir.rs
pub struct IRClosureValue {
    pub params: Vec<String>,
    pub body: Box<IRNode>,
    pub captured_env: Vec<(String, Value)>,
}

pub struct IRFuncRef {
    pub name: String,
}
```

These are stored as `Value::Variant("__ir_closure__", ...)` or similar, or more cleanly as new variants on the `Value` enum:

```rust
// Option A: extend Value enum (preferred)
pub enum Value {
    // ... existing variants ...
    IRClosure(IRClosureValue),
    IRFuncRef(String),  // reference to a named function in the VM's table
}
```

When the VM executes `IRNode::Lambda`, it captures the current environment and creates `Value::IRClosure(...)`. When executing `IRNode::Func`, it registers the function in the VM's function table and optionally binds the name to `Value::IRFuncRef(name)` for passing functions as values.

### Function Calls

```rust
fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    // Try builtin first
    if let Some(result) = self.builtins.call(name, &args) {
        return result;
    }
    // Then user-defined function
    if let Some(func) = self.functions.get(name).cloned() {
        self.push_frame();
        for (param, val) in func.params.iter().zip(args) {
            self.env_bind(param, val);
        }
        // Bind function name for recursion
        self.env_bind(name, Value::IRFuncRef(name.to_string()));
        let result = self.exec(&func.body);
        self.pop_frame();
        return result;
    }
    Err(RuntimeError::UndefinedSymbol(name.to_string()))
}
```

### Computed Callee Calls (IRCallExpr)

When the callee is a computed expression (lambda, higher-order function):

```rust
IRNode::CallExpr(callee_expr, args) => {
    let callee = self.exec(callee_expr)?;
    let arg_vals: Vec<Value> = args.iter()
        .map(|a| self.exec(a))
        .collect::<Result<_, _>>()?;
    match callee {
        Value::IRClosure(closure) => {
            self.push_frame();
            // Restore captured environment
            for (name, val) in &closure.captured_env {
                self.env_bind(name, val.clone());
            }
            // Bind parameters
            for (param, val) in closure.params.iter().zip(arg_vals) {
                self.env_bind(param, val);
            }
            let result = self.exec(&closure.body);
            self.pop_frame();
            result
        }
        Value::IRFuncRef(name) => self.call_function(&name, arg_vals),
        Value::BuiltinFn(name) => self.builtins.call(&name, &arg_vals)
            .unwrap_or(Err(RuntimeError::UndefinedSymbol(name))),
        _ => Err(RuntimeError::TypeError("calling a non-function value".into())),
    }
}
```

### Tail Call Optimization

Basic self-TCO is included in v1 because the bootstrap lexer's `lex-loop` makes ~15,000+ recursive calls per parse. Without TCO, the Rust stack grows proportionally. The mechanism is simple: in `call_function`, detect when the body's outermost expression is a self-recursive `IRCall` and loop instead of recursing:

```rust
fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    if let Some(func) = self.functions.get(name).cloned() {
        let mut current_args = args;
        loop {
            self.push_frame();
            for (param, val) in func.params.iter().zip(&current_args) {
                self.env_bind(param, val.clone());
            }
            self.env_bind(name, Value::IRFuncRef(name.to_string()));
            match self.exec_tco(&func.body, name)? {
                TcoResult::Value(v) => { self.pop_frame(); return Ok(v); }
                TcoResult::TailCall(new_args) => { self.pop_frame(); current_args = new_args; }
            }
        }
    }
    // ... builtin fallback
}
```

`exec_tco` is a variant of `exec` that, in tail position (last expression of if/do/let/match), returns `TcoResult::TailCall` instead of recursing when it encounters `IRCall(name, args)` matching the current function name. Same pattern as the current trampoline in `eval.rs`.

### Stdlib Availability

The stdlib is pure AIRL (46 functions in `prelude.airl`, `math.airl`, `result.airl`, `string.airl`, `map.airl`). These are loaded via `include_str!` and evaluated before user code. For the IR VM, the same approach works:

1. The Rust driver compiles stdlib source through the AIRL compiler (lex → parse → compile-program)
2. The resulting IR nodes are executed on the VM, which populates the function table with stdlib functions
3. Then user code is compiled and executed on the same VM instance

This means `run-ir` should NOT create a fresh VM. Instead, the builtin receives a reference to a persistent `IrVm` that has already been initialized with stdlib. The pipeline is:

```rust
// In pipeline.rs
let mut vm = IrVm::new_with_builtins();
vm.exec_program(&compiled_stdlib_ir)?;  // load stdlib into function table
vm.exec_program(&compiled_user_ir)?;     // run user code
```

The AIRL-side `run-ir` builtin shares the VM instance across calls within the same program execution.

### Environment

Same scoping model as the bootstrap eval: list of frames, innermost first. Lookup walks outward.

```rust
fn env_lookup(&self, name: &str) -> Result<Value, RuntimeError> {
    for frame in &self.env {
        if let Some(val) = frame.get(name) {
            return Ok(val.clone());
        }
    }
    Err(RuntimeError::UndefinedSymbol(name.to_string()))
}
```

## Value-to-IR Marshalling

The self-hosted AIRL compiler produces IR nodes as AIRL `Value` variants — `Value::Variant("IRInt", Box<Value::Int(42)>)`. The Rust VM needs to convert these to its native `IRNode` enum before execution.

### Conversion

```rust
fn value_to_ir(val: &Value) -> Result<IRNode, RuntimeError> {
    match val {
        Value::Variant(tag, inner) => match tag.as_str() {
            "IRInt" => Ok(IRNode::Int(inner.as_int()?)),
            "IRFloat" => Ok(IRNode::Float(inner.as_float()?)),
            "IRStr" => Ok(IRNode::Str(inner.as_str()?.to_string())),
            "IRBool" => Ok(IRNode::Bool(inner.as_bool()?)),
            "IRNil" => Ok(IRNode::Nil),
            "IRLoad" => Ok(IRNode::Load(inner.as_str()?.to_string())),
            "IRIf" => {
                let items = inner.as_list()?;
                Ok(IRNode::If(
                    Box::new(value_to_ir(&items[0])?),
                    Box::new(value_to_ir(&items[1])?),
                    Box::new(value_to_ir(&items[2])?),
                ))
            }
            "IRCall" => {
                let items = inner.as_list()?;
                let name = items[0].as_str()?.to_string();
                let args = items[1].as_list()?
                    .iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::Call(name, args))
            }
            // ... one arm per IR node type
            _ => Err(RuntimeError::TypeError(format!("unknown IR node: {}", tag))),
        }
        _ => Err(RuntimeError::TypeError("expected IR node variant".into())),
    }
}
```

### Pattern Marshalling

Patterns are marshalled from AIRL variants to Rust IR pattern structs:

```rust
fn value_to_pattern(val: &Value) -> Result<IRPattern, RuntimeError> {
    match val {
        Value::Variant(tag, inner) => match tag.as_str() {
            "IRPatWild" => Ok(IRPattern::Wild),
            "IRPatBind" => Ok(IRPattern::Bind(inner.as_str()?.to_string())),
            "IRPatLit" => Ok(IRPattern::Lit(inner.clone())),
            "IRPatVariant" => {
                let items = inner.as_list()?;
                let tag = items[0].as_str()?.to_string();
                let sub_pats = items[1].as_list()?.iter()
                    .map(value_to_pattern).collect::<Result<_, _>>()?;
                Ok(IRPattern::Variant(tag, sub_pats))
            }
            _ => Err(RuntimeError::TypeError(format!("unknown pattern: {}", tag))),
        }
        _ => Err(RuntimeError::TypeError("expected pattern variant".into())),
    }
}
```

### Integration Point

A new builtin `run-ir` registered in the builtins table:

```rust
fn builtin_run_ir(args: &[Value]) -> Result<Value, RuntimeError> {
    let ir_nodes = args[0].as_list()?;
    let compiled: Vec<IRNode> = ir_nodes.iter()
        .map(value_to_ir)
        .collect::<Result<_, _>>()?;
    let mut vm = IrVm::new();
    vm.exec_program(&compiled)
}
```

The AIRL side calls `(run-ir (compile-program ast-nodes))`.

## Self-Hosted Compiler (bootstrap/compiler.airl)

### Structure

The compiler mirrors `eval.airl` structurally but produces IR nodes instead of executing:

| eval.airl function | compiler.airl equivalent | Output |
|-------------------|-------------------------|--------|
| `eval-node` | `compile-expr` | IR expression node |
| `eval-top-level` | `compile-top-level` | IR top-level node |
| `eval-program` | `compile-program` | list of IR nodes |
| `call-builtin` | (not needed) | builtins stay as `IRCall` |
| `eval-match-arms` | `compile-match-arms` | `[(IRArm pat ir-body) ...]` |

### Core Function

```clojure
(defn compile-expr
  :sig [(node : List) -> List]
  :intent "Compile an AST expression to an IR node"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTInt v _ _)       (Ok (IRInt v))
    (ASTFloat v _ _)     (Ok (IRFloat v))
    (ASTStr s _ _)       (Ok (IRStr s))
    (ASTBool b _ _)      (Ok (IRBool b))
    (ASTNil _ _)         (Ok (IRNil))
    (ASTKeyword k _ _)   (Ok (IRStr (join [":" k] "")))
    (ASTSymbol name _ _) (Ok (IRLoad name))

    (ASTIf cond then else _ _)
      (match (compile-expr cond)
        (Err e) (Err e)
        (Ok cc) (match (compile-expr then)
          (Err e) (Err e)
          (Ok ct) (match (compile-expr else)
            (Err e) (Err e)
            (Ok ce) (Ok (IRIf cc ct ce)))))

    (ASTCall callee args _ _)
      (match callee
        (ASTSymbol name _ _)
          (match (compile-args args)
            (Err e) (Err e)
            (Ok cargs) (Ok (IRCall name cargs)))
        _ (match (compile-expr callee)
            (Err e) (Err e)
            (Ok cc) (match (compile-args args)
              (Err e) (Err e)
              (Ok cargs) (Ok (IRCallExpr cc cargs)))))

    (ASTLet bindings body _ _)
      (match (compile-let-bindings bindings)
        (Err e) (Err e)
        (Ok cbindings) (match (compile-expr body)
          (Err e) (Err e)
          (Ok cbody) (Ok (IRLet cbindings cbody))))

    (ASTDo exprs _ _)
      (match (compile-args exprs)
        (Err e) (Err e)
        (Ok cexprs) (Ok (IRDo cexprs)))

    (ASTMatch scrutinee arms _ _)
      (match (compile-expr scrutinee)
        (Err e) (Err e)
        (Ok cscr) (match (compile-match-arms arms)
          (Err e) (Err e)
          (Ok carms) (Ok (IRMatch cscr carms))))

    (ASTLambda params body _ _)
      (match (compile-expr body)
        (Err e) (Err e)
        (Ok cbody) (Ok (IRLambda params cbody)))

    (ASTVariant name args _ _)
      (match (compile-args args)
        (Err e) (Err e)
        (Ok cargs) (Ok (IRVariant name cargs)))

    (ASTList items _ _)
      (match (compile-args items)
        (Err e) (Err e)
        (Ok citems) (Ok (IRList citems)))

    (ASTTry expr _ _)
      (match (compile-expr expr)
        (Err e) (Err e)
        (Ok cexpr) (Ok (IRTry cexpr)))

    _ (Err "compile-expr: unknown node type")))
```

### Helper: compile-let-bindings

```clojure
(defn compile-let-bindings
  :sig [(bindings : List) -> List]
  :body
    (if (empty? bindings) (Ok [])
      (match (head bindings)
        (ASTBinding name _ val-expr)
          (match (compile-expr val-expr)
            (Err e) (Err e)
            (Ok cval) (match (compile-let-bindings (tail bindings))
              (Err e) (Err e)
              (Ok rest) (Ok (cons (IRBinding name cval) rest))))
        _ (Err "invalid let binding"))))
```

### Compile vs Eval Comparison

The compiler is simpler than the evaluator because it doesn't need to:
- Maintain an environment (just emit `IRLoad`/`IRFunc`)
- Dispatch builtins (just emit `IRCall`)
- Handle errors at compile time (runtime errors happen in the VM)
- Track recursion depth, tail context, or function names

### Top-Level and Program

```clojure
(defn compile-top-level
  :sig [(node : List) -> List]
  :body (match node
    (ASTDefn name sig _ _ _ body _ _)
      (match sig
        (ASTSig params ret-name)
          (let (param-names : List (map (fn [p] (match p (ASTParam n _) n _ "?")) params))
            (match (compile-expr body)
              (Err e) (Err e)
              (Ok cbody) (Ok (IRFunc name param-names cbody))))
        _ (Err "defn requires sig"))
    (ASTDefType _ _ _ _ _) (Ok (IRNil))  ;; type decls are compile-time only; variant
                                        ;; constructors (Ok, Err, etc.) are built into the
                                        ;; runtime — they don't need explicit registration
    _ (compile-expr node)))

(defn compile-program
  :sig [(nodes : List) -> List]
  ;; returns (Ok ir-node-list) or (Err msg)
```

### Pipeline Entry

```clojure
(defn run-compiled
  :sig [(source : String) -> List]
  :body
    (match (lex source)
      (Err e) (Err e)
      (Ok tokens)
        (match (parse-sexpr-all tokens)
          (Err e) (Err e)
          (Ok sexprs)
            (match (parse-program sexprs)
              (Err e) (Err e)
              (Ok ast-nodes)
                (match (compile-program ast-nodes)
                  (Err e) (Err e)
                  (Ok ir-nodes) (run-ir ir-nodes))))))
```

## Testing Strategy

### Phase 1: Rust VM tests

Rust `#[cfg(test)]` in `ir_vm.rs`:
- Construct `IRNode` values directly in Rust
- Test each node type: literals, if, let, do, call, match, lambda, variant, list
- Test function definition and recursive calls
- Test builtin dispatch (arithmetic, comparison, list ops, string ops, map ops)

### Phase 2: Compiler tests

`bootstrap/compiler_test.airl` — self-contained, includes lexer + parser + compiler:
- For each test case, parse a source string, compile it, run-ir it, and compare against expected value
- Also run the same source through eval.airl and verify identical results
- Progressive: literals → arithmetic → if → let → functions → recursion → match → lambdas

### Phase 3: Integration validation

The key correctness test: compile `lexer.airl` through the new pipeline and run the lexer self-parse test. Compare token output against the tree-walking interpreter's output. They must be identical.

### Phase 4: Benchmark

Time the lexer self-parse through both paths:
```bash
# Tree-walking (current)
time cargo run --release -- run bootstrap/lexer_selfparse.airl

# Compiled
time cargo run --release -- run bootstrap/lexer_selfparse_compiled.airl
```

Target: 10-30x speedup (56s → 2-5s).

## Incremental Phases

### Phase 1: Rust VM + IR format (~800 lines Rust)

Files:
- Create: `crates/airl-runtime/src/ir.rs` — `IRNode` enum
- Create: `crates/airl-runtime/src/ir_vm.rs` — `IrVm` implementation
- Create: `crates/airl-runtime/src/ir_marshal.rs` — `value_to_ir` conversion
- Modify: `crates/airl-runtime/src/builtins.rs` — register `run-ir`
- Modify: `crates/airl-runtime/src/lib.rs` — add modules

Success: Rust tests pass for all IR node types.

### Phase 2: Self-hosted compiler (~400 lines AIRL)

Files:
- Create: `bootstrap/compiler.airl` — `compile-expr`, `compile-program`
- Create: `bootstrap/compiler_test.airl` — comparison tests

Success: All compiler tests pass, results match eval.airl.

### Phase 3: Full pipeline integration (~100 lines AIRL)

Files:
- Modify: `bootstrap/compiler.airl` — add `run-compiled`
- Create: `bootstrap/compiler_integration_test.airl` — lexer self-parse via compile path

Success: Lexer self-parse produces identical tokens through both paths. Benchmark shows 10x+ speedup.

### Phase 4: Wire as default execution path (~50 lines Rust)

Files:
- Modify: `crates/airl-driver/src/pipeline.rs` — add compiled execution mode
- Modify: `CLAUDE.md` — document performance numbers

Success: `cargo run -- run file.airl` uses compiled path by default. All existing tests pass.

## Future Extensions

Once the IR VM is working:

1. **Flatten IR to linear bytecode** — Add a pass that converts the IR tree to a flat `Vec<Instruction>` with explicit jumps. Another 2-5x speedup from eliminating tree traversal.
2. **Contract compilation** — Compile `:requires`/`:ensures` as `IRIf` checks around the body.
3. **Mutual TCO** — v1 handles self-recursive TCO. Extend to mutual recursion (A calls B in tail position, B calls A).
4. **Self-hosted VM** — Rewrite the VM's `exec` loop in AIRL targeting the bytecoded version of itself. This is the ultimate self-hosting milestone.
