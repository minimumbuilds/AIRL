# Bootstrap Evaluator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tree-walking evaluator written in AIRL that interprets AST nodes from the bootstrap parser, completing the lex→parse→eval pipeline entirely in AIRL.

**Architecture:** The evaluator dispatches on AST node type via pattern matching. Values are tagged variants (`ValInt`, `ValStr`, etc.). The environment is a list of maps (frame stack). Builtins delegate to real AIRL builtins since the evaluator runs on the Rust interpreter. All functions return `(Ok result)` or `(Err msg)` for error propagation.

**Tech Stack:** Pure AIRL. No new Rust code. No external dependencies.

**Spec:** `docs/superpowers/specs/2026-03-22-bootstrap-eval-design.md`

---

## File Structure

| File | Purpose |
|------|---------|
| Create: `bootstrap/eval.airl` | Evaluator: value helpers, env ops, eval-node, call-builtin, try-match-pattern, eval-top-level, eval-program, run-source (~350-450 lines) |
| Create: `bootstrap/eval_test.airl` | Tests — must include all function defs from eval.airl (no import system). Progressive tests from atoms to full pipeline. |

No Rust changes. No modifications to existing bootstrap files.

---

### Task 1: Value Helpers and Unwrap Functions

**Files:**
- Create: `bootstrap/eval.airl`

Set up the file with value constructor helpers and unwrap functions.

- [ ] **Step 1: Create eval.airl with value unwrap helpers**

```airl
;; bootstrap/eval.airl — Bootstrap evaluator for AIRL
;; Interprets AST nodes produced by bootstrap/parser.airl

;; ─── Value Unwrap Helpers ─────────────────────────────────

(defn unwrap-int
  :sig [(v : Any) -> i64]
  :intent "Extract integer from ValInt"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValInt n) n
    _ (do (print "ERROR: expected ValInt, got" v) 0)))

(defn unwrap-float
  :sig [(v : Any) -> f64]
  :intent "Extract float from ValFloat"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValFloat n) n
    _ (do (print "ERROR: expected ValFloat, got" v) 0.0)))

(defn unwrap-str
  :sig [(v : Any) -> Str]
  :intent "Extract string from ValStr"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValStr s) s
    _ (do (print "ERROR: expected ValStr, got" v) "")))

(defn unwrap-bool
  :sig [(v : Any) -> Bool]
  :intent "Extract bool from ValBool"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValBool b) b
    _ (do (print "ERROR: expected ValBool, got" v) false)))

(defn unwrap-list
  :sig [(v : Any) -> List]
  :intent "Extract list from ValList"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValList xs) xs
    _ (do (print "ERROR: expected ValList, got" v) [])))

(defn unwrap-raw
  :sig [(v : Any) -> Any]
  :intent "Extract the raw AIRL value from any Val wrapper"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValInt n) n
    (ValFloat n) n
    (ValStr s) s
    (ValBool b) b
    (ValNil) nil
    _ v))
```

- [ ] **Step 2: Create eval_test.airl with first tests**

```airl
;; bootstrap/eval_test.airl — Tests for the bootstrap evaluator

;; ─── Test Helper ──────────────────────────────────────────

(defn assert-eq
  :sig [(a : Any) (b : Any) -> Bool]
  :intent "Assert equality"
  :requires [(valid a) (valid b)]
  :ensures [(valid result)]
  :body (if (= a b) true
          (do (print "  FAIL: expected" b "got" a) false)))

;; ─── Include eval.airl functions ──────────────────────────
;; (All function defs from eval.airl must be copied here since
;;  AIRL has no import system. For now, just the unwrap helpers.)

(defn unwrap-int
  :sig [(v : Any) -> i64]
  :intent "Extract integer from ValInt"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValInt n) n
    _ (do (print "ERROR: expected ValInt, got" v) 0)))

(defn unwrap-float
  :sig [(v : Any) -> f64]
  :intent "Extract float from ValFloat"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValFloat n) n
    _ (do (print "ERROR: expected ValFloat, got" v) 0.0)))

(defn unwrap-str
  :sig [(v : Any) -> Str]
  :intent "Extract string from ValStr"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValStr s) s
    _ (do (print "ERROR: expected ValStr, got" v) "")))

(defn unwrap-bool
  :sig [(v : Any) -> Bool]
  :intent "Extract bool from ValBool"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValBool b) b
    _ (do (print "ERROR: expected ValBool, got" v) false)))

(defn unwrap-list
  :sig [(v : Any) -> List]
  :intent "Extract list from ValList"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValList xs) xs
    _ (do (print "ERROR: expected ValList, got" v) [])))

(defn unwrap-raw
  :sig [(v : Any) -> Any]
  :intent "Extract the raw AIRL value from any Val wrapper"
  :requires [(valid v)]
  :ensures [(valid result)]
  :body (match v
    (ValInt n) n
    (ValFloat n) n
    (ValStr s) s
    (ValBool b) b
    (ValNil) nil
    _ v))

;; ─── Task 1: Value Helper Tests ──────────────────────────

(print "=== Task 1: Value helpers ===")
(assert-eq (unwrap-int (ValInt 42)) 42)
(assert-eq (unwrap-str (ValStr "hi")) "hi")
(assert-eq (unwrap-bool (ValBool true)) true)
(assert-eq (unwrap-list (ValList [1 2 3])) [1 2 3])
(assert-eq (unwrap-raw (ValInt 99)) 99)
(assert-eq (unwrap-raw (ValStr "x")) "x")
(assert-eq (unwrap-raw (ValBool false)) false)
(print "=== Task 1 tests complete ===")
```

- [ ] **Step 3: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: "=== Task 1 tests complete ===" with no FAIL

