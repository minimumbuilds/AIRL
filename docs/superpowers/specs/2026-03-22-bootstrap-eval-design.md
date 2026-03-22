# Bootstrap Evaluator Design Spec

**Goal:** A tree-walking evaluator written in AIRL that interprets AST nodes produced by the bootstrap parser, completing the lex→parse→eval pipeline in AIRL.

**Scope:** Run simple AIRL programs (arithmetic, recursion, list processing, pattern matching) end-to-end through the bootstrap pipeline. Full self-compilation (running the lexer/parser through this evaluator) is a future goal, not in scope.

**Constraints:**
- No external dependencies — only AIRL primitives and stdlib
- No import system — test files must contain all function definitions
- `and`/`or` are eager — use nested `if` for short-circuit logic
- Recursion depth limit is 50K — deep eval chains benefit from the Rust trampoline underneath

---

## Value Representation

Every value produced by the evaluator is a tagged variant:

```airl
(ValInt 42)
(ValFloat 3.14)
(ValStr "hello")
(ValBool true)
(ValNil)
(ValList [v1 v2 v3])            ;; list of Val* values
(ValVariant "Ok" (ValInt 42))    ;; variant with inner value
(ValFn name params body env)     ;; user function closure
(ValLambda params body env)      ;; anonymous function closure
(ValBuiltin "+")                 ;; builtin function reference
```

**Unwrap helpers** extract raw values for builtin dispatch:
- `(unwrap-int v)` — returns integer from `(ValInt n)`, errors otherwise
- `(unwrap-float v)`, `(unwrap-str v)`, `(unwrap-bool v)` — same pattern
- `(unwrap-raw v)` — extracts the raw AIRL value from any Val* (for comparison ops)

---

## Environment

The environment is a **list of maps** (stack of frames). The head is the innermost scope.

```
env = [(map: {x → ValInt(1)})     ;; innermost frame (let/match/function scope)
       (map: {f → ValFn(...)})     ;; outer frame
       (map: {+ → ValBuiltin("+")}) ;; global frame with builtins
       ...]
```

### Operations

| Function | Signature | Description |
|----------|-----------|-------------|
| `env-new` | `() → env` | Single empty map (global frame) |
| `env-push` | `(env) → env` | `(cons (map-new) env)` — push empty frame |
| `env-pop` | `(env) → env` | `(tail env)` — pop innermost frame |
| `env-bind` | `(env name val) → env` | `map-set` on top frame, return new env |
| `env-get` | `(env name) → Result` | Walk frames head→tail, `map-get` each. Returns `(Ok val)` or `(Err "undefined symbol: ...")` |

### Initialization

Before evaluation, the global frame is populated with builtin names:

```airl
(env-bind env "+" (ValBuiltin "+"))
(env-bind env "-" (ValBuiltin "-"))
;; ... all ~25 builtins
```

`(make-initial-env)` creates an env with a single frame and binds all builtin names to their `ValBuiltin` wrappers. This is the starting env passed to `eval-program`.

### Threading

Since AIRL maps are immutable, `env-bind` returns a new environment. The evaluator threads the environment: `(eval-node node env)` returns `(Ok [val new-env])` so that `defn` bindings persist across top-level expressions.

---

## Evaluator Core

`(eval-node node env)` dispatches on AST node type. Returns `(Ok [val env])` or `(Err msg)`.

### Dispatch Table

