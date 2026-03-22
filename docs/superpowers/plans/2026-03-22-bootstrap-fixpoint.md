# Bootstrap Fixpoint Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the AIRL self-hosted compiler is correctly bootstrapped via functional equivalence and compiler fixpoint tests.

**Architecture:** Two test files. `equivalence_test.airl` compares interpreted eval vs compiled run-ir across ~20 programs. `fixpoint_test.airl` proves the compiled compiler produces identical IR to the interpreted compiler at three tiers of scale (small program, compiler itself, full chain).

**Tech Stack:** Pure AIRL (no new Rust code). Uses existing builtins: `run-ir`, `read-file`. Test files are self-contained (concatenated lexer + parser + eval + compiler).

**Spec:** `docs/superpowers/specs/2026-03-22-bootstrap-fixpoint-design.md`

**AIRL Constraints (apply to ALL AIRL tasks):**
- `and`/`or` are **eager** — use nested `if` for short-circuit logic
- No import system — test files must be self-contained
- All functions need `:sig`, `:requires [(valid ...)]` or `:ensures [(valid result)]`, `:body`
- Capitalized names are variant constructors
- Lambda params have no type annotations
- `join` signature is `(join list separator)`
- Run AIRL tests with: `cargo run -- run bootstrap/<test>.airl`
- **MUST read `AIRL-LLM-Guide.md` and all `stdlib/*.md` before writing AIRL code**

**Reference files:**
- Bootstrap lexer: `bootstrap/lexer.airl` (~364 lines)
- Bootstrap parser: `bootstrap/parser.airl` (~930 lines)
- Bootstrap evaluator: `bootstrap/eval.airl` (~616 lines)
- Bootstrap compiler: `bootstrap/compiler.airl` (~220 lines)
- Existing compiler tests: `bootstrap/compiler_test.airl` (pattern to follow)
- Existing pipeline tests: `bootstrap/pipeline_test.airl` (pattern for eval + compile comparison)

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `bootstrap/equivalence_test.airl` | Functional equivalence: interpreted eval vs compiled run-ir |
| Create: `bootstrap/fixpoint_test.airl` | Compiler self-compilation fixpoint proof (3 tiers) |
| Create: `bootstrap/fixpoint_tier1_program.airl` | Small test program for Tier 1 fixpoint (avoids string escaping) |

---

### Task 1: Functional Equivalence Test — Setup and Helpers

**Files:**
- Create: `bootstrap/equivalence_test.airl`

Build the test file by concatenating source, then append helpers and tests.

- [ ] **Step 1: Create the base file by concatenating dependencies**

```bash
cd /mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AIRL
cat bootstrap/lexer.airl bootstrap/parser.airl bootstrap/eval.airl bootstrap/compiler.airl > bootstrap/equivalence_test.airl
```

- [ ] **Step 2: Append test infrastructure**

Append to `bootstrap/equivalence_test.airl`:

```clojure
;; ── Equivalence Test Infrastructure ────────────────────────

(defn assert-eq
  :sig [(actual : List) (expected : List) (name : String) -> List]
  :requires [(valid actual)]
  :ensures [(valid result)]
  :body (if (= actual expected)
    (do (print "PASS:" name) true)
    (do (print "FAIL:" name "expected" expected "got" actual) false)))

;; Unwrap bootstrap eval ValXxx variants to raw values for comparison
(defn unwrap-val
  :sig [(v : List) -> List]
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValInt x)   x
    (ValFloat x) x
    (ValStr x)   x
    (ValBool x)  x
    (ValNil)     nil
    (ValList xs) (map (fn [item] (unwrap-val item)) xs)
    (ValVariant name inner) (match inner
      (ValNil) (Variant name nil)
      (ValList items) (Variant name (map (fn [item] (unwrap-val item)) items))
      _ (Variant name (unwrap-val inner)))
    _            v))

;; Run source through the bootstrap interpreted eval, return unwrapped value
(defn eval-interpreted
  :sig [(source : String) -> List]
  :intent "Evaluate source via bootstrap eval-program, return (Ok raw-value)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (run-source source)
      (Err e) (Err e)
      (Ok pair) (Ok (unwrap-val (at pair 0)))))

;; Run source through the compiled pipeline, return value
(defn eval-compiled
  :sig [(source : String) -> List]
  :intent "Compile and run source via compile-program + run-ir"
  :requires [(valid source)]
  :ensures [(valid result)]
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
                  (Ok ir-nodes) (Ok (run-ir ir-nodes)))))))

;; Compare both paths for a given source string
(defn test-equiv
  :sig [(source : String) (name : String) -> List]
  :intent "Assert interpreted and compiled paths produce the same result"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (let (interp : List (eval-interpreted source))
      (let (compiled : List (eval-compiled source))
        (assert-eq interp compiled name))))
```