- [ ] **Step 4: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add eval value helpers and unwrap functions"
```

---

### Task 2: Environment Operations

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add the environment frame stack: env-new, env-push, env-pop, env-bind, env-get.

- [ ] **Step 1: Add env operations to eval.airl**

Append after the unwrap helpers:

```airl
;; ─── Environment Operations ──────────────────────────────
;; Environment is a list of maps (frame stack). Head = innermost scope.

(defn env-new
  :sig [() -> List]
  :intent "Create a new environment with one empty frame"
  :requires [(valid 0)]
  :ensures [(valid result)]
  :body [(map-new)])

(defn env-push
  :sig [(env : List) -> List]
  :intent "Push a new empty frame onto the environment"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body (cons (map-new) env))

(defn env-pop
  :sig [(env : List) -> List]
  :intent "Pop the innermost frame from the environment"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body (tail env))

(defn env-bind
  :sig [(env : List) (name : Str) (val : Any) -> List]
  :intent "Bind a name to a value in the top frame"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body (cons (map-set (head env) name val) (tail env)))

(defn env-get
  :sig [(env : List) (name : Str) -> Any]
  :intent "Look up a name in the environment, searching from innermost frame outward"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body (if (empty? env)
    (Err (join ["undefined symbol: " name] ""))
    (if (map-has (head env) name)
      (Ok (map-get (head env) name))
      (env-get (tail env) name))))
```

- [ ] **Step 2: Add env tests to eval_test.airl**

Copy the env functions into eval_test.airl, then add:

```airl
;; ─── Task 2: Environment Tests ───────────────────────────

(print "=== Task 2: Environment ===")

;; Basic bind and get
(let ([e (env-bind (env-new) "x" (ValInt 42))])
  (match (env-get e "x")
    (Ok v) (assert-eq (unwrap-int v) 42)
    (Err _) (print "  FAIL: x not found")))

;; Undefined symbol
(match (env-get (env-new) "nope")
  (Ok _) (print "  FAIL: should be undefined")
  (Err msg) (assert-eq (contains msg "undefined") true))

;; Push/pop frame with shadowing
(let ([e0 (env-bind (env-new) "x" (ValInt 1))]
      [e1 (env-bind (env-push e0) "x" (ValInt 2))])
  (do
    ;; Inner frame shadows outer
    (match (env-get e1 "x")
      (Ok v) (assert-eq (unwrap-int v) 2)
      (Err _) (print "  FAIL"))
    ;; After pop, outer value restored
    (match (env-get (env-pop e1) "x")
      (Ok v) (assert-eq (unwrap-int v) 1)
      (Err _) (print "  FAIL"))))

;; Inner frame can see outer bindings
(let ([e0 (env-bind (env-new) "y" (ValStr "hello"))]
      [e1 (env-push e0)])
  (match (env-get e1 "y")
    (Ok v) (assert-eq (unwrap-str v) "hello")
    (Err _) (print "  FAIL: y not visible in inner frame")))

(print "=== Task 2 tests complete ===")
```

- [ ] **Step 3: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Task 1 and Task 2 complete, no FAIL

- [ ] **Step 4: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add eval environment operations"
```

---

### Task 3: Eval Atoms and Symbol Lookup

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add `eval-node` with atom and symbol dispatch.

- [ ] **Step 1: Add eval-node with atom cases to eval.airl**

```airl
;; ─── Evaluator Core ──────────────────────────────────────

(defn eval-node
  :sig [(node : Any) (env : List) -> Any]
  :intent "Evaluate a single AST node, returning (Ok [val env]) or (Err msg)"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTInt v _ _) (Ok [(ValInt v) env])
    (ASTFloat v _ _) (Ok [(ValFloat v) env])
    (ASTStr v _ _) (Ok [(ValStr v) env])
    (ASTBool v _ _) (Ok [(ValBool v) env])
    (ASTNil _ _) (Ok [(ValNil) env])
    (ASTKeyword k _ _) (Ok [(ValStr (join [":" k] "")) env])

    (ASTSymbol name _ _)
      (match (env-get env name)
        (Ok val) (Ok [val env])
        (Err msg) (Err msg))

    (ASTList items _ _)
      (match (eval-list-items items env [])
        (Err e) (Err e)
        (Ok pair) (Ok [(ValList (at pair 0)) (at pair 1)]))

    _ (Err "eval-node: unknown node type")))

(defn eval-list-items
  :sig [(items : List) (env : List) (acc : List) -> Any]
  :intent "Evaluate list literal items left-to-right"
  :requires [(valid items)]
  :ensures [(valid result)]
  :body
    (if (empty? items) (Ok [acc env])
      (match (eval-node (head items) env)
        (Err e) (Err e)
        (Ok pair)
          (eval-list-items (tail items) (at pair 1) (append acc [(at pair 0)])))))
```

- [ ] **Step 2: Add atom/symbol tests to eval_test.airl**

Copy `eval-node` into eval_test.airl, then add:

```airl
;; ─── Task 3: Atom and Symbol Tests ───────────────────────

(print "=== Task 3: Atoms and symbols ===")

(let ([env (env-new)])
  (do
    ;; Integer literal
    (match (eval-node (ASTInt 42 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 42)
      (Err e) (print "  FAIL:" e))

    ;; String literal
    (match (eval-node (ASTStr "hello" 0 0) env)
      (Ok pair) (assert-eq (unwrap-str (at pair 0)) "hello")
      (Err e) (print "  FAIL:" e))

    ;; Bool literal
    (match (eval-node (ASTBool true 0 0) env)
      (Ok pair) (assert-eq (unwrap-bool (at pair 0)) true)
      (Err e) (print "  FAIL:" e))

    ;; Nil literal
    (match (eval-node (ASTNil 0 0) env)
      (Ok pair) (assert-eq (unwrap-raw (at pair 0)) nil)
      (Err e) (print "  FAIL:" e))

    ;; Keyword
    (match (eval-node (ASTKeyword "foo" 0 0) env)
      (Ok pair) (assert-eq (unwrap-str (at pair 0)) ":foo")
      (Err e) (print "  FAIL:" e))))

;; Symbol lookup
(let ([env (env-bind (env-new) "x" (ValInt 99))])
  (match (eval-node (ASTSymbol "x" 0 0) env)
    (Ok pair) (assert-eq (unwrap-int (at pair 0)) 99)
    (Err e) (print "  FAIL:" e)))

;; Undefined symbol
(match (eval-node (ASTSymbol "nope" 0 0) (env-new))
  (Ok _) (print "  FAIL: should error on undefined")
  (Err msg) (assert-eq (contains msg "undefined") true))

;; List literal: [1 2 3]
(let ([env (env-new)])
  (match (eval-node (ASTList [(ASTInt 1 0 0) (ASTInt 2 0 0) (ASTInt 3 0 0)] 0 0) env)
    (Ok pair) (assert-eq (length (unwrap-list (at pair 0))) 3)
    (Err e) (print "  FAIL:" e)))

(print "=== Task 3 tests complete ===")
```

- [ ] **Step 3: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Tasks 1-3 complete, no FAIL

- [ ] **Step 4: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add eval-node atom and symbol dispatch"
```

---

### Task 4: Builtin Dispatch

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add `call-builtin` for arithmetic, comparison, logic, and list builtins. Add `make-initial-env`. Add the `ASTCall` branch to `eval-node`.

- [ ] **Step 1: Add call-builtin to eval.airl**

Add before `eval-node`:

```airl
;; ─── Builtin Dispatch ────────────────────────────────────

