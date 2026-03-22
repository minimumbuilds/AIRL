# Bootstrap Type Checker Design

**Date:** 2026-03-22
**Status:** Draft
**Scope:** Self-hosted type checker for the bootstrap compiler, per the AIRL Language Specification §3

## Overview

A self-hosted type checker for AIRL, written in pure AIRL. Operates on the AST produced by `bootstrap/parser.airl` and enforces the type system described in the AIRL Language Specification §3: explicit types, no inference, dependent type-level naturals, and linear ownership annotations.

Built interleaved with bootstrap code fixes — one module at a time (lexer → parser → eval), fixing type annotations and verifying with the checker as we go.

## Spec Alignment

The AIRL spec (§3.1) states: "There is no type inference. All types are explicit." The current bootstrap code violates this with heavy use of `Any`. Note that `Any` is not a recognized type in the AIRL type system — it does not appear in the Rust `PrimTy` enum, and the Rust type checker would reject it. The bootstrap code only works because bootstrap files are executed via `cargo run -- run`, which runs evaluation with type warnings (not errors). The bootstrap code has never been type-checked.

**Current state of bootstrap type annotations:**

| Module | Functions | Sigs using `Any` | Total `Any` occurrences | Custom types used | Declared |
|--------|-----------|-------------------|------------------------|-------------------|----|
| Lexer | 17 | 0 | 1 | 1 (Token) | 0 |
| Parser | 19 | 9 | ~24 | 21 (SList, AST*, etc.) | 0 |
| Eval | 24 | 18 | ~83 | 21 (Val*, AST*, Pat*) | 0 |
| **Total** | **60** | **27** | **~108** | **43** | **0** |

### `Any` Migration Strategy

Each module's `Any` annotations are fixed BEFORE that module is type-checked. The interleaved approach guarantees this:

1. Fix lexer annotations → type-check lexer (lexer has ~0 `Any` — essentially free)
2. Fix parser annotations (~24 occurrences) → type-check parser
3. Fix eval annotations (~83 occurrences) → type-check eval

The type checker itself will NOT have an `Any` escape hatch. If `Any` appears in code being checked, it is a type error. This forces the migration to happen in order.

## Architecture

### Pipeline Position

```
source → lex → parse → **type check** → eval
```

The type checker runs after parsing, before evaluation. It takes the AST node list from `parse-program` and either returns `(Ok ())` or `(Err errors)` where errors is a list of `(TypeError msg line col)`.

### File Structure

| File | Purpose | Estimated Lines |
|------|---------|-----------------|
| `bootstrap/types.airl` | Type representations, environment, registry | ~200 |
| `bootstrap/typecheck.airl` | Type checking logic (check-expr, check-fn, etc.) | ~500 |
| `bootstrap/typecheck_test.airl` | Tests | ~500 |

Plus modifications to:
- `bootstrap/parser.airl` — add `deftype` parsing (~80-120 lines)
- `bootstrap/lexer.airl` — add `deftype` declarations, minor annotation fixes (~10 lines)
- `bootstrap/parser.airl` — fix ~24 `Any` annotations to proper types
- `bootstrap/eval.airl` — fix ~83 `Any` annotations to proper types

## Phase 0: Add `deftype` Parsing

Before the type checker can work, the parser needs to handle `deftype` top-level forms so types can be declared in AIRL source.

### Syntax

Must match the Rust parser's `parse_deftype` implementation at `crates/airl-syntax/src/parser.rs:685`. Two forms:

**Sum types** — variant fields are positional (no `: Type` names):
```clojure
(deftype Result [T E]
  (| (Ok T) (Err E)))

(deftype SExpr
  (| (SList List i64 i64)
     (SBracket List i64 i64)
     (SAtom Token)))
```

**Product types** — fields are named with `: Type` syntax:
```clojure
(deftype Token
  (& (kind : String) (line : i64) (col : i64)))
```

### Parser Changes

Add to `bootstrap/parser.airl`:

| Function | Input | Output | Lines |
|----------|-------|--------|-------|
| `parse-deftype` | SList items, line, col | `(Ok (ASTDefType name params body line col))` | ~35 |
| `parse-type-params` | SBracket items | list of `(ASTTypeParam name bound)` | ~20 |
| `parse-sum-body` | SList items after `\|` | `(Ok (ASTSumBody variants))` | ~15 |
| `parse-product-body` | SList items after `&` | `(Ok (ASTProductBody fields))` | ~15 |
| `parse-variant` | SList items | `(ASTVariantDef name field-types)` | ~10 |
| `parse-field` | SList items | `(ASTFieldDef name type-name)` | ~10 |

Add dispatch in `parse-top-level`:
```clojure
"deftype" (parse-deftype (tail items) line col)
```

### New AST Nodes

```clojure
(ASTDefType name type-params body line col)
(ASTTypeParam name bound)
(ASTSumBody variants)           ;; (| ...)
(ASTProductBody fields)         ;; (& ...)
(ASTVariantDef name field-types)
(ASTFieldDef name type-name)
```

## Phase 1: Type Representation

### Type Variants

```clojure
;; Primitives (one per spec §3.2 type)
(TyBool)
(TyI8) (TyI16) (TyI32) (TyI64)
(TyU8) (TyU16) (TyU32) (TyU64)
(TyF16) (TyF32) (TyF64) (TyBF16)
(TyNat)
(TyStr)
(TyUnit)
(TyNever)

;; Compound
(TyFunc param-types ret-type)   ;; param-types: List of types, ret-type: a type
(TyList elem-type)              ;; homogeneous list
(TyNamed name arg-types)        ;; user-defined type (from deftype), with type args
(TyVar name)                    ;; type variable (unresolved/polymorphic)
(TyTensor elem-type shape)      ;; tensor with element type and dimension list
```

### Recursive Types

AST types are inherently recursive (e.g., `ASTIf` contains sub-expressions that are themselves AST nodes). The checker handles this through `TyNamed` — named type references are never expanded inline. When the checker encounters a field typed as `ASTNode`, it stores `(TyNamed "ASTNode" [])`, not the full expanded sum type. Compatibility checking for `TyNamed` compares names and type arguments, never recursively expanding the definition. This is the same strategy the Rust checker uses (structural comparison of `Named { name, args }` at `checker.rs:682-694`).

### Type Resolution

Map AST type name strings to type variants:

```clojure
(defn resolve-type-name [name registry]
  (match name
    "bool"   (Ok (TyBool))
    "Bool"   (Ok (TyBool))
    "i8"     (Ok (TyI8))
    "i16"    (Ok (TyI16))
    "i32"    (Ok (TyI32))
    "i64"    (Ok (TyI64))
    "u8"     (Ok (TyU8))
    "u16"    (Ok (TyU16))
    "u32"    (Ok (TyU32))
    "u64"    (Ok (TyU64))
    "f16"    (Ok (TyF16))
    "f32"    (Ok (TyF32))
    "f64"    (Ok (TyF64))
    "bf16"   (Ok (TyBF16))
    "Nat"    (Ok (TyNat))
    "String" (Ok (TyStr))
    "Str"    (Ok (TyStr))
    "Unit"   (Ok (TyUnit))
    "Never"  (Ok (TyNever))
    "List"   (Ok (TyList (TyVar "_")))
    _        ;; look up in type registry → TyNamed if found, error if not
    ))
```

Both `"String"` and `"Str"` resolve to `(TyStr)`. Both are accepted as aliases.

### Type Environment

Scoped binding stack using the map-based frame pattern from `bootstrap/eval.airl`:

```clojure
;; Environment is a list of frames, each frame is a map
;; Lookup walks frames from top to bottom (innermost scope first)

(defn type-env-new [] [(map-new)])
(defn type-env-push [env] (cons (map-new) env))
(defn type-env-pop [env] (tail env))
(defn type-env-bind [env name ty] ...)  ;; bind in topmost frame
(defn type-env-lookup [env name] ...)   ;; search all frames
```

### Type Registry

Stores `deftype` declarations. Two maps:

1. **Type map:** type-name → `(TypeDef name params body)` — the full definition
2. **Constructor map:** constructor-name → `(CtorInfo parent-type-name field-types)` — for variant lookup

```clojure
;; When (deftype Result [T E] (| (Ok T) (Err E))) is registered:
;; Type map:        "Result" → (TypeDef "Result" ["T" "E"] (SumBody [...]))
;; Constructor map: "Ok"     → (CtorInfo "Result" [(TyVar "T")])
;;                  "Err"    → (CtorInfo "Result" [(TyVar "E")])
```

### Standard Prelude Types

The checker pre-loads a standard set of `deftype` declarations before processing user code. These provide `Result`, `Option`, and other commonly used types:

```clojure
(deftype Result [T E] (| (Ok T) (Err E)))
(deftype Option [T]   (| (Some T) (None)))
```

Module-specific types (Token, SExpr, AST nodes, Val types, Pat types) are declared in their respective source files via `deftype` and processed during the registration pass.

## Phase 2: Expression Checking

### check-expr

Dispatches on AST node kind, returns `(Ok type)` or `(Err (TypeError msg line col))`:

| AST Node | Type Rule |
|----------|-----------|
| `ASTInt` | → `(TyI64)` |
| `ASTFloat` | → `(TyF64)` |
| `ASTStr` | → `(TyStr)` |
| `ASTBool` | → `(TyBool)` |
| `ASTNil` | → `(TyUnit)` |
| `ASTKeyword` | → `(TyStr)` |
| `ASTSymbol name` | → lookup `name` in env; error if undefined |
| `ASTIf cond then else` | cond must be `TyBool`; then and else must agree |
| `ASTLet bindings body` | push scope, bind each (check value type vs declared type), check body, pop scope |
| `ASTDo exprs` | check all, return type of last |
| `ASTCall callee args` | callee must be `TyFunc` or `TyVar("builtin")`; check arg count (for typed builtins) and types; return ret type |
| `ASTMatch scrutinee arms` | check scrutinee, check each arm body (must all agree), bind pattern vars |
| `ASTLambda params body` | push scope, bind params, check body, return `TyFunc` |
| `ASTVariant name args` | lookup constructor in registry, check field types |
| `ASTList items` | all items must have same type; return `TyList(elem-type)` |
| `ASTTry inner` | if inner is `Result(T, E)`, return `T`; else pass through |

### check-fn

```
1. Push scope
2. For each param: resolve declared type, bind name → type in env
3. Resolve declared return type
4. check-expr on body → actual return type
5. Verify actual return type compatible with declared return type
6. Pop scope
7. Bind function name → TyFunc(param-types, ret-type) in outer env
```

### check-pattern

Bind pattern variables into scope with appropriate types:

| Pattern | Binding Rule |
|---------|--------------|
| `PatWild` | no binding |
| `PatBind name` | bind `name → scrutinee-type` (or field type from variant registry lookup) |
| `PatLit value` | no binding (check compatible with scrutinee type) |
| `PatVariant name sub-pats` | look up variant in constructor registry, bind sub-patterns to field types |

### Type Compatibility

Two types are compatible if:
- They are identical
- Either is `TyVar` (type variable — compatible with anything, used for polymorphic builtins and unresolved generics)
- Either is `TyNever` (bottom type)
- Both are numeric: integer↔integer or float↔float coercion allowed (per Rust checker behavior)
- Both are `TyFunc` with compatible param/ret types (structural)
- Both are `TyNamed` with same name and compatible type args
- Both are `TyList` with compatible element types

## Phase 3: Module-by-Module Bootstrap Fixes

### Module ordering and dependencies

Since there is no import system, files are self-contained when tested. The checking order is:

1. **Lexer** — depends on: nothing. Declares `Token` type.
2. **Parser** — depends on: lexer types (`Token`). Declares S-expr types and all AST node types.
3. **Eval** — depends on: parser types (all AST nodes). Declares Val types.
4. **Type checker** — depends on: parser types (AST nodes). Declares Ty types.

When type-checking module N, all `deftype` declarations from modules 1..N must be available in the registry (included in the test file).

