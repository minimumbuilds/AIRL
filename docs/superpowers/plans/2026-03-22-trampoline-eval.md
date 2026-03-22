# Trampoline Eval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert AIRL's tree-walking interpreter from recursive to trampoline-based evaluation, eliminating stack overflow for tail-position expressions and self-recursive functions.

**Architecture:** Split `eval()` into `eval()` (trampoline driver loop) + `eval_inner()` (single-step evaluator returning `EvalResult`). Add `eval_body()` for self-TCO detection in `call_fn_inner`. Tail-position expressions (`if` branches, `do` last expr) return `Continue(Expr)` instead of recursing. Self-recursive function calls return `TailCall(args)`, causing `call_fn_inner` to loop instead of recursing.

**Tech Stack:** Rust. Changes confined to `crates/airl-runtime/src/eval.rs`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-03-22-trampoline-eval-design.md`

---

## File Structure

| File | Purpose |
|------|---------|
| Modify: `crates/airl-runtime/src/eval.rs` | Add EvalResult/ContinueWith/BodyResult enums, split eval→eval+eval_inner+eval_body, trampoline tail positions, self-TCO in call_fn_inner, add current_fn_name field |

No new files. No changes to builtins.rs, env.rs, value.rs, parser, or any other crate.

---

## Reference: Current eval.rs Structure

| Section | Lines | Purpose |
|---------|-------|---------|
| Interpreter struct + new() | 20-81 | 11 fields, builtin symbol registration |
| `eval()` | 83-449 | Main dispatch on 19 ExprKind variants |
| `call_fn()` | 451-460 | Wrapper: save/restore exec_target, delegate to call_fn_inner |
| `call_fn_inner()` | 462-618 | Push frame, bind params, check contracts, JIT/interpret, check contracts, pop frame |
| `call_lambda()` | 620-637 | Lambda invocation |
| Agent builtins | 722-1207 | 9 builtins needing &mut self |
| `eval_top_level()` | 1209-1230 | Top-level dispatch |
| Tests | 1428-1798 | 40+ inline tests using eval_str() helper |

---

### Task 1: Add EvalResult and ContinueWith Enums

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs:1-12`

Add the trampoline control types above the Interpreter struct. No behavior changes yet.

- [ ] **Step 1: Add the enum definitions after the use statements (before line 13)**

```rust
/// Result of a single eval step — either a final value or a continuation
enum EvalResult {
    Done(Value),
    Continue(ContinueWith),
}

/// What to do next when eval_inner returns Continue
enum ContinueWith {
    /// Re-evaluate this expression (tail position in if/do)
    Expr(Expr),
    /// Self-recursive tail call: rebind params and re-evaluate body
    TailCall(Vec<Value>),
}

/// Result of eval_body — like EvalResult but surfaces TailCall to call_fn_inner
enum BodyResult {
    Value(Value),
    SelfTailCall(Vec<Value>),
}
```

- [ ] **Step 2: Add `current_fn_name` field to Interpreter struct**

In the Interpreter struct (line 20-35), add after `exec_target`:

```rust
    /// Name of the function currently being evaluated (for self-TCO detection)
    current_fn_name: Option<String>,
```

And in `Interpreter::new()` (line 39-56), add to the initializer:

```rust
    current_fn_name: None,
```

- [ ] **Step 3: Run tests to verify no regressions**

Run: `cargo test -p airl-runtime`
Expected: All existing tests pass (enums are defined but unused)

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): add EvalResult/ContinueWith/BodyResult enums for trampoline"
```

---

### Task 2: Split eval into eval + eval_inner (Trampoline Shell)

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs:83-449`

Rename the current `eval()` to `eval_inner()` (returns `EvalResult`), and create a new `eval()` that runs the trampoline loop. In this task, `eval_inner` wraps every return in `EvalResult::Done()` — no behavior change yet.

- [ ] **Step 1: Rename `eval` to `eval_inner` and change return type**