| AST Node | Behavior |
|----------|----------|
| `ASTInt v _ _` | `(Ok [(ValInt v) env])` |
| `ASTFloat v _ _` | `(Ok [(ValFloat v) env])` |
| `ASTStr v _ _` | `(Ok [(ValStr v) env])` |
| `ASTBool v _ _` | `(Ok [(ValBool v) env])` |
| `ASTNil _ _` | `(Ok [(ValNil) env])` |
| `ASTKeyword k _ _` | `(Ok [(ValStr (join "" [":" k])) env])` |
| `ASTSymbol name _ _` | Look up `name` in env via `env-get` |
| `ASTIf cond then else _ _` | Eval condition; eval chosen branch |
| `ASTLet bindings body _ _` | Push frame, bind each sequentially (threading env), eval body, pop frame |
| `ASTDo exprs _ _` | Eval each expr sequentially (threading env), return last value |
| `ASTMatch scrutinee arms _ _` | Eval scrutinee, try each arm's pattern, eval first matching body |
| `ASTCall callee args _ _` | Eval callee + args, dispatch based on callee type |
| `ASTLambda params body _ _` | Capture current env, return `(ValLambda params body env)` |
| `ASTTry expr _ _` | Eval inner; unwrap `(ValVariant "Ok" v)` → `v`, propagate Err |
| `ASTVariant name args _ _` | Eval args, return `(ValVariant name inner)` |
| `ASTList items _ _` | Eval each item left-to-right (items are arbitrary expressions, not just constants), return `(ValList [...])` |

**Not dispatched directly:** `ASTArm`, `ASTBinding`, `ASTSig`, `ASTParam` are structural sub-nodes — they appear only as children of `ASTMatch`, `ASTLet`, and `ASTDefn` respectively, and are destructured by those handlers rather than by `eval-node`.

### Environment Threading

Most branches pass `env` through unchanged. Branches that modify env:

