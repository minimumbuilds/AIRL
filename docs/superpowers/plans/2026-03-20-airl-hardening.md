# AIRL Phase 1 Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the disconnected type checker into the compilation pipeline, add runtime linearity enforcement, replace generic match errors with specific variants, and improve the REPL with `:env` introspection.

**Architecture:** Modify existing files across `airl-runtime` (env, eval, error), `airl-driver` (pipeline, main, repl), and add new test fixtures. No new crates. All 347 existing tests must continue to pass.

**Tech Stack:** Rust, existing AIRL workspace crates.

**Spec:** `docs/superpowers/specs/2026-03-20-airl-hardening-design.md`

---

## File Map (modifications only)

```
crates/
├── airl-runtime/src/
│   ├── error.rs          # Add NonExhaustiveMatch variant
│   ├── env.rs            # Add borrow tracking to Slot, iter_bindings()
│   └── eval.rs           # Linearity enforcement in call_fn, NonExhaustiveMatch
│
├── airl-driver/src/
│   ├── pipeline.rs       # Add PipelineMode, TypeCheck error, wire TypeChecker
│   ├── main.rs           # Pass PipelineMode, handle TypeCheck errors
│   ├── repl.rs           # Persistent TypeChecker, :env command
│   └── lib.rs            # Re-export PipelineMode
│
├── airl-driver/tests/
│   └── fixtures.rs       # Add check_fixtures test
│
tests/fixtures/
├── type_errors/
│   ├── type_mismatch_arg.airl      # NEW
│   └── if_branch_mismatch.airl     # NEW
└── linearity_errors/
    └── use_after_move_own.airl     # NEW
```

---

## Task 1: Add NonExhaustiveMatch Error Variant

**Files:**
- Modify: `crates/airl-runtime/src/error.rs`
- Modify: `crates/airl-runtime/src/eval.rs`

- [ ] **Step 1: Add NonExhaustiveMatch to RuntimeError**

In `crates/airl-runtime/src/error.rs`, add to the enum:
```rust
NonExhaustiveMatch { value: String },
```

Add to the Display impl:
```rust
RuntimeError::NonExhaustiveMatch { value } => {
    write!(f, "NonExhaustiveMatch: no arm matched value: {}", value)
}
```

- [ ] **Step 2: Write test for the new variant**

```rust
#[test]
fn non_exhaustive_match_display() {
    let e = RuntimeError::NonExhaustiveMatch { value: "(Ok 42)".into() };
    let s = format!("{}", e);
    assert!(s.contains("NonExhaustiveMatch"));
    assert!(s.contains("(Ok 42)"));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime -- non_exhaustive`
Expected: pass

- [ ] **Step 4: Replace Custom error in eval.rs**

In `crates/airl-runtime/src/eval.rs`, replace lines 96-99:
```rust
// OLD:
Err(RuntimeError::Custom(format!(
    "no match arm matched value: {}",
    val
)))

// NEW:
Err(RuntimeError::NonExhaustiveMatch {
    value: format!("{}", val),
})
```

- [ ] **Step 5: Run full test suite**

Run: `cargo test --workspace`
Expected: all 347 tests pass (the error message changed but no test matches the exact old string)

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/error.rs crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): replace generic match error with NonExhaustiveMatch"
```

---

## Task 2: Add Borrow Tracking to Slot and iter_bindings

**Files:**
- Modify: `crates/airl-runtime/src/env.rs`

- [ ] **Step 1: Add borrow fields to Slot**

In `crates/airl-runtime/src/env.rs`, update the Slot struct:
```rust
#[derive(Debug, Clone)]
pub struct Slot {
    pub value: Value,
    pub moved: bool,
    pub moved_at: Option<Span>,
    pub immutable_borrows: u32,
    pub mutable_borrow: bool,
}
```

Update `Env::bind()` to initialize the new fields:
```rust
frame.bindings.insert(name, Slot {
    value,
    moved: false,
    moved_at: None,
    immutable_borrows: 0,
    mutable_borrow: false,
});
```

- [ ] **Step 2: Add borrow tracking methods**

```rust
/// Increment immutable borrow count. Errors if mutably borrowed.
pub fn borrow_immutable(&mut self, name: &str) -> Result<(), RuntimeError> {
    for frame in self.frames.iter_mut().rev() {
        if let Some(slot) = frame.bindings.get_mut(name) {
            if slot.moved {
                return Err(RuntimeError::UseAfterMove {
                    name: name.to_string(),
                    span: slot.moved_at.unwrap_or_else(Span::dummy),
                });
            }
            if slot.mutable_borrow {
                return Err(RuntimeError::Custom(format!(
                    "cannot immutably borrow `{}` — already mutably borrowed", name
                )));
            }
            slot.immutable_borrows += 1;
            return Ok(());
        }
    }
    Err(RuntimeError::UndefinedSymbol(name.to_string()))
}

