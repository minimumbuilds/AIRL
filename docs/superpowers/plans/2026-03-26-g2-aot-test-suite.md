# G2 AOT Test Suite — Compile and Run Every Bootstrap Function

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Verify every G2 bootstrap function produces correct output when AOT-compiled to a native binary. Catch register allocation bugs that the bytecode VM masks.

**Architecture:** Write AIRL test programs that call bootstrap functions with known inputs and expected outputs. Compile them via the G2 pipeline (bootstrap compiler → `compile-bytecode-to-executable`) to native binaries. Run the binaries and compare output. Three rounds of increasing coverage.

**Tech Stack:** AIRL, bash test runner, G2 compiler pipeline

**Working directory:** `.worktrees/v0.6.0-aot-unification/` (branch `v0.6.0`)

---

## Background

The G2 compiler has a register allocation bug in `bc_compiler.airl` that causes values to be clobbered when compiled to native code via AOT. The bytecode VM masks this by cloning values. We need tests that exercise the AOT path to find all instances of this bug before attempting G3 self-compilation.

**How to compile a test via G2 to a native binary:**
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release --features jit,aot -- run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl -- <test.airl> -o <test_bin>
./test_bin
```

**How to compile a test via G1 (Rust pipeline) to a native binary (for comparison):**
```bash
cargo run --release --features jit,aot -- compile <test.airl> -o <test_bin>
./test_bin
```

If a test passes via G1 but fails via G2, the bug is in the bootstrap bytecode compiler's register allocator. If it fails via both, the bug is in the AOT codegen or airl-rt.

**IMPORTANT:** The `--load` flags load bootstrap modules into the Rust VM. The `--` separates G3 compiler args from the source files to compile. The G3 compiler reads the source files, compiles them through the bootstrap pipeline, and links them with `libairl_rt.a` into a native binary. The native binary has ALL stdlib functions (prelude, math, result, string, map, set) compiled from source by the bootstrap compiler.

**Test format:**
```airl
;; EXPECT: expected output here
(print "actual output here\n")
```

First line is a comment with expected output. The test runner extracts it and compares.

---

## Function Inventory

### Lexer (21 functions in `bootstrap/lexer.airl`)

| # | Function | Signature | Test strategy |
|---|----------|-----------|---------------|
| 1 | `is-whitespace?` | `(ch : String) -> Bool` | Test space, tab, newline, letter, digit |
| 2 | `is-digit?` | `(ch : String) -> Bool` | Test 0-9, a, space |
| 3 | `digit-value` | `(ch : String) -> i64` | Test 0-9 → 0-9 |
| 4 | `is-symbol-start?` | `(ch : String) -> Bool` | Test a-z, A-Z, special chars, digits |
| 5 | `is-symbol-char?` | `(ch : String) -> Bool` | Test a-z, digits (both valid) |
| 6 | `peek-next-is?` | `(source pos expected) -> Bool` | Test in-bounds, out-of-bounds, match, no-match |
| 7 | `skip-block-comment` | `(source pos line col) -> [pos line col]` | Test `#| ... |#` blocks |
| 8 | `skip-line-comment` | `(source pos line col) -> [pos line col]` | Test `;; ...` to newline |
| 9 | `skip-ws` | `(source pos line col) -> [pos line col]` | Test spaces, tabs, newlines, comments |
| 10 | `read-symbol-chars` | `(source pos) -> pos` | Test reading identifier chars |
| 11 | `lex-symbol` | `(source pos line col) -> (Token pos line col)` | Test identifiers like `foo`, `+`, `my-var` |
| 12 | `lex-keyword` | `(source pos line col) -> (Token pos line col)` | Test `:keyword` |
| 13 | `digit-value-f` | `(ch : String) -> f64` | Test 0-9 → 0.0-9.0 |
| 14 | `int-to-float` | `(n : i64) -> f64` | Test 0, 1, 42, -1 |
| 15 | `read-digits` | `(source pos acc) -> [int pos]` | Test reading sequences of digits |
| 16 | `read-frac-digits` | `(source pos acc scale) -> [float pos]` | Test reading fractional parts |
| 17 | `lex-number` | `(source pos line col) -> (Token pos line col)` | Test integers and floats |
| 18 | `lex-string` | `(source pos line col) -> (Token pos line col)` | Test `"hello"`, escapes |
| 19 | `next-token` | `(source pos line col) -> (Token pos line col)` | Test all token types |
| 20 | `lex-loop` | `(source pos line col tokens) -> tokens` | Test multi-token input |
| 21 | `lex` | `(source) -> Result[tokens, error]` | End-to-end lexing |

### Parser (38 functions in `bootstrap/parser.airl`)