- [ ] **Step 3: Verify the file parses (no syntax errors)**

Run: `source "$HOME/.cargo/env" && cargo run -- check bootstrap/equivalence_test.airl 2>&1 | head -5`
Expected: No fatal parse errors (type checker warnings about undefined symbols are OK — stdlib functions load at runtime)

- [ ] **Step 4: Commit**

```bash
git add bootstrap/equivalence_test.airl
git commit -m "feat(bootstrap): equivalence test infrastructure with unwrap-val"
```

---

### Task 2: Functional Equivalence Test — Test Suite

**Files:**
- Modify: `bootstrap/equivalence_test.airl`

Append the full test suite (~20 test cases).

- [ ] **Step 1: Append test cases**

Append to `bootstrap/equivalence_test.airl`:

```clojure
;; ── Equivalence Tests ────────────────────────────────

;; Literals
(test-equiv "42" "int literal")
(test-equiv "3.14" "float literal")
(test-equiv "true" "bool true")
(test-equiv "false" "bool false")
(test-equiv "nil" "nil literal")

;; Arithmetic
(test-equiv "(+ 1 2)" "addition")
(test-equiv "(- 10 3)" "subtraction")
(test-equiv "(* 3 4)" "multiplication")
(test-equiv "(/ 10 2)" "division")
(test-equiv "(% 7 3)" "modulo")

;; Comparisons
(test-equiv "(< 1 2)" "less than")
(test-equiv "(= 5 5)" "equality")

;; Control flow
(test-equiv "(if true 1 2)" "if true")
(test-equiv "(if false 1 2)" "if false")
(test-equiv "(if (< 1 2) 10 20)" "if with comparison")

;; Let bindings
(test-equiv "(let (x : i64 10) (+ x 5))" "let binding")
(test-equiv "(let (x : i64 5) (let (y : i64 10) (+ x y)))" "nested let")

;; Do block
(test-equiv "(do 1 2 3)" "do block")

;; Functions
(test-equiv "(defn add1 :sig [(x : i64) -> i64] :requires [(valid x)] :ensures [(valid result)] :body (+ x 1)) (add1 99)" "defn and call")
(test-equiv "(defn add :sig [(a : i64) (b : i64) -> i64] :requires [(valid a)] :ensures [(valid result)] :body (+ a b)) (add 3 4)" "multi-arg function")

;; Recursion
(test-equiv "(defn fact :sig [(n : i64) -> i64] :requires [(valid n)] :ensures [(valid result)] :body (if (<= n 1) 1 (* n (fact (- n 1))))) (fact 5)" "factorial")

;; Match
(test-equiv "(match (Ok 42) (Ok v) v _ 0)" "match Ok")
(test-equiv "(match (Err 99) (Ok v) v (Err e) e _ 0)" "match Err")

;; Recursion — fibonacci
(test-equiv "(defn fib :sig [(n : i64) -> i64] :requires [(valid n)] :ensures [(valid result)] :body (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 10)" "fibonacci")

;; Lambda
(test-equiv "((fn [x] (+ x 1)) 10)" "lambda application")

;; Closure capture
(test-equiv "(let (offset : i64 100) ((fn [x] (+ x offset)) 5))" "closure capture")

;; Higher-order function
(test-equiv "(defn apply-twice :sig [(f : fn) (x : i64) -> i64] :requires [(valid f)] :ensures [(valid result)] :body (f (f x))) (apply-twice (fn [x] (+ x 1)) 10)" "higher-order function")

;; List operations
(test-equiv "(head [10 20 30])" "list head")
(test-equiv "(head (tail [10 20 30]))" "list tail")
(test-equiv "(head (cons 1 [2 3]))" "list cons")

;; Nested variant match
(test-equiv "(match (Ok (Ok 42)) (Ok (Ok v)) v _ 0)" "nested variant match")

;; Multiple definitions
(test-equiv "(defn double :sig [(x : i64) -> i64] :requires [(valid x)] :ensures [(valid result)] :body (* x 2)) (defn quad :sig [(x : i64) -> i64] :requires [(valid x)] :ensures [(valid result)] :body (double (double x))) (quad 3)" "multi-function")

(print "equivalence tests complete")
```

