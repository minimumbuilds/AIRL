# Bootstrap Type Checker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-hosted type checker for the AIRL bootstrap compiler, written in pure AIRL, that enforces the type system from the AIRL Language Specification §3.

**Architecture:** Two-pass type checker (registration + checking) operating on the AST from `bootstrap/parser.airl`. Built interleaved with bootstrap code fixes — one module at a time (lexer → parser → eval). Uses map-based scoped environments for type bindings and a constructor registry for variant lookup.

**Tech Stack:** Pure AIRL — uses existing map builtins (`map-new`, `map-get`, `map-set`, `map-has`), list builtins, and the self-hosted lexer/parser. No Rust changes required.

**Spec:** `docs/superpowers/specs/2026-03-22-bootstrap-type-checker-design.md`

**AIRL Constraints (apply to ALL tasks):**
- `and`/`or` are **eager** — use nested `if` for short-circuit logic
- No import system — test files must be self-contained (include all needed functions)
- Use `(match ...)` and nested `(let ...)` for control flow
- All functions need `:sig`, `:intent`, `:requires [(valid ...)]`, `:ensures [(valid result)]`, `:body`
- Run tests with: `cargo run -- run bootstrap/<test-file>.airl`
- Capitalized names are variant constructors (e.g., `Ok`, `Err`, `TyI64`, `ASTDefType`)
- **Test files must define `assert-eq`** — it is not a builtin. Copy from existing test files (e.g., `bootstrap/integration_test.airl`):
  ```clojure
  (defn assert-eq :sig [(a : Any) (b : Any) -> Bool] :intent "Assert equality"
    :requires [(valid a) (valid b)] :ensures [(valid result)]
    :body (if (= a b) true (do (print "FAIL: expected" b "got" a) false)))
  ```
- **Test file growth pattern:** Each test file is regenerated as a complete self-contained unit, not appended to. When Task 4 adds function/match tests, the entire `typecheck_test.airl` is rewritten with all functions (lexer + parser + types + typecheck) and all tests (from Tasks 3 AND 4).

**Reference files:**
- Rust type checker: `crates/airl-types/src/checker.rs` (~715 lines)
- Rust type env: `crates/airl-types/src/env.rs` (~110 lines)
- Rust type representation: `crates/airl-types/src/ty.rs` (~182 lines)
- Rust deftype parser: `crates/airl-syntax/src/parser.rs:685-765`
- Bootstrap parser: `bootstrap/parser.airl` (~744 lines)
- Bootstrap eval: `bootstrap/eval.airl` (~616 lines)
- Bootstrap lexer: `bootstrap/lexer.airl` (~362 lines)
- LLM Guide: `AIRL-LLM-Guide.md` — **MUST read before writing AIRL code**

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `bootstrap/types.airl` | Type variant constructors, type environment (scoped map stack), type registry (constructor → field types), resolve-type-name, types-compatible |
| Create: `bootstrap/typecheck.airl` | check-expr, check-fn, check-pattern, check-top-level, type-check-program, builtin registration |
| Create: `bootstrap/typecheck_test.airl` | Self-contained test file (includes lexer + parser + types + typecheck functions + test assertions) |
| Create: `bootstrap/deftype_test.airl` | Self-contained test file for deftype parsing (includes lexer + parser functions + test assertions) |
| Modify: `bootstrap/parser.airl` | Add `deftype` parsing: parse-deftype, parse-type-params, parse-sum-body, parse-product-body, parse-variant, parse-field, plus dispatch in parse-top-level |
| Modify: `bootstrap/lexer.airl` | Add `deftype Token` declaration at top, fix 1 `Any` occurrence |
| Modify: `bootstrap/parser.airl` | Fix ~24 `Any` annotations to proper types |
| Modify: `bootstrap/eval.airl` | Fix ~83 `Any` annotations to proper types |

---

### Task 1: Add `deftype` Parsing to Bootstrap Parser

**Files:**
- Modify: `bootstrap/parser.airl` (add ~100 lines at end, before `parse-top-level`)
- Modify: `bootstrap/parser.airl:688-708` (add `"deftype"` dispatch in `parse-top-level`)
- Create: `bootstrap/deftype_test.airl` (test file, ~200 lines)

This task adds `deftype` top-level form parsing to the bootstrap parser. The implementation must match the Rust parser's `parse_deftype` at `crates/airl-syntax/src/parser.rs:685-765`.

- [ ] **Step 1: Write deftype test file with test cases**

Create `bootstrap/deftype_test.airl` with lexer + parser functions included, plus these tests:

```clojure
;; Test 1: Simple sum type
(let (r : List (parse "(deftype Color (| (Red) (Green) (Blue)))"))
  (match r
    (Ok nodes) (match (head nodes)
      (ASTDefType name params body _ _)
        (do
          (assert-eq name "Color")
          (assert-eq (length params) 0)
          (match body
            (ASTSumBody variants) (assert-eq (length variants) 3)
            _ (print "FAIL: expected ASTSumBody")))
      _ (print "FAIL: expected ASTDefType"))
    (Err e) (print "FAIL:" e)))

;; Test 2: Sum type with type params
(let (r : List (parse "(deftype Result [T E] (| (Ok T) (Err E)))"))
  (match r
    (Ok nodes) (match (head nodes)
      (ASTDefType name params body _ _)
        (do
          (assert-eq name "Result")
          (assert-eq (length params) 2))
      _ (print "FAIL"))
    (Err e) (print "FAIL:" e)))

;; Test 3: Product type
(let (r : List (parse "(deftype Point (& (x : i64) (y : i64)))"))
  (match r
    (Ok nodes) (match (head nodes)
      (ASTDefType name params body _ _)
        (do
          (assert-eq name "Point")
          (match body
            (ASTProductBody fields) (assert-eq (length fields) 2)
            _ (print "FAIL: expected ASTProductBody")))
      _ (print "FAIL"))
    (Err e) (print "FAIL:" e)))

;; Test 4: Sum with positional fields
(let (r : List (parse "(deftype SExpr (| (SList List i64 i64) (SAtom Token)))"))
  (match r
    (Ok nodes) (match (head nodes)
      (ASTDefType name params body _ _)
        (match body
          (ASTSumBody variants)
            (do
              (assert-eq (length variants) 2)
              (match (head variants)
                (ASTVariantDef vname field-types)
                  (do
                    (assert-eq vname "SList")
                    (assert-eq (length field-types) 3))
                _ (print "FAIL")))
          _ (print "FAIL"))
      _ (print "FAIL"))
    (Err e) (print "FAIL:" e)))

(print "=== deftype tests complete ===")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo run -- run bootstrap/deftype_test.airl`
Expected: FAIL — `parse-deftype` not defined, or `"deftype"` not dispatched

- [ ] **Step 3: Add `parse-variant` and `parse-field` helper functions**

Add to `bootstrap/parser.airl` before `parse-top-level`:

```clojure
;; ── deftype parsing ──────────────────────────────

(defn parse-variant
  :sig [(sexpr : List) -> List]
  :intent "Parse a sum variant: (Name Type1 Type2 ...) → (ASTVariantDef name field-type-names)"
  :requires [(valid sexpr)]
  :ensures [(valid result)]
  :body
    (match sexpr
      (SList items line col)
        (if (empty? items)
          (Err (ParseError "empty variant" line col))
          (match (head items)
            (SAtom tok)
              (match tok
                (Token kind value _ _)
                  (if (= kind "symbol")
                    ;; Collect remaining items as field type names
                    (Ok (ASTVariantDef value (tail items)))
                    (Err (ParseError "variant name must be a symbol" line col))))
            _ (Err (ParseError "variant name must be a symbol" line col))))
      _ (Err (ParseError "variant must be a list" 0 0))))

(defn parse-field
  :sig [(sexpr : List) -> List]
  :intent "Parse a product field: (name : Type) → (ASTFieldDef name type-name)"
  :requires [(valid sexpr)]
  :ensures [(valid result)]
  :body
    (match sexpr
      (SList items line col)
        (if (< (length items) 3)
          (Err (ParseError "field requires (name : Type)" line col))
          (match (at items 0)
            (SAtom tok0)
              (match tok0
                (Token k0 name _ _)
                  (if (= k0 "symbol")
                    (match (at items 2)
                      (SAtom tok2)
                        (match tok2
                          (Token k2 type-name _ _)
                            (if (= k2 "symbol")
                              (Ok (ASTFieldDef name type-name))
                              (Err (ParseError "field type must be a symbol" line col))))
                      _ (Err (ParseError "field type must be a symbol" line col)))
                    (Err (ParseError "field name must be a symbol" line col))))
            _ (Err (ParseError "field name must be a symbol" line col))))
      _ (Err (ParseError "field must be a list" 0 0))))
```

- [ ] **Step 4: Add `parse-sum-body` and `parse-product-body`**