- **ASTLet:** Push frame → bind each binding (each binding sees previous ones) → eval body → pop frame. Returns outer env (let bindings don't leak).
- **ASTDo:** Thread env through each expression. A `defn` at position N is visible at N+1. Returns final env.
- **ASTDefn (via eval-top-level):** Bind function name in env. Returns updated env.

### Function Calls

When `ASTCall`'s callee evaluates to:

- **`ValBuiltin name`** → call `(call-builtin name evaluated-args)`
- **`ValFn name params body captured-env`** → push frame onto *captured* env (lexical scoping), bind params to args, eval body. Return to *caller's* env.
- **`ValLambda params body captured-env`** → same as ValFn

Lexical scoping means the function body sees the environment from where it was *defined*, not where it was *called*.

---

## Builtin Dispatch

`(call-builtin name args)` maps builtin name strings to actual AIRL operations. Each builtin unwraps args, calls the real AIRL builtin, and wraps the result.

### Required Builtins

| Category | Builtins | Arity |
|----------|----------|-------|
| Arithmetic | `+`, `-`, `*`, `/`, `%` | 2 |
| Comparison | `=`, `!=`, `<`, `>`, `<=`, `>=` | 2 |
| Logic | `and`, `or` | 2 |
| Logic | `not` | 1 |
| Lists | `head`, `tail`, `empty?`, `length` | 1 |
| Lists | `cons`, `at`, `append` | 2 |
| Strings | `char-at`, `substring` | 2-3 |
| Strings | `contains`, `split`, `join` | 2 |
| Strings | `chars` | 1 |
| I/O | `print` | 1 |
| Introspection | `type-of` | 1 |

Argument evaluation order is left-to-right. Before calling a builtin, validate argument count — return `(Err "wrong number of args for <name>")` on mismatch.

### Example

```airl
(if (= name "+")
  (Ok (ValInt (+ (unwrap-int (at args 0)) (unwrap-int (at args 1)))))
(if (= name "head")
  (Ok (head (unwrap-list (at args 0))))
  ...))
```

For `type-of`, the evaluator inspects the Val* tag rather than calling the Rust `type-of` (since values are wrapped).

For comparison builtins (`=`, `<`, etc.), `unwrap-raw` extracts the raw AIRL value so the real operator works directly.

---

## Pattern Matching

`(try-match-pattern pattern value)` → `(Ok bindings)` or `(Err "no match")`

Where `bindings` is a list of `[name val]` pairs.

### Pattern Dispatch

| Pattern | Behavior |
|---------|----------|
| `PatWild _ _` | Always matches, bindings = `[]` |
| `PatBind name _ _` | Always matches, bindings = `[[name value]]` |
| `PatLit lit_val _ _` | Match if `value` equals `lit_val` (unwrap and compare). Type mismatch (e.g., int pattern vs string value) is a non-match, not an error — the evaluator tries the next arm. |
| `PatVariant name sub_patterns _ _` | Value must be `(ValVariant name inner)`. Recursively match sub-patterns against inner fields. |

### Nested Patterns

`PatVariant` sub-patterns can be any pattern type, enabling patterns like `(Ok (Some x))`:

```
PatVariant("Ok", [PatVariant("Some", [PatBind("x")])])
```

### Multi-Field Variants

For multi-argument variants like `(Token kind value line col)`, the evaluator wraps the arguments in a `ValList`: `(ValVariant "Token" (ValList [val-kind val-value val-line val-col]))`. Single-argument variants store the inner value directly: `(ValVariant "Ok" (ValInt 42))`. Zero-argument variants use `ValNil`: `(ValVariant "None" (ValNil))`.

Pattern matching on multi-field variants recursively matches each sub-pattern against the corresponding list element.

### Match Execution

For each `ASTArm` in the match:
1. Call `try-match-pattern` with the arm's pattern and the scrutinee value
2. On match: push frame, bind all captured names from `bindings`, eval arm body, pop frame, return result
3. If no arm matches: `(Err "non-exhaustive match")`

---

## Top-Level Evaluation

### eval-top-level

`(eval-top-level node env)` handles top-level forms:

- **`ASTDefn name sig intent requires ensures body line col`** → Extract param names from `sig`: the `ASTSig` contains a list of `ASTParam(name, type_name)` nodes — map over them to collect a list of name strings. Create `(ValFn name param-names body env)`. Bind in env. Returns `(Ok [ValNil updated-env])`.
- **Any other node** → delegate to `eval-node`

### eval-program

`(eval-program nodes env)` evaluates a list of top-level AST nodes sequentially, threading env:

```airl
(defn eval-program
  :sig [(nodes : List) (env : List) -> List]
  :body
    (if (empty? nodes) (Ok [(ValNil) env])
      (let ([result (eval-top-level (head nodes) env)])
        (match result
          (Ok pair) (eval-program (tail nodes) (at pair 1))
          (Err e) (Err e)))))
```

The last expression's value is the program result.

### Full Pipeline

```airl
(defn run-source
  :sig [(source : Str) -> List]
  :body
    (let ([tokens (lex source)]
          [sexprs (parse-sexpr-all tokens)]
          [ast (parse-program sexprs)]
          [env (make-initial-env)])
      (eval-program ast env)))
```

Each stage returns `(Ok result)` or `(Err error)`, unwrapped via match/let at each step.

---

## File Structure

| File | Purpose |
|------|---------|
| `bootstrap/eval.airl` | Evaluator: env ops, value helpers, eval-node, call-builtin, try-match-pattern, eval-program (~300-400 lines) |
| `bootstrap/eval_test.airl` | Tests — must include all bootstrap function defs (lexer, parser, eval) since no import system |

---

## Test Strategy

Progressive tests in `bootstrap/eval_test.airl`:

1. **Atoms** — eval integer, string, bool, nil literals
2. **Symbol lookup** — eval a symbol after binding it
3. **Arithmetic** — `(+ 1 2)` → `(ValInt 3)`
4. **Comparison/logic** — `(= 1 1)` → `(ValBool true)`
5. **If expression** — true/false branches
6. **Let binding** — `(let ([x 1]) x)` → `(ValInt 1)`
7. **Do block** — sequential evaluation
8. **Defn + call** — define a function, call it
9. **Recursion** — factorial: `(fact 5)` → `(ValInt 120)`
10. **List operations** — `(head [1 2 3])` → `(ValInt 1)`
11. **Pattern matching** — match on variants, nested patterns
12. **Lambda** — `((fn [x] (+ x 1)) 10)` → `(ValInt 11)`
13. **End-to-end pipeline** — lex→parse→eval a multi-function program string

---

## Not In Scope

- Contracts (`:requires`, `:ensures`, `:invariant`)
- JIT compilation
- Ownership/linearity tracking
- Agent builtins (`spawn-agent`, `send`, etc.)
- Type checking
- Trampoline/TCO in the AIRL evaluator itself (relies on Rust trampoline underneath)
- Full self-compilation (running lexer/parser through this evaluator)