- [ ] **Step 2: Run the equivalence test**

Run: `source "$HOME/.cargo/env" && cargo run -- run bootstrap/equivalence_test.airl 2>&1 | grep -E "PASS|FAIL|complete"`
Expected: All PASS, "equivalence tests complete" at the end

If any tests FAIL, debug by examining the `eval-interpreted` vs `eval-compiled` output. The most likely issue is `unwrap-val` not handling a particular ValXxx variant. Fix `unwrap-val` as needed.

- [ ] **Step 3: Commit**

```bash
git add bootstrap/equivalence_test.airl
git commit -m "feat(bootstrap): functional equivalence test suite (interpreted vs compiled)"
```

---

### Task 3: IR Serializer and Fixpoint Infrastructure

**Files:**
- Create: `bootstrap/fixpoint_test.airl`

Build the fixpoint test file with the IR serializer.

- [ ] **Step 1: Create the base file by concatenating dependencies**

```bash
cd /mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AIRL
cat bootstrap/lexer.airl bootstrap/parser.airl bootstrap/compiler.airl > bootstrap/fixpoint_test.airl
```

- [ ] **Step 2: Append IR serializer and test infrastructure**

Append to `bootstrap/fixpoint_test.airl`:

```clojure
;; ── Fixpoint Test Infrastructure ────────────────────────

(defn assert-eq
  :sig [(actual : List) (expected : List) (name : String) -> List]
  :requires [(valid actual)]
  :ensures [(valid result)]
  :body (if (= actual expected)
    (do (print "PASS:" name) true)
    (do (print "FAIL:" name) (print "  expected:" expected) (print "  got:" actual) false)))

;; ── IR Serializer ──────────────────────────────────────
;; Converts IR variant nodes to canonical string form for comparison.
;; Correctness is load-bearing: any unhandled node type must produce
;; a visible error, not a silent false positive.

(defn ir-to-string
  :sig [(node : List) -> String]
  :intent "Serialize an IR node to a canonical string representation"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (IRInt v)       (join ["(IRInt " (+ "" v) ")"] "")
    (IRFloat v)     (join ["(IRFloat " (+ "" v) ")"] "")
    (IRStr s)       (join ["(IRStr " s ")"] "")
    (IRBool b)      (join ["(IRBool " (if b "true" "false") ")"] "")
    (IRNil _)       "(IRNil)"
    (IRLoad name)   (join ["(IRLoad " name ")"] "")
    (IRIf parts)    (let (c : String (ir-to-string (at parts 0)))
                      (let (t : String (ir-to-string (at parts 1)))
                        (let (e : String (ir-to-string (at parts 2)))
                          (join ["(IRIf " c " " t " " e ")"] ""))))
    (IRDo exprs)    (join ["(IRDo [" (join (map (fn [e] (ir-to-string e)) exprs) " ") "])"] "")
    (IRLet parts)   (let (bindings : List (at parts 0))
                      (let (body : String (ir-to-string (at parts 1)))
                        (let (bs : String (join (map (fn [b] (ir-binding-to-string b)) bindings) " "))
                          (join ["(IRLet [" bs "] " body ")"] ""))))
    (IRFunc parts)  (let (name : String (at parts 0))
                      (let (params : List (at parts 1))
                        (let (body : String (ir-to-string (at parts 2)))
                          (join ["(IRFunc " name " [" (join params " ") "] " body ")"] ""))))
    (IRLambda parts) (let (params : List (at parts 0))
                       (let (body : String (ir-to-string (at parts 1)))
                         (join ["(IRLambda [" (join params " ") "] " body ")"] "")))
    (IRCall parts)  (let (name : String (at parts 0))
                      (let (args : List (at parts 1))
                        (join ["(IRCall " name " [" (join (map (fn [a] (ir-to-string a)) args) " ") "])"] "")))
    (IRCallExpr parts) (let (callee : String (ir-to-string (at parts 0)))
                         (let (args : List (at parts 1))
                           (join ["(IRCallExpr " callee " [" (join (map (fn [a] (ir-to-string a)) args) " ") "])"] "")))
    (IRList items)  (join ["(IRList [" (join (map (fn [i] (ir-to-string i)) items) " ") "])"] "")
    (IRVariant parts) (let (tag : String (at parts 0))
                        (let (args : List (at parts 1))
                          (join ["(IRVariant " tag " [" (join (map (fn [a] (ir-to-string a)) args) " ") "])"] "")))
    (IRMatch parts) (let (scr : String (ir-to-string (at parts 0)))
                      (let (arms : List (at parts 1))
                        (join ["(IRMatch " scr " [" (join (map (fn [a] (ir-arm-to-string a)) arms) " ") "])"] "")))
    (IRTry expr)    (join ["(IRTry " (ir-to-string expr) ")"] "")
    _               "<UNKNOWN-IR-NODE>"))

(defn ir-binding-to-string
  :sig [(b : List) -> String]
  :requires [(valid b)]
  :ensures [(valid result)]
  :body (match b
    (IRBinding parts) (join ["(IRBinding " (at parts 0) " " (ir-to-string (at parts 1)) ")"] "")
    _ "<UNKNOWN-IR-BINDING>"))

(defn ir-arm-to-string
  :sig [(a : List) -> String]
  :requires [(valid a)]
  :ensures [(valid result)]
  :body (match a
    (IRArm parts) (join ["(IRArm " (ir-pattern-to-string (at parts 0)) " " (ir-to-string (at parts 1)) ")"] "")
    _ "<UNKNOWN-IR-ARM>"))

(defn ir-pattern-to-string
  :sig [(p : List) -> String]
  :requires [(valid p)]
  :ensures [(valid result)]
  :body (match p
    (IRPatWild _)      "(IRPatWild)"
    (IRPatBind name)   (join ["(IRPatBind " name ")"] "")
    (IRPatLit v)       (join ["(IRPatLit " (+ "" v) ")"] "")
    (IRPatVariant parts) (let (tag : String (at parts 0))
                           (let (sub-pats : List (at parts 1))
                             (join ["(IRPatVariant " tag " [" (join (map (fn [p] (ir-pattern-to-string p)) sub-pats) " ") "])"] "")))
    _ "<UNKNOWN-IR-PATTERN>"))

;; Serialize a list of IR nodes
(defn ir-nodes-to-string
  :sig [(nodes : List) -> String]
  :requires [(valid nodes)]
  :ensures [(valid result)]
  :body (join (map (fn [n] (ir-to-string n)) nodes) "\n"))
```

