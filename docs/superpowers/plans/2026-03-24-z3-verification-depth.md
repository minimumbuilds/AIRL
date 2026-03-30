# Z3 Verification Depth — List and ADT Proofs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the Z3 SMT solver integration to prove properties about lists and algebraic data types (Result, Option, custom variants), moving contract verification from "only integer arithmetic" to "catches the semantic bugs LLMs actually produce."

**Architecture:** Z3 has built-in theories for sequences (lists) and algebraic datatypes. The work is in the translation layer — converting AIRL contract expressions that reference list operations (`length`, `head`, `tail`, `empty?`, `map`, `filter`) and ADT operations (`match`, variant constructors) into Z3 terms. The existing `Translator` in `crates/airl-solver/src/translate.rs` handles Int, Bool, and Real sorts. We add Seq(Int) for integer lists and declare Z3 datatypes for Result/Option.

**Tech Stack:** Z3 (via `z3` Rust crate), existing `airl-solver` infrastructure, SMT-LIB sequence and datatype theories

---

## Current State

The translator (`crates/airl-solver/src/translate.rs`, 565 lines) handles:
- `VarSort::Int` — integer variables, arithmetic (`+`, `-`, `*`, `/`, `%`)
- `VarSort::Bool` — boolean variables, logical operators (`and`, `or`, `not`)
- `VarSort::Real` — float variables, real arithmetic
- Quantifiers (`forall`, `exists`) with `where` guards
- Comparison operators (`=`, `!=`, `<`, `>`, `<=`, `>=`)
- `valid(x)` → always true

It **cannot** handle:
- `(length xs)` — list length
- `(= (length result) (length input))` — length preservation
- `(sorted result)` — ordering properties
- `(match result (Ok v) ... (Err e) ...)` — ADT case analysis
- `(empty? xs)` — list emptiness

## What Z3 Provides

**Sequences (SMT-LIB `Seq` sort):**
- `seq.len(s)` — length
- `seq.nth(s, i)` — element access
- `seq.++ (s1, s2)` — concatenation
- `seq.unit(x)` — singleton list
- `seq.empty` — empty list
- `seq.contains(s, sub)` — subsequence check
- `seq.extract(s, offset, len)` — substring/sublist

**Algebraic Datatypes (SMT-LIB `declare-datatypes`):**
- Constructor declarations: `(Ok value)`, `(Err error)`
- Tester predicates: `is-Ok`, `is-Err`
- Accessor functions: `Ok-value`, `Err-error`