Change the signature at line 83 from:
```rust
    pub fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
```
to:
```rust
    fn eval_inner(&mut self, expr: &Expr) -> Result<EvalResult, RuntimeError> {
```

- [ ] **Step 2: Wrap all return values in EvalResult::Done**

Every `Ok(Value::...)` return in eval_inner becomes `Ok(EvalResult::Done(Value::...))`.
Every `self.eval(...)` call within eval_inner becomes `self.eval(...)` (still calls the public eval, which we'll add next — so no change needed for recursive calls).
Every final expression like `result` at line 402 becomes `Ok(EvalResult::Done(result?))` or similar.

The key changes in eval_inner:
- Line 85-90 (atoms): `Ok(Value::Int(*v))` → `Ok(EvalResult::Done(Value::Int(*v)))` etc.
- Line 94: `self.env.get(name).cloned()` → `Ok(EvalResult::Done(self.env.get(name)?.clone()))`
- Line 97-104 (If): keep calling `self.eval()` for now, wrap result: `Ok(EvalResult::Done(self.eval(then_branch)?))` (we'll change to Continue in Task 3)
- Lines 106-447: same pattern — wrap final Ok values in Done, keep self.eval() calls as-is

**Important:** The `self.eval(...)` calls inside eval_inner call the NEW public `eval()` (trampoline), not `eval_inner`. This is correct — sub-expressions are fully evaluated.

- [ ] **Step 3: Create new public `eval()` with trampoline loop**

Add above `eval_inner`:

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
                    unreachable!("TailCall should only appear inside eval_body");
                }
            }
        }
    }
