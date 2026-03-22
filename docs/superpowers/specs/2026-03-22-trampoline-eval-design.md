# Trampoline Eval Design

**Date:** 2026-03-22
**Status:** Approved
**Goal:** Eliminate stack overflow in AIRL's tree-walking interpreter by converting tail-position evaluation to a trampoline loop, and adding self-recursive tail call optimization.

## Problem

The recursive `eval()` function in `eval.rs` (1,798 lines) accumulates Rust stack frames for every nested expression. Each function call adds ~8-10 Rust frames. Deeply nested AIRL programs — especially the bootstrap self-hosted parser evaluating deeply nested source — overflow the default 8MB Rust thread stack.

**Specific failure:** The self-hosted parser cannot parse `lexer.airl` (360 lines, 10-deep `if` chains in `next-token`) because parsing it through the AIRL interpreter creates multiplicative Rust stack depth.

**Current workaround:** 1GB thread stack in `main.rs`. This is a safety net, not a solution.

## Approach: Hybrid Trampoline + Large Stack

1. **Trampoline** for tail-position expressions: `if`/`let`/`do`/`match` bodies return a `Continue` signal instead of recursing. A driver loop re-evaluates without growing the Rust stack.

2. **Self-recursive TCO** for function calls: when a function's body evaluates to a tail call to itself, reuse the frame (rebind params, re-evaluate body) instead of pushing a new call stack frame.

3. **Keep 1GB thread stack** as safety net for deep non-tail expressions (function arguments, contract checks, variant constructor args).

## Core Data Types

```rust
/// Result of eval_inner — either a final value or a request to continue evaluation
enum EvalResult {
    Done(Value),
    Continue(ContinueWith),
}

/// What the trampoline loop should do next
enum ContinueWith {
    /// Re-evaluate this expression in the current scope (tail position in if/let/do/match)
    Expr(Expr),
    /// Self-recursive tail call: rebind params and re-evaluate body
    TailCall(Vec<Value>),
}
```

`TailCall` carries only args (not the function) because self-TCO only applies when the callee is the same function currently executing.

## Trampoline Driver Loop

The public `eval()` becomes a thin loop:

```rust
pub fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
    let mut current = expr.clone();
    loop {
        match self.eval_inner(&current)? {
            EvalResult::Done(val) => return Ok(val),
            EvalResult::Continue(ContinueWith::Expr(next)) => {
                current = next;
                // Loop back — no Rust stack growth
            }
            EvalResult::Continue(ContinueWith::TailCall(_)) => {
                // TailCall is only produced inside call_fn_inner's own loop.
                // If it leaks to eval(), treat as a bug.
                unreachable!("TailCall should be handled in call_fn_inner");
            }
        }
    }
}
```

## eval_inner Changes

`eval_inner` has the same signature as the old `eval` but returns `EvalResult`:

```rust
fn eval_inner(&mut self, expr: &Expr) -> Result<EvalResult, RuntimeError>
```

### Tail positions (return `Continue(Expr)` instead of recursing):

**If:**
```rust
ExprKind::If(cond, then_expr, else_expr) => {
    let cond_val = self.eval(cond)?;        // NOT tail — fully evaluate
    if is_truthy(&cond_val) {
        Ok(EvalResult::Continue(ContinueWith::Expr((**then_expr).clone())))
    } else {
        Ok(EvalResult::Continue(ContinueWith::Expr((**else_expr).clone())))
    }
}
```

**Let:**
```rust
ExprKind::Let(bindings, body) => {
    self.env.push_frame(FrameKind::Let);
    for binding in bindings {
        let val = self.eval(&binding.value)?;  // NOT tail
        self.env.bind(&binding.name, val);
    }
    // Body is tail position — but we need the Let frame to stay active
    // So we evaluate body fully, then pop frame
    let result = self.eval(body)?;
    self.env.pop_frame();
    Ok(EvalResult::Done(result))
}
```

**Note on Let:** The body cannot simply return `Continue(body)` because the Let frame must remain active during body evaluation and be popped after. The trampoline loop doesn't know about frames. So Let evaluates body via `self.eval(body)` (which itself trampolines internally), then pops. This still benefits from trampolining within the body — if body is `(if ...)`, that `if` will trampoline.

**Do:**
```rust
ExprKind::Do(exprs) => {
    for expr in &exprs[..exprs.len()-1] {
        self.eval(expr)?;                     // NOT tail — side effects
    }
    Ok(EvalResult::Continue(ContinueWith::Expr(exprs.last().unwrap().clone())))
}
```