Both are well-supported in Z3 4.x and available through the `z3` Rust crate's `ast::Datatype` and sequence APIs.

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-solver/src/translate.rs` | Modify | Add VarSort::Seq, VarSort::Result. Add translate_seq, translate_adt methods. Wire list/ADT builtins into translate_bool and translate_int. |
| `crates/airl-solver/src/prover.rs` | Modify | Declare Seq/Result variables for function parameters with List/Result types |
| `tests/fixtures/valid/` | Create | New fixtures testing list and ADT contract verification |
| `tests/fixtures/contract_errors/` | Create | New fixtures testing list/ADT contract violations |

---

### Task 1: Add VarSort::Seq for Integer Lists

**Files:**
- Modify: `crates/airl-solver/src/translate.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn translate_list_length() {
    let ctx = make_ctx();
    let mut t = Translator::new(&ctx);
    t.declare_seq("xs");
    // (length xs) should translate to a Z3 Int
    let callee = Expr { kind: ExprKind::SymbolRef("length".into()), span: Span::dummy() };
    let xs = Expr { kind: ExprKind::SymbolRef("xs".into()), span: Span::dummy() };
    let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![xs]), span: Span::dummy() };
    assert!(t.translate_int(&expr).is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-solver translate_list_length`
Expected: FAIL — `declare_seq` doesn't exist yet

- [ ] **Step 3: Add VarSort::Seq and declare_seq**

In `translate.rs`:

```rust
pub enum VarSort {
    Int,
    Bool,
    Real,
    Seq,  // Z3 Seq(Int) — integer list
}
```

Add to `Translator`:
```rust
seq_vars: HashMap<String, ast::Dynamic<'ctx>>,

pub fn declare_seq(&mut self, name: &str) {
    let int_sort = ast::Sort::int(self.ctx);
    let seq_sort = ast::Sort::seq(self.ctx, &int_sort);
    let var = ast::Dynamic::new_const(self.ctx, name, &seq_sort);
    self.seq_vars.insert(name.to_string(), var);
}
```

- [ ] **Step 4: Handle `length` in translate_int**

In `translate_int`, add to the FnCall match:

```rust
"length" => {
    if let Some(seq) = self.translate_seq(&args[0]).ok() {
        Ok(seq.seq_len())  // returns Z3 Int
    } else {
        Err(TranslateError::UnsupportedExpression("length: not a list".into()))
    }
}
```

Add `translate_seq` method:
```rust
pub fn translate_seq(&self, expr: &Expr) -> Result<ast::Dynamic<'ctx>, TranslateError> {
    match &expr.kind {
        ExprKind::SymbolRef(name) => {
            self.seq_vars.get(name).cloned()
                .ok_or_else(|| TranslateError::UndefinedVariable(name.clone()))
        }
        _ => Err(TranslateError::UnsupportedExpression("seq context".into()))
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p airl-solver translate_list_length`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/airl-solver/src/translate.rs
git commit -m "feat(z3): add VarSort::Seq and length translation for integer lists"
```

---

### Task 2: Translate List Operations to Z3 Sequences

**Files:**
- Modify: `crates/airl-solver/src/translate.rs`

- [ ] **Step 1: Write failing tests for list operations**

```rust
#[test]
fn translate_empty_check() {
    let ctx = make_ctx();
    let mut t = Translator::new(&ctx);
    t.declare_seq("xs");
    // (empty? xs) → Z3 Bool
    let callee = Expr { kind: ExprKind::SymbolRef("empty?".into()), span: Span::dummy() };
    let xs = Expr { kind: ExprKind::SymbolRef("xs".into()), span: Span::dummy() };
    let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![xs]), span: Span::dummy() };
    assert!(t.translate_bool(&expr).is_ok());
}

#[test]
fn translate_length_preservation() {
    // Prove: (= (length result) (length xs)) is expressible
    let ctx = make_ctx();
    let mut t = Translator::new(&ctx);
    t.declare_seq("result");
    t.declare_seq("xs");
    let len_result = make_call("length", vec![make_sym("result")]);
    let len_xs = make_call("length", vec![make_sym("xs")]);
    let eq = make_call("=", vec![len_result, len_xs]);
    assert!(t.translate_bool(&eq).is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-solver translate_empty translate_length_preservation`

- [ ] **Step 3: Add list operation translations**

In `translate_bool`, add to the FnCall match:

```rust
"empty?" => {
    let seq = self.translate_seq(&args[0])?;
    let len = seq.seq_len();
    let zero = ast::Int::from_i64(self.ctx, 0);
    Ok(len._eq(&zero))
}
```

In `translate_int`, the `length` case already works from Task 1. Add:

```rust
"head" => {
    let seq = self.translate_seq(&args[0])?;
    let zero = ast::Int::from_i64(self.ctx, 0);
    Ok(seq.seq_nth(&zero))  // returns element at index 0
}
```

In `translate_seq`, add FnCall handling:

```rust
ExprKind::FnCall(callee, args) => {
    if let ExprKind::SymbolRef(op) = &callee.kind {
        match op.as_str() {
            "tail" => {
                let seq = self.translate_seq(&args[0])?;
                let one = ast::Int::from_i64(self.ctx, 1);
                let len = seq.seq_len();
                let tail_len = ast::Int::sub(self.ctx, &[&len, &one]);
                Ok(seq.seq_extract(&one, &tail_len))
            }
            "cons" => {
                let elem = self.translate_int(&args[0])?;
                let seq = self.translate_seq(&args[1])?;
                let unit = ast::Dynamic::seq_unit(&elem.into());
                Ok(unit.seq_concat(&seq))
            }
            "concat" => {
                let a = self.translate_seq(&args[0])?;
                let b = self.translate_seq(&args[1])?;
                Ok(a.seq_concat(&b))
            }
            _ => Err(TranslateError::UnsupportedExpression(format!("seq: {}", op)))
        }
    } else {
        Err(TranslateError::UnsupportedExpression("seq: non-symbol".into()))
    }
}
ExprKind::ListLit(items) => {
    // Translate [1, 2, 3] as seq.unit(1) ++ seq.unit(2) ++ seq.unit(3)
    // ... build from units
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p airl-solver`

- [ ] **Step 5: Commit**

```bash
git add crates/airl-solver/src/translate.rs
git commit -m "feat(z3): translate list operations (empty?, head, tail, cons, concat) to Z3 sequences"
```

---

### Task 3: Wire Seq Variables into the Prover

**Files:**
- Modify: `crates/airl-solver/src/prover.rs`

The prover declares variables for function parameters before translating contracts. It currently only handles Int, Bool, and Real types. It needs to declare Seq variables for `List` type parameters.

- [ ] **Step 1: Read `prover.rs` to understand how variables are declared**

- [ ] **Step 2: Add List type detection**

In the parameter loop where `sort_from_type_name` is called, add:

```rust
"List" => {
    translator.declare_seq(&param.name);
}
```

Also declare `result` as Seq when the return type is List.

- [ ] **Step 3: Write a fixture test**

Create `tests/fixtures/valid/list_length_contract.airl`:
```clojure
;; EXPECT: [2 4 6]
(defn double-all
  :sig [(xs : List) -> List]
  :intent "Double every element in a list"
  :requires [(valid xs)]
  :ensures [(= (length result) (length xs))]
  :body (map (fn [x] (* x 2)) xs))

(print (double-all [1 2 3]))
```

- [ ] **Step 4: Run `cargo run --features jit -- check` on the fixture to verify Z3 attempts the proof**

- [ ] **Step 5: Run fixture tests**

Run: `cargo test -p airl-driver --test fixtures --features jit`

- [ ] **Step 6: Commit**

```bash
git add crates/airl-solver/src/prover.rs tests/fixtures/valid/list_length_contract.airl
git commit -m "feat(z3): wire List type parameters into Z3 prover as Seq variables"
```

---

### Task 4: Add Z3 Algebraic Datatype Support for Result/Option

**Files:**
- Modify: `crates/airl-solver/src/translate.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn translate_match_result() {
    let ctx = make_ctx();
    let mut t = Translator::new(&ctx);
    t.declare_result("result");
    // (match result (Ok v) (> v 0) (Err _) true)
    // This should be translatable to a Z3 Bool
    // ... build AST for match expression
    // assert!(t.translate_bool(&match_expr).is_ok());
}
```

- [ ] **Step 2: Declare Z3 Result datatype**

```rust
pub fn declare_result_sort(&mut self) -> z3::Sort<'ctx> {
    // Use Z3's datatype facility to declare:
    // (declare-datatypes ((Result 0))
    //   ((Ok (ok-value Int)) (Err (err-value Int))))
    let result_sort = z3::DatatypeBuilder::new(self.ctx, "Result")
        .variant("Ok", vec![("ok-value", z3::DatatypeAccessor::Sort(ast::Sort::int(self.ctx)))])
        .variant("Err", vec![("err-value", z3::DatatypeAccessor::Sort(ast::Sort::int(self.ctx)))])
        .finish();
    result_sort
}
```

Note: The exact z3 Rust crate API for datatypes needs verification. The `z3` crate (version used in this project) may use `DatatypeBuilder` or a different API. Check `z3` crate docs.

- [ ] **Step 3: Translate match expressions over Result**

In `translate_bool`, handle `ExprKind::Match`:

```rust
ExprKind::Match(scrutinee, arms) => {
    // For each arm, translate: (is-Ok scrutinee) => arm_body AND (is-Err scrutinee) => arm_body
    // Combine with Z3 and/or based on pattern
    // Use the Result datatype's tester predicates and accessor functions
}
```

- [ ] **Step 4: Add `is-ok?` and `is-err?` to translate_bool**

```rust
"is-ok?" => {
    let val = self.translate_result(&args[0])?;
    // Use Z3 tester: ((_ is Ok) val)
    Ok(val.as_datatype().unwrap().variant_is("Ok"))
}
```

- [ ] **Step 5: Write fixture tests for Result contracts**

Create `tests/fixtures/valid/result_contract.airl`:
```clojure
;; EXPECT: (Ok 5)
(defn safe-divide
  :sig [(a : i64) (b : i64) -> Result]
  :intent "Divide a by b safely"
  :requires [(!= b 0)]
  :ensures [(is-ok? result)]
  :body (Ok (/ a b)))

(print (safe-divide 10 2))
```

- [ ] **Step 6: Run tests**
- [ ] **Step 7: Commit**

```bash
git add crates/airl-solver/src/translate.rs tests/fixtures/valid/
git commit -m "feat(z3): algebraic datatype support for Result — match, is-ok?, is-err?"
```

---

### Task 5: Integration Testing and Edge Cases

**Files:**
- Create: `tests/fixtures/valid/z3_list_proven.airl`
- Create: `tests/fixtures/valid/z3_result_proven.airl`
- Modify: `crates/airl-solver/src/translate.rs` (edge cases)

- [ ] **Step 1: Create comprehensive list contract fixtures**

`tests/fixtures/valid/z3_list_proven.airl`:
```clojure
;; EXPECT: [3 2 1]
;; Tests: Z3 can reason about list length preservation
(defn my-reverse
  :sig [(xs : List) -> List]
  :intent "Reverse a list"
  :requires [(valid xs)]
  :ensures [(= (length result) (length xs))]
  :body (reverse xs))

(print (my-reverse [1 2 3]))
```

- [ ] **Step 2: Create contract violation fixture for lists**

`tests/fixtures/contract_errors/list_length_violation.airl`:
```clojure
;; ERROR: Ensures
;; This function drops elements — ensures clause should fail at runtime
(defn bad-filter
  :sig [(xs : List) -> List]
  :intent "Filter but claim same length"
  :requires [(valid xs)]
  :ensures [(= (length result) (length xs))]
  :body (filter (fn [x] (> x 2)) xs))

(print (bad-filter [1 2 3 4 5]))
```

- [ ] **Step 3: Test with `check` mode to see Z3 proof attempts**

```bash
cargo run --features jit -- check tests/fixtures/valid/z3_list_proven.airl
# Should show "note: contract proven" or gracefully fall back to runtime
```

- [ ] **Step 4: Run full fixture suite**

```bash
cargo test -p airl-driver --test fixtures --features jit
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/
git commit -m "test(z3): list and ADT contract verification fixtures"
```

---

## Important Notes for the Implementing Agent

1. **Z3 Rust crate API.** The `z3` crate version in this project may not have `ast::Dynamic::seq_len()` directly. Check the crate's API surface. You may need to use `z3::ast::Dynamic` with sort-specific methods, or call Z3 functions via `z3::FuncDecl`. Read `Cargo.lock` or `crates/airl-solver/Cargo.toml` for the exact z3 crate version, then check its docs.

2. **Graceful degradation is essential.** If Z3 can't translate a list contract (e.g., it references `map` or `filter` which are complex), the translator should return `TranslateError::UnsupportedExpression` — NOT panic. The prover already handles this by falling back to runtime checking for untranslatable contracts.

3. **Start with `length` — it has the highest value/effort ratio.** `(= (length result) (length input))` is the single most common list contract and is trivially expressible in Z3's sequence theory. Get this working end-to-end before tackling more complex operations.

4. **Match expressions are the hardest part.** Translating `(match result (Ok v) (> v 0) (Err _) true)` to Z3 requires: declaring Result as a datatype, using tester predicates (`is-Ok`), using accessor functions (`Ok-value`), and combining arm conditions with implications. This is Task 4 — do it last.

5. **The prover (`prover.rs`) determines variable sorts from parameter types.** Currently it uses `sort_from_type_name` which only handles primitive types. You need to extend it to detect `List` and `Result` type names and declare the appropriate Z3 sorts.

## Dependency Graph

```
Task 1 (VarSort::Seq + length)
    │
    └─→ Task 2 (list operations: empty?, head, tail, cons, concat)
            │
            └─→ Task 3 (wire into prover)
                    │
                    ├─→ Task 5 (integration tests)
                    │
Task 4 (Result/Option ADTs) ─────┘
```

Tasks 1→2→3 are sequential (each builds on the last). Task 4 is independent of 1-3 (different Z3 theory). Task 5 depends on both tracks.