- [ ] **Step 3: Verify the file parses**

Run: `source "$HOME/.cargo/env" && cargo run -- check bootstrap/fixpoint_test.airl 2>&1 | head -5`
Expected: No fatal parse errors

- [ ] **Step 4: Commit**

```bash
git add bootstrap/fixpoint_test.airl
git commit -m "feat(bootstrap): fixpoint test infrastructure with IR serializer"
```

---

### Task 4: Fixpoint Test — Tier 1 (Small Program)

**Files:**
- Create: `bootstrap/fixpoint_tier1_program.airl`
- Modify: `bootstrap/fixpoint_test.airl`

Test that the interpreted compiler and compiled compiler produce identical IR for a small test program.

- [ ] **Step 1: Create the tier 1 test program**

Create `bootstrap/fixpoint_tier1_program.airl` — a small program that exercises all major AST node types (no string literals, to avoid escaping):

```clojure
(defn fact
  :sig [(n : i64) -> i64]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (if (<= n 1) 1 (* n (fact (- n 1)))))

(defn apply-fn
  :sig [(f : fn) (x : i64) -> i64]
  :requires [(valid f)]
  :ensures [(valid result)]
  :body (f x))

(let (result : i64 (apply-fn (fn [x] (+ x 10)) 5))
  (match (Ok result)
    (Ok v) (+ v (fact 5))
    (Err e) 0
    _ -1))
```