```

- [ ] **Step 4: Run tests to verify no regressions**

Run: `cargo test -p airl-runtime`
Expected: All tests pass — behavior is identical, just wrapped in Done

- [ ] **Step 5: Run bootstrap tests**

Run: `cargo run -p airl-driver -- run bootstrap/parser_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: All task tests complete, no FAIL

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "refactor(runtime): split eval into trampoline driver + eval_inner"
```

---

### Task 3: Trampoline Tail Positions — If and Do

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs` (eval_inner's If and Do branches)

Convert `If` branches and `Do` last expression to return `Continue(Expr)` instead of recursing.

- [ ] **Step 1: Write a test for deep if-chain**

Add to the test module at the bottom of eval.rs:

```rust
    #[test]
    fn test_deep_if_chain() {
        // 1000-deep nested if chain — would stack overflow without trampoline
        let mut code = String::new();
        for _ in 0..1000 {
            code.push_str("(if true ");
        }
        code.push_str("42");
        for _ in 0..1000 {
            code.push_str(" 0)");
        }
        assert_eq!(eval_str(&code), Value::Int(42));
    }
```

- [ ] **Step 2: Run test to verify it currently works (or overflows)**

Run: `cargo test -p airl-runtime test_deep_if_chain`
Expected: May pass (1000 deep is within 1GB stack) or fail. Either way, verify test compiles.

- [ ] **Step 3: Convert If branch to trampoline**

In eval_inner, change the If branch from:
```rust
ExprKind::If(cond, then_branch, else_branch) => {
    let cond_val = self.eval(cond)?;
    if is_truthy(&cond_val) {
        Ok(EvalResult::Done(self.eval(then_branch)?))
    } else {
        Ok(EvalResult::Done(self.eval(else_branch)?))
    }
}
```
to:
```rust
ExprKind::If(cond, then_branch, else_branch) => {
    let cond_val = self.eval(cond)?;
    if is_truthy(&cond_val) {
        Ok(EvalResult::Continue(ContinueWith::Expr((**then_branch).clone())))
    } else {
        Ok(EvalResult::Continue(ContinueWith::Expr((**else_branch).clone())))
    }
}
```

- [ ] **Step 4: Convert Do branch to trampoline**

Change Do from:
```rust
ExprKind::Do(exprs) => {
    let mut result = Value::Unit;
    for e in exprs {
        result = self.eval(e)?;
    }
    Ok(EvalResult::Done(result))
}
```
to:
```rust
ExprKind::Do(exprs) => {
    if exprs.is_empty() {
        return Ok(EvalResult::Done(Value::Unit));
    }
    for e in &exprs[..exprs.len() - 1] {
        self.eval(e)?;
    }
    Ok(EvalResult::Continue(ContinueWith::Expr(exprs.last().unwrap().clone())))
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test -p airl-runtime`
Expected: All tests pass

- [ ] **Step 6: Run bootstrap tests**

Run: `cargo run -p airl-driver -- run bootstrap/parser_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: All task tests complete, no FAIL

- [ ] **Step 7: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): trampoline if-branches and do-last-expr"
```

---

### Task 4: Add eval_body for Self-TCO Detection

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs`

Add the `eval_body()` method that runs the trampoline but surfaces `TailCall` to `call_fn_inner`.

- [ ] **Step 1: Add eval_body method**

Add after the `eval()` method:

```rust
    /// Like eval(), but returns BodyResult to surface self-tail-calls to call_fn_inner
    fn eval_body(&mut self, expr: &Expr) -> Result<BodyResult, RuntimeError> {
        let mut current = expr.clone();
        loop {
            match self.eval_inner(&current)? {
                EvalResult::Done(val) => return Ok(BodyResult::Value(val)),
                EvalResult::Continue(ContinueWith::Expr(next)) => {
                    current = next;
                }
                EvalResult::Continue(ContinueWith::TailCall(args)) => {
                    return Ok(BodyResult::SelfTailCall(args));
                }
            }
        }
    }
```

- [ ] **Step 2: Run tests (no behavior change yet — eval_body is unused)**

Run: `cargo test -p airl-runtime`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): add eval_body for self-TCO detection"
```

---

### Task 5: Self-TCO Detection in FnCall Branch

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs` (eval_inner's FnCall branch, ~lines 161-403)

When eval_inner encounters a FnCall to a user function whose name matches `self.current_fn_name`, return `TailCall(args)` instead of calling `call_fn`.

- [ ] **Step 1: Write a test for deep self-recursion**

Add to the test module:

```rust
    #[test]
    fn test_self_tco_deep_recursion() {
        // count-down from 100000 — would overflow without TCO
        let code = r#"
            (defn count-down
              :sig [(n : i64) -> i64]
              :requires [(valid n)]
              :ensures [(valid result)]
              :body (if (= n 0) 0 (count-down (- n 1))))
            (count-down 100000)
        "#;
        assert_eq!(eval_str(code), Value::Int(0));
    }
```

- [ ] **Step 2: Modify FnCall branch to detect self-tail-calls**

In eval_inner's FnCall branch, the final match (around line 375) currently has:

```rust
Value::Function(ref fn_val) => {
    let fn_val = fn_val.clone();
    self.call_fn(&fn_val, arg_vals)
}
```

Change the `Value::Function` arm to:

```rust
Value::Function(ref fn_val) => {
    // Self-TCO: if calling the same function currently executing,
    // return TailCall to let call_fn_inner loop instead of recurse
    if let Some(ref current_name) = self.current_fn_name {
        if &fn_val.name == current_name {
            // Release borrows before returning TailCall
            for (name, is_mutable) in &borrow_ledger {
                if *is_mutable {
                    self.env.release_mutable_borrow(name);
                } else {
                    self.env.release_immutable_borrow(name);
                }
            }
            return Ok(EvalResult::Continue(ContinueWith::TailCall(arg_vals)));
        }
    }
    let fn_val = fn_val.clone();
    EvalResult::Done(self.call_fn(&fn_val, arg_vals)?)
}
```

**Important:** The borrow ledger must be released BEFORE returning TailCall, because call_fn_inner will pop the frame (which clears the frame's borrow state). The normal non-TCO path releases borrows after the call returns (lines 393-400), so the TailCall path must release them explicitly.

Also wrap the other arms (BuiltinFn, Lambda) in `EvalResult::Done(...)` to match the new return type.

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime`
Expected: The new test_self_tco_deep_recursion test may not pass yet (current_fn_name is never set). That's OK — Task 6 wires it up.

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): detect self-recursive tail calls in FnCall branch"
```

---

### Task 6: Self-TCO Loop in call_fn_inner

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs:462-618`