### Lexer (~1 `Any` occurrence, ~10 lines of changes)

The lexer is already well-typed. Needs:
- `deftype Token (& (kind : String) (value : String) (line : i64) (col : i64))`
- Fix the 1 `Any` occurrence
- Verify all 17 functions pass type checking

**Return types:** Functions return `List` but actually return `(Ok [...])` or `(Err ...)`. After `Result` is in the prelude, these should be updated to the proper Result type. For Phase 1, `List` is acceptable since `(Ok ...)` / `(Err ...)` are constructed as variant values inside lists.

### Parser (~24 `Any` occurrences, ~50 lines of changes)

1. **Token accessors** (`token-line`, `token-col`, `token-kind`, `token-value`) — param `Any` → `Token`
2. **S-expr processors** (`parse-atom`, `parse-expr`, `parse-let-binding`, `parse-pattern`, `parse-param`, `parse-sig`, `parse-top-level`) — param `Any` → `SExpr`
3. **Defn walker** (`walk-defn-clauses`) — `sig`/`body` params `Any` → `SExpr`/`ASTNode`
4. **Let bindings** — ~14 `Any` annotations → proper types

### Eval (~83 `Any` occurrences, ~100 lines of changes)

1. **Value unwrappers** (6 fns) — param `Any` → `Val`
2. **Environment functions** — val params/returns `Any` → `Val`
3. **Eval functions** — `(node : Any)` → `ASTNode`, returns → `Val`
4. **Builtin dispatch** — return `Any` → `Val`
5. **Let bindings** — ~60+ `Any` annotations → proper types
6. **Dummy params** — `(_u : Any)` in `env-new` and `make-initial-env` → `(_u : Unit)`

## Builtin Registration

The type checker pre-registers builtin functions in the type environment.

### Typed builtins (full signatures)

| Builtin | Signature |
|---------|-----------|
| `+`, `-`, `*`, `/`, `%` | `TyFunc([TyI64, TyI64], TyI64)` |
| `<`, `>`, `<=`, `>=`, `=`, `!=` | `TyFunc([TyI64, TyI64], TyBool)` |
| `and`, `or` | `TyFunc([TyBool, TyBool], TyBool)` |
| `not` | `TyFunc([TyBool], TyBool)` |
| `length` | `TyFunc([TyVar "collection"], TyI64)` |
| `at` | `TyFunc([TyList (TyVar "T"), TyI64], TyVar "T")` |
| `head` | `TyFunc([TyList (TyVar "T")], TyVar "T")` |
| `tail` | `TyFunc([TyList (TyVar "T")], TyList (TyVar "T"))` |
| `empty?` | `TyFunc([TyList (TyVar "T")], TyBool)` |
| `cons` | `TyFunc([TyVar "T", TyList (TyVar "T")], TyList (TyVar "T"))` |
| `append` | `TyFunc([TyList (TyVar "T"), TyVar "T"], TyList (TyVar "T"))` |
| `char-at` | `TyFunc([TyStr, TyI64], TyStr)` |
| `substring` | `TyFunc([TyStr, TyI64, TyI64], TyStr)` |
| `contains` | `TyFunc([TyStr, TyStr], TyBool)` |
| `index-of` | `TyFunc([TyStr, TyStr], TyI64)` |
| `length` (string) | `TyFunc([TyStr], TyI64)` |
| `read-file` | `TyFunc([TyStr], TyStr)` |
| `write-file` | `TyFunc([TyStr, TyStr], TyBool)` |
| `file-exists?` | `TyFunc([TyStr], TyBool)` |

### Polymorphic builtins (arity-checked, types unchecked)

For builtins where precise typing is impractical (print accepts any number of any-typed args, map builtins are generic over key/value types), register with known arity but `TyVar` for argument/return types:

| Builtin | Arity | Notes |
|---------|-------|-------|
| `print` | variadic | Check args exist, return `TyUnit` |
| `type-of` | 1 | Return `TyStr` |
| `valid` | 1 | Return `TyBool` |
| `map-new` | 0 | Return `TyVar "map"` |
| `map-get`, `map-has`, `map-set`, `map-remove`, `map-keys`, `map-values`, `map-size` | 2-3 | Arity checked, types unchecked |
| `split`, `join`, `trim`, `to-upper`, `to-lower`, `replace`, `starts-with`, `ends-with` | 1-3 | String builtins, arity checked |

## Scoped Out (Future Work)

The following are explicitly NOT part of this design:

- **Match exhaustiveness checking** — The spec (§7.2) requires "exhaustive, compiler-verified" matches. The Rust compiler has `exhaustiveness.rs` for this. Deferred to a future phase — the current Rust checker does not enforce exhaustiveness either (it accepts any match with at least one arm).
- **Linear ownership / borrow checking** — Spec §3.4 requires static ownership verification. This is a separate checker (`LinearityChecker` in Rust) and will be built as a separate bootstrap module after the type checker.
- **Dependent type verification** — Spec §3.5 describes type-level `Nat` for tensor dimensions. The type representation includes `TyNat` and `TyTensor`, but dimension constraint solving (Z3) is deferred.
- **Full generic type instantiation** — When `Result[i64, String]` is used, the checker does not substitute type variables in the constructor field types. It uses `TyVar` compatibility (any TyVar matches anything). Full generic instantiation is deferred.

## Error Reporting

```clojure
(TypeError msg line col)
```

The type checker accumulates errors (does not stop at first error) and returns the full list. Error messages follow the Rust checker's format:
- `"undefined symbol: \`name\`"`
- `"type mismatch: expected TyI64, got TyStr"`
- `"if branches have different types: TyI64 vs TyBool"`
- `"function expects 2 arguments, got 3"`
- `"unknown type: \`Any\`"` — produced when encountering unfixed annotations

## Entry Point

```clojure
(defn type-check-program
  :sig [(nodes : List) (prelude-types : List) -> List]
  :intent "Type-check a list of AST nodes. Returns (Ok ()) or (Err [TypeError ...])"
  :requires [(valid nodes)]
  :ensures [(valid result)]
  :body ...)
```

Two passes over the AST:
1. **Registration pass** — process all `ASTDefType` nodes, populate the type registry with constructor info
2. **Checking pass** — check all `ASTDefn` and expression nodes against the registry and environment

The `prelude-types` parameter allows pre-loading type declarations from earlier modules (e.g., when checking the parser, pass the lexer's `Token` type).

## Testing Strategy

Tests are written in AIRL and run through the bootstrap pipeline:

**Tier 1 — Type resolution:**
- Primitive type names resolve correctly
- Unknown type names produce errors
- `"Any"` produces an error (not a valid type)

**Tier 2 — Expression checking:**
- Literals return correct types
- Symbol lookup works / undefined symbol errors
- If condition must be Bool, branches must agree
- Let binding type annotations checked
- Function call arg count/type checking
- Builtin arity checking

**Tier 3 — Function checking:**
- Body return type matches declared return type
- Function registered in env after checking
- Pattern matching binds correct types
- Variant constructors checked against registry

**Tier 4 — Integration:**
- Type-check the lexer source (should pass with 0 errors)
- Type-check the parser source (after fixes, 0 errors)
- Type-check the eval source (after fixes, 0 errors)
- Type-check the type checker itself (deferred — the checker's own types need to be declared first)

## Dependencies

- **Parser:** `bootstrap/parser.airl` (with `deftype` support added)
- **Lexer:** `bootstrap/lexer.airl` (no changes needed to logic)
- **Stdlib:** `map-new`, `map-get`, `map-set`, `map-has`, `map-keys` (map builtins for env/registry)
- **Builtins:** `length`, `at`, `head`, `tail`, `empty?`, `cons`, `print`

## AIRL Constraints

Same as all bootstrap code:
- **Eager `and`/`or`:** Use nested `if` for short-circuit logic
- **No import system:** Test file must be self-contained
- **Self-TCO through match/let:** Recursive checking functions will benefit from the TCO fix
- **`String` / `Str` aliasing:** Both accepted, both resolve to `TyStr`