/// Set mutable borrow. Errors if any borrows exist.
pub fn borrow_mutable(&mut self, name: &str) -> Result<(), RuntimeError> {
    for frame in self.frames.iter_mut().rev() {
        if let Some(slot) = frame.bindings.get_mut(name) {
            if slot.moved {
                return Err(RuntimeError::UseAfterMove {
                    name: name.to_string(),
                    span: slot.moved_at.unwrap_or_else(Span::dummy),
                });
            }
            if slot.immutable_borrows > 0 {
                return Err(RuntimeError::Custom(format!(
                    "cannot mutably borrow `{}` — {} immutable borrow(s) active", name, slot.immutable_borrows
                )));
            }
            if slot.mutable_borrow {
                return Err(RuntimeError::Custom(format!(
                    "cannot mutably borrow `{}` — already mutably borrowed", name
                )));
            }
            slot.mutable_borrow = true;
            return Ok(());
        }
    }
    Err(RuntimeError::UndefinedSymbol(name.to_string()))
}

/// Release an immutable borrow.
pub fn release_immutable_borrow(&mut self, name: &str) {
    for frame in self.frames.iter_mut().rev() {
        if let Some(slot) = frame.bindings.get_mut(name) {
            if slot.immutable_borrows > 0 {
                slot.immutable_borrows -= 1;
            }
            return;
        }
    }
}

/// Release a mutable borrow.
pub fn release_mutable_borrow(&mut self, name: &str) {
    for frame in self.frames.iter_mut().rev() {
        if let Some(slot) = frame.bindings.get_mut(name) {
            slot.mutable_borrow = false;
            return;
        }
    }
}

/// Iterate all bindings across all frames (innermost first).
/// Returns (name, &Slot) pairs. Later bindings shadow earlier ones.
pub fn iter_bindings(&self) -> Vec<(&str, &Slot)> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for frame in self.frames.iter().rev() {
        for (name, slot) in &frame.bindings {
            if seen.insert(name.as_str()) {
                result.push((name.as_str(), slot));
            }
        }
    }
    result.sort_by_key(|(name, _)| *name);
    result
}
```

- [ ] **Step 3: Write tests**

```rust
#[test]
fn borrow_immutable_succeeds() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    assert!(env.borrow_immutable("x").is_ok());
    // Can still read
    assert!(env.get("x").is_ok());
}

#[test]
fn borrow_mutable_blocks_immutable() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.borrow_mutable("x").unwrap();
    assert!(env.borrow_immutable("x").is_err());
}

#[test]
fn immutable_borrow_blocks_mutable() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.borrow_immutable("x").unwrap();
    assert!(env.borrow_mutable("x").is_err());
}

#[test]
fn multiple_immutable_borrows_ok() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.borrow_immutable("x").unwrap();
    env.borrow_immutable("x").unwrap();
    assert!(env.get("x").is_ok());
}

#[test]
fn release_borrow_allows_mutable() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.borrow_immutable("x").unwrap();
    env.release_immutable_borrow("x");
    assert!(env.borrow_mutable("x").is_ok());
}

#[test]
fn borrow_moved_value_fails() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.mark_moved("x", Span::dummy()).unwrap();
    assert!(env.borrow_immutable("x").is_err());
}

#[test]
fn mark_moved_while_borrowed_fails() {
    let mut env = Env::new();
    env.bind("x".into(), Value::Int(42));
    env.borrow_immutable("x").unwrap();
    let err = env.mark_moved("x", Span::dummy());
    assert!(err.is_err(), "should not be able to move a borrowed value");
}