Convert `call_fn_inner` from a single-pass function to a `'tco: loop` that re-executes on `SelfTailCall`.

- [ ] **Step 1: Refactor call_fn_inner to use eval_body and TCO loop**

Replace `call_fn_inner` (lines 462-618) with:

```rust
    fn call_fn_inner(&mut self, fn_val: &FnValue, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let mut current_args = args;
        let def = &fn_val.def;

        'tco: loop {
            if self.recursion_depth >= 50_000 {
                return Err(RuntimeError::TypeError(
                    "maximum recursion depth (50000) exceeded".into(),
                ));
            }
            self.recursion_depth += 1;

            // 1. Push Function frame
            self.env.push_frame(FrameKind::Function);

            // 2. Bind params to arg values
            for (i, param) in def.params.iter().enumerate() {
                let val = current_args.get(i).cloned().unwrap_or(Value::Nil);
                self.env.bind(param.name.clone(), val);
            }

            // 3. Check :requires contracts
            for contract in &def.requires {
                let contract_result = self.eval(contract)?;
                if contract_result != Value::Bool(true) {
                    self.recursion_depth -= 1;
                    self.env.pop_frame();
                    return Err(RuntimeError::ContractViolation(
                        airl_contracts::violation::ContractViolation {
                            function: fn_val.name.clone(),
                            contract_kind: airl_contracts::violation::ContractKind::Requires,
                            clause_source: contract.to_airl(),
                            bindings: self.capture_bindings(),
                            evaluated: format!("{}", contract_result),
                            span: contract.span,
                        },
                    ));
                }
            }

            // 4. Try JIT path (unchanged)
            if let Some(ref mut jit) = self.jit {
                let raw_args: Result<Vec<_>, _> = current_args.iter().map(|val| {
                    value_to_raw(val)
                }).collect();

                if let Ok(raw_args) = raw_args {
                    match jit.try_call(def, &raw_args) {
                        Ok(Some(raw_result)) => {
                            let result_val = raw_to_value(raw_result, &def.return_type);
                            self.env.bind("result".to_string(), result_val.clone());
                            // Check contracts on JIT result
                            for contract in &def.invariants {
                                let contract_result = self.eval(contract)?;
                                if contract_result != Value::Bool(true) {
                                    self.recursion_depth -= 1;
                                    self.env.pop_frame();
                                    return Err(RuntimeError::ContractViolation(
                                        airl_contracts::violation::ContractViolation {
                                            function: fn_val.name.clone(),
                                            contract_kind: airl_contracts::violation::ContractKind::Invariant,
                                            clause_source: contract.to_airl(),
                                            bindings: self.capture_bindings(),
                                            evaluated: format!("{}", contract_result),
                                            span: contract.span,
                                        },
                                    ));
                                }
                            }
                            for contract in &def.ensures {
                                let contract_result = self.eval(contract)?;
                                if contract_result != Value::Bool(true) {
                                    self.recursion_depth -= 1;
                                    self.env.pop_frame();
                                    return Err(RuntimeError::ContractViolation(
                                        airl_contracts::violation::ContractViolation {
                                            function: fn_val.name.clone(),
                                            contract_kind: airl_contracts::violation::ContractKind::Ensures,
                                            clause_source: contract.to_airl(),
                                            bindings: self.capture_bindings(),
                                            evaluated: format!("{}", contract_result),
                                            span: contract.span,
                                        },
                                    ));
                                }
                            }
                            self.recursion_depth -= 1;
                            self.env.pop_frame();
                            return Ok(result_val);
                        }
                        Ok(None) => {} // not compilable, fall through
                        Err(_e) => {} // JIT error, fall through
                    }
                }
            }

            // 5. Set current_fn_name for self-TCO detection
            let prev_fn = self.current_fn_name.take();
            self.current_fn_name = Some(fn_val.name.clone());

            // 6. Eval body via eval_body (trampoline + TailCall detection)
            let body_result = self.eval_body(&def.body);

            // 7. Restore current_fn_name
            self.current_fn_name = prev_fn;

            match body_result {
                Ok(BodyResult::Value(result_val)) => {
                    // 8. Check contracts on final result
                    self.env.bind("result".to_string(), result_val.clone());

                    for contract in &def.invariants {
                        let contract_result = self.eval(contract)?;
                        if contract_result != Value::Bool(true) {
                            self.recursion_depth -= 1;
                            self.env.pop_frame();
                            return Err(RuntimeError::ContractViolation(
                                airl_contracts::violation::ContractViolation {
                                    function: fn_val.name.clone(),
                                    contract_kind: airl_contracts::violation::ContractKind::Invariant,
                                    clause_source: contract.to_airl(),
                                    bindings: self.capture_bindings(),
                                    evaluated: format!("{}", contract_result),
                                    span: contract.span,
                                },
                            ));
                        }
                    }

                    for contract in &def.ensures {
                        let contract_result = self.eval(contract)?;
                        if contract_result != Value::Bool(true) {
                            self.recursion_depth -= 1;
                            self.env.pop_frame();
                            return Err(RuntimeError::ContractViolation(
                                airl_contracts::violation::ContractViolation {
                                    function: fn_val.name.clone(),
                                    contract_kind: airl_contracts::violation::ContractKind::Ensures,
                                    clause_source: contract.to_airl(),
                                    bindings: self.capture_bindings(),
                                    evaluated: format!("{}", contract_result),
                                    span: contract.span,
                                },
                            ));
                        }
                    }

                    // 9. Cleanup and return
                    self.recursion_depth -= 1;
                    self.env.pop_frame();
                    return Ok(result_val);
                }
                Ok(BodyResult::SelfTailCall(new_args)) => {
                    // 10. Self-TCO: pop frame, loop with new args
                    self.env.pop_frame();
                    self.recursion_depth -= 1;
                    current_args = new_args;
                    continue 'tco;
                }
                Err(e) => {
                    self.recursion_depth -= 1;
                    self.env.pop_frame();
                    return Err(e);
                }
            }
        }
    }
```

