# Self-Hosted Parser Design

**Date:** 2026-03-21
**Status:** Approved
**Scope:** Bootstrap subset (B) — sufficient to parse the lexer and parser themselves

## Overview

A self-hosted parser for AIRL, written in pure AIRL. Takes the token stream from the self-hosted lexer (`bootstrap/lexer.airl`) and produces a typed AST. Uses a two-phase architecture: tokens → S-expressions → AST.

## Scope — Bootstrap Subset

The parser handles the constructs needed to parse itself and the lexer:

**Included:**
- `defn` with `:sig`, `:intent`, `:requires`, `:ensures`, `:body`
- Expressions: `if`, `let`, `do`, `match`, `fn` (lambda), `try`, function calls
- Atoms: integers, floats, strings, booleans, nil, symbols, keywords
- List literals `[...]`
- Variant constructors (capitalized head: `Ok`, `Err`, `Token`, etc.)
- Patterns: wildcard `_`, bindings, literals, variant destructuring (nested)
- Simple named types in signatures

**Excluded (added later without restructuring):**
- `deftype`, `module`, `task`, `use`
- `forall`/`exists` quantifiers
- Ownership annotations (`own`/`ref`/`mut`/`copy`)
- Struct literals (keyword-field pairs)
- Type applications (`Result[T, E]`)
- `:invariant`, `:execute-on`, `:priority`

## Architecture

### Three-Stage Pipeline

```
source string → lex → tokens → parse-sexpr-all → s-exprs → parse-program → AST
```

Each stage is independently testable. Any stage can fail with a structured error.

### Phase 1: Token → S-Expression Grouping

Groups the flat token stream into a nested tree by matching parentheses and brackets.

**S-Expression nodes:**

```clojure
(SAtom token)                ;; wraps a single Token
(SList items line col)       ;; (...) grouped contents
(SBracket items line col)    ;; [...] grouped contents
```

**Functions:**

| Function | Signature | Purpose |
|----------|-----------|---------|
| `parse-sexpr-all` | `(tokens) → (Ok sexprs) \| (Err ...)` | Entry point; parses all tokens to S-expr list |
| `parse-sexprs` | `(tokens pos) → (Ok [sexprs pos])` | Accumulates S-exprs until EOF/RParen/RBracket |
| `parse-sexpr` | `(tokens pos) → (Ok [sexpr pos]) \| (Err ...)` | Parses one S-expr; dispatches on token kind |

**State threading:** Position is an integer index into the token list. Functions return `(Ok [result new-pos])`. Same pattern as the lexer but walking a list with `(at tokens pos)` instead of a string with `(char-at source pos)`.

**Error cases:**
- Unclosed `(` → `ParseError` with location of opening paren
- Unclosed `[` → `ParseError` with location of opening bracket
- Unexpected `)` or `]` → `ParseError` at the closer's location

**~40 lines of AIRL.**

### Phase 2: S-Expression → AST

Recursive descent over the S-expression tree. Dispatches on the head symbol of each `SList`.

**Top-level dispatch:**

| Head Symbol | Parser | AST Node |
|-------------|--------|----------|
| `"defn"` | `parse-defn` | `ASTDefn` |
| anything else | `parse-expr` | expression node |

**Expression dispatch (SList head):**

| Head | Parser | AST Node |
|------|--------|----------|
| `"if"` | `parse-if` | `ASTIf` |
| `"let"` | `parse-let` | `ASTLet` |
| `"do"` | `parse-do` | `ASTDo` |
| `"match"` | `parse-match` | `ASTMatch` |
| `"fn"` | `parse-lambda` | `ASTLambda` |
| `"try"` | `parse-try` | `ASTTry` |
| Capitalized | (inline) | `ASTVariant` |
| other symbol | (inline) | `ASTCall` |
| non-symbol head | (inline) | `ASTCall` (callee is any expr, e.g. `((fn [x] x) 42)`) |

**SAtom** → `parse-atom` (dispatches on token kind to `ASTInt`, `ASTFloat`, etc.)

**SBracket** → `parse-list-literal` (maps `parse-expr` over items → `ASTList`)

**~200 lines of AIRL.**

## Data Representation

### AST Nodes

```clojure
;; Top-level
(ASTDefn name sig intent requires ensures body line col)

;; Signature
(ASTSig params return-type)
(ASTParam name type-name)

;; Expressions
(ASTInt value line col)
(ASTFloat value line col)
(ASTStr value line col)
(ASTBool value line col)
(ASTNil line col)
(ASTSymbol name line col)
(ASTKeyword name line col)
(ASTIf cond then-expr else-expr line col)
(ASTLet bindings body line col)
(ASTDo exprs line col)
(ASTMatch scrutinee arms line col)
(ASTLambda params body line col)
(ASTCall callee args line col)
(ASTList items line col)
(ASTVariant name args line col)
(ASTTry expr line col)

;; Let binding
(ASTBinding name type-name expr)

;; Match arm
(ASTArm pattern body)

;; Patterns
(PatWild line col)
(PatBind name line col)
(PatLit value line col)
(PatVariant name sub-pats line col)
```

Every AST node carries `line col` from its source location. Variant constructors are distinguished from function calls by the capitalization rule (same as the Rust parser).

### Structured Errors

```clojure
(ParseError msg line col)
```

All functions return `(Ok result)` or `(Err (ParseError msg line col))`. This is matchable by callers and avoids the need for `to-string` on integers.

## Function Catalog

### Phase 1 Functions (~40 lines)

| Function | Input | Output |
|----------|-------|--------|
| `parse-sexpr-all` | token list | `(Ok sexprs)` |
| `parse-sexprs` | tokens, pos | `(Ok [sexprs pos])` |
| `parse-sexpr` | tokens, pos | `(Ok [sexpr pos])` |