```clojure
(defn parse-variants-acc
  :sig [(items : List) (pos : i64) (acc : List) -> List]
  :intent "Accumulate parsed variants from items list"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length items))
      (Ok (reverse acc))
      (match (parse-variant (at items pos))
        (Ok v) (parse-variants-acc items (+ pos 1) (cons v acc))
        (Err e) (Err e))))

(defn parse-sum-body
  :sig [(items : List) (line : i64) (col : i64) -> List]
  :intent "Parse sum type body: (| (Variant1 ...) (Variant2 ...)) → (ASTSumBody variants)"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    ;; items[0] is the "|" symbol, variants start at index 1
    (match (parse-variants-acc items 1 [])
      (Ok variants) (Ok (ASTSumBody variants))
      (Err e) (Err e)))

(defn parse-fields-acc
  :sig [(items : List) (pos : i64) (acc : List) -> List]
  :intent "Accumulate parsed fields from items list"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length items))
      (Ok (reverse acc))
      (match (parse-field (at items pos))
        (Ok f) (parse-fields-acc items (+ pos 1) (cons f acc))
        (Err e) (Err e))))

(defn parse-product-body
  :sig [(items : List) (line : i64) (col : i64) -> List]
  :intent "Parse product type body: (& (f1 : T1) (f2 : T2)) → (ASTProductBody fields)"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    ;; items[0] is the "&" symbol, fields start at index 1
    (match (parse-fields-acc items 1 [])
      (Ok fields) (Ok (ASTProductBody fields))
      (Err e) (Err e)))
```

- [ ] **Step 5: Add `parse-type-params` and `parse-deftype`**

```clojure
(defn parse-type-params-acc
  :sig [(items : List) (pos : i64) (acc : List) -> List]
  :intent "Parse type parameters from bracket items: [T E] or [T : Type, E : Type]"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length items))
      (Ok (reverse acc))
      (match (at items pos)
        (SAtom tok)
          (match tok
            (Token kind value _ _)
              (if (= kind "symbol")
                ;; Check if next item is ":" for bounded param
                (if (< (+ pos 1) (length items))
                  (match (at items (+ pos 1))
                    (SAtom colon-tok)
                      (match colon-tok
                        (Token ck cv _ _)
                          (if (= cv ":")
                            ;; Bounded: name : Bound
                            (if (< (+ pos 2) (length items))
                              (match (at items (+ pos 2))
                                (SAtom bound-tok)
                                  (match bound-tok
                                    (Token bk bv _ _)
                                      (parse-type-params-acc items (+ pos 3)
                                        (cons (ASTTypeParam value bv) acc)))
                                _ (Err (ParseError "expected type bound" 0 0)))
                              (Err (ParseError "expected type bound after :" 0 0)))
                            ;; Not a colon — unbounded param
                            (parse-type-params-acc items (+ pos 1)
                              (cons (ASTTypeParam value "Type") acc))))
                    _ (parse-type-params-acc items (+ pos 1)
                        (cons (ASTTypeParam value "Type") acc)))
                  ;; Last item — unbounded param
                  (parse-type-params-acc items (+ pos 1)
                    (cons (ASTTypeParam value "Type") acc)))
                (Err (ParseError "type param must be a symbol" 0 0))))
        _ (Err (ParseError "type param must be an atom" 0 0)))))

(defn parse-deftype
  :sig [(items : List) (line : i64) (col : i64) -> List]
  :intent "Parse (deftype Name [Params] Body) → (ASTDefType ...)"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    (if (empty? items)
      (Err (ParseError "deftype requires a name" line col))
      (match (head items)
        (SAtom tok)
          (match tok
            (Token kind name _ _)
              (if (= kind "symbol")
                ;; Parse optional type params and body
                (let (rest : List (tail items))
                  (if (empty? rest)
                    (Err (ParseError "deftype requires a body" line col))
                    (match (head rest)
                      ;; Check if first item after name is bracket (type params)
                      (SBracket bp-items bp-line bp-col)
                        (if (< (length rest) 2)
                          (Err (ParseError "deftype requires a body after type params" line col))
                          (match (parse-type-params-acc bp-items 0 [])
                            (Ok params)
                              ;; Body is next item after bracket
                              (match (at rest 1)
                                (SList body-items bl bc)
                                  (if (empty? body-items)
                                    (Err (ParseError "empty deftype body" bl bc))
                                    (match (head body-items)
                                      (SAtom head-tok)
                                        (match head-tok
                                          (Token hk hv _ _)
                                            (if (= hv "|")
                                              (match (parse-sum-body body-items bl bc)
                                                (Ok body) (Ok (ASTDefType name params body line col))
                                                (Err e) (Err e))
                                              (if (= hv "&")
                                                (match (parse-product-body body-items bl bc)
                                                  (Ok body) (Ok (ASTDefType name params body line col))
                                                  (Err e) (Err e))
                                                (Err (ParseError "deftype body must start with | or &" bl bc)))))
                                      _ (Err (ParseError "deftype body must start with | or &" bl bc))))
                                _ (Err (ParseError "deftype body must be a list" line col)))
                            (Err e) (Err e)))
                      ;; No type params — body is first item after name
                      (SList body-items bl bc)
                        (if (empty? body-items)
                          (Err (ParseError "empty deftype body" bl bc))
                          (match (head body-items)
                            (SAtom head-tok)
                              (match head-tok
                                (Token hk hv _ _)
                                  (if (= hv "|")
                                    (match (parse-sum-body body-items bl bc)
                                      (Ok body) (Ok (ASTDefType name [] body line col))
                                      (Err e) (Err e))
                                    (if (= hv "&")
                                      (match (parse-product-body body-items bl bc)
                                        (Ok body) (Ok (ASTDefType name [] body line col))
                                        (Err e) (Err e))
                                      (Err (ParseError "deftype body must start with | or &" bl bc)))))
                            _ (Err (ParseError "deftype body must start with | or &" bl bc))))
                      _ (Err (ParseError "deftype requires type params or body" line col)))))
                (Err (ParseError "deftype name must be a symbol" line col))))
        _ (Err (ParseError "deftype name must be a symbol" line col)))))
```

