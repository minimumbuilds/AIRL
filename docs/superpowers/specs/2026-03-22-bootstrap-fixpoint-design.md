# Bootstrap Fixpoint Test Design

**Date:** 2026-03-22
**Status:** Draft
**Scope:** Two-tier bootstrapping verification — functional equivalence + compiler fixpoint

## Overview

Prove that AIRL's self-hosted compiler is correctly bootstrapped through two complementary tests: (1) functional equivalence — the compiled path produces the same results as the interpreted path, and (2) compiler fixpoint — the compiler compiled by itself produces identical IR to the compiler compiled by the interpreter.

## Goals

1. **Functional equivalence** — Prove `run-compiled` produces identical results to interpreted `eval-program` across a diverse test suite
2. **Compiler fixpoint** — Prove the compiled compiler produces identical IR output to the interpreted compiler (the classic T-diagram bootstrap proof)
3. **No new Rust code** — Pure AIRL test files using existing builtins and infrastructure
4. **Tiered performance** — Fast equivalence test (seconds) separates from slow fixpoint test (minutes)

## Non-Goals

- Proving the Rust VM is correct (assumed — it has its own unit tests)
- Bootstrapping the lexer or parser independently (they're validated transitively)
- Performance benchmarking (separate concern)
- Full 1,514-line chain fixpoint in CI (stretch goal, not required)

## Architecture

### File Structure

| File | Purpose | Runtime |
|------|---------|---------|
| `bootstrap/equivalence_test.airl` | Interpreted vs compiled output comparison | Seconds |
| `bootstrap/fixpoint_test.airl` | Compiler self-compilation fixpoint proof | Minutes |

### Test 1: Functional Equivalence

**File:** `bootstrap/equivalence_test.airl`

**Dependencies:** lexer.airl + parser.airl + eval.airl + compiler.airl (concatenated, self-contained)

**Approach:** For each test program in a suite of ~20 programs:
1. Parse the source string to AST via `lex` → `parse-sexpr-all` → `parse-program`
2. **Interpreted path:** Run AST through `eval-program` (bootstrap evaluator) → result₁
3. **Compiled path:** Run AST through `compile-program` → `run-ir` → result₂
4. Assert `result₁ == result₂`

**Test suite coverage:**
- Literals: int, float, string, bool, nil
- Arithmetic: +, -, *, /, %, comparisons
- Control flow: if/else, nested if
- Bindings: let, nested let, multiple bindings
- Blocks: do with side effects
- Functions: defn + call, multi-argument, multi-function programs
- Recursion: factorial, fibonacci
- Pattern matching: match on variants (Ok/Err), wildcard, literal patterns, nested patterns
- Lambdas: immediate application, closure capture
- Higher-order: function passed as argument
- Lists: list literals, head/tail/cons
- Variants: constructor calls, nested variants

**Key detail:** `eval-program` returns AIRL `Value` variants (`ValInt`, `ValStr`, etc.) while `run-ir` returns Rust `Value` types (`Int`, `Str`, etc.). Both are compared at the AIRL level after `run-ir` returns — the Rust VM's return values are auto-marshalled back to AIRL values by the runtime.

### Test 2: Compiler Fixpoint

**File:** `bootstrap/fixpoint_test.airl`

**Dependencies:** lexer.airl + parser.airl + compiler.airl (concatenated, self-contained). No eval.airl needed.

**Approach — three tiers:**

#### Tier 1: Small program fixpoint

1. Define a small test program source string (20-30 lines, covering if/let/match/recursion/lambda)
2. Parse it to AST
3. **Interpreted compiler:** Call `compile-program(ast)` directly → `ir-nodes-1`
4. **Compiled compiler:** Build a source string containing the compiler functions + code that parses and compiles the test program. Feed through `run-compiled`. The VM executes the compiled compiler on the test program → `ir-nodes-2`
5. Serialize both IR node trees to strings via `ir-to-string`
6. Assert string representations are identical

The source string for the compiled compiler path looks like:
```
<compiler.airl source>

(let (test-src : String "<escaped test program>")
  (match (lex test-src)
    (Ok tokens)
      (match (parse-sexpr-all tokens)
        (Ok sexprs)
          (match (parse-program sexprs)
            (Ok ast) (compile-program ast)
            _ (Err "parse failed"))
          _ (Err "sexpr failed"))
      _ (Err "lex failed")))
```

When `run-compiled` processes this:
1. The interpreted compiler compiles the compiler functions + trailing expression to IR
2. The Rust VM loads the compiled compiler functions
3. The VM executes the trailing expression, which calls `compile-program` — **the compiled compiler compiling the test program**
4. Returns IR nodes

#### Tier 2: Compiler compiles itself

Same approach as Tier 1, but the test program IS the compiler source itself. The source string becomes the compiler source + code that parses and compiles the compiler source (read via `read-file`).

This proves: the compiler compiled by itself produces the same IR as the compiler compiled by the interpreter.

#### Tier 3 (stretch): Full chain fixpoint

The test program is the full lexer + parser + compiler (1,514 lines). This is the ultimate bootstrap proof but may be slow. Gate behind a flag or separate test file.

### IR Serialization Helper

**Function:** `ir-to-string` (pure AIRL, defined in the test files)

Converts IR variant nodes to a deterministic string representation for comparison. Example:
- `(IRInt 42)` → `"(IRInt 42)"`
- `(IRCall ["+" [(IRInt 1) (IRInt 2)]])` → `"(IRCall + [(IRInt 1) (IRInt 2)])"`

This is needed because:
- Deep equality (`=`) on nested variant trees works but gives no debuggable output on failure
- String comparison provides both equality check and diff-able output
- Deterministic serialization ensures no false negatives from ordering differences

The serializer recursively traverses IR variant nodes (`IRInt`, `IRCall`, `IRFunc`, etc.) and produces a canonical string form. It must handle all IR node types, all IR pattern types, `IRBinding`, and `IRArm`.

## String Escaping

Test programs embedded as string literals need quote escaping. For Tier 1, use test programs that avoid string literals (arithmetic, control flow, variants). For Tier 2+, the compiler source doesn't contain string literals, so `read-file` + direct string manipulation avoids escaping issues.

For programs that need embedded strings, use `read-file` to load from a separate `.airl` file rather than inline string literals.

## Error Handling

Both test files should:
- Print clear PASS/FAIL per test with the test name
- On failure, print both expected and actual values (or IR string representations)
- Continue running remaining tests after a failure (don't abort early)
- Print a summary at the end: "N/M tests passed"

## Testing Strategy

```bash
# Fast equivalence test — run regularly
cargo run -- run bootstrap/equivalence_test.airl

# Fixpoint test — run in release mode (slow)
cargo run --release -- run bootstrap/fixpoint_test.airl
```

Both test files follow the existing bootstrap test pattern: self-contained, `assert-eq` helper, print-based reporting.