(defn call-builtin
  :sig [(name : Str) (args : List) -> Any]
  :intent "Dispatch a builtin call by name, returning (Ok val) or (Err msg)"
  :requires [(valid name)]
  :ensures [(valid result)]
  :body
    ;; Arithmetic (arity 2)
    (if (= name "+") (Ok (ValInt (+ (unwrap-int (at args 0)) (unwrap-int (at args 1)))))
    (if (= name "-") (Ok (ValInt (- (unwrap-int (at args 0)) (unwrap-int (at args 1)))))
    (if (= name "*") (Ok (ValInt (* (unwrap-int (at args 0)) (unwrap-int (at args 1)))))
    (if (= name "/") (Ok (ValInt (/ (unwrap-int (at args 0)) (unwrap-int (at args 1)))))
    (if (= name "%") (Ok (ValInt (% (unwrap-int (at args 0)) (unwrap-int (at args 1)))))

    ;; Comparison (arity 2)
    (if (= name "=")  (Ok (ValBool (= (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))
    (if (= name "!=") (Ok (ValBool (!= (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))
    (if (= name "<")  (Ok (ValBool (< (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))
    (if (= name ">")  (Ok (ValBool (> (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))
    (if (= name "<=") (Ok (ValBool (<= (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))
    (if (= name ">=") (Ok (ValBool (>= (unwrap-raw (at args 0)) (unwrap-raw (at args 1)))))

    ;; Logic
    (if (= name "not") (Ok (ValBool (not (unwrap-bool (at args 0)))))
    (if (= name "and") (Ok (ValBool (and (unwrap-bool (at args 0)) (unwrap-bool (at args 1)))))
    (if (= name "or")  (Ok (ValBool (or (unwrap-bool (at args 0)) (unwrap-bool (at args 1)))))

    ;; List operations
    (if (= name "head")   (Ok (head (unwrap-list (at args 0))))
    (if (= name "tail")   (Ok (ValList (tail (unwrap-list (at args 0)))))
    (if (= name "cons")   (Ok (ValList (cons (at args 0) (unwrap-list (at args 1)))))
    (if (= name "empty?") (Ok (ValBool (empty? (unwrap-list (at args 0)))))
    (if (= name "length") (Ok (ValInt (length (unwrap-list (at args 0)))))
    (if (= name "at")     (Ok (at (unwrap-list (at args 0)) (unwrap-int (at args 1))))
    (if (= name "append") (Ok (ValList (append (unwrap-list (at args 0)) (unwrap-list (at args 1)))))

    ;; String operations
    (if (= name "char-at")    (Ok (ValStr (char-at (unwrap-str (at args 0)) (unwrap-int (at args 1)))))
    (if (= name "substring")  (Ok (ValStr (substring (unwrap-str (at args 0)) (unwrap-int (at args 1)) (unwrap-int (at args 2)))))
    (if (= name "contains")   (Ok (ValBool (contains (unwrap-str (at args 0)) (unwrap-str (at args 1)))))
    (if (= name "split")      (let ([parts (split (unwrap-str (at args 0)) (unwrap-str (at args 1)))])
                                 (Ok (ValList (map (fn [s] (ValStr s)) parts))))
    (if (= name "join")       (Ok (ValStr (join (map (fn [v] (unwrap-str v)) (unwrap-list (at args 1))) (unwrap-str (at args 0))))))
    (if (= name "chars")      (let ([cs (chars (unwrap-str (at args 0)))])
                                 (Ok (ValList (map (fn [c] (ValStr c)) cs))))

    ;; I/O
    (if (= name "print") (do (print (unwrap-raw (at args 0))) (Ok (ValNil)))

    ;; Introspection
    (if (= name "type-of")
      (match (at args 0)
        (ValInt _) (Ok (ValStr "Int"))
        (ValFloat _) (Ok (ValStr "Float"))
        (ValStr _) (Ok (ValStr "Str"))
        (ValBool _) (Ok (ValStr "Bool"))
        (ValNil) (Ok (ValStr "Nil"))
        (ValList _) (Ok (ValStr "List"))
        (ValVariant _ _) (Ok (ValStr "Variant"))
        (ValFn _ _ _ _) (Ok (ValStr "Fn"))
        (ValLambda _ _ _) (Ok (ValStr "Lambda"))
        (ValBuiltin _) (Ok (ValStr "Builtin"))
        _ (Ok (ValStr "Unknown")))

    ;; Unknown builtin
    (Err (join ["unknown builtin: " name] ""))
    )))))))))))))))))))))))))))))
```

- [ ] **Step 2: Add make-initial-env to eval.airl**

```airl
(defn make-initial-env
  :sig [() -> List]
  :intent "Create environment with all builtins bound"
  :requires [(valid 0)]
  :ensures [(valid result)]
  :body
    (let ([e (env-new)]
          [e (env-bind e "+" (ValBuiltin "+"))]
          [e (env-bind e "-" (ValBuiltin "-"))]
          [e (env-bind e "*" (ValBuiltin "*"))]
          [e (env-bind e "/" (ValBuiltin "/"))]
          [e (env-bind e "%" (ValBuiltin "%"))]
          [e (env-bind e "=" (ValBuiltin "="))]
          [e (env-bind e "!=" (ValBuiltin "!="))]
          [e (env-bind e "<" (ValBuiltin "<"))]
          [e (env-bind e ">" (ValBuiltin ">"))]
          [e (env-bind e "<=" (ValBuiltin "<="))]
          [e (env-bind e ">=" (ValBuiltin ">="))]
          [e (env-bind e "not" (ValBuiltin "not"))]
          [e (env-bind e "and" (ValBuiltin "and"))]
          [e (env-bind e "or" (ValBuiltin "or"))]
          [e (env-bind e "head" (ValBuiltin "head"))]
          [e (env-bind e "tail" (ValBuiltin "tail"))]
          [e (env-bind e "cons" (ValBuiltin "cons"))]
          [e (env-bind e "empty?" (ValBuiltin "empty?"))]
          [e (env-bind e "length" (ValBuiltin "length"))]
          [e (env-bind e "at" (ValBuiltin "at"))]
          [e (env-bind e "append" (ValBuiltin "append"))]
          [e (env-bind e "char-at" (ValBuiltin "char-at"))]
          [e (env-bind e "substring" (ValBuiltin "substring"))]
          [e (env-bind e "contains" (ValBuiltin "contains"))]
          [e (env-bind e "split" (ValBuiltin "split"))]
          [e (env-bind e "join" (ValBuiltin "join"))]
          [e (env-bind e "chars" (ValBuiltin "chars"))]
          [e (env-bind e "print" (ValBuiltin "print"))]
          [e (env-bind e "type-of" (ValBuiltin "type-of"))])
      e))
```

- [ ] **Step 3: Add ASTCall branch to eval-node**

Add to eval-node's match, before the `_ (Err ...)` fallback:

```airl
    (ASTCall callee args _ _)
      (match (eval-node callee env)
        (Err e) (Err e)
        (Ok callee-pair)
          (let ([callee-val (at callee-pair 0)]
                [env2 (at callee-pair 1)])
            (match (eval-args args env2)
              (Err e) (Err e)
              (Ok args-pair)
                (let ([arg-vals (at args-pair 0)]
                      [env3 (at args-pair 1)])
                  (match callee-val
                    (ValBuiltin name)
                      (match (call-builtin name arg-vals)
                        (Ok val) (Ok [val env3])
                        (Err e) (Err e))
                    _ (Err "eval: not callable"))))))
```

Also add `eval-args` helper:

```airl
(defn eval-args
  :sig [(args : List) (env : List) -> Any]
  :intent "Evaluate a list of argument expressions left-to-right"
  :requires [(valid args)]
  :ensures [(valid result)]
  :body (eval-args-acc args env []))

(defn eval-args-acc
  :sig [(args : List) (env : List) (acc : List) -> Any]
  :intent "Accumulate evaluated argument values"
  :requires [(valid args)]
  :ensures [(valid result)]
  :body
    (if (empty? args) (Ok [acc env])
      (match (eval-node (head args) env)
        (Err e) (Err e)
        (Ok pair)
          (eval-args-acc (tail args) (at pair 1) (append acc [(at pair 0)])))))
```

- [ ] **Step 4: Add builtin tests to eval_test.airl**

Copy new functions into eval_test.airl, then add:

```airl
;; ─── Task 4: Builtin Tests ──────────────────────────────

(print "=== Task 4: Builtins ===")

(let ([env (make-initial-env)])
  (do
    ;; Arithmetic: (+ 2 3) → 5
    (match (eval-node (ASTCall (ASTSymbol "+" 0 0) [(ASTInt 2 0 0) (ASTInt 3 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 5)
      (Err e) (print "  FAIL:" e))

    ;; Comparison: (= 1 1) → true
    (match (eval-node (ASTCall (ASTSymbol "=" 0 0) [(ASTInt 1 0 0) (ASTInt 1 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-bool (at pair 0)) true)
      (Err e) (print "  FAIL:" e))

    ;; Comparison: (< 1 2) → true
    (match (eval-node (ASTCall (ASTSymbol "<" 0 0) [(ASTInt 1 0 0) (ASTInt 2 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-bool (at pair 0)) true)
      (Err e) (print "  FAIL:" e))

    ;; Logic: (not true) → false
    (match (eval-node (ASTCall (ASTSymbol "not" 0 0) [(ASTBool true 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-bool (at pair 0)) false)
      (Err e) (print "  FAIL:" e))

    ;; List: (head [1 2 3])
    (match (eval-node (ASTCall (ASTSymbol "head" 0 0) [(ASTList [(ASTInt 1 0 0) (ASTInt 2 0 0) (ASTInt 3 0 0)] 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 1)
      (Err e) (print "  FAIL:" e))

    ;; Nested: (* (+ 1 2) (- 5 3)) → 6
    (match (eval-node
      (ASTCall (ASTSymbol "*" 0 0)
        [(ASTCall (ASTSymbol "+" 0 0) [(ASTInt 1 0 0) (ASTInt 2 0 0)] 0 0)
         (ASTCall (ASTSymbol "-" 0 0) [(ASTInt 5 0 0) (ASTInt 3 0 0)] 0 0)]
        0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 6)
      (Err e) (print "  FAIL:" e))))

(print "=== Task 4 tests complete ===")
```

- [ ] **Step 5: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Tasks 1-4 complete, no FAIL

- [ ] **Step 6: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add builtin dispatch and ASTCall evaluation"
```

---

### Task 5: Control Flow — If, Let, Do

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add the `ASTIf`, `ASTLet`, and `ASTDo` branches to `eval-node`.

- [ ] **Step 1: Add If branch to eval-node**

Add to eval-node's match:

```airl
    (ASTIf cond then-expr else-expr _ _)
      (match (eval-node cond env)
        (Err e) (Err e)
        (Ok cond-pair)
          (let ([cond-val (at cond-pair 0)]
                [env2 (at cond-pair 1)])
            (if (unwrap-bool cond-val)
              (eval-node then-expr env2)
              (eval-node else-expr env2))))
```

- [ ] **Step 2: Add Let branch to eval-node**

```airl
    (ASTLet bindings body _ _)
      (let ([env2 (env-push env)])
        (match (eval-let-bindings bindings env2)
          (Err e) (Err e)
          (Ok bound-env)
            (match (eval-node body bound-env)
              (Err e) (Err e)
              (Ok pair) (Ok [(at pair 0) (env-pop (at pair 1))]))))
```

Also add `eval-let-bindings` helper:

```airl
(defn eval-let-bindings
  :sig [(bindings : List) (env : List) -> Any]
  :intent "Evaluate let bindings sequentially, threading env"
  :requires [(valid bindings)]
  :ensures [(valid result)]
  :body
    (if (empty? bindings) (Ok env)
      (match (head bindings)
        (ASTBinding name _ val-expr)
          (match (eval-node val-expr env)
            (Err e) (Err e)
            (Ok pair)
              (eval-let-bindings (tail bindings) (env-bind (at pair 1) name (at pair 0))))
        _ (Err "eval: invalid let binding"))))
```

- [ ] **Step 3: Add Do branch to eval-node**

```airl
    (ASTDo exprs _ _)
      (eval-do exprs env)
```

Also add `eval-do` helper:

```airl
(defn eval-do
  :sig [(exprs : List) (env : List) -> Any]
  :intent "Evaluate a sequence of expressions, threading env, return last value"
  :requires [(valid exprs)]
  :ensures [(valid result)]
  :body
    (if (empty? exprs) (Ok [(ValNil) env])
      (match (eval-node (head exprs) env)
        (Err e) (Err e)
        (Ok pair)
          (if (empty? (tail exprs))
            (Ok pair)
            (eval-do (tail exprs) (at pair 1))))))
```

- [ ] **Step 4: Add control flow tests to eval_test.airl**

Copy new functions, then add:

```airl
;; ─── Task 5: Control Flow Tests ─────────────────────────

(print "=== Task 5: Control flow ===")

(let ([env (make-initial-env)])
  (do
    ;; If true branch
    (match (eval-node (ASTIf (ASTBool true 0 0) (ASTInt 1 0 0) (ASTInt 2 0 0) 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 1)
      (Err e) (print "  FAIL:" e))

    ;; If false branch
    (match (eval-node (ASTIf (ASTBool false 0 0) (ASTInt 1 0 0) (ASTInt 2 0 0) 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 2)
      (Err e) (print "  FAIL:" e))

    ;; Let binding: (let ([x 10]) (+ x 1)) → 11
    (match (eval-node
      (ASTLet [(ASTBinding "x" "i64" (ASTInt 10 0 0))]
        (ASTCall (ASTSymbol "+" 0 0) [(ASTSymbol "x" 0 0) (ASTInt 1 0 0)] 0 0)
        0 0)
      env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 11)
      (Err e) (print "  FAIL:" e))

    ;; Let with multiple bindings: (let ([a 3] [b 4]) (+ a b)) → 7
    (match (eval-node
      (ASTLet [(ASTBinding "a" "i64" (ASTInt 3 0 0))
               (ASTBinding "b" "i64" (ASTInt 4 0 0))]
        (ASTCall (ASTSymbol "+" 0 0) [(ASTSymbol "a" 0 0) (ASTSymbol "b" 0 0)] 0 0)
        0 0)
      env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 7)
      (Err e) (print "  FAIL:" e))

    ;; Do block: (do 1 2 3) → 3
    (match (eval-node (ASTDo [(ASTInt 1 0 0) (ASTInt 2 0 0) (ASTInt 3 0 0)] 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 3)
      (Err e) (print "  FAIL:" e))

    ;; If with comparison: (if (= 1 1) 42 0)
    (match (eval-node
      (ASTIf (ASTCall (ASTSymbol "=" 0 0) [(ASTInt 1 0 0) (ASTInt 1 0 0)] 0 0)
        (ASTInt 42 0 0) (ASTInt 0 0 0) 0 0)
      env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 42)
      (Err e) (print "  FAIL:" e))))

(print "=== Task 5 tests complete ===")
```

- [ ] **Step 5: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Tasks 1-5 complete, no FAIL

- [ ] **Step 6: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add if/let/do evaluation"
```

---

### Task 6: Function Definition and Calls

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add `eval-top-level` (handles `ASTDefn`), function call dispatch for `ValFn`, and `eval-program`.

- [ ] **Step 1: Add eval-top-level and extract-param-names to eval.airl**

```airl
;; ─── Top-Level Evaluation ────────────────────────────────

(defn extract-param-names
  :sig [(sig : Any) -> List]
  :intent "Extract parameter name strings from an ASTSig node"
  :requires [(valid sig)]
  :ensures [(valid result)]
  :body (match sig
    (ASTSig params _)
      (map (fn [p] (match p (ASTParam name _) name _ "?")) params)
    _ []))

(defn eval-top-level
  :sig [(node : Any) (env : List) -> Any]
  :intent "Evaluate a top-level node (defn or expression)"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTDefn name sig _ _ _ body _ _)
      (let ([param-names (extract-param-names sig)]
            [fn-val (ValFn name param-names body env)])
        (Ok [(ValNil) (env-bind env name fn-val)]))
    _ (eval-node node env)))
```

- [ ] **Step 2: Add ValFn/ValLambda call dispatch to eval-node's ASTCall branch**

Update the `ASTCall` callee match to handle functions and lambdas:

```airl
                  (match callee-val
                    (ValBuiltin name)
                      (match (call-builtin name arg-vals)
                        (Ok val) (Ok [val env3])
                        (Err e) (Err e))

                    (ValFn fn-name params body captured-env)
                      (let ([call-env (bind-params (env-push captured-env) params arg-vals)])
                        (match (eval-node body call-env)
                          (Err e) (Err e)
                          (Ok pair) (Ok [(at pair 0) env3])))

                    (ValLambda params body captured-env)
                      (let ([call-env (bind-params (env-push captured-env) params arg-vals)])
                        (match (eval-node body call-env)
                          (Err e) (Err e)
                          (Ok pair) (Ok [(at pair 0) env3])))

                    _ (Err "eval: not callable"))
```

Also add `bind-params` helper:

```airl
(defn bind-params
  :sig [(env : List) (names : List) (vals : List) -> List]
  :intent "Bind parameter names to argument values in the top frame"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body
    (if (empty? names) env
      (bind-params
        (env-bind env (head names) (if (empty? vals) (ValNil) (head vals)))
        (tail names)
        (if (empty? vals) [] (tail vals)))))
```

- [ ] **Step 3: Add eval-program to eval.airl**

```airl
(defn eval-program
  :sig [(nodes : List) (env : List) -> Any]
  :intent "Evaluate a list of top-level nodes sequentially, threading env"
  :requires [(valid nodes)]
  :ensures [(valid result)]
  :body
    (if (empty? nodes) (Ok [(ValNil) env])
      (match (eval-top-level (head nodes) env)
        (Err e) (Err e)
        (Ok pair)
          (if (empty? (tail nodes))
            (Ok pair)
            (eval-program (tail nodes) (at pair 1))))))
```

- [ ] **Step 4: Add function tests to eval_test.airl**

Copy new functions, then add:

```airl
;; ─── Task 6: Function Tests ─────────────────────────────

(print "=== Task 6: Functions ===")

;; Define and call a simple function
;; (defn double :sig [(x : i64) -> i64] :body (* x 2))
;; (double 21) → 42
(let ([env (make-initial-env)]
      [defn-node (ASTDefn "double"
        (ASTSig [(ASTParam "x" "i64")] "i64")
        nil [] []
        (ASTCall (ASTSymbol "*" 0 0) [(ASTSymbol "x" 0 0) (ASTInt 2 0 0)] 0 0)
        0 0)]
      [call-node (ASTCall (ASTSymbol "double" 0 0) [(ASTInt 21 0 0)] 0 0)])
  ;; Eval defn then call via eval-program
  (match (eval-program [defn-node call-node] env)
    (Ok pair) (assert-eq (unwrap-int (at pair 0)) 42)
    (Err e) (print "  FAIL:" e)))

;; Recursive function: factorial
;; (defn fact :sig [(n : i64) -> i64] :body (if (= n 0) 1 (* n (fact (- n 1)))))
;; (fact 5) → 120
(let ([env (make-initial-env)]
      [defn-node (ASTDefn "fact"
        (ASTSig [(ASTParam "n" "i64")] "i64")
        nil [] []
        (ASTIf
          (ASTCall (ASTSymbol "=" 0 0) [(ASTSymbol "n" 0 0) (ASTInt 0 0 0)] 0 0)
          (ASTInt 1 0 0)
          (ASTCall (ASTSymbol "*" 0 0)
            [(ASTSymbol "n" 0 0)
             (ASTCall (ASTSymbol "fact" 0 0)
               [(ASTCall (ASTSymbol "-" 0 0) [(ASTSymbol "n" 0 0) (ASTInt 1 0 0)] 0 0)]
               0 0)]
            0 0)
          0 0)
        0 0)]
      [call-node (ASTCall (ASTSymbol "fact" 0 0) [(ASTInt 5 0 0)] 0 0)])
  (match (eval-program [defn-node call-node] env)
    (Ok pair) (assert-eq (unwrap-int (at pair 0)) 120)
    (Err e) (print "  FAIL:" e)))

(print "=== Task 6 tests complete ===")
```

- [ ] **Step 5: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Tasks 1-6 complete, no FAIL

- [ ] **Step 6: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add function definition, calls, and recursion"
```

---

### Task 7: Pattern Matching and Variants

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add `try-match-pattern`, `ASTMatch` branch, `ASTVariant` branch, `ASTTry` branch, and `ASTLambda` branch to `eval-node`.

- [ ] **Step 1: Add try-match-pattern to eval.airl**

```airl
;; ─── Pattern Matching ────────────────────────────────────

(defn try-match-pattern
  :sig [(pattern : Any) (value : Any) -> Any]
  :intent "Try to match a pattern against a value, returning (Ok bindings) or (Err msg)"
  :requires [(valid pattern)]
  :ensures [(valid result)]
  :body (match pattern
    (PatWild _ _) (Ok [])
    (PatBind name _ _) (Ok [[name value]])
    (PatLit lit-val _ _)
      (if (= (unwrap-raw value) lit-val) (Ok [])
        (Err "no match"))
    (PatVariant tag sub-pats _ _)
      (match value
        (ValVariant vtag inner)
          (if (= tag vtag)
            (if (empty? sub-pats)
              (Ok [])
              (if (= (length sub-pats) 1)
                ;; Single sub-pattern matches inner directly
                (try-match-pattern (head sub-pats) inner)
                ;; Multiple sub-patterns: inner must be ValList
                (match inner
                  (ValList items) (try-match-patterns sub-pats items)
                  _ (Err "no match"))))
            (Err "no match"))
        _ (Err "no match"))
    _ (Err "no match")))

(defn try-match-patterns
  :sig [(patterns : List) (values : List) -> Any]
  :intent "Match a list of patterns against a list of values, collecting bindings"
  :requires [(valid patterns)]
  :ensures [(valid result)]
  :body
    (if (empty? patterns) (Ok [])
      (if (empty? values) (Err "no match")
        (match (try-match-pattern (head patterns) (head values))
          (Err e) (Err e)
          (Ok binds1)
            (match (try-match-patterns (tail patterns) (tail values))
              (Err e) (Err e)
              (Ok binds2) (Ok (append binds1 binds2)))))))
```

- [ ] **Step 2: Add ASTMatch, ASTVariant, ASTTry, ASTLambda branches to eval-node**

```airl
    (ASTMatch scrutinee arms _ _)
      (match (eval-node scrutinee env)
        (Err e) (Err e)
        (Ok scr-pair)
          (eval-match-arms arms (at scr-pair 0) (at scr-pair 1)))

    (ASTVariant name args _ _)
      (if (empty? args)
        (Ok [(ValVariant name (ValNil)) env])
        (match (eval-args args env)
          (Err e) (Err e)
          (Ok args-pair)
            (let ([vals (at args-pair 0)]
                  [env2 (at args-pair 1)])
              (if (= (length vals) 1)
                (Ok [(ValVariant name (head vals)) env2])
                (Ok [(ValVariant name (ValList vals)) env2])))))

    (ASTTry expr _ _)
      (match (eval-node expr env)
        (Err e) (Err e)
        (Ok pair)
          (match (at pair 0)
            (ValVariant "Ok" inner) (Ok [inner (at pair 1)])
            (ValVariant "Err" inner) (Err (join ["Err: " (unwrap-raw inner)] ""))
            _ (Err "try on non-Result value")))

    (ASTLambda params body _ _)
      (Ok [(ValLambda params body env) env])
```

Also add `eval-match-arms` helper:

```airl
(defn eval-match-arms
  :sig [(arms : List) (value : Any) (env : List) -> Any]
  :intent "Try each match arm against the scrutinee value"
  :requires [(valid arms)]
  :ensures [(valid result)]
  :body
    (if (empty? arms)
      (Err "non-exhaustive match")
      (match (head arms)
        (ASTArm pattern body)
          (match (try-match-pattern pattern value)
            (Ok bindings)
              (let ([match-env (bind-match-bindings (env-push env) bindings)])
                (match (eval-node body match-env)
                  (Err e) (Err e)
                  (Ok pair) (Ok [(at pair 0) (env-pop (at pair 1))])))
            (Err _) (eval-match-arms (tail arms) value env))
        _ (Err "eval: invalid match arm"))))

(defn bind-match-bindings
  :sig [(env : List) (bindings : List) -> List]
  :intent "Bind pattern match capture bindings into the top frame"
  :requires [(valid env)]
  :ensures [(valid result)]
  :body
    (if (empty? bindings) env
      (let ([pair (head bindings)])
        (bind-match-bindings
          (env-bind env (at pair 0) (at pair 1))
          (tail bindings)))))
```

- [ ] **Step 3: Add pattern matching tests to eval_test.airl**

Copy new functions, then add:

```airl
;; ─── Task 7: Pattern Matching Tests ─────────────────────

(print "=== Task 7: Pattern matching ===")

(let ([env (make-initial-env)])
  (do
    ;; Match on variant: (match (Ok 42) (Ok v) v (Err _) 0) → 42
    (match (eval-node
      (ASTMatch
        (ASTVariant "Ok" [(ASTInt 42 0 0)] 0 0)
        [(ASTArm (PatVariant "Ok" [(PatBind "v" 0 0)] 0 0) (ASTSymbol "v" 0 0))
         (ASTArm (PatVariant "Err" [(PatWild 0 0)] 0 0) (ASTInt 0 0 0))]
        0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 42)
      (Err e) (print "  FAIL:" e))

    ;; Match with wildcard
    (match (eval-node
      (ASTMatch (ASTInt 99 0 0)
        [(ASTArm (PatWild 0 0) (ASTStr "matched" 0 0))]
        0 0) env)
      (Ok pair) (assert-eq (unwrap-str (at pair 0)) "matched")
      (Err e) (print "  FAIL:" e))

    ;; Match with literal pattern
    (match (eval-node
      (ASTMatch (ASTInt 1 0 0)
        [(ASTArm (PatLit 1 0 0) (ASTStr "one" 0 0))
         (ASTArm (PatLit 2 0 0) (ASTStr "two" 0 0))
         (ASTArm (PatWild 0 0) (ASTStr "other" 0 0))]
        0 0) env)
      (Ok pair) (assert-eq (unwrap-str (at pair 0)) "one")
      (Err e) (print "  FAIL:" e))

    ;; Variant constructor: (Ok 10)
    (match (eval-node (ASTVariant "Ok" [(ASTInt 10 0 0)] 0 0) env)
      (Ok pair) (match (at pair 0)
        (ValVariant tag inner) (do (assert-eq tag "Ok") (assert-eq (unwrap-int inner) 10))
        _ (print "  FAIL: not a variant"))
      (Err e) (print "  FAIL:" e))

    ;; Try: (try (Ok 7)) → 7
    (match (eval-node (ASTTry (ASTVariant "Ok" [(ASTInt 7 0 0)] 0 0) 0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 7)
      (Err e) (print "  FAIL:" e))

    ;; Lambda: ((fn [x] (+ x 1)) 10) → 11
    (match (eval-node
      (ASTCall
        (ASTLambda ["x"] (ASTCall (ASTSymbol "+" 0 0) [(ASTSymbol "x" 0 0) (ASTInt 1 0 0)] 0 0) 0 0)
        [(ASTInt 10 0 0)]
        0 0) env)
      (Ok pair) (assert-eq (unwrap-int (at pair 0)) 11)
      (Err e) (print "  FAIL:" e))))

(print "=== Task 7 tests complete ===")
```

- [ ] **Step 4: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: Tasks 1-7 complete, no FAIL

- [ ] **Step 5: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add pattern matching, variants, try, and lambdas"
```

---

### Task 8: End-to-End Pipeline

**Files:**
- Modify: `bootstrap/eval.airl`
- Modify: `bootstrap/eval_test.airl`

Add `run-source` that chains lex→parse→eval. Write end-to-end tests that evaluate AIRL source strings.

- [ ] **Step 1: Add run-source to eval.airl**

```airl
;; ─── Full Pipeline ───────────────────────────────────────

(defn run-source
  :sig [(source : Str) -> Any]
  :intent "Lex, parse, and evaluate an AIRL source string"
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
                (eval-program ast-nodes (make-initial-env))))))
```

- [ ] **Step 2: Add end-to-end pipeline tests to eval_test.airl**

This test file must include all function defs from lexer.airl, parser.airl, and eval.airl. Add pipeline tests:

```airl
;; ─── Task 8: End-to-End Pipeline Tests ──────────────────

(print "=== Task 8: Pipeline ===")

;; Simple expression
(match (run-source "(+ 1 2)")
  (Ok pair) (assert-eq (unwrap-int (at pair 0)) 3)
  (Err e) (print "  FAIL:" e))

;; If expression
(match (run-source "(if (= 1 1) 42 0)")
  (Ok pair) (assert-eq (unwrap-int (at pair 0)) 42)
  (Err e) (print "  FAIL:" e))

;; Let expression
(match (run-source "(let ([x 10]) (+ x 5))")
  (Ok pair) (assert-eq (unwrap-int (at pair 0)) 15)
  (Err e) (print "  FAIL:" e))

;; Multi-expression program with defn
(match (run-source "(defn add1 :sig [(x : i64) -> i64] :body (+ x 1)) (add1 99)")
  (Ok pair) (assert-eq (unwrap-int (at pair 0)) 100)
  (Err e) (print "  FAIL:" e))

;; Recursive function via pipeline
(match (run-source "(defn fact :sig [(n : i64) -> i64] :body (if (= n 0) 1 (* n (fact (- n 1))))) (fact 6)")
  (Ok pair) (assert-eq (unwrap-int (at pair 0)) 720)
  (Err e) (print "  FAIL:" e))

(print "=== Task 8 tests complete ===")
(print "=== All eval tests complete ===")
```

- [ ] **Step 3: Run tests**

Run: `cargo run -p airl-driver -- run bootstrap/eval_test.airl 2>&1 | grep -E "FAIL|complete"`
Expected: All 8 task blocks complete, no FAIL

- [ ] **Step 4: Run all existing bootstrap tests to verify no regressions**

Run: `cargo run -p airl-driver -- run bootstrap/lexer_test.airl 2>&1 | grep -E "FAIL|complete"`
Run: `cargo run -p airl-driver -- run bootstrap/parser_test.airl 2>&1 | grep -E "FAIL|complete"`
Run: `cargo run -p airl-driver -- run bootstrap/integration_test.airl 2>&1 | grep -E "FAIL|PASS|complete"`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add bootstrap/eval.airl bootstrap/eval_test.airl
git commit -m "feat(bootstrap): add run-source pipeline and end-to-end tests"
```

---

### Task 9: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update Self-Hosting status in CLAUDE.md**

In the "Self-Hosting (Phase 3)" section, update the status:

```markdown
**Status:** Lexer, parser, and evaluator complete. The self-hosted lexer (`bootstrap/lexer.airl`, ~360 lines) tokenizes AIRL source strings. The self-hosted parser (`bootstrap/parser.airl`, ~250 lines) converts token streams to typed AST nodes. The self-hosted evaluator (`bootstrap/eval.airl`, ~400 lines) interprets AST nodes using tagged value variants (`ValInt`, `ValStr`, etc.), a map-based environment frame stack, and builtin delegation to the Rust runtime. Tested by `bootstrap/eval_test.airl` with progressive tests from atoms through full lex→parse→eval pipeline.
```

Add to Bootstrap Compiler section:

```markdown
cargo run -- run bootstrap/eval_test.airl        # Evaluator tests
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with bootstrap evaluator status"
```