- [ ] **Step 6: Add `"deftype"` dispatch to `parse-top-level`**

In `bootstrap/parser.airl`, modify `parse-top-level` to add the deftype case. Change:

```clojure
                    (if (= value "defn")
                      (parse-defn items line col)
                      (parse-expr sexpr))
```

To:

```clojure
                    (if (= value "defn")
                      (parse-defn items line col)
                      (if (= value "deftype")
                        (parse-deftype (tail items) line col)
                        (parse-expr sexpr)))
```

- [ ] **Step 7: Run deftype tests**

Run: `cargo run -- run bootstrap/deftype_test.airl`
Expected: All tests pass, "deftype tests complete"

- [ ] **Step 8: Run existing parser tests to verify no regressions**

Run: `cargo run -- run bootstrap/parser_test.airl`
Expected: All existing tests still pass

Run: `cargo run -- run bootstrap/integration_test.airl`
Expected: All existing tests still pass

- [ ] **Step 9: Run full Rust test suite**

Run: `RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir`
Expected: All 461+ tests pass

- [ ] **Step 10: Commit**

```bash
git add bootstrap/parser.airl bootstrap/deftype_test.airl
git commit -m "feat(bootstrap): add deftype parsing to self-hosted parser

Support sum types (| ...) and product types (& ...) with optional
type parameters. Matches Rust parser's parse_deftype syntax."
```

---

### Task 2: Type Representation and Environment

**Files:**
- Create: `bootstrap/types.airl` (~200 lines)
- Create: `bootstrap/types_test.airl` (~150 lines)

Build the foundational type data structures: type variants, type environment (scoped map stack), type registry (constructor map), resolve-type-name, and types-compatible.

- [ ] **Step 1: Write types_test.airl with initial tests**

Create `bootstrap/types_test.airl` — must be self-contained (no imports). Include the type functions inline, then test:

```clojure
;; Test resolve-type-name
(assert-eq (resolve-type-name "i64" (map-new)) (Ok (TyI64)))
(assert-eq (resolve-type-name "String" (map-new)) (Ok (TyStr)))
(assert-eq (resolve-type-name "Str" (map-new)) (Ok (TyStr)))
(assert-eq (resolve-type-name "Bool" (map-new)) (Ok (TyBool)))
(assert-eq (resolve-type-name "bool" (map-new)) (Ok (TyBool)))
(assert-eq (resolve-type-name "Unit" (map-new)) (Ok (TyUnit)))
(match (resolve-type-name "Any" (map-new))
  (Err _) (print "PASS: Any rejected")
  _ (print "FAIL: Any should be rejected"))
(match (resolve-type-name "Nonexistent" (map-new))
  (Err _) (print "PASS: unknown type rejected")
  _ (print "FAIL"))

;; Test type environment
(let (env : List (type-env-new))
  (let (env2 : List (type-env-bind env "x" (TyI64)))
    (do
      (assert-eq (type-env-lookup env2 "x") (Ok (TyI64)))
      (match (type-env-lookup env2 "y")
        (Err _) (print "PASS: undefined lookup")
        _ (print "FAIL")))))

;; Test scoping
(let (env : List (type-env-new))
  (let (env2 : List (type-env-bind env "x" (TyI64)))
    (let (env3 : List (type-env-push env2))
      (let (env4 : List (type-env-bind env3 "x" (TyStr)))
        (do
          (assert-eq (type-env-lookup env4 "x") (Ok (TyStr)))
          (let (env5 : List (type-env-pop env4))
            (assert-eq (type-env-lookup env5 "x") (Ok (TyI64)))))))))

;; Test types-compatible
(assert-eq (types-compatible (TyI64) (TyI64)) true)
(assert-eq (types-compatible (TyI64) (TyStr)) false)
(assert-eq (types-compatible (TyVar "x") (TyI64)) true)  ;; TyVar matches anything
(assert-eq (types-compatible (TyI64) (TyVar "_")) true)
(assert-eq (types-compatible (TyNever) (TyStr)) true)     ;; Never is bottom
(assert-eq (types-compatible (TyI32) (TyI64)) true)       ;; int↔int coercion
(assert-eq (types-compatible (TyF32) (TyF64)) true)       ;; float↔float
(assert-eq (types-compatible (TyI64) (TyF64)) false)      ;; no int↔float

;; Test type registry
(let (reg : List (registry-new))
  (let (reg2 : List (registry-add-ctor reg "Ok" (CtorInfo "Result" [(TyVar "T")])))
    (do
      (assert-eq (registry-has-ctor reg2 "Ok") true)
      (assert-eq (registry-has-ctor reg2 "Foo") false)
      (match (registry-get-ctor reg2 "Ok")
        (Ok info) (match info
          (CtorInfo parent fields)
            (do (assert-eq parent "Result")
                (assert-eq (length fields) 1)))
          _ (print "FAIL"))
        (Err _) (print "FAIL: Ok should be in registry")))))

(print "=== types tests complete ===")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo run -- run bootstrap/types_test.airl`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement `bootstrap/types.airl`**

