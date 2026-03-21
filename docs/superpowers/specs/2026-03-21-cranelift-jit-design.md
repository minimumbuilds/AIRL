# AIRL Phase 2a — JIT Compilation via Cranelift

**Date:** 2026-03-21
**Status:** Approved
**Depends on:** Phase 1 + Hardening + Agent Wiring (376 tests)

---

## Overview

Add a JIT compilation backend using Cranelift. Functions with all-primitive signatures are transparently compiled to native code on first call, cached, and called directly on subsequent invocations. Contracts are still checked by the interpreter. Functions with complex types or unsupported expressions silently fall back to interpretation.

This is the first external dependency in the project: `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`.

---

## 1. New Crate: `airl-codegen`

Position in dependency chain:
```
airl-syntax → airl-types → airl-contracts → airl-runtime → airl-codegen → airl-driver
```

Dependencies:
- `airl-syntax` — AST types (Expr, ExprKind, FnDef, AstType, etc.)
- `airl-types` — PrimTy for type classification
- `cranelift-codegen` — IR builder, register allocator, code emitter
- `cranelift-frontend` — FunctionBuilder API for SSA construction
- `cranelift-jit` — JIT module, memory management
- `cranelift-module` — Module trait for symbol resolution

All other crates remain dependency-free.

---

## 2. Compilation Eligibility

A function is eligible for JIT compilation if:

1. **All parameter types are primitive** — i32, i64, f32, f64, bool
2. **Return type is primitive** — same set
3. **The function body can be lowered** — contains only supported expression forms

If any condition fails, the function is marked "uncompilable" and stays interpreted forever (silent fallback, not an error).

---

## 3. Type Mapping

| AIRL Type | Cranelift Type | Notes |
|---|---|---|
| `i32` | `types::I32` | |
| `i64` | `types::I64` | Default for bare integer literals |
| `f32` | `types::F32` | |
| `f64` | `types::F64` | Default for bare float literals |
| `bool` | `types::I8` | 0 or 1 |

---

## 4. Expression Lowering

### Supported (compiles to native)

| AIRL Expression | Cranelift IR |
|---|---|
| `IntLit(v)` | `iconst(I64, v)` |
| `FloatLit(v)` | `f64const(v)` |
| `BoolLit(v)` | `iconst(I8, v as i64)` |
| `SymbolRef(name)` | Look up in local variable map → SSA value |
| `(+ a b)` int | `iadd(a, b)` |
| `(+ a b)` float | `fadd(a, b)` |
| `(- a b)` | `isub` / `fsub` |
| `(* a b)` | `imul` / `fmul` |
| `(/ a b)` | `sdiv` / `fdiv` |
| `(% a b)` | `srem` |
| `(= a b)` | `icmp(Equal)` / `fcmp(Equal)` → `I8` |
| `(< a b)` | `icmp(SignedLessThan)` / `fcmp(LessThan)` |
| `(> a b)` | `icmp(SignedGreaterThan)` / `fcmp(GreaterThan)` |
| `(<= a b)` | `icmp(SignedLessEqual)` / `fcmp(LessEqual)` |
| `(>= a b)` | `icmp(SignedGreaterEqual)` / `fcmp(GreaterEqual)` |
| `(!= a b)` | `icmp(NotEqual)` / `fcmp(NotEqual)` |
| `(and a b)` | `band(a, b)` |
| `(or a b)` | `bor(a, b)` |
| `(not a)` | `bxor(a, iconst(1))` |
| `(if c t e)` | Branch to then/else blocks, merge with block parameter |
| `(let bindings body)` | Evaluate bindings, store in variable map, evaluate body |
| `(do e1 ... en)` | Evaluate sequentially, return last value |

### Unsupported (triggers fallback to interpreter)

- `Match` — pattern matching is complex to lower
- `Lambda` — requires heap-allocated closures
- `Try` — requires Result type handling
- `FnCall` to non-builtin — would need interpreter callback
- `VariantCtor`, `StructLit`, `ListLit` — non-primitive types
- Any builtin not in the arithmetic/comparison/logic set
- `KeywordLit` — not a primitive value

When the lowerer encounters any unsupported form, it returns `Err(UnsupportedExpression)` and the function is marked uncompilable.

---

## 5. JIT Cache

```rust
pub struct JitCache {
    module: JITModule,
    compiled: HashMap<String, CompiledFn>,
    uncompilable: HashSet<String>,
}
```

`CompiledFn` wraps a native function pointer with type information for marshaling:

```rust
struct CompiledFn {
    ptr: *const u8,              // native code pointer
    param_types: Vec<PrimTy>,    // for Value → raw conversion
    return_type: PrimTy,         // for raw → Value conversion
}
```

### try_call flow

```rust
pub fn try_call(&mut self, def: &FnDef, args: &[Value]) -> Result<Option<Value>, RuntimeError> {
    // 1. Check uncompilable set → return Ok(None)
    // 2. Check compiled cache → call if present
    // 3. Try to compile → on failure, mark uncompilable, return Ok(None)
    // 4. Call compiled function
    // 5. Marshal result back to Value
}
```

### Value Marshaling

Before calling native code:
- `Value::Int(v)` → `v as i64`
- `Value::Float(v)` → `v as f64`
- `Value::Bool(v)` → `v as u8` (0 or 1)

After native code returns:
- `i64` → `Value::Int(v)`
- `f64` → `Value::Float(v)`
- `u8` → `Value::Bool(v != 0)`

For functions with i32/f32 params, widen to i64/f64 at the call boundary (Cranelift internally uses the correct width).

---

## 6. Integration with Interpreter

### Changes to `airl-runtime/eval.rs`

```rust
pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    pub jit: Option<airl_codegen::JitCache>,  // NEW
}
```

In `Interpreter::new()`, create JitCache:
```rust
jit: Some(airl_codegen::JitCache::new()),
```

In `call_fn`, before evaluating the body:
```rust
// After :requires check, before body evaluation:
if let Some(ref mut jit) = self.jit {
    if let Some(result) = jit.try_call(&fn_val.def, &args)? {
        // Bind result for :ensures
        self.env.bind("result".to_string(), result.clone());
        // Check :ensures contracts
        // ...
        self.env.pop_frame();
        return Ok(result);
    }
}
// Fall through to interpreted evaluation
```

### Dependency

`airl-runtime` gains a dependency on `airl-codegen`. This is a new link in the chain but follows the existing linear pattern.

Actually — this creates a problem. `airl-codegen` depends on `airl-syntax` and `airl-types`. `airl-runtime` also depends on those. Adding `airl-runtime → airl-codegen` is fine (no cycle). But we should consider whether `airl-codegen` should depend on `airl-runtime` (for `Value` marshaling). It shouldn't — keep the marshaling at the `airl-runtime` level. `airl-codegen` returns raw bytes/values, and `airl-runtime` wraps them in `Value`.

Revised dependency:
```
airl-codegen depends on: airl-syntax, airl-types
airl-runtime depends on: airl-syntax, airl-types, airl-contracts, airl-codegen
```

---

## 7. `if` Expression Lowering (Detail)

The `if` expression is the most complex lowering. Cranelift uses explicit basic blocks:

```
entry_block:
    cond = <evaluate condition>
    brif cond, then_block, else_block

then_block:
    then_val = <evaluate then branch>
    jump merge_block(then_val)

else_block:
    else_val = <evaluate else branch>
    jump merge_block(else_val)

merge_block(result):
    return result
```

The merge block uses a block parameter to receive the value from whichever branch was taken. This is standard SSA phi-node encoding.

---

## 8. Testing

### Unit tests (airl-codegen)

- Lowering individual expressions to IR (verify instruction types)
- Compile + execute simple functions:
  - `(+ 1 2)` → 3
  - `(* 6 7)` → 42
  - `(if (> 5 3) 10 20)` → 10
  - `(let (x : i64 5) (y : i64 3) (+ x y))` → 8
  - `(do 1 2 (+ 3 4))` → 7
- Type dispatch: int ops produce int results, float ops produce float
- Fallback: functions with unsupported expressions return None

### Integration tests

- Same function produces same result via JIT and interpreter
- Contracts checked around JIT calls
- Mixed program: some functions JIT-compiled, others interpreted

### Fixture

`tests/fixtures/valid/jit_arithmetic.airl`:
```clojure
;; EXPECT: 37
(defn compute
  :sig [(x : i64) -> i64]
  :intent "polynomial"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body (+ (* x x) (* 3 x) 7))
(compute 5)
```

---

## 9. Not In Scope

- Tensor operation compilation (future — requires loop generation)
- GPU targeting (future — requires MLIR)
- Recursive function JIT (compiled function calling itself)
- Ahead-of-time compilation / `airl compile` command
- `:execute-on` annotation handling
- MLIR integration (future Phase 2b, behind `--features mlir`)
- Z3 / SMT solver integration
