# Self-Hosted Lexer — Design Spec

**Date:** 2026-03-22
**Purpose:** First component of AIRL self-hosting (Phase 3). A lexer written in pure AIRL that tokenizes AIRL source strings, proving the language is expressive enough for compiler construction.

## Overview

A pure AIRL program that takes a source string and produces a list of tokens. Located at `bootstrap/lexer.airl` as a user-space program, not a stdlib module. Tested by `bootstrap/lexer_test.airl` and a Rust fixture test.

## Token Representation

Tokens are AIRL variants: `(Token kind value line col)`

| Kind | Value | Example |
|------|-------|---------|
| `"integer"` | `i64` | `(Token "integer" 42 1 0)` |
| `"float"` | `f64` | `(Token "float" 3.14 1 5)` |
| `"string"` | `String` (unescaped) | `(Token "string" "hello" 2 3)` |
| `"symbol"` | `String` | `(Token "symbol" "defn" 3 1)` |
| `"keyword"` | `String` (colon stripped) | `(Token "keyword" "sig" 3 7)` |
| `"bool"` | `Bool` | `(Token "bool" true 4 0)` |
| `"nil"` | `nil` | `(Token "nil" nil 4 5)` |
| `"lparen"` | `"("` | `(Token "lparen" "(" 1 0)` |
| `"rparen"` | `")"` | `(Token "rparen" ")" 1 5)` |
| `"lbracket"` | `"["` | `(Token "lbracket" "[" 2 0)` |
| `"rbracket"` | `"]"` | `(Token "rbracket" "]" 2 3)` |
| `"colon"` | `":"` | `(Token "colon" ":" 3 5)` |
| `"arrow"` | `"->"` | `(Token "arrow" "->" 3 10)` |
| `"comma"` | `","` | `(Token "comma" "," 3 15)` |
| `"eof"` | `nil` | `(Token "eof" nil 5 0)` |

Matches the Rust `TokenKind` enum. A `"colon"` token is produced when `:` appears without a following symbol (bare colon).

## Architecture

### State Passing

No mutable state. Lexer state is a 4-tuple `(source pos line col)` threaded through recursive calls. Sub-lexers return `(Ok [token pos line col])` or `(Err msg)` to propagate errors.

### Core Functions

| Function | Signature | Purpose |
|----------|-----------|---------|
| `lex` | `(lex source) → (Ok tokens) or (Err msg)` | Entry point. Calls `lex-loop`, reverses result on success. |
| `lex-loop` | `(lex-loop source pos line col tokens) → (Ok tokens) or (Err msg)` | Accumulator. Checks `pos >= length` for EOF. Calls `next-token`, conses result, recurses. Propagates errors from `next-token`. |
| `next-token` | `(next-token source pos line col) → (Ok [token pos line col]) or (Err msg)` | Skip whitespace, dispatch on first char. Checks bounds before `char-at`. |
| `skip-ws` | `(skip-ws source pos line col) → (Ok [pos line col]) or (Err msg)` | Skip spaces, tabs, newlines, `;` line comments, `#\|...\|#` block comments (nestable, tracked with depth counter). |

### Bounds Checking

`lex-loop` checks `(>= pos (length source))` before calling `next-token`. If at end, it conses an EOF token and returns. `next-token` also guards `char-at` calls with bounds checks. This prevents `char-at` from erroring on out-of-bounds access.

### Dispatch Table (in `next-token`)

After whitespace is skipped, dispatch on `(char-at source pos)`:

```
"("  → Token lparen, advance 1
")"  → Token rparen, advance 1
"["  → Token lbracket, advance 1
"]"  → Token rbracket, advance 1
","  → Token comma, advance 1
"\"" → lex-string
":"  → if next char is symbol-char: lex-keyword
       else: Token colon, advance 1
"-"  → if next char is ">": Token arrow, advance 2
       if next char is digit: lex-number (negative)
       else: lex-symbol
0-9  → lex-number
else → lex-symbol (checks for true/false/nil at end)
```

### Sub-Lexers

Each returns `(Ok [token pos line col])` or `(Err msg)`.