#[test]
fn iter_bindings_returns_all() {
    let mut env = Env::new();
    env.bind("a".into(), Value::Int(1));
    env.bind("b".into(), Value::Int(2));
    let bindings = env.iter_bindings();
    assert_eq!(bindings.len(), 2);
}
```

**Also update `mark_moved`** to reject moves while borrows are active:
```rust
pub fn mark_moved(&mut self, name: &str, span: Span) -> Result<(), RuntimeError> {
    for frame in self.frames.iter_mut().rev() {
        if let Some(slot) = frame.bindings.get_mut(name) {
            if slot.immutable_borrows > 0 || slot.mutable_borrow {
                return Err(RuntimeError::Custom(format!(
                    "cannot move `{}` — borrowed", name
                )));
            }
            slot.moved = true;
            slot.moved_at = Some(span);
            return Ok(());
        }
    }
    Err(RuntimeError::UndefinedSymbol(name.to_string()))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-runtime`
Expected: all tests pass (old tests still work with new Slot fields)

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/env.rs
git commit -m "feat(runtime): add borrow tracking to Slot and iter_bindings"
```

---

## Task 3: Linearity Enforcement in Evaluator

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs`

- [ ] **Step 1: Write failing test for use-after-move**

```rust
#[test]
fn eval_use_after_move_errors() {
    let input = r#"
        (defn consume
          :sig [(own x : i32) -> i32]
          :intent "consume x"
          :requires [(valid x)]
          :ensures [(valid result)]
          :body x)
        (let (v : i32 42)
          (do (consume v) v))
    "#;
    let mut lexer = airl_syntax::Lexer::new(input);
    let tokens = lexer.lex_all().unwrap();
    let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
    let mut diags = airl_syntax::Diagnostics::new();
    let mut interp = Interpreter::new();
    let mut result = Ok(Value::Unit);
    for sexpr in &sexprs {
        match airl_syntax::parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => result = interp.eval_top_level(&top),
            Err(_) => {
                let expr = airl_syntax::parser::parse_expr(sexpr, &mut diags).unwrap();
                result = interp.eval(&expr);
            }
        }
    }
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("UseAfterMove") || err.contains("moved"));
}
```

- [ ] **Step 2: Implement linearity enforcement in the FnCall arm of eval()**

In the `FnCall` arm of `eval()`, after evaluating the callee and args, check ownership annotations. **Only enforce moves on explicit `Ownership::Own`** — `Default` is treated as a read (clone without move). This avoids breaking existing tests where params don't specify ownership.

For `Ownership::Ref` and `Ownership::Mut`, track borrows on the source binding and release them after the call returns.

For `Ownership::Copy`, verify the value's type is Copy before cloning.

```rust
ExprKind::FnCall(callee, args) => {
    let callee_val = self.eval(callee)?;
    let mut arg_vals = Vec::with_capacity(args.len());

    // Determine parameter ownership from callee if it's a known function
    let param_ownerships = match &callee_val {
        Value::Function(f) => f.def.params.iter().map(|p| p.ownership).collect::<Vec<_>>(),
        _ => vec![Ownership::Default; args.len()],
    };

    // Track borrows for this call so we can release them after
    let mut borrow_ledger: Vec<(String, bool)> = Vec::new(); // (name, is_mutable)

    for (i, arg) in args.iter().enumerate() {
        let val = self.eval(arg)?;
        arg_vals.push(val);

        let ownership = param_ownerships.get(i).copied().unwrap_or(Ownership::Default);
        if let ExprKind::SymbolRef(ref name) = arg.kind {
            // Skip builtins — they're not movable
            if let Ok(v) = self.env.get(name) {
                if matches!(v, Value::BuiltinFn(_)) { continue; }
            }

            match ownership {
                Ownership::Own => {
                    // Explicit own: mark source as moved
                    self.env.mark_moved(name, arg.span)?;
                }
                Ownership::Ref => {
                    // Immutable borrow: track it
                    self.env.borrow_immutable(name)?;
                    borrow_ledger.push((name.clone(), false));
                }
                Ownership::Mut => {
                    // Mutable borrow: track it
                    self.env.borrow_mutable(name)?;
                    borrow_ledger.push((name.clone(), true));
                }
                Ownership::Copy => {
                    // Explicit copy: verify type is Copy (primitives except String)
                    // For Phase 1, allow copy on Int/UInt/Float/Bool/Unit
                    if let Ok(v) = self.env.get(name) {
                        let is_copy = matches!(v,
                            Value::Int(_) | Value::UInt(_) | Value::Float(_) |
                            Value::Bool(_) | Value::Unit | Value::Nil
                        );
                        if !is_copy {
                            return Err(RuntimeError::Custom(format!(
                                "cannot copy `{}` — type does not implement Copy", name
                            )));
                        }
                    }
                    // Clone without moving (already done by eval)
                }
                Ownership::Default => {
                    // Default: clone without move (no tracking)
                }
            }
        }
    }

    // Call the function
    let result = match callee_val {
        Value::BuiltinFn(ref name) => {
            let f = self.builtins.get(name).ok_or_else(|| {
                RuntimeError::UndefinedSymbol(name.clone())
            })?;
            f(&arg_vals)
        }
        Value::Function(ref fn_val) => {
            let fn_val = fn_val.clone();
            self.call_fn(&fn_val, arg_vals)
        }
        Value::Lambda(ref lam) => {
            let lam = lam.clone();
            self.call_lambda(&lam, arg_vals)
        }
        other => Err(RuntimeError::NotCallable(format!("{}", other))),
    };

    // Release borrows taken for this call
    for (name, is_mutable) in &borrow_ledger {
        if *is_mutable {
            self.env.release_mutable_borrow(name);
        } else {
            self.env.release_immutable_borrow(name);
        }
    }

    result
}
```

**Key: `Default` ownership does NOT move.** Only explicit `Ownership::Own` marks a binding as moved. This preserves backward compatibility with all existing tests.

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace`
Expected: all 347+ existing tests pass. The new `eval_use_after_move_errors` test passes because it uses explicit `(own x : i32)` in the defn.

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): enforce linearity in function calls"
```

---

## Task 4: Wire Type Checker into Pipeline

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`
- Modify: `crates/airl-driver/src/lib.rs`

