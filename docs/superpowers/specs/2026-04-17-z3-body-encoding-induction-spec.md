# Z3 Body Encoding & Inductive Verification — Implementation Spec

**Date:** 2026-04-17
**Status:** Draft
**Priority:** Highest — blocks correct verification of all ecosystem contracts
**Scope:** Both compilers (Rust `airl-solver` + g3 `z3_bridge_g3.airl`)
**Prerequisite:** None (builds on existing Z3 infrastructure)
**Cascading effect:** Requires full ecosystem re-sweep after implementation

## Problem Statement

The Z3 contract verifier has a fundamental gap: **function bodies are not fully encoded into SMT-LIB assertions**, so `result` is treated as an unconstrained free variable. This causes two classes of incorrect results:

1. **False DISPROVEN**: A valid implementation is reported as contract-violating because Z3 can find a `result` value that breaks the postcondition — since it doesn't know `result` is constrained by the body.
2. **Deceptive workarounds**: Developers use `(valid result)` (always true — line 414 of z3_bridge_g3.airl) or `(abs 0)` tricks to suppress false DISPROVENs, which silently erases real contracts. This happened in issues 144-147.

### Concrete Example

```airl
(defn clamp-positive
  :sig [(x : i64) -> i64]
  :requires [(valid x)]
  :ensures [(>= result 0)]
  :body (if (>= x 0) x 0))
```

**Expected:** PROVEN (body always returns ≥ 0)
**Actual (g3):** UNKNOWN or DISPROVEN — `z3-try-translate-body` returns 0 for the `if` expression, so `result` is unconstrained. Z3 finds `result = -1` as a counterexample.
**Actual (Rust):** PROVEN — `translate_int` handles `if` expressions correctly and binds `result = ite(x >= 0, x, 0)`.

The Rust compiler already handles this case. The g3 compiler does not, despite having the expression translator infrastructure.

## Current Architecture

### Rust Compiler (`crates/airl-solver/`)

| File | Role |
|------|------|
| `prover.rs` | Main verify loop: declare params → assert requires → bind result=body → negate ensures → check |
| `translate.rs` | AIRL AST → Z3 AST translation. Sort-stratified: `translate_int`, `translate_bool`, `translate_real`, `translate_string` |

**Body encoding status:**
- ✅ Int/Bool/Real/String returns: full expression translation (if, let, do, match, arithmetic, comparisons, string ops)
- ✅ Unknown function calls: uninterpreted function declarations (Z3 congruence reasoning)
- ❌ Seq (List) returns: explicitly skipped (line 182: `Some(VarSort::Seq) => false`)
- ❌ Recursive calls: treated as uninterpreted → body-bound but recursive relationship lost
- ❌ Higher-order functions (fold, map, filter): TranslateError
- ❌ Side effects (IO, mutation): TranslateError
- ❌ ADT constructor patterns in match: not encoded

### g3 Compiler (`bootstrap/z3_bridge_g3.airl`)

| Function | Lines | Role |
|----------|-------|------|
| `z3-verify-function` | 895-947 | Main verify: params → requires → body → ensures → check |
| `z3-try-translate-body` | 874-893 | **Conservative stub** — only handles atoms |
| `z3-translate-ast-expr` | 378-448 | **Full expression translator** — handles if, let, do, match, calls |
| `z3-translate-ast-op` | 529-624 | Operator translation (arithmetic, comparison, logic, strings, lists) |
| `z3-translate-ast-let` | 631-646 | Let binding translation with param-map extension |

**Body encoding status:**
- ✅ `z3-translate-ast-expr` handles: if, let, do, match, all binary ops, comparisons, logic, string ops, list ops, quantifiers
- ❌ `z3-try-translate-body` only handles: integer literals, boolean literals, symbol refs — **the full translator is available but not called**
- ❌ Uninterpreted function fallback exists (line 610-623) but body translation doesn't reach it
- ❌ Recursive calls: would cause infinite recursion in `z3-translate-ast-expr`

## Prerequisite Bugs (Phase 0)

These bugs exist independently of body encoding but become **blocking** once body encoding is enabled.

### 0A. g3: `result` Sort Is Always `Int`

**File:** `bootstrap/z3_bridge_g3.airl`, line 906