| # | Function | Signature | Test strategy |
|---|----------|-----------|---------------|
| 1 | `is-upper?` | `(ch : String) -> Bool` | Test A-Z, a-z, digits |
| 2-6 | `token-line/col/kind/value`, `parse-error` | accessors | Test on Token values |
| 7 | `parse-sexpr` | `(tokens pos) -> Result` | Test `(a b c)`, `[1 2]`, atoms |
| 8 | `parse-sexprs` | `(tokens pos) -> Result` | Test multiple sexprs |
| 9 | `parse-sexpr-all` | `(tokens) -> Result` | End-to-end sexpr parse |
| 10 | `parse-atom` | `(sexpr) -> AST` | Test int, float, string, bool, symbol |
| 11 | `parse-expr` | `(sexpr) -> Result` | Test all expression types |
| 12-18 | `parse-if/let/do/match/lambda/try/pattern` | individual parsers | Test each form |
| 19-25 | `parse-defn/deftype/sig/top-level/program` | definition parsers | Test function/type defs |
| 26 | `parse` | `(source) -> Result` | End-to-end: source → AST |

### Bytecode Compiler (90+ functions in `bootstrap/bc_compiler.airl`)

Most bc_compiler functions are internal (compiler state manipulation). Test via integration: compile an AIRL expression, run the result.

| # | Category | Test strategy |
|---|----------|---------------|
| 1 | Opcode constants | Verify `op-load-const` returns 0, `op-move` returns 4, etc. |
| 2 | Compiler state | `make-compiler-state`, `cs-alloc-reg`, `cs-add-const`, `cs-emit` |
| 3 | Expression compilation | Compile literals, arithmetic, comparisons, let, if, do |
| 4 | Function calls | Named calls, local calls (CallReg), builtin calls |
| 5 | Pattern matching | match with variants, wildcards, literals |
| 6 | Closures/lambdas | Lambda compilation with captures, fold/map with closures |
| 7 | Full programs | `bc-compile-program` → BCFunc → run-bytecode |
| 8 | Integration | `lex` → `parse` → `bc-compile-program` → `compile-bytecode-to-executable` → run |

### G3 Compiler (4 functions in `bootstrap/g3_compiler.airl`)

| # | Function | Test strategy |
|---|----------|---------------|
| 1 | `g3-compile-source` | Compile simple source → BCFunc list |
| 2 | `g3-find-stdlib-dir` | Returns "stdlib" or env var |
| 3 | `g3-compile-stdlib` | Compile all 6 stdlib modules |
| 4 | `g3-parse-args` | Parse `["-o" "out" "file.airl"]` → `[["file.airl"] "out"]` |

---

## Task 1: Test Runner Script

Create `tests/aot/run_aot_tests.sh`:

```bash
#!/bin/bash
# G2 AOT Test Runner
# Compiles each .airl test file via G2 pipeline to a native binary, runs it, compares output.
set -e

PASS=0
FAIL=0
ERRORS=""
AIRL="cargo run --release --features jit,aot --"
G2_ARGS="run --load bootstrap/lexer.airl --load bootstrap/parser.airl --load bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl --"

for test in tests/aot/round*.airl; do
  name=$(basename "$test" .airl)
  expected=$(head -1 "$test" | sed 's/^;; EXPECT: //')
  bin="/tmp/aot_test_${name}"

  # Compile via G2 pipeline
  if ! $AIRL $G2_ARGS "$test" -o "$bin" 2>/dev/null; then
    echo "COMPILE_FAIL: $name"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  COMPILE_FAIL: $name"
    continue
  fi

  # Run the native binary
  actual=$("$bin" 2>&1) || true
  rm -f "$bin"

  if [ "$actual" = "$expected" ]; then
    echo "PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $name"
    echo "  expected: '$expected'"
    echo "  actual:   '$actual'"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  FAIL: $name (expected '$expected', got '$actual')"
  fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed"
if [ $FAIL -gt 0 ]; then
  echo -e "Failures:$ERRORS"
  exit 1
fi
```

- [ ] Create the script
- [ ] Make it executable
- [ ] Commit

---

## Task 2: Round 1 — Lexer Tests (1 per function, 21 tests)

Create `tests/aot/round1_lexer_helpers.airl`:
Tests for: `is-whitespace?`, `is-digit?`, `digit-value`, `is-symbol-start?`, `is-symbol-char?`, `peek-next-is?`

Create `tests/aot/round1_lexer_skip.airl`:
Tests for: `skip-ws`, `skip-line-comment`, `skip-block-comment`, `read-symbol-chars`

Create `tests/aot/round1_lexer_tokens.airl`:
Tests for: `lex-symbol`, `lex-keyword`, `lex-number`, `lex-string`, `next-token`

Create `tests/aot/round1_lexer_integration.airl`:
Tests for: `lex-loop`, `lex` (full lexing of a simple program)