- [ ] **Step 2: Run all tests including the deep recursion test**

Run: `cargo test -p airl-runtime`
Expected: All tests pass, including `test_self_tco_deep_recursion`

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace --exclude airl-mlir`
Expected: All 452+ tests pass

- [ ] **Step 4: Run bootstrap tests**

Run: `cargo run -p airl-driver -- run bootstrap/parser_test.airl 2>&1 | grep -E "FAIL|complete"`
Run: `cargo run -p airl-driver -- run bootstrap/integration_test.airl 2>&1 | grep -E "PASS|FAIL|complete"`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): self-TCO loop in call_fn_inner via eval_body"
```

---

### Task 7: Comprehensive Tests and Stack Reduction

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs` (test module)
- Modify: `crates/airl-driver/src/main.rs` (reduce stack size)

Add comprehensive trampoline tests and verify we can reduce the thread stack.

- [ ] **Step 1: Add comprehensive trampoline tests**

Add to the test module:

```rust
    #[test]
    fn test_tco_accumulator_pattern() {
        // Tail-recursive sum with accumulator
        let code = r#"
            (defn sum-acc
              :sig [(n : i64) (acc : i64) -> i64]
              :requires [(valid n)]
              :ensures [(valid result)]
              :body (if (= n 0) acc (sum-acc (- n 1) (+ acc n))))
            (sum-acc 10000 0)
        "#;
        assert_eq!(eval_str(code), Value::Int(50005000));
    }

    #[test]
    fn test_tco_with_match_in_body() {
        // Self-recursive function with match → if → tail call
        let code = r#"
            (defn process
              :sig [(xs : List) (acc : i64) -> i64]
              :requires [(valid xs)]
              :ensures [(valid result)]
              :body (match xs
                (Ok v) (+ acc v)
                (Err _) acc))
            (process (Ok 42) 10)
        "#;
        assert_eq!(eval_str(code), Value::Int(52));
    }

    #[test]
    fn test_non_tail_call_still_works() {
        // (+ (f x) 1) — f is NOT in tail position
        let code = r#"
            (defn double
              :sig [(x : i64) -> i64]
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (* x 2))
            (+ (double 21) 0)
        "#;
        assert_eq!(eval_str(code), Value::Int(42));
    }

    #[test]
    fn test_contracts_on_tco_function() {
        // :ensures is only checked on the final return
        let code = r#"
            (defn count-up
              :sig [(n : i64) (target : i64) -> i64]
              :requires [(valid n)]
              :ensures [(= result target)]
              :body (if (= n target) n (count-up (+ n 1) target)))
            (count-up 0 100)
        "#;
        assert_eq!(eval_str(code), Value::Int(100));
    }

    #[test]
    fn test_do_tail_position() {
        // Last expr in do is trampolined
        let code = "(do 1 2 3 (if true 42 0))";
        assert_eq!(eval_str(code), Value::Int(42));
    }

    #[test]
    fn test_mutual_recursion_still_works() {
        // Mutual recursion doesn't use TCO (different function names)
        let code = r#"
            (defn is-even
              :sig [(n : i64) -> Bool]
              :requires [(valid n)]
              :ensures [(valid result)]
              :body (if (= n 0) true (is-odd (- n 1))))
            (defn is-odd
              :sig [(n : i64) -> Bool]
              :requires [(valid n)]
              :ensures [(valid result)]
              :body (if (= n 0) false (is-even (- n 1))))
            (is-even 10)
        "#;
        assert_eq!(eval_str(code), Value::Bool(true));
    }