```airl
(let (result-const : _ (z3-make-named-const ctx "result" int-sort))
```

`result` is always declared with `int-sort` regardless of the function's return type. A function returning `Bool` gets `result : Int`, which causes sort mismatches when the body is a boolean expression.

**Fix:** Pass the return type through `z3-verify-function` and use `z3-sort-for-type` to pick the correct sort:

```airl
;; In z3-verify-function signature, add return-type parameter
;; Then:
(let (result-sort : _ (z3-sort-for-type ctx return-type))
  (let (result-const : _ (z3-make-named-const ctx "result" result-sort))
    ...))
```

**Propagation:** `z3-verify-defn` (line 966) must extract the return type from `ASTSig` and pass it. Currently it only extracts `params`.

### 0B. g3: Uninterpreted Functions Are Sort-Blind

**File:** `bootstrap/z3_bridge_g3.airl`, lines 508-527 and 610-623

`z3-apply-uninterpreted` hardcodes all parameter sorts and return sort to `int-sort`:

```airl
(let (fd : _ (airl_z3_mk_func_decl1 ctx op int-sort int-sort))
```

This means calling a Bool-returning function from a contract gets the wrong sort. Currently harmless because body encoding rarely succeeds, but Phase 2's inductive hypothesis injection requires correct sorts for recursive calls.

**Fix:** Look up the function's actual parameter and return types from a type environment. This requires threading type information through the verification pipeline — either via a global function-signature map built during AST traversal, or by extending the param-map to carry sort metadata.

### 0C. Cache Key Must Include Body Hash

**File:** `bootstrap/z3_cache.airl`

The cache key is `SHA-256(name|params|requires|ensures)` — the body is excluded. Once body encoding works, changing a function's body without changing its contracts returns a stale cached result.

**Fix:** Include the body's string representation (or hash) in the cache key. The cache entry format becomes: `SHA-256(name|params|requires|ensures|body-hash)`.

**Note:** This invalidates all existing cache entries. Add a cache version discriminator or simply clear `.g3-z3-cache` when deploying.

---

## Implementation Plan

### Phase 1: Full Body Encoding for Non-Recursive Functions

**Goal:** Use `z3-translate-ast-expr` (which already works) instead of the conservative `z3-try-translate-body` stub. This is the single highest-impact change. **Requires Phase 0A (result sort fix) to be done first.**

#### 1A. g3 Compiler — Replace `z3-try-translate-body`

**File:** `bootstrap/z3_bridge_g3.airl`, lines 874-893

**Current** (conservative stub):
```airl
(defn z3-try-translate-body
  :body
    (let (ty : String (type-of body-ast))
      (if (= ty "string")
        (z3-translate-expr ctx body-ast param-map int-sort)
        (match body-ast
          (ASTInt value _ _) ...
          (ASTBool value _ _) ...
          (ASTSymbol name _ _) ...
          _ 0))))  ;; ← gives up on everything else
```

**Target:** Delegate to full translator with error recovery:
```airl
(defn z3-try-translate-body
  :body
    (z3-translate-ast-expr ctx body-ast param-map int-sort))
```

That's it. `z3-translate-ast-expr` already returns 0 for unsupported expressions, which `z3-verify-function` handles correctly (sets `body-bound` to false → reports "unknown" instead of false "disproven").

**Risk:** Low — `z3-translate-ast-expr` has been tested against all contract expressions across the ecosystem. The only new input is function body ASTs, which use the same AST node types.

**Potential issue:** Recursive function bodies will cause `z3-translate-ast-expr` to recurse into the function call, which will be treated as an uninterpreted function (line 610-623: `z3-is-user-function` check). This is correct behavior — it won't infinite-loop because the recursive call name resolves through `z3-is-user-function → true → uninterpreted function declaration`, not through re-entering body translation.

#### 1B. Rust Compiler — Already Done

The Rust compiler already does full body encoding via `translate_int`/`translate_bool`/`translate_real`/`translate_string`. No changes needed for Phase 1.

#### 1C. Verification

After Phase 1:
- All non-recursive functions with arithmetic/comparison/logic/if/let/match bodies → PROVEN or real DISPROVEN
- Recursive functions → UNKNOWN (recursive calls become uninterpreted)
- Functions with fold/map/filter/IO → UNKNOWN (unsupported expressions return 0)
- The `(valid result)` and `(abs 0)` workarounds become unnecessary for non-recursive functions