- [ ] **Step 2: Append Tier 1 fixpoint test**

Append to `bootstrap/fixpoint_test.airl`. Note: `full-source` must include lexer + parser + compiler so the compiled compiler has access to `lex`, `parse-sexpr-all`, `parse-program` inside the VM.

```clojure
;; ── Tier 1: Small Program Fixpoint ──────────────────────

(defn parse-source
  :sig [(source : String) -> List]
  :intent "Parse source to AST nodes"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (lex source)
      (Err e) (Err e)
      (Ok tokens)
        (match (parse-sexpr-all tokens)
          (Err e) (Err e)
          (Ok sexprs) (parse-program sexprs))))

(print "── Tier 1: Small program fixpoint ──")

;; Read the test program from file (avoids string escaping)
(let (test-source : String (read-file "bootstrap/fixpoint_tier1_program.airl"))

  ;; Parse to AST
  (match (parse-source test-source)
    (Err e) (do (print "FAIL: could not parse tier 1 program:" e) false)
    (Ok ast-nodes)
      ;; Path 1: Interpreted compiler compiles the test program
      (match (compile-program ast-nodes)
        (Err e) (do (print "FAIL: interpreted compile failed:" e) false)
        (Ok ir1)
          (let (ir1-str : String (ir-nodes-to-string ir1))

            ;; Path 2: Compiled compiler compiles the test program
            ;; full-source = lexer + parser + compiler + trailing expression
            (let (lexer-source : String (read-file "bootstrap/lexer.airl"))
              (let (parser-source : String (read-file "bootstrap/parser.airl"))
                (let (compiler-source : String (read-file "bootstrap/compiler.airl"))
                  (let (full-source : String (join [
                        lexer-source "\n"
                        parser-source "\n"
                        compiler-source "\n"
                        "(let (src : String (read-file \"bootstrap/fixpoint_tier1_program.airl\"))\n"
                        "  (match (lex src)\n"
                        "    (Err e) (Err e)\n"
                        "    (Ok tokens)\n"
                        "      (match (parse-sexpr-all tokens)\n"
                        "        (Err e) (Err e)\n"
                        "        (Ok sexprs)\n"
                        "          (match (parse-program sexprs)\n"
                        "            (Err e) (Err e)\n"
                        "            (Ok ast) (compile-program ast)))))\n"
                        ] ""))

                    ;; run-compiled: the compiled compiler compiles the test program
                    (match (run-compiled full-source)
                      (Err e) (do (print "FAIL: compiled compile failed:" e) false)
                      (Ok ir2-result)
                        (match ir2-result
                          (Ok ir2)
                            (let (ir2-str : String (ir-nodes-to-string ir2))
                              (assert-eq ir1-str ir2-str "tier 1 fixpoint"))
                          (Err e) (do (print "FAIL: compiled compile returned Err:" e) false)
                          _ (do (print "FAIL: unexpected result") false)))
))))))))  ;; closes: let full-source, let compiler-source, let parser-source, let lexer-source, let ir1-str, match compile-program, match parse-source, let test-source
```

- [ ] **Step 3: Run Tier 1 fixpoint test**

Run: `source "$HOME/.cargo/env" && cargo run --release -- run bootstrap/fixpoint_test.airl 2>&1 | grep -E "PASS|FAIL|Tier"`
Expected: "PASS: tier 1 fixpoint"