Create `bootstrap/types.airl` with:

1. **resolve-type-name** — match on name string, return `(Ok TyXxx)` or `(Err ...)` for unknown types. Fall back to registry lookup for non-primitive names.

2. **Type environment** — `type-env-new`, `type-env-push`, `type-env-pop`, `type-env-bind`, `type-env-lookup` using list-of-maps pattern (same as eval.airl's env).

3. **Type registry** — `registry-new`, `registry-add-ctor`, `registry-get-ctor`, `registry-has-ctor`, `registry-add-type`. Two maps: type-name → TypeDef, ctor-name → CtorInfo.

4. **types-compatible** — structural comparison with TyVar/TyNever wildcards and numeric coercion.

- [ ] **Step 4: Run types tests**

Run: `cargo run -- run bootstrap/types_test.airl`
Expected: All pass, "types tests complete"

- [ ] **Step 5: Commit**

```bash
git add bootstrap/types.airl bootstrap/types_test.airl
git commit -m "feat(bootstrap): add type representation, environment, and registry"
```

---

### Task 3: Core Expression Type Checking

**Files:**
- Create: `bootstrap/typecheck.airl` (~300 lines initial)
- Create: `bootstrap/typecheck_test.airl` (~300 lines initial)

Implement check-expr for literals, symbols, if, let, do, function calls. Register typed builtins.

- [ ] **Step 1: Write typecheck_test.airl with expression tests**

Self-contained file including lexer + parser + types + typecheck functions. Tests:

```clojure
;; Helper: parse and type-check a single expression
(defn check-source
  :sig [(source : String) -> List]
  :intent "Parse source, type-check, return type or error"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (parse source)
      (Err e) (Err e)
      (Ok nodes)
        (if (empty? nodes)
          (Err "no expressions")
          (let (env : List (make-builtin-env))
            (let (reg : List (registry-new))
              (check-expr (head nodes) env reg))))))

;; Literals
(assert-eq (check-source "42") (Ok (TyI64)))
(assert-eq (check-source "3.14") (Ok (TyF64)))
(assert-eq (check-source "true") (Ok (TyBool)))
(assert-eq (check-source "\"hello\"") (Ok (TyStr)))
(assert-eq (check-source "nil") (Ok (TyUnit)))

;; Arithmetic
(assert-eq (check-source "(+ 1 2)") (Ok (TyI64)))
(match (check-source "(+ 1 \"hi\")")
  (Err _) (print "PASS: type mismatch")
  _ (print "FAIL"))

;; Comparison
(assert-eq (check-source "(< 1 2)") (Ok (TyBool)))

;; If
(assert-eq (check-source "(if true 1 2)") (Ok (TyI64)))
(match (check-source "(if 42 1 2)")
  (Err _) (print "PASS: if cond must be bool")
  _ (print "FAIL"))
(match (check-source "(if true 1 \"hi\")")
  (Err _) (print "PASS: if branches must agree")
  _ (print "FAIL"))

;; Let
(assert-eq (check-source "(let (x : i64 42) x)") (Ok (TyI64)))
(match (check-source "(let (x : i64 \"hi\") x)")
  (Err _) (print "PASS: let type mismatch")
  _ (print "FAIL"))

;; Do
(assert-eq (check-source "(do 1 2 3)") (Ok (TyI64)))
(assert-eq (check-source "(do 1 true)") (Ok (TyBool)))

;; Undefined symbol
(match (check-source "undefined_var")
  (Err _) (print "PASS: undefined symbol")
  _ (print "FAIL"))

(print "=== typecheck expression tests complete ===")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo run -- run bootstrap/typecheck_test.airl`
Expected: FAIL — check-expr not defined

- [ ] **Step 3: Implement `bootstrap/typecheck.airl` with check-expr and builtins**

Implement:

1. **make-builtin-env** — creates a type environment pre-populated with all typed builtins (arithmetic, comparison, boolean ops, string ops, collection ops, file I/O).

2. **check-expr** — dispatches on AST node variant:
   - `ASTInt` → `(Ok (TyI64))`
   - `ASTFloat` → `(Ok (TyF64))`
   - `ASTStr` → `(Ok (TyStr))`
   - `ASTBool` → `(Ok (TyBool))`
   - `ASTNil` → `(Ok (TyUnit))`
   - `ASTKeyword` → `(Ok (TyStr))`
   - `ASTSymbol name` → `type-env-lookup env name`
   - `ASTIf cond then else` → check cond is TyBool, check branches agree
   - `ASTLet bindings body` → push scope, bind each, check body
   - `ASTDo exprs` → check all, return last type
   - `ASTCall callee args` → check callee is TyFunc, check arg count/types
   - `ASTList items` → check all same type, return TyList

- [ ] **Step 4: Run typecheck tests**

Run: `cargo run -- run bootstrap/typecheck_test.airl`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add bootstrap/typecheck.airl bootstrap/typecheck_test.airl
git commit -m "feat(bootstrap): add core expression type checking

Implements check-expr for literals, symbols, if, let, do, function
calls, and list literals. Registers typed builtins for arithmetic,
comparison, boolean, string, collection, and file I/O operations."
```

---

### Task 4: Function, Match, Lambda, and Variant Checking

**Files:**
- Modify: `bootstrap/typecheck.airl` (add ~200 lines)
- Modify: `bootstrap/typecheck_test.airl` (add ~200 lines)

Extend the type checker with check-fn, check-pattern, match arms, lambda, variant constructors, and try.

- [ ] **Step 1: Add tests for functions, match, lambda, variants**

Add to `bootstrap/typecheck_test.airl`:

```clojure
;; Function definition and call
;; (Need to check a defn then call it)
(let (source : String "(defn add :sig [(a : i64) (b : i64) -> i64] :requires [(valid a)] :ensures [(valid result)] :body (+ a b))")
  ;; ... parse, register, then check (add 1 2) returns TyI64

;; Match
(assert-eq (check-source "(match 42 x x)") (Ok (TyI64)))

;; Lambda
(assert-eq (check-source "(fn [(x : i64)] (+ x 1))") (Ok (TyFunc [(TyI64)] (TyI64))))

;; Variant constructor (after registry setup)
;; ... test that (Ok 42) type-checks when Ok is registered

;; Try expression
;; ... test that (try expr) unwraps Result type
```

- [ ] **Step 2: Implement check-fn, check-pattern, check-match, check-lambda, check-variant, check-try**

Add to `bootstrap/typecheck.airl`:

- **check-fn** — push scope, bind params, resolve return type, check body, verify return type matches, bind function in outer env
- **check-pattern** — dispatch on PatWild/PatBind/PatLit/PatVariant, bind variables with appropriate types
- **check-match** — check scrutinee, for each arm push scope + check pattern + check body, verify all arm types agree
- **check-lambda** — push scope, bind params, check body, return TyFunc
- **check-variant** — lookup in constructor registry, check field types
- **check-try** — check inner, if Result unwrap Ok type
- **check-top-level** — dispatch on ASTDefn (call check-fn), ASTDefType (register in registry), expression (call check-expr)
- **make-prelude-registry** — creates a registry pre-populated with standard types: `(deftype Result [T E] (| (Ok T) (Err E)))` and `(deftype Option [T] (| (Some T) (None)))`. Registers constructors: Ok → CtorInfo("Result", [TyVar "T"]), Err → CtorInfo("Result", [TyVar "E"]), Some → CtorInfo("Option", [TyVar "T"]), None → CtorInfo("Option", [])
- **type-check-program** — two passes: registration then checking. Takes optional `prelude-types` list of ASTDefType nodes to pre-register (for cross-module type declarations). Merges prelude registry with user-declared types before checking.

- [ ] **Step 3: Run all typecheck tests**

Run: `cargo run -- run bootstrap/typecheck_test.airl`
Expected: All pass

- [ ] **Step 4: Run full Rust test suite for regressions**

Run: `RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add bootstrap/typecheck.airl bootstrap/typecheck_test.airl
git commit -m "feat(bootstrap): add function, match, lambda, and variant type checking

Implements check-fn, check-pattern, check-match, check-lambda,
check-variant, check-try, check-top-level, and type-check-program.
Full two-pass type checking: registration then verification."
```

---

### Task 5: Type-Check the Lexer (First Integration Target)

**Files:**
- Modify: `bootstrap/lexer.airl` (add deftype at top, fix 1 `Any`, ~5 lines)
- Modify: `bootstrap/typecheck_test.airl` (add integration test)

The lexer has 0 functions with `Any` in signatures — it's the easiest integration target.

- [ ] **Step 1: Add `deftype Token` to `bootstrap/lexer.airl`**

Add at the top of the file, after the header comment:

```clojure
(deftype Token
  (& (kind : String) (value : String) (line : i64) (col : i64)))
```

- [ ] **Step 2: Fix the 1 `Any` occurrence in lexer.airl**

Find and replace the `Any` annotation with the proper type.

- [ ] **Step 3: Add lexer integration test to typecheck_test.airl**

```clojure
;; Integration: type-check the lexer source
(let (lexer-source : String (read-file "bootstrap/lexer.airl"))
  (match (parse lexer-source)
    (Err e) (print "FAIL: parse error:" e)
    (Ok nodes)
      (match (type-check-program nodes [])
        (Ok _) (print "PASS: lexer type-checks clean")
        (Err errors) (do
          (print "FAIL: lexer has type errors:")
          (print errors)))))
```

- [ ] **Step 4: Run integration test**

Run: `cargo run -- run bootstrap/typecheck_test.airl`
Expected: "PASS: lexer type-checks clean"

If there are errors, fix them iteratively — the lexer should be very close to passing already.

- [ ] **Step 5: Run existing lexer tests for regressions**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: All existing tests still pass

- [ ] **Step 6: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/typecheck_test.airl
git commit -m "feat(bootstrap): lexer passes self-hosted type checking

Add deftype Token declaration, fix Any annotation. All 17 lexer
functions type-check cleanly with the bootstrap type checker."
```

---

### Task 6: Fix Parser Type Annotations and Type-Check

**Files:**
- Modify: `bootstrap/parser.airl` (fix ~24 `Any` annotations, add deftype declarations)
- Modify: `bootstrap/typecheck_test.airl` (add parser integration test)

- [ ] **Step 1: Add `deftype` declarations to parser.airl**

Add S-expr and AST type declarations at the top of `bootstrap/parser.airl`:

```clojure
;; S-expression types
(deftype SExpr
  (| (SList List i64 i64)
     (SBracket List i64 i64)
     (SAtom Token)))

(deftype ParseError
  (& (msg : String) (line : i64) (col : i64)))

;; AST node types (sum type for all expression/top-level forms)
;; ... ASTInt, ASTFloat, ASTStr, ASTBool, ASTNil, ASTKeyword, ASTSymbol,
;;     ASTIf, ASTLet, ASTDo, ASTMatch, ASTLambda, ASTCall, ASTList,
;;     ASTVariant, ASTTry, ASTDefn, ASTDefType, etc.
```

- [ ] **Step 2: Replace `Any` annotations in parser function signatures**

Change all 9 functions with `Any` params:
- `token-line`, `token-col`, `token-kind`, `token-value` — `(tok : Any)` → `(tok : Token)`
- `parse-atom`, `parse-expr`, `parse-let-binding`, `parse-pattern`, `parse-param`, `parse-sig`, `parse-top-level` — `(sexpr : Any)` → `(sexpr : SExpr)`
- `walk-defn-clauses` — `(sig : Any) (body : Any)` → proper types

- [ ] **Step 3: Replace `Any` in let bindings**

Fix ~14 let bindings with `Any` type annotations → proper types based on what the values actually are.

- [ ] **Step 4: Run existing parser tests**

Run: `cargo run -- run bootstrap/parser_test.airl`
Run: `cargo run -- run bootstrap/integration_test.airl`
Expected: All existing tests still pass (annotation changes don't affect runtime behavior)

- [ ] **Step 5: Add parser integration type-check test**

```clojure
(let (parser-source : String (read-file "bootstrap/parser.airl"))
  (let (lexer-types : List (get-lexer-type-declarations))
    (match (parse parser-source)
      (Err e) (print "FAIL: parse error:" e)
      (Ok nodes)
        (match (type-check-program nodes lexer-types)
          (Ok _) (print "PASS: parser type-checks clean")
          (Err errors) (do (print "FAIL:" errors))))))
```

- [ ] **Step 6: Fix any remaining type errors iteratively**

Run the test, examine errors, fix annotations, repeat until clean.

- [ ] **Step 7: Run full test suite**

Run: `RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir`
Run: `cargo run -- run bootstrap/pipeline_test.airl`
Expected: All pass

- [ ] **Step 8: Commit**

```bash
git add bootstrap/parser.airl bootstrap/typecheck_test.airl
git commit -m "feat(bootstrap): parser passes self-hosted type checking

Add deftype declarations for SExpr, ParseError, and all AST nodes.
Replace 24 Any annotations with proper types. Parser type-checks
cleanly with bootstrap type checker."
```

---

### Task 7: Fix Eval Type Annotations and Type-Check

**Files:**
- Modify: `bootstrap/eval.airl` (fix ~83 `Any` annotations, add deftype declarations)
- Modify: `bootstrap/typecheck_test.airl` (add eval integration test)

This is the largest annotation task. The eval module has 83 `Any` occurrences across 24 functions and 65+ let bindings.

- [ ] **Step 1: Add `deftype` declarations to eval.airl**

```clojure
;; Value types (tagged wrappers for the evaluator)
(deftype Val
  (| (ValInt i64)
     (ValFloat f64)
     (ValStr String)
     (ValBool Bool)
     (ValNil)
     (ValList List)
     (ValMap List)
     (ValVariant String List)
     (ValFn String List List List)
     (ValLambda List List List)
     (ValBuiltin String)))

;; Pattern types
(deftype Pat
  (| (PatWild i64 i64)
     (PatBind String i64 i64)
     (PatLit String i64 i64)
     (PatVariant String List i64 i64)))
```

- [ ] **Step 2: Fix value unwrapper signatures (6 functions)**

Change `(v : Any)` → `(v : Val)` in: `unwrap-int`, `unwrap-float`, `unwrap-str`, `unwrap-bool`, `unwrap-list`, `unwrap-raw`

- [ ] **Step 3: Fix environment function signatures**

Change `Any` → proper types in: `env-bind` (val param), `env-get` (return), `env-new` and `make-initial-env` (dummy `_u` → `Unit`)

- [ ] **Step 4: Fix eval function signatures**

Change `(node : Any)` → `(node : ASTNode)` and return `Any` → `Val` in:
`eval-node`, `eval-top-level`, `eval-program`, `eval-args`, `eval-args-acc`, `eval-let-bindings`, `eval-do`, `eval-match-arms`, `eval-list-items`, `try-match-pattern`, `try-match-patterns`, `call-builtin`, `extract-param-names`, `run-source`, `run-file`

- [ ] **Step 5: Fix ~60+ let binding annotations**

Replace `Any` with proper types in all let bindings throughout eval.airl.

- [ ] **Step 6: Run existing eval tests**

Run: `cargo run -- run bootstrap/eval_test.airl`
Run: `cargo run -- run bootstrap/pipeline_test.airl`
Expected: All existing tests still pass

- [ ] **Step 7: Add eval integration type-check test**

- [ ] **Step 8: Fix remaining type errors iteratively**

- [ ] **Step 9: Run full test suite**

Run: `RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir`
Expected: All pass

- [ ] **Step 10: Commit**

```bash
git add bootstrap/eval.airl bootstrap/typecheck_test.airl
git commit -m "feat(bootstrap): eval passes self-hosted type checking

Add deftype declarations for Val and Pat types. Replace 83 Any
annotations with proper types across 24 functions and 65+ let
bindings. Eval type-checks cleanly with bootstrap type checker."
```

---

### Task 8: Update CLAUDE.md and Final Verification

**Files:**
- Modify: `CLAUDE.md` (update Completed Tasks, Bootstrap Compiler sections)
- Run all tests end-to-end

- [ ] **Step 1: Run complete test suite**

```bash
RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir
cargo run -- run bootstrap/lexer_test.airl
cargo run -- run bootstrap/parser_test.airl
cargo run -- run bootstrap/integration_test.airl
cargo run -- run bootstrap/eval_test.airl
cargo run -- run bootstrap/pipeline_test.airl
cargo run -- run bootstrap/deftype_test.airl
cargo run -- run bootstrap/types_test.airl
cargo run -- run bootstrap/typecheck_test.airl
```

Expected: All pass

- [ ] **Step 2: Update CLAUDE.md**

Add to Completed Tasks:
- **Bootstrap Type Checker** — Self-hosted type checker in AIRL (`bootstrap/types.airl`, `bootstrap/typecheck.airl`). Two-pass architecture: registration (deftype → constructor registry) then checking (expressions, functions, patterns). Eliminates all `Any` usage from bootstrap code. Type-checks lexer, parser, and eval cleanly.
- **`deftype` Parsing** — Bootstrap parser handles `(deftype Name [Params] (| ...))` sum types and `(deftype Name (& ...))` product types.

Update Bootstrap Compiler section with new files and test commands.

Update Remaining Tasks — Self-Hosting status.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with bootstrap type checker milestone"
```