### Phase 2: Inductive Verification for Recursive Functions

**Goal:** When a function calls itself recursively, assume the inductive hypothesis (the function's own `:ensures` holds for recursive calls) and prove the postcondition.

#### Background: Why Standard Z3 Can't Do This

Z3's `define-fun-rec` exists but Z3 does NOT reason about recursive function properties automatically. It's essentially an axiom that unfolds one step. To prove properties of recursive functions, you need **induction** — which Z3 does not perform natively.

The standard approach in program verification (Dafny, F*, Liquid Haskell) is:

1. **Detect recursive calls** in the function body
2. **Replace** each recursive call `f(args')` with a fresh variable `rec_result_i`
3. **Assert the inductive hypothesis**: the function's own `:ensures` holds for each `rec_result_i`, given that `:requires` holds for `args'`
4. **Bind `result`** to the body with recursive calls replaced
5. **Prove** that `:ensures` holds for `result`

This proves the **inductive step**. The **base case** is automatically verified because non-recursive branches don't involve recursive calls.

#### 2A. Recursive Call Detection

Both compilers need a predicate: "does this function body contain a call to itself?"

**g3 implementation:**
```airl
(defn z3-body-has-self-call
  :sig [(body : _) (fn-name : String) -> Bool]
  :intent "Check if body AST contains a recursive call to fn-name"
  :body
    (match body
      (ASTCall callee args _ _)
        (let (op : String (match callee (ASTSymbol n _ _) n _ ""))
          (if (= op fn-name) true
            (z3-any-has-self-call args fn-name)))
      (ASTIf c t e _ _)
        (if (z3-body-has-self-call c fn-name) true
          (if (z3-body-has-self-call t fn-name) true
            (z3-body-has-self-call e fn-name)))
      (ASTLet bindings body _ _)
        (if (z3-bindings-have-self-call bindings fn-name) true
          (z3-body-has-self-call body fn-name))
      (ASTDo exprs _ _)
        (z3-any-has-self-call exprs fn-name)
      (ASTMatch scrut arms _ _)
        (if (z3-body-has-self-call scrut fn-name) true
          (z3-arms-have-self-call arms fn-name))
      _ false))
```

**Rust implementation:** Walk `ExprKind` tree, check for `FnCall` where callee is `SymbolRef(name)` matching the function name.

#### 2B. Inductive Hypothesis Injection

**Approach:** For each recursive call `f(a1, a2, ..., an)` in the body:

1. Create a fresh Z3 variable `__rec_result_k` with the function's return sort
2. Assert `requires(a1, ..., an)` — the precondition applied to the recursive call's arguments
3. Assert `ensures(__rec_result_k)` — the postcondition with `result` bound to `__rec_result_k`
4. In the body translation, replace the recursive call with `__rec_result_k`

This is equivalent to saying: "assume the recursive call returns a value satisfying the contract."

**g3 implementation sketch:**

New function `z3-translate-body-with-induction`:
```airl
(defn z3-translate-body-inductive
  :sig [(ctx : _) (fn-name : String) (body : _) (param-map : List)
        (int-sort : _) (solver : _) (params : List)
        (requires-list : List) (ensures-list : List) (rec-counter : i64) -> _]
  :intent "Translate body, replacing recursive calls with fresh vars + IH assertions"
  :body
    (match body
      (ASTCall callee args _ _)
        (let (op : String (match callee (ASTSymbol n _ _) n _ ""))
          (if (= op fn-name)
            ;; Recursive call detected: create fresh result var, assert IH
            (let (rec-name : String (str "__rec_" (int-to-string rec-counter)))
              (let (rec-var : _ (z3-make-named-const ctx rec-name int-sort))
                ;; Build param-map binding recursive call args to param names
                ;; Assert requires(args) and ensures(rec-var) on solver
                ;; Return rec-var as the translated expression
                rec-var))
            ;; Non-recursive call: translate normally
            (z3-translate-ast-expr ctx body param-map int-sort)))
      ;; ... recurse through if/let/do/match, threading rec-counter
      _ (z3-translate-ast-expr ctx body param-map int-sort)))
```

**Rust implementation:** Same pattern in `translate.rs` — when encountering `FnCall` where callee matches the current function name, create a fresh Z3 variable, assert IH, substitute.

#### 2C. Termination Argument (Important Limitation)

Induction is only sound if the function terminates. Without a termination proof, the inductive hypothesis could be vacuously true (the function diverges, so the postcondition never needs to hold).

**Pragmatic approach for AIRL:**
- **Structural recursion** (argument strictly decreasing via `tail`, `(- n 1)`, etc.) is the common pattern in the ecosystem
- AIRL already has the linearity checker — extend it to flag non-obviously-terminating recursion as a warning
- For Phase 2, **trust structural recursion by default** and add a `:decreases` annotation for non-obvious cases later

```airl
;; Future: explicit termination measure
(defn factorial
  :sig [(n : i64) -> i64]
  :requires [(>= n 0)]
  :ensures [(>= result 1)]
  :decreases [n]  ;; ← optional annotation, default inferred for (- n 1) patterns
  :body (if (= n 0) 1 (* n (factorial (- n 1)))))
```

For Phase 2 implementation, we do NOT implement termination checking — we apply induction optimistically. False proofs due to non-termination are a theoretical risk but not a practical one in the AIRL ecosystem (no infinite recursion in any current code).

#### 2D. Verification

After Phase 2:
- Simple recursive functions (factorial, fibonacci, list-length, fold-like) → PROVEN
- Mutually recursive functions → UNKNOWN (Phase 3)
- Functions with both recursive and non-recursive branches → inductive step verifies recursive branches, base cases verified directly

### Phase 3: Remaining ~7% — Future Work

These constructs remain UNKNOWN after Phases 0-2. Documented here as requirements for future implementation.

#### 3A. Higher-Order Function Inlining (largest gap — ~4% of ecosystem)

**Problem:** `fold`, `map`, `filter` are the most common patterns across the ecosystem. Every repo uses `fold` for accumulation. These are opaque to Z3 because the lambda argument cannot be translated — Z3 has no notion of functions-as-values.

**Affected functions:** Any function whose body uses `(fold (fn [acc x] ...) init list)`, `(map (fn [x] ...) list)`, or `(filter (fn [x] ...) list)`. These are pervasive in parsers, serializers, and data transformers.

**Possible approaches:**
1. **Compile-time lambda inlining**: When the lambda is a literal (not a variable), inline its definition into a loop-like Z3 encoding. Requires bounded unrolling or quantified axioms over sequences.
2. **Fold-specific axioms**: For `(fold f init xs)`, assert: `result = f(fold(f, init, tail(xs)), head(xs))` with structural induction on `xs`. Essentially treat fold as a recursive function and apply Phase 2 induction.
3. **Abstract interpretation**: Don't try to encode the exact body — instead, derive properties from the lambda. E.g., if lambda body is `(+ acc x)` and all elements are ≥ 0, then result ≥ init.

**Complexity:** High. Approach 2 is most tractable but requires recognizing fold/map/filter as special forms.

#### 3B. Mutual Recursion (~1% of ecosystem)

**Problem:** When `f` calls `g` which calls `f`, single-function induction doesn't work. Need to verify both simultaneously.

**Affected functions:** Rare in current ecosystem. Primarily in parsers (e.g., `parse-expr` ↔ `parse-atom` in some repos).

**Approach:** Strengthened induction — verify both functions' contracts together, assuming both IHs simultaneously. Standard technique from Dafny/F*.

**Complexity:** Medium. Requires detecting SCC (strongly connected components) in the call graph, then verifying each SCC as a unit.

#### 3C. Seq/List Body Encoding (~1% of ecosystem)

**Problem:** Both compilers skip body encoding for List-returning functions. Rust compiler line 182: `Some(VarSort::Seq) => false`. g3 has no Seq body translation at all.

**Affected functions:** Any function returning `List` — list builders, parsers returning token lists, `split`, `words`, etc.

**Approach:** Track Z3 `Seq(Int)` sort through body translation. Encode `cons` as `seq.unit ++ rest`, `[]` as `seq.empty`, `head`/`tail` as `seq.nth`/`seq.extract`. The Z3 sequence theory already supports these operations.

**Complexity:** Medium. Main challenge is the g3 bridge's sort-blindness (F4 partially addresses this). Need sort-aware body translation, not just sort-aware contracts.

#### 3D. IO/Side-Effect Bodies (~1% of ecosystem)

**Problem:** Functions whose body performs IO (`shell-exec`, `read-file`, `write-file`, `println`, network ops) cannot be encoded in Z3. The result depends on external state.

**Affected repos:** AirDB, airlhttp, AirPost, AirLog, airshell — IO-heavy repos.

**Approach:** These will likely always remain UNKNOWN for body encoding. Contracts on IO functions should focus on structural properties of arguments (`:requires`) rather than result properties (`:ensures`). Alternatively, model IO as returning `Result` and verify the pure transformation applied to the result.

**Complexity:** Low (accept UNKNOWN) or Very High (model external state).

#### 3E. Termination Checking (soundness requirement for Phase 2)

**Problem:** Phase 2 induction is only sound if recursive functions terminate. Currently trusted — all ecosystem code terminates structurally. A future non-terminating function could produce a false PROVEN.

**Approach:** Add `:decreases` clause support. Verify that the `:decreases` measure strictly decreases (well-founded ordering) on every recursive call path. Auto-infer for obvious patterns like `(- n 1)` and `(tail xs)`.

**Complexity:** Medium. Requires a well-founded ordering checker and integration with the existing `:requires`/`:ensures` contract system.

## Changes by File

### `bootstrap/z3_bridge_g3.airl`

| Change | Lines | Phase | Description |
|--------|-------|-------|-------------|
| Fix `result` sort to use return type | 906 | 0A | Use `z3-sort-for-type` instead of hardcoded `int-sort` |
| Fix `z3-verify-defn` to pass return type | 966-987 | 0A | Extract return type from `ASTSig`, pass to `z3-verify-function` |
| Fix uninterpreted function sorts | 508-527, 610-623 | 0B | Look up actual param/return sorts instead of hardcoded `int-sort` |
| Replace `z3-try-translate-body` | 874-893 | 1 | Use `z3-translate-ast-expr` instead of atom-only stub |
| Add `z3-body-has-self-call` | new | 2 | Recursive call detection |
| Add `z3-translate-body-inductive` | new | 2 | Body translation with IH injection |
| Modify `z3-verify-function` | 895-947 | 2 | Route recursive functions through inductive path |

**Estimated delta:** ~150 lines added, ~30 lines modified.

### `bootstrap/z3_cache.airl`

| Change | Lines | Phase | Description |
|--------|-------|-------|-------------|
| Include body hash in cache key | cache key computation | 0C | Add body-hash to SHA-256 input |

**Estimated delta:** ~10 lines modified.

### `crates/airl-solver/src/prover.rs`

| Change | Lines | Description |
|--------|-------|-------------|
| Add `is_recursive` helper | new | Check if body contains self-call |
| Add inductive verification path | after line 186 | When recursive: inject IH, translate with substitution |

**Estimated delta:** ~80 lines added.

### `crates/airl-solver/src/translate.rs`

| Change | Lines | Description |
|--------|-------|-------------|
| Add `translate_with_substitution` | new | Like existing translate_* but replaces named calls with fresh vars |

**Estimated delta:** ~60 lines added.

### `crates/airl-driver/src/pipeline.rs`

| Change | Lines | Description |
|--------|-------|-------------|
| Pass function name to prover | ~231, ~267 | Prover needs to know the current function name for recursion detection |

**Estimated delta:** ~5 lines modified.

## Implementation Order

```
Phase 0A: Fix result sort (g3 — must use return type, not hardcoded Int)
    ↓
Phase 0B: Fix uninterpreted function sorts (g3 — type-aware declarations)
    ↓
Phase 0C: Add body hash to Z3 cache key
    ↓
Phase 1A: g3 z3-try-translate-body → z3-translate-ast-expr (1 line change)
    ↓
Phase 1C: Verify against stdlib + ecosystem (g3 --verify)
    ↓
Phase 2A: Recursive call detection (both compilers)
    ↓
Phase 2B: Inductive hypothesis injection (both compilers)
    ↓
Phase 2D: Verify recursive functions across ecosystem
    ↓
ECOSYSTEM RE-SWEEP: All 26 repos through g3 --verify again
```

Phase 0 fixes prerequisite bugs. Phase 1A is then a one-line change. Phase 2 is the substantial work.

## Ecosystem Re-Sweep

After both phases land, a full ecosystem re-sweep is required:

### Why

1. Functions previously reporting UNKNOWN (body not translated) will now report PROVEN or DISPROVEN
2. Functions with `(valid result)` workarounds can be restored to real contracts
3. Some functions may have real contract violations masked by UNKNOWN
4. Recursive functions (common in parsers, list processing, tree traversal) become verifiable

### Expected Impact by Repo

| Category | Repos | Expected change |
|----------|-------|-----------------|
| Pure arithmetic/logic | AirSeal, AirWire, AIReqL | Phase 1 fixes most — bodies are if/let chains |
| Parser/string processing | CairLI, AirParse, airtools, AIRLchart | Phase 1 + Phase 2 — mix of recursive and non-recursive |
| State machines | airline, AirTraffic, AirMux | Phase 1 — mostly non-recursive |
| IO-heavy | AirDB, airlhttp, AirPost, AirLog | Minimal change — IO bodies remain untranslatable |
| Crypto | AirLock, AirSeal | Phase 2 — several recursive hash/encode functions |
| Large codebases | AIRL_castle, airl_kafka_cli, platy-airl | Mix of all categories |

### Sweep Order

Same dependency-layer order as the current sweep:
1. stdlib (should mostly go from UNKNOWN → PROVEN)
2. Layer 1 (no deps) — parallel batch
3. Layer 2 (depends on Layer 1) — parallel batch
4. Layer 3-4 — cascading

### Principle (Unchanged)

**Always move towards enforcement — fix code to pass contracts, never weaken or loosen contracts.**

Any newly-DISPROVEN function has a real implementation bug that was previously hidden by UNKNOWN. Fix the implementation.

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Phase 1 causes new false DISPROVENs | Low | `z3-translate-ast-expr` returns 0 for unsupported → falls back to UNKNOWN, same as before |
| Induction applied to non-terminating function | Very low | No infinite recursion exists in ecosystem; add `:decreases` later |
| Z3 timeout increase from body encoding | Medium | Bodies add more constraints; may need per-function timeout tuning. Current 5s timeout should suffice for most. |
| Ecosystem re-sweep finds many real bugs | Medium | This is the point — better to find them now than in production |
| Phase 2 IH injection interacts badly with quantifiers | Low | Quantifiers in ensures are rare; test carefully |
| Sort mismatch crashes Z3 if Phase 0A skipped | High | Phase 0A is mandatory before Phase 1 — Bool/String body + Int result-const = Z3 sort error |
| Cache returns stale results after body changes | Medium | Phase 0C adds body hash; clear `.g3-z3-cache` on deploy |

## Testing

### Phase 1 Test Cases

1. `clamp-positive` example above → PROVEN
2. `aireql-is-unreserved-char` (if-chain returning bool) → PROVEN
3. `aireql-byte-to-hex` (let + string ops) → UNKNOWN (string construction too complex) or PROVEN
4. Functions with `(valid result)` contracts → still PROVEN (valid = true is trivially satisfied)
5. Functions with IO in body → UNKNOWN (correct — IO can't be encoded)

### Phase 2 Test Cases

1. `factorial(n)` with `:ensures [(>= result 1)]` → PROVEN (IH: factorial(n-1) >= 1, base: 1 >= 1)
2. `list-length(xs)` with `:ensures [(>= result 0)]` → PROVEN
3. `fibonacci(n)` with `:ensures [(>= result 0)]` → PROVEN
4. Mutual recursion `f` ↔ `g` → UNKNOWN (correct — Phase 3)

## Decision Log

| Decision | Rationale |
|----------|-----------|
| Phase 1 before Phase 2 | Phase 1 is a 1-line change with massive impact; Phase 2 requires new code |
| Trust structural recursion without termination proof | Pragmatic — all ecosystem recursion is obviously terminating |
| Same dependency-layer sweep order | Proven to work; parallelism structure is sound |
| Both compilers simultaneously | g3 and Rust must agree on verdicts; Rust is already ahead for Phase 1 |
| Uninterpreted functions for non-recursive unknown calls | Standard SMT technique; preserves soundness |