If it fails, debug by comparing `ir1-str` and `ir2-str` — print both on failure. The most likely issues:
- Missing source files in `full-source` (lexer/parser not included)
- `run-compiled` not finding `read-file` in the VM (it's a builtin, should work)
- IR serialization differences (node structure mismatch)

- [ ] **Step 4: Commit**

```bash
git add bootstrap/fixpoint_test.airl bootstrap/fixpoint_tier1_program.airl
git commit -m "feat(bootstrap): tier 1 fixpoint test (small program)"
```

---

### Task 5: Fixpoint Test — Tier 2 (Compiler Compiles Itself)

**Files:**
- Modify: `bootstrap/fixpoint_test.airl`

Prove the compiler compiled by itself produces identical IR to the compiler compiled by the interpreter.

- [ ] **Step 1: Append Tier 2 fixpoint test**

Append to `bootstrap/fixpoint_test.airl`:

```clojure
;; ── Tier 2: Compiler compiles itself ──────────────────

(print "── Tier 2: Compiler self-compilation fixpoint ──")

;; Read the compiler source
(let (compiler-source : String (read-file "bootstrap/compiler.airl"))

  ;; Parse compiler source to AST
  (match (parse-source compiler-source)
    (Err e) (do (print "FAIL: could not parse compiler source:" e) false)
    (Ok compiler-ast)
      (do
        ;; Path 1: Interpreted compiler compiles the compiler
        (match (compile-program compiler-ast)
          (Err e) (do (print "FAIL: interpreted self-compile failed:" e) false)
          (Ok ir1)
            (let (ir1-str : String (ir-nodes-to-string ir1))

              ;; Path 2: Compiled compiler compiles the compiler
              ;; full-source = lexer + parser + compiler + code that reads and compiles compiler.airl
              (let (lexer-source : String (read-file "bootstrap/lexer.airl"))
                (let (parser-source : String (read-file "bootstrap/parser.airl"))
                  (let (full-source : String (join [
                        lexer-source "\n"
                        parser-source "\n"
                        compiler-source "\n"
                        "(let (src : String (read-file \"bootstrap/compiler.airl\"))\n"
                        "  (match (lex src)\n"
                        "    (Err e) (Err e)\n"
                        "    (Ok tokens)\n"
                        "      (match (parse-sexpr-all tokens)\n"
                        "        (Err e) (Err e)\n"
                        "        (Ok sexprs)\n"
                        "          (match (parse-program sexprs)\n"
                        "            (Err e) (Err e)\n"
                        "            (Ok ast) (compile-program ast)))))\n"
                        ] ""))

                    (match (run-compiled full-source)
                      (Err e) (do (print "FAIL: compiled self-compile failed:" e) false)
                      (Ok ir2-result)
                        (match ir2-result
                          (Ok ir2)
                            (let (ir2-str : String (ir-nodes-to-string ir2))
                              (assert-eq ir1-str ir2-str "tier 2 fixpoint: compiler self-compilation"))
                          (Err e) (do (print "FAIL: compiled self-compile returned Err:" e) false)
                          _ (do (print "FAIL: unexpected result") false)))
))))))))  ;; closes: let full-source, let parser-source, let lexer-source, let ir1-str, match compile-program, do, match parse-source, let compiler-source

(print "fixpoint tests complete")
```

- [ ] **Step 2: Run Tier 2 fixpoint test**

Run: `source "$HOME/.cargo/env" && cargo run --release -- run bootstrap/fixpoint_test.airl 2>&1 | grep -E "PASS|FAIL|Tier|complete"`
Expected: Both "PASS: tier 1 fixpoint" and "PASS: tier 2 fixpoint: compiler self-compilation"

This will be slow — the compiled compiler path requires compiling ~1,514 lines (lexer+parser+compiler) through the interpreted compiler, then running in the VM. Use `--release` mode.

- [ ] **Step 3: Commit**

```bash
git add bootstrap/fixpoint_test.airl
git commit -m "feat(bootstrap): tier 2 fixpoint test (compiler self-compilation)"
```

---

### Task 6: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add test commands to Bootstrap Compiler section**

Add after the existing `typecheck_test.airl` line:

```bash
cargo run -- run bootstrap/equivalence_test.airl            # Interpreted vs compiled equivalence
cargo run --release -- run bootstrap/fixpoint_test.airl      # Compiler fixpoint test (slow, use --release)
```

- [ ] **Step 2: Update Completed Tasks**

Add:
- **Bootstrap Fixpoint Verification** — Functional equivalence test proves interpreted eval and compiled run-ir produce identical results across 20+ test programs. Compiler fixpoint test proves the compiled compiler produces identical IR to the interpreted compiler (Tier 1: small program, Tier 2: compiler self-compilation).

- [ ] **Step 3: Run full workspace tests**

Run: `source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir 2>&1 | grep -E "^test result|FAILED"`
Expected: All pass (no Rust changes, so this is a safety check)

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with bootstrap fixpoint verification milestone"
```