```

- [ ] **Step 2: Run all tests**

Run: `cargo test -p airl-runtime`
Expected: All pass

- [ ] **Step 3: Try reducing thread stack to 64MB**

In `crates/airl-driver/src/main.rs`, change:
```rust
let builder = std::thread::Builder::new().stack_size(1024 * 1024 * 1024);
```
to:
```rust
let builder = std::thread::Builder::new().stack_size(64 * 1024 * 1024);
```

- [ ] **Step 4: Run bootstrap tests with reduced stack**

Run: `cargo run -p airl-driver -- run bootstrap/parser_test.airl 2>&1 | grep -E "FAIL|complete"`
Run: `cargo run -p airl-driver -- run bootstrap/integration_test.airl 2>&1 | grep -E "PASS|FAIL|complete"`
Expected: All pass. If they fail, try 128MB instead.

- [ ] **Step 5: Run full workspace tests**

Run: `cargo test --workspace --exclude airl-mlir`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/eval.rs crates/airl-driver/src/main.rs
git commit -m "feat(runtime): comprehensive trampoline tests and reduce thread stack to 64MB"
```

---

### Task 8: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update Completed Tasks section**

Add to the Completed Tasks list:

```markdown
- **Trampoline Eval + Self-TCO** — `eval()` split into trampoline driver loop + `eval_inner()` single-step evaluator. Tail-position expressions (`if` branches, `do` last expr) return `Continue(Expr)` instead of recursing on Rust stack. Self-recursive function calls detected by `current_fn_name` and looped in `call_fn_inner` via `eval_body()`. Eliminates stack overflow for tail-recursive AIRL functions (bootstrap lexer/parser loops, fold, map). Thread stack reduced from 1GB to 64MB.
```

- [ ] **Step 2: Update Known Issues section**

Update the self-hosting known limitation to reflect the fix:

Change from: "Parsing deeply nested files ... is too slow for the tree-walking interpreter"
To: "Parsing deeply nested files is computationally intensive in the tree-walking interpreter but no longer causes stack overflow thanks to the trampoline. Full lexer self-parse may be slow but terminates correctly."

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with trampoline eval status"
```