### Phase 2 Functions (~200 lines)

| Function | Input | Output |
|----------|-------|--------|
| `parse-program` | sexpr list | `(Ok ast-nodes)` |
| `parse-top-level` | sexpr | `(Ok ast-node)` |
| `parse-defn` | SList items, line, col | `(Ok ASTDefn)` |
| `parse-sig` | SBracket items | `(Ok ASTSig)` — find `->` arrow, params before it, return type after |
| `parse-param` | sexpr | `(Ok ASTParam)` |
| `parse-expr` | sexpr | `(Ok ast-expr)` |
| `parse-atom` | token | `(Ok ast-atom)` |
| `parse-list-literal` | SBracket items, line, col | `(Ok ASTList)` |
| `parse-if` | items, line, col | `(Ok ASTIf)` |
| `parse-let` | items, line, col | `(Ok ASTLet)` |
| `parse-let-binding` | sexpr | `(Ok ASTBinding)` |
| `parse-do` | items, line, col | `(Ok ASTDo)` |
| `parse-match` | items, line, col | `(Ok ASTMatch)` |
| `parse-lambda` | items, line, col | `(Ok ASTLambda)` |
| `parse-try` | items, line, col | `(Ok ASTTry)` |
| `parse-pattern` | sexpr | `(Ok pattern)` |
| `parse-error` | msg, line, col | `(Err (ParseError msg line col))` |

### Entry Point (~10 lines)

| Function | Input | Output |
|----------|-------|--------|
| `parse` | source string | `(Ok ast-nodes)` |

**Total:** ~250 lines of AIRL.

## Defn Keyword Clause Parsing

The `parse-defn` function walks the items list after the name, looking for keyword clauses. Keywords can appear in any order. The walker uses a recursive accumulator pattern:

```
walk-defn-clauses(items pos sig intent requires ensures body) →
  if pos >= length(items): return accumulated values
  if items[pos] is keyword:
    match keyword:
      "sig"      → parse-sig(items[pos+1]), recurse with pos+2
      "intent"   → extract string from items[pos+1], recurse with pos+2
      "requires" → parse exprs from bracket items[pos+1], recurse with pos+2
      "ensures"  → parse exprs from bracket items[pos+1], recurse with pos+2
      "body"     → parse-expr(items[pos+1]), recurse with pos+2
      _          → skip unknown keyword, recurse with pos+2
  else: error "expected keyword clause"
```

This is forward-compatible — new keywords are silently skipped.

## Pattern Matching Details

**Dispatch on S-expression shape:**

| S-expr | Pattern |
|--------|---------|
| `SAtom` symbol `"_"` | `PatWild` |
| `SAtom` symbol (lowercase) | `PatBind` |
| `SAtom` integer/float/string/bool/nil | `PatLit` |
| `SList` with capitalized head | `PatVariant` (recursive sub-patterns) |
| anything else | error |

Nested patterns work naturally: `(Token kind _ _ _)` is an `SList` with head `"Token"`, so it becomes `PatVariant("Token", [PatBind("kind"), PatWild, PatWild, PatWild])`.

## Capitalization Rule

A symbol is considered a variant constructor if its first character is uppercase (A-Z). This matches the Rust parser's behavior. The check uses:

```clojure
(defn is-upper? (ch)
  (contains "ABCDEFGHIJKLMNOPQRSTUVWXYZ" ch))
```

Applied in both `parse-expr` (SList with capitalized head → `ASTVariant`) and `parse-pattern` (SList with capitalized head → `PatVariant`).

## AIRL Constraints

The same constraints from the lexer apply:

- **Eager `and`/`or`:** Use nested `if` for bounds-safe checks
- **No mixed int/float arithmetic:** Not an issue here (parser doesn't do arithmetic)
- **No import system:** Parser file must be self-contained; test file includes all definitions
- **Recursion budget (50K):** Parser recursion depth is bounded by AST nesting depth, which is far smaller than token count. Not a concern.
- **List access:** Use `(at list index)` for positional access, `(head list)` and `(tail list)` for destructuring

## Testing Strategy

**Test file:** `bootstrap/parser_test.airl` (~400-500 lines)

**Tier 1 — S-expr grouping:**
- Simple atoms, flat lists, bracket lists, nested lists
- Error cases: unclosed parens/brackets, unexpected closers
- Multiple top-level forms

**Tier 2 — Atoms & simple expressions:**
- All atom types → correct AST nodes
- List literals, function calls, variant constructors

**Tier 3 — Compound expressions:**
- `if`, `let`, `do`, `match`, `fn`, `try` → correct AST nodes
- Error cases: wrong argument counts, malformed bindings

**Tier 4 — Pattern matching:**
- Wildcard, binding, literal, variant (including nested)

**Tier 5 — defn parsing:**
- Minimal defn, full defn, different keyword orders
- Missing `:body` → error

**Tier 6 — Integration:**
- Parse the lexer source: `(parse (read-file "bootstrap/lexer.airl"))`
- Parse the parser source itself (self-hosting proof)

## Dependencies

- **Lexer:** `bootstrap/lexer.airl` (all functions)
- **Stdlib:** `reverse` (from prelude), string builtins (`char-at`, `contains`, `length`, `substring`), `map` (from prelude)
- **Builtins:** `head`, `tail`, `at`, `empty?`, `cons`, `length`, `read-file`

## Extension Points

Adding full spec coverage later requires only:
- New top-level match arms in `parse-top-level` for `deftype`, `module`, `task`, `use`
- New expression match arms in `parse-expr` for `forall`, `exists`
- Extended `parse-param` for ownership annotations
- Extended `parse-type` for type applications
- New `parse-struct-lit` for keyword-field construction

None of these require restructuring existing code.