**Match:**
```rust
ExprKind::Match(scrutinee, arms) => {
    let scrutinee_val = self.eval(scrutinee)?;  // NOT tail
    for arm in arms {
        if let Some(bindings) = try_match(&arm.pattern, &scrutinee_val) {
            self.env.push_frame(FrameKind::Match);
            for (name, val) in bindings {
                self.env.bind(&name, val);
            }
            let result = self.eval(&arm.body)?;
            self.env.pop_frame();
            return Ok(EvalResult::Done(result));
        }
    }
    // No match — error
}
```

**Note on Match:** Same pattern as Let — the Match frame must be active during body evaluation, so body is evaluated via `self.eval()` (which trampolines internally), then frame is popped.

### Non-tail positions (return `Done(value)` as before):

- **Atoms** (IntLit, FloatLit, etc.): `Done(value)` directly
- **FnCall arguments**: fully evaluated via `self.eval(arg)?`
- **FnCall callee**: fully evaluated
- **VariantCtor args**: fully evaluated
- **ListLit items**: fully evaluated
- **Try inner expr**: fully evaluated (wraps in Ok/Err)
- **Lambda**: captures env, returns `Done(Value::Lambda(...))`
- **Contract expressions**: fully evaluated in call_fn_inner

## Self-Recursive Tail Call Optimization

### Detection Mechanism

The self-TCO detection works entirely within `call_fn_inner`. No new `eval_for_tco` method is needed. The mechanism:

1. `call_fn_inner` sets `self.current_fn_name = Some(fn_val.name.clone())` before evaluating the body.
2. `call_fn_inner` calls `self.eval(&def.body)` — which runs the trampoline loop.
3. The trampoline loop calls `eval_inner`. For tail-position expressions (if/do), `eval_inner` returns `Continue(Expr(next))`.
4. The trampoline loop calls `eval_inner` again on `next`. If `next` is a `FnCall`:
   - `eval_inner` evaluates the callee and args normally (non-tail, via `self.eval()`)
   - `eval_inner` checks: is callee a user function AND `callee_name == self.current_fn_name`?
   - If yes: return `EvalResult::Continue(ContinueWith::TailCall(evaluated_args))`
   - If no: call `call_fn` / dispatch builtin normally, return `Done(value)`
5. The trampoline loop in `eval()` receives `Continue(TailCall(args))`. It cannot handle this — it's meant for `call_fn_inner`. So `eval()` returns it as a special value up to `call_fn_inner`.

**Problem:** The trampoline loop in `eval()` doesn't know about `TailCall`. It needs to propagate it.

**Solution:** `eval()` returns `Result<Value, RuntimeError>` to all callers. But `call_fn_inner` needs to distinguish "body returned a value" from "body wants a self-tail-call." Use a **separate enum for call_fn_inner's body evaluation only:**

```rust
enum BodyResult {
    Value(Value),
    SelfTailCall(Vec<Value>),
}
```

`call_fn_inner` calls a private `eval_body()` method instead of `self.eval()`. `eval_body` runs the same trampoline loop as `eval`, but when `eval_inner` returns `Continue(TailCall(args))`, it returns `BodyResult::SelfTailCall(args)` instead of looping.

```rust
/// Like eval(), but can return SelfTailCall for self-TCO detection
fn eval_body(&mut self, expr: &Expr) -> Result<BodyResult, RuntimeError> {
    let mut current = expr.clone();
    loop {
        match self.eval_inner(&current)? {
            EvalResult::Done(val) => return Ok(BodyResult::Value(val)),
            EvalResult::Continue(ContinueWith::Expr(next)) => {
                current = next;  // trampoline — loop back
            }
            EvalResult::Continue(ContinueWith::TailCall(args)) => {
                return Ok(BodyResult::SelfTailCall(args));
            }
        }
    }
}
```

And `eval()` is the public version that panics on TailCall (it should never appear outside `call_fn_inner`):

```rust
pub fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
    let mut current = expr.clone();
    loop {
        match self.eval_inner(&current)? {
            EvalResult::Done(val) => return Ok(val),
            EvalResult::Continue(ContinueWith::Expr(next)) => {
                current = next;
            }
            EvalResult::Continue(ContinueWith::TailCall(_)) => {
                unreachable!("TailCall outside call_fn_inner");
            }
        }
    }
}
```

### TailCall Detection in eval_inner's FnCall Branch

In the FnCall branch of `eval_inner`, after evaluating callee and args:

```rust
ExprKind::FnCall(callee_expr, arg_exprs) => {
    let callee = self.eval(callee_expr)?;  // NOT tail
    let args = arg_exprs.iter()
        .map(|a| self.eval(a))             // NOT tail
        .collect::<Result<Vec<_>, _>>()?;

    match &callee {
        Value::Fn(fn_val) => {
            // Self-TCO check: same function as current caller?
            if let Some(ref current) = self.current_fn_name {
                if &fn_val.name == current {
                    // Return TailCall — let call_fn_inner handle the loop
                    return Ok(EvalResult::Continue(ContinueWith::TailCall(args)));
                }
            }
            // Not self-recursive — normal call
            let result = self.call_fn(fn_val, args)?;
            Ok(EvalResult::Done(result))
        }
        Value::BuiltinFn(name) => {
            // Builtins are never TCO candidates
            // ... existing builtin dispatch, borrow ledger, etc. ...
            Ok(EvalResult::Done(result))
        }
        Value::Lambda(lam) => {
            let result = self.call_lambda(lam, args)?;
            Ok(EvalResult::Done(result))
        }
        _ => Err(RuntimeError::TypeError("not callable".into())),
    }
}
```

**Key:** The TailCall check happens AFTER evaluating all arguments. Args are evaluated via `self.eval()` (non-tail, fully evaluated). The borrow ledger for the arguments is NOT built for the self-tail-call path — the borrows from the *current* call's frame are released when `call_fn_inner` pops the frame before continuing the 'tco loop. The *new* iteration will build fresh borrows.

### call_fn_inner with Self-TCO Loop

```rust
fn call_fn_inner(&mut self, fn_val: &FnValue, mut current_args: Vec<Value>)
    -> Result<Value, RuntimeError>
{
    'tco: loop {
        // 1. Recursion depth check
        if self.recursion_depth >= 50_000 {
            return Err(RuntimeError::TypeError("max recursion depth exceeded".into()));
        }
        self.recursion_depth += 1;

        // 2. Push frame, bind params
        self.env.push_frame(FrameKind::Function);
        for (param, arg) in fn_val.def.params.iter().zip(current_args.iter()) {
            self.env.bind(&param.name, arg.clone(), ...);
        }

        // 3. Check :requires contracts
        for contract in &fn_val.def.requires {
            let val = self.eval(contract)?;
            if !is_truthy(&val) { /* contract violation error, pop frame, return Err */ }
        }

        // 4. Try JIT path (unchanged from current code)
        // ...

        // 5. Set current_fn_name for self-TCO detection
        let prev_fn = self.current_fn_name.take();
        self.current_fn_name = Some(fn_val.name.clone());

        // 6. Evaluate body via eval_body (trampoline + TailCall detection)
        let body_result = self.eval_body(&fn_val.def.body)?;

        // 7. Restore current_fn_name
        self.current_fn_name = prev_fn;

        match body_result {
            BodyResult::Value(val) => {
                // 8. Check :invariant and :ensures on final value
                self.env.bind("result", val.clone(), ...);
                for contract in &fn_val.def.invariants {
                    let cv = self.eval(contract)?;
                    if !is_truthy(&cv) { /* error */ }
                }
                for contract in &fn_val.def.ensures {
                    let cv = self.eval(contract)?;
                    if !is_truthy(&cv) { /* error */ }
                }
                // 9. Cleanup
                self.env.pop_frame();
                self.recursion_depth -= 1;
                return Ok(val);
            }
            BodyResult::SelfTailCall(new_args) => {
                // 10. Self-TCO: pop old frame, loop with new args
                // Contracts: :requires will be re-checked next iteration.
                // :ensures is deferred until final return.
                self.env.pop_frame();
                self.recursion_depth -= 1;
                current_args = new_args;
                continue 'tco;
            }
        }
    }
}
```

### Borrow Ledger and Ownership in TCO Path

The borrow ledger is built in the FnCall arm of `eval_inner` for normal (non-TCO) calls. For the self-TCO path:

- **Arguments are fully evaluated** before returning `TailCall(args)`. No borrows are taken for the tail call — the args are owned `Value`s.
- **The current frame's borrows** are released when `call_fn_inner` pops the frame (`self.env.pop_frame()`).
- **The next iteration** starts fresh: pushes a new frame, binds new args. If the function has `Ref`/`Mut` params, the borrows are established fresh each iteration.

This is correct: the tail call semantically replaces the current call, so the old frame's borrows must end before the new frame begins.

### exec_target Handling

`call_fn` (the wrapper around `call_fn_inner`) saves/restores `self.exec_target`. The TCO loop is entirely inside `call_fn_inner`, so `exec_target` is set once by `call_fn` and remains stable throughout all TCO iterations. This is correct — a self-recursive function has the same `:execute-on` annotation on every call.

### Lambda Bodies

`call_lambda` calls `self.eval(&lam.body)`. Since `eval()` now runs the trampoline loop, lambda bodies automatically benefit from tail-position trampolining (if/do/match within the lambda body). Self-TCO does NOT apply to lambdas because they lack a stable name for detection (`current_fn_name` is only set by `call_fn_inner`). This is fine — anonymous recursion through lambdas is rare and not a bootstrap bottleneck.

## Scope of Changes