Each file is a standalone AIRL program with `;; EXPECT:` header. Functions are called with specific inputs, output is printed, and compared by the runner.

- [ ] Write all 4 lexer test files
- [ ] Run via test runner, fix any failures
- [ ] Commit passing tests

---

## Task 3: Round 1 — Parser Tests (1 per function group, ~10 test files)

Create test files for:
- `round1_parser_helpers.airl` — `is-upper?`, token accessors
- `round1_parser_sexpr.airl` — `parse-sexpr`, `parse-sexprs`, `parse-sexpr-all`
- `round1_parser_atoms.airl` — `parse-atom` for each atom type
- `round1_parser_exprs.airl` — `parse-expr` for if/let/do/match/lambda
- `round1_parser_defn.airl` — `parse-defn`, `parse-top-level`, `parse-program`
- `round1_parser_integration.airl` — `parse` end-to-end (source → AST)

- [ ] Write all parser test files
- [ ] Run via test runner, fix any failures
- [ ] Commit passing tests

---

## Task 4: Round 1 — BC Compiler Tests (integration, ~8 test files)

The bc_compiler has 90+ internal functions. Test via integration:

- `round1_bc_opcodes.airl` — Verify opcode constants (op-load-const=0, etc.)
- `round1_bc_state.airl` — Compiler state operations (alloc-reg, add-const, emit)
- `round1_bc_literals.airl` — Compile and run: int, float, bool, string, nil literals
- `round1_bc_arithmetic.airl` — Compile and run: +, -, *, /, %, comparisons
- `round1_bc_control.airl` — Compile and run: if, let, do, match
- `round1_bc_functions.airl` — Compile and run: defn, call, recursion
- `round1_bc_closures.airl` — Compile and run: lambda, fold with closure, map
- `round1_bc_integration.airl` — Full pipeline: lex → parse → compile → run

For compile-and-run tests, use `run-compiled-bc` builtin or the G2 pipeline to AOT compile small programs and verify output.

- [ ] Write all bc_compiler test files
- [ ] Run via test runner, fix any failures
- [ ] Commit passing tests

---

## Task 5: Round 1 — G3 Compiler Tests (4 test files)

- `round1_g3_compile_source.airl` — Compile a simple source string
- `round1_g3_find_stdlib.airl` — Verify stdlib directory detection
- `round1_g3_parse_args.airl` — Test argument parsing
- `round1_g3_integration.airl` — Compile a simple program end-to-end

- [ ] Write all g3_compiler test files
- [ ] Run via test runner, fix any failures
- [ ] Commit passing tests

---

## Task 6: Fix Failures from Round 1

Any test that fails reveals a register allocation bug or AOT codegen issue. For each failure:

1. Compare G1-compiled vs G2-compiled output (identify which compiler has the bug)
2. Dump bytecode for the failing function from both compilers
3. Find the register conflict
4. Fix in `bc_compiler.airl` or `bytecode_aot.rs`
5. Re-run tests

- [ ] Fix all Round 1 failures
- [ ] 100% pass rate
- [ ] Commit fixes

---

## Task 7: Round 2 — 5 Tests Per Function

Expand each Round 1 test to 5 unique input sets. Add edge cases:
- Empty strings, empty lists
- Single-element inputs
- Maximum-size inputs (long strings, deeply nested sexprs)
- Unicode characters
- Boundary values (0, -1, MAX_INT)

- [ ] Write Round 2 test files (5 inputs each)
- [ ] Run, fix failures
- [ ] Commit

---

## Task 8: Round 3 — 10 Tests Per Function

Expand to 10 unique input sets. Stress tests:
- 1000-character strings
- 100-element lists
- Deeply nested expressions (5+ levels)
- All escape sequences in strings
- All operator types in one program
- Programs with 50+ functions

- [ ] Write Round 3 test files (10 inputs each)
- [ ] Run, fix failures
- [ ] Commit

---

## Task 9: Retry G3 Self-Compilation

After all three rounds pass with 100% rate:

```bash
cargo run --release --features jit,aot -- run \
  --load bootstrap/lexer.airl --load bootstrap/parser.airl \
  --load bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl -- \
  bootstrap/lexer.airl bootstrap/parser.airl \
  bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl -o g3

./g3 test.airl -o test_bin && ./test_bin
```

- [ ] G3 binary compiles
- [ ] G3 binary runs and produces correct output
- [ ] Commit as v0.6.0 milestone

---

## Verification

```bash
# Run all AOT tests
bash tests/aot/run_aot_tests.sh

# Run existing Rust test suite (should still pass)
cargo test -p airl-runtime -p airl-driver

# Run bootstrap VM tests (should still pass)
cargo run --release -- run bootstrap/lexer_test.airl
cargo run --release -- run bootstrap/parser_test.airl
cargo run --release -- run bootstrap/eval_test.airl
```