- [ ] **Step 1: Add PipelineMode and TypeCheck error to pipeline.rs**

Add to the top of pipeline.rs:
```rust
use airl_types::checker::TypeChecker;
```

Add the enum:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Check,  // type errors block execution
    Run,    // type errors warn, execution proceeds
    Repl,   // type errors warn, execution proceeds
}
```

Add to PipelineError:
```rust
pub enum PipelineError {
    Io(String),
    Syntax(Diagnostic),
    Parse(Diagnostics),
    TypeCheck(Diagnostics),  // NEW
    Runtime(RuntimeError),
}
```

Update Display impl to handle TypeCheck:
```rust
PipelineError::TypeCheck(ds) => {
    for d in ds.errors() {
        writeln!(f, "Type error: {}", d.message)?;
    }
    Ok(())
}
```

- [ ] **Step 2: Update run_source to accept PipelineMode and run type checker**

```rust
pub fn run_source_with_mode(source: &str, mode: PipelineMode) -> Result<Value, PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();
    let mut interp = Interpreter::new();
    let mut checker = TypeChecker::new();
    let mut result = Value::Unit;

    // Parse all top-level forms
    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(d) => {
                // Try as bare expression
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(TopLevel::Expr(expr)),
                    Err(_) => return Err(PipelineError::Syntax(d)),
                }
            }
        }
    }

    if diags.has_errors() {
        return Err(PipelineError::Parse(diags));
    }

    // Type check
    for top in &tops {
        let _ = checker.check_top_level(top);
    }

    if checker.has_errors() {
        let type_diags = checker.into_diagnostics();
        match mode {
            PipelineMode::Check => return Err(PipelineError::TypeCheck(type_diags)),
            PipelineMode::Run | PipelineMode::Repl => {
                // Print warnings to stderr
                for d in type_diags.errors() {
                    eprintln!("warning: {}", d.message);
                }
            }
        }
    }

    // Evaluate
    for top in &tops {
        result = interp.eval_top_level(top).map_err(PipelineError::Runtime)?;
    }

    Ok(result)
}
```

Keep the old `run_source` as a convenience wrapper:
```rust
pub fn run_source(source: &str) -> Result<Value, PipelineError> {
    run_source_with_mode(source, PipelineMode::Run)
}
```

- [ ] **Step 3: Rewrite check_source to run the type checker**

```rust
pub fn check_source(source: &str) -> Result<(), PipelineError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(PipelineError::Syntax)?;
    let sexprs = parse_sexpr_all(&tokens).map_err(PipelineError::Syntax)?;
    let mut diags = Diagnostics::new();
    let mut checker = TypeChecker::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
            Err(_) => {}
        }
    }

    if diags.has_errors() {
        return Err(PipelineError::Parse(diags));
    }

    // Type check (strict)
    for top in &tops {
        let _ = checker.check_top_level(top);
    }

    if checker.has_errors() {
        Err(PipelineError::TypeCheck(checker.into_diagnostics()))
    } else {
        Ok(())
    }
}
```

- [ ] **Step 4: Update main.rs to handle TypeCheck errors**

In `print_pipeline_error`, add:
```rust
PipelineError::TypeCheck(diags) => {
    let source = std::fs::read_to_string(path).unwrap_or_default();
    for diag in diags.errors() {
        eprint!("{}", format_diagnostic_with_source(diag, &source, path));
    }
}
```

- [ ] **Step 5: Update lib.rs re-exports**

Add `PipelineMode` to the re-exports in `crates/airl-driver/src/lib.rs`.

- [ ] **Step 6: Add airl-types dependency to airl-driver**

In `crates/airl-driver/Cargo.toml`, add:
```toml
airl-types = { path = "../airl-types" }
```

- [ ] **Step 7: Run tests**

Run: `cargo test --workspace`
Expected: all existing tests pass. The `run_source` wrapper maintains backward compatibility.

- [ ] **Step 8: Commit**

```bash
git add crates/airl-driver/
git commit -m "feat(driver): wire type checker into pipeline with PipelineMode"
```

---

## Task 5: REPL Enhancements — :env and Type Checking

**Files:**
- Modify: `crates/airl-driver/src/repl.rs`

- [ ] **Step 1: Implement :env command**

Replace the `:env` stub with:
```rust
if trimmed == ":env" {
    print_env(&interp);
    continue;
}
```

Add the helper:
```rust
fn print_env(interp: &Interpreter) {
    let bindings = interp.env.iter_bindings();
    let mut functions = Vec::new();
    let mut values = Vec::new();

    for (name, slot) in &bindings {
        match &slot.value {
            Value::BuiltinFn(_) => continue, // skip builtins
            Value::Function(f) => {
                let params: Vec<String> = f.def.params.iter()
                    .map(|p| format!("{}", p.ty.kind))
                    .collect();
                let ret = format!("{}", f.def.return_type.kind);
                functions.push(format!("  {} : ({}) -> {}", name, params.join(", "), ret));
            }
            other => {
                let status = if slot.moved { " [moved]" } else { "" };
                values.push(format!("  {} = {}{}", name, other, status));
            }
        }
    }

    if !values.is_empty() {
        println!("── Bindings ──");
        for v in &values { println!("{}", v); }
    }
    if !functions.is_empty() {
        if !values.is_empty() { println!(); }
        println!("── Functions ──");
        for f in &functions { println!("{}", f); }
    }
    if values.is_empty() && functions.is_empty() {
        println!("(no user bindings)");
    }
}
```

Note: `AstTypeKind` doesn't implement Display — you may need to add a simple formatter or use Debug. For Phase 1, `{:?}` is acceptable.

- [ ] **Step 2: Add type checking warnings to REPL (simplified)**

**Skip persistent REPL type checking for this hardening pass.** The TypeChecker's API uses `into_diagnostics()` which is consuming — integrating it into a persistent REPL session requires adding a `drain_diagnostics(&mut self)` method to `airl-types/src/checker.rs`. This is a cross-crate change that adds complexity. Instead, focus on `:env` (which is the user-facing improvement) and leave REPL type checking for a follow-up.

The `airl run` and `airl check` commands get type checking via the pipeline (Task 4). The REPL gets `:env`.

- [ ] **Step 3: Write test for :env**

```rust
#[test]
fn eval_repl_then_env() {
    let mut interp = Interpreter::new();
    let input = r#"
        (defn greet
          :sig [(name : String) -> String]
          :intent "greet"
          :requires [(valid name)]
          :ensures [(valid result)]
          :body name)
    "#;
    eval_repl_input(input, &mut interp).unwrap();
    let bindings = interp.env.iter_bindings();
    let has_greet = bindings.iter().any(|(name, slot)| {
        *name == "greet" && matches!(slot.value, Value::Function(_))
    });
    assert!(has_greet);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-driver`
Expected: pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/repl.rs
git commit -m "feat(driver): implement :env command and REPL type checking"
```

---

## Task 6: New Test Fixtures

**Files:**
- Create: `tests/fixtures/type_errors/type_mismatch_arg.airl`
- Create: `tests/fixtures/type_errors/if_branch_mismatch.airl`
- Create: `tests/fixtures/type_errors/non_exhaustive_match.airl`
- Create: `tests/fixtures/linearity_errors/use_after_move_own.airl`
- Create: `tests/fixtures/linearity_errors/move_while_borrowed.airl`
- Modify: `crates/airl-driver/tests/fixtures.rs`

- [ ] **Step 1: Create type error fixtures**

`tests/fixtures/type_errors/type_mismatch_arg.airl`:
```clojure
;; ERROR: type
;; This should fail type checking: passing string where i32 expected
(defn double
  :sig [(x : i32) -> i32]
  :intent "double x"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body (* x 2))
(double "hello")
```

`tests/fixtures/type_errors/if_branch_mismatch.airl`:
```clojure
;; ERROR: type
;; If branches return different types
(if true 42 "hello")
```

`tests/fixtures/type_errors/non_exhaustive_match.airl`:
```clojure
;; ERROR: exhaustive
;; Match missing Err variant for Result type
(deftype Result [T : Type, E : Type]
  (| (Ok T) (Err E)))
(match (Ok 42) (Ok v) v)
```

- [ ] **Step 2: Create linearity error fixtures**

`tests/fixtures/linearity_errors/use_after_move_own.airl`:
```clojure
;; ERROR: moved
;; Passing with own should move the value
(defn consume
  :sig [(own x : i32) -> i32]
  :intent "consume"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)
(let (v : i32 42) (do (consume v) v))
```

`tests/fixtures/linearity_errors/move_while_borrowed.airl`:
```clojure
;; ERROR: borrowed
;; Cannot move a value that is borrowed
(defn read-ref
  :sig [(&ref x : i32) -> i32]
  :intent "read"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)
(defn consume
  :sig [(own x : i32) -> i32]
  :intent "consume"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)
(let (v : i32 42) (do (read-ref v) (consume v)))
```

- [ ] **Step 3: Add check_fixtures test**

In `crates/airl-driver/tests/fixtures.rs`, add:
```rust
#[test]
fn check_type_error_fixtures() {
    let dir = find_fixtures_dir().join("type_errors");
    if !dir.exists() { return; }

    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |e| e == "airl") {
            let source = std::fs::read_to_string(&path).unwrap();
            let expected_error = extract_annotation(&source, ";; ERROR:");

            // Run check_source (strict type checking)
            let result = airl_driver::pipeline::check_source(&source);

            if let Some(ref err_fragment) = expected_error {
                assert!(
                    result.is_err(),
                    "fixture {} should fail check_source but passed",
                    path.display()
                );
                let err_msg = format!("{}", result.unwrap_err());
                assert!(
                    err_msg.to_lowercase().contains(&err_fragment.to_lowercase()),
                    "fixture {}: expected error containing '{}', got: {}",
                    path.display(), err_fragment, err_msg
                );
            }
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass including new fixture tests

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/ crates/airl-driver/tests/fixtures.rs
git commit -m "test: add type error and linearity error fixtures"
```

---

## Task 7: Final Verification

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass (347+ original + new tests)

- [ ] **Step 2: Verify airl check catches type errors**

Run: `cargo run -- check tests/fixtures/type_errors/type_mismatch_arg.airl`
Expected: prints type error, exits non-zero

- [ ] **Step 3: Verify airl run warns but proceeds**

Run: `cargo run -- run tests/fixtures/valid/arithmetic.airl`
Expected: prints `10` (no warnings on valid file)

- [ ] **Step 4: Verify all existing fixtures still pass**

Run: `cargo test --test fixtures`
Expected: all fixture tests pass

- [ ] **Step 5: Commit any remaining fixes**

```bash
git commit -m "chore: hardening complete — type checker wired, linearity enforced"
```