### Files Modified

| File | Change |
|------|--------|
| `crates/airl-runtime/src/eval.rs` | Add EvalResult/ContinueWith enums, split eval→eval/eval_inner, trampoline loop, self-TCO in call_fn_inner, current_fn_name field |

### What Does NOT Change

- `builtins.rs` — pure functions, untouched
- `env.rs` — frame push/pop unchanged
- `value.rs` — no new Value variants
- Parser, type checker, contracts, agent runtime — untouched
- JIT dispatch paths — unchanged
- `main.rs` — keep 1GB stack as safety net (can reduce later if trampoline proves sufficient)

## Tail Position Summary

| Expression | Tail position | Trampoline behavior |
|-----------|--------------|-------------------|
| `If` branches | Yes | `Continue(Expr(branch))` |
| `Do` last expr | Yes | `Continue(Expr(last))` |
| `Let` body | Indirect | `eval()` on body (trampolines internally via nested loop) |
| `Match` arm body | Indirect | `eval()` on body (trampolines internally via nested loop) |
| Self-recursive FnCall | Yes | `SelfTailCall(args)` → loop in call_fn_inner |
| FnCall arguments | No | `eval()` fully |
| Contract exprs | No | `eval()` fully |
| VariantCtor/StructLit/ListLit args | No | `eval()` fully |
| Forall/Exists | No | Iterates and produces boolean |
| Lambda (capture) | No | Returns `Done(Value::Lambda(...))` |
| Try inner expr | No | Wraps in Ok/Err |

**"Indirect" tail positions:** Let and Match bodies can't return `Continue` directly because their frames must remain active. But they call `eval()` which runs its own trampoline loop, so `if`/`do`/fn-calls within those bodies still trampoline. The net effect is the same — the Rust stack doesn't grow for nested `if` chains inside a `let` body.

## Contract Interaction with Self-TCO

Contracts (`:requires`, `:ensures`, `:invariant`) are checked per iteration of the TCO loop:

- `:requires` — checked at the top of each iteration (after rebinding params)
- `:ensures` — checked only when the body produces a final value (not a SelfTailCall)
- `:invariant` — checked only when the body produces a final value

This is correct: `:ensures` is about the function's return value, which is only known when the recursion terminates. Intermediate tail calls are not returns.

**Exception:** If a user writes `:ensures [(> result 0)]` on a function that tail-calls itself, the ensures is only checked on the final non-tail-call result. This matches the semantics — the contract is about what the function promises to return, and the tail call IS the return (it returns whatever the recursive call returns).

## Performance Impact

| Scenario | Before | After |
|----------|--------|-------|
| `(lex-loop ...)` over 300 tokens | 300 Rust call frames | 1 Rust call frame (self-TCO) |
| `(parse-sexprs ...)` over 300 tokens | 300 Rust call frames | 1 Rust call frame (self-TCO) |
| 10-deep nested `if` in tail position | 10 eval frames | 0 eval frames (trampoline) |
| `(fold f init big-list)` | O(n) Rust frames | O(1) Rust frames (self-TCO in fold's recursion) |
| `(fact 10000)` | stack overflow | O(1) Rust frames (self-TCO) |
| `(+ (f x) 1)` (non-tail) | unchanged | unchanged |

## Testing

**Existing tests (452+):** Must all pass unchanged. The trampoline is semantically transparent.

**New tests:**

| Test | Purpose |
|------|---------|
| Deep self-recursion (100K) | Self-TCO eliminates stack growth |
| `if` chain in tail position | Trampoline handles deeply nested branches |
| `do` last-expr tail position | Last expression trampolines |
| `let` body with nested `if` | Indirect trampolining works |
| `match` arm with nested `if` | Indirect trampolining works |
| Non-tail FnCall still works | `(+ (f x) 1)` evaluates correctly |
| Contracts on self-recursive fn | `:ensures` checked on final return only |
| Mutual recursion unchanged | A→B→A still works (no self-TCO, but correct) |
| Bootstrap parser_test.airl | Unit tests pass (the original motivation) |
| Bootstrap integration_test.airl | Integration tests pass |

**Stack reduction test:** Reduce `main.rs` thread stack from 1GB to 64MB and verify bootstrap tests still pass.

## Dependencies

None. This is a self-contained change to `eval.rs` (and its Interpreter struct).

## Extension Points

- **Cross-function TCO:** Later, extend `TailCall` to carry a function reference for A→B tail calls. Requires tracking A's contracts to check on B's return.
- **Full explicit stack (Approach 2):** Replace the trampoline loop with a continuation stack. The `EvalResult` enum and driver loop structure are a natural foundation for this.
- **Reduce thread stack:** Once trampoline is proven, reduce from 1GB to 64MB or even default 8MB.