| Function | Handles |
|----------|---------|
| `lex-string` | Read until closing `"`, handle `\n \t \\ \"` escapes. Returns `(Err ...)` on unterminated string. |
| `lex-number` | Read digits, detect `.` for float, parse via arithmetic. Only called after confirming first char is a digit (or `-` followed by digit). |
| `lex-keyword` | Skip `:`, read symbol chars, return keyword token |
| `lex-symbol` | Read symbol-legal chars, then check if value is `true`/`false`/`nil` |

### Error Propagation

Sub-lexers return `(Ok [token pos line col])` on success or `(Err msg)` on failure. `lex-loop` matches on each `next-token` result:
- `(Ok [token pos line col])` → cons token, recurse with new state
- `(Err msg)` → return `(Err msg)` immediately (short-circuit)

### Helper Predicates

Use string-contains for character classification — no character codes needed:

| Function | Implementation |
|----------|---------------|
| `is-digit?` | `(contains "0123456789" ch)` |
| `is-symbol-start?` | `(contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!$%&*+-./<=>?@^_~" ch)` |
| `is-symbol-char?` | `(or (is-symbol-start? ch) (is-digit? ch))` |
| `is-whitespace?` | `(or (= ch " ") (= ch "\t") (= ch "\n") (= ch "\r"))` |

The `is-symbol-start?` character set is derived from the Rust lexer's `is_symbol_start` function.

## Number Parsing

No `parseInt`/`parseFloat` builtins exist, so numbers are parsed from characters.

**Integers:** Walk digits, accumulate: `(+ (* acc 10) (index-of "0123456789" ch))`. Handle leading `-` as a sign flag. `index-of` is safe here because `lex-number` is only called after confirming the character is a digit.

**Floats:** Same digit walk; when `.` is hit, switch to fractional mode. Track a `divisor` starting at `10.0` (float literal to force float division), each fractional digit contributes `(/ digit divisor)`. The integer base is coerced to float via `(+ base 0.0)` before adding the fractional part.

**Phase 1 scope:** Decimal only. Hex (`0x`) and binary (`0b`) prefixes are skipped — rarely used in AIRL programs.

## Block Comment Nesting

Block comments `#|...|#` are nestable. `skip-ws` tracks a depth counter. When it sees `#|`, depth increments; when it sees `|#`, depth decrements. The comment ends when depth returns to 0. This matches the Rust lexer's behavior.

## Error Handling

`lex` returns `(Ok tokens)` on success, `(Err "message at line:col")` on failure. Error conditions: unterminated string, unterminated block comment, unexpected character.

## Recursion Budget

`lex-loop` recurses once per token. Sub-lexers recurse once per character within a token. For a source file with N tokens averaging M characters each, the max recursion depth is approximately N + M (not N*M, since sub-lexer recursion unwinds before the next `lex-loop` call). With the 50K limit, this comfortably handles source files with tens of thousands of tokens — well beyond the size of any AIRL program needed for bootstrapping.

## Testing

**`bootstrap/lexer_test.airl`** — AIRL test program using an inline `assert-eq` helper:

- Single-character tokens: `( ) [ ] , :`
- Multi-character tokens: `->`, symbols, keywords
- Literals: integers, negative integers, floats, strings with escapes, booleans, nil
- Comments: line comments skipped, block comments skipped (including nested)
- Multi-line: line/col tracking across newlines
- Errors: unterminated string returns `(Err ...)`

**Rust fixture test:** `tests/fixtures/valid/lexer_bootstrap.airl` — run lexer on small input, check token count and first token.

## Dependencies

Uses only existing AIRL builtins and stdlib:
- `char-at`, `substring`, `length` — character access
- `contains`, `index-of` — character classification and digit parsing
- `+`, `-`, `*`, `/`, `=`, `<`, `>`, `>=` — arithmetic and comparison
- `head`, `tail`, `cons`, `empty?` — list building
- `reverse` — reverse accumulated token list (built in cons order)

## Non-Goals

- Hex/binary number literals (add later)
- Source maps or rich span objects (line/col in token is sufficient)
- Incremental lexing or streaming
- Performance optimization (correctness first)
