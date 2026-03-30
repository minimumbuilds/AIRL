# Design Spec: `define`, Character Classification, Radix Parsing, Enhanced Process Spawning

**Date:** 2026-03-30
**Status:** Draft
**Scope:** New language form + 12 new builtins + 1 enhanced builtin

## Context

AIRL has a complete string library and file I/O system, but has genuine gaps in four areas:

1. **No lightweight function definition** — `defn` requires `:sig`, `:requires`, `:ensures` for every function. Quick scripting, prototyping, and AIRLOS's embedded evaluator need a simpler form.
2. **Character classification is hand-rolled** — The bootstrap lexer manually defines `is-whitespace?`, `is-digit?`, `is-symbol-start?`, `is-symbol-char?` using string containment checks, duplicated across 4 files. These should be proper builtins.
3. **No radix parsing** — The Rust lexer supports `0x` and `0b` literals, but AIRL code cannot parse or format integers in arbitrary bases at runtime.
4. **`shell-exec` loses stderr and exit codes** — Returns only stdout as a string. Cannot determine if a command succeeded or failed. Cannot send stdin to child processes.

## Feature 1: `define` — Distinct Simpler Form

### Syntax

```clojure
(define factorial (n)
  (if (<= n 1)
    1
    (* n (factorial (- n 1)))))

(define greet (name)
  (str "Hello, " name "!"))

(define identity (x) x)
```

### Semantics

- **No contracts**: No `:requires`, `:ensures`, `:invariant`
- **No type annotations**: Parameters are untyped (`Any`)
- **No `:pub`**: Always module-private. Use `defn` for exports
- **No `:intent`**: No documentation string
- **Recursion**: Function name is in scope within body (self-recursive calls work)
- **TCO**: Self-recursive tail calls are optimized (same mechanism as `defn`)
- **Scope**: Top-level only (like `defn`), not expression-level

### AST

New struct in `crates/airl-syntax/src/ast.rs`:

```rust
pub struct SimpleFnDef {
    pub name: Symbol,
    pub params: Vec<Symbol>,
    pub body: Expr,
    pub span: Span,
}
```

New variant: `TopLevel::Define(SimpleFnDef)`

### Compilation Path

`define` lowers to the same `IRNode::Func(name, params, body)` that `defn` produces. This means:
- The bytecode compiler's `compile_expr_tail` handles TCO automatically
- The bytecode VM needs no changes
- AOT and JIT compilation work unchanged
- The distinction is purely at the parser/front-end level

### Changes Required

| Layer | File | Change |
|-------|------|--------|
| AST | `crates/airl-syntax/src/ast.rs` | New `SimpleFnDef` struct + `Define` variant on `TopLevel` |
| Parser | `crates/airl-syntax/src/parser.rs` | New `parse_define()` (~30 lines), add `"define"` arm in `parse_top_level` |
| Type checker | `crates/airl-types/src/checker.rs` | Skip — return `Ok(())` for `Define` |
| Pipeline | `crates/airl-driver/src/pipeline.rs` | Add `Define` arms at all `TopLevel` match sites (~4 locations) |
| IR lowering | Various | Lower `Define` to `IRNode::Func` |
| Agent runtime | `crates/airl-agent/src/runtime.rs` | Add `Define` arm |
| Bootstrap parser | `bootstrap/parser.airl` | New `parse-define` + `parse-define-params` functions |
| Bootstrap compiler | `bootstrap/bc_compiler.airl` | Add `ASTDefine` handling in `bc-compile-top-level`, `bc-split-loop`, `bc-compile-defns-prefixed` |

### Design Rationale

**Why not sugar for `defn`?** A desugaring approach would carry hidden contract machinery (`(valid true)` defaults), produce confusing error messages referencing `defn` internals, and couple `define` to `defn`'s evolution. A distinct form gives clean semantics, clean errors, and future flexibility.

**Why no `:pub`?** `define` is intentionally simple. The language already has `defn` for public, contracted, typed functions. `define` serves a different purpose: quick definitions where formalism is overhead.

## Feature 2: Character Classification Builtins

### New Builtins

| Builtin | Signature | Semantics |
|---------|-----------|-----------|
| `char-alpha?` | `(char-alpha? s) -> Bool` | True if first char is Unicode alphabetic |
| `char-digit?` | `(char-digit? s) -> Bool` | True if first char is ASCII digit 0-9 |
| `char-whitespace?` | `(char-whitespace? s) -> Bool` | True if first char is Unicode whitespace |
| `char-upper?` | `(char-upper? s) -> Bool` | True if first char is Unicode uppercase |
| `char-lower?` | `(char-lower? s) -> Bool` | True if first char is Unicode lowercase |

### Implementation

Each is a new `extern "C"` function in `crates/airl-rt/src/string.rs`. All 5 follow the same pattern — extract first char, call the corresponding Rust `char::is_*` method. Each is ~4 lines.

### Impact on Bootstrap

Once these builtins exist, the bootstrap lexer's hand-rolled `is-whitespace?` and `is-digit?` functions can be replaced with builtin calls. This is an optional cleanup — the hand-rolled versions will continue to work. The builtins will be faster (direct Rust `char::is_*` vs. AIRL string-contains checks).

## Feature 3: Radix Parsing Builtins

### New Builtins

| Builtin | Signature | Semantics |
|---------|-----------|-----------|
| `parse-int-radix` | `(parse-int-radix s base) -> Result[i64, String]` | Parse string as integer in given base (2-36) |
| `int-to-string-radix` | `(int-to-string-radix n base) -> String` | Format integer in given base (2-36) |

### Implementation

In `crates/airl-rt/src/misc.rs`. Uses Rust's `i64::from_str_radix` for parsing (well-tested, handles edge cases). For formatting, implements a manual digit-extraction loop using `"0123456789abcdefghijklmnopqrstuvwxyz"` lookup — Rust lacks a built-in arbitrary-radix formatter.

### Use Cases

```clojure
(parse-int-radix "ff" 16)       ;; => (Ok 255)
(parse-int-radix "1010" 2)      ;; => (Ok 10)
(int-to-string-radix 255 16)    ;; => "ff"
(int-to-string-radix 10 2)      ;; => "1010"
```

Essential for AIRLOS kernel work (memory addresses), protocol parsing (hex encoding), and general utility.

## Feature 4: Case-Insensitive String Compare

### New Builtin

| Builtin | Signature | Semantics |
|---------|-----------|-----------|
| `string-ci=?` | `(string-ci=? a b) -> Bool` | Unicode case-folded equality comparison |

### Implementation

In `crates/airl-rt/src/string.rs`. Uses `to_lowercase()` for Unicode-correct case folding. Allocates two temporary strings — for hot-path usage, a future optimization could compare char-by-char without allocation.

## Feature 5: Enhanced `shell-exec`

### Current Behavior

```clojure
(shell-exec "ls" ["-la"])  ;; => (Ok "file1\nfile2\n") or (Err "error msg")
```

Returns `Result[String, String]`. Stderr is lost. Exit code is lost.

### New Behavior

```clojure
(shell-exec "ls" ["-la"])
;; => (Ok {"stdout": "file1\nfile2\n", "stderr": "", "exit-code": 0})

(shell-exec "false" [])
;; => (Ok {"stdout": "", "stderr": "", "exit-code": 1})
;; Note: non-zero exit is NOT an Err — the command ran. Err is for spawn failure.

(shell-exec "nonexistent" [])
;; => (Err "No such file or directory")
```

Returns `Result[Map, String]` where the Ok map has keys:
- `"stdout"` — captured stdout as string
- `"stderr"` — captured stderr as string
- `"exit-code"` — integer exit code (0 = success, -1 if signal-killed)

### Breaking Change

The `Ok` variant changes from `String` to `Map`. Existing callers must update.

**Affected files:**
- `tests/aot/round3_builtin_shell.airl` — update pattern matches
- No bootstrap files use `shell-exec`

### Implementation

Replace `airl_shell_exec` in `crates/airl-rt/src/misc.rs`. Uses `std::process::Command::new(&command).args(&cmd_args).output()` (execFile-style — no shell interpolation, arguments are passed directly as an array, preventing command injection).

### New: `shell-exec-with-stdin`

| Builtin | Signature | Semantics |
|---------|-----------|-----------|
| `shell-exec-with-stdin` | `(shell-exec-with-stdin cmd args stdin-str) -> Result[Map, String]` | Execute with stdin piped, return same {stdout, stderr, exit-code} map |

Uses `Command::new().args().stdin(Stdio::piped()).spawn()` followed by `child.wait_with_output()`. Arguments passed as array (no shell injection risk).

### Use Case

```clojure
;; Pipe data to a command
(shell-exec-with-stdin "wc" ["-l"] "line1\nline2\nline3\n")
;; => (Ok {"stdout": "3\n", "stderr": "", "exit-code": 0})
```

## Feature 6: Utility Builtins

### New Builtins

| Builtin | Signature | File | Semantics |
|---------|-----------|------|-----------|
| `get-cwd` | `() -> String` | `misc.rs` | Return current working directory |
| `temp-file` | `(prefix) -> String` | `io.rs` | Create temp file, return path |
| `temp-dir` | `(prefix) -> String` | `io.rs` | Create temp directory, return path |
| `file-mtime` | `(path) -> i64` | `io.rs` | Return modification time as epoch millis |

## File I/O — No Changes Needed

The following already exist and are complete:
- `read-file`, `write-file`, `append-file`, `file-exists?`, `is-dir?`, `file-size`
- `read-dir`, `create-dir`, `delete-file`, `delete-dir`, `rename-file`
- `path-join`, `path-parent`, `path-filename`, `path-extension`, `is-absolute?`

## Summary of All New/Changed Builtins

| # | Builtin | Category | Arity | Status |
|---|---------|----------|-------|--------|
| 1 | `char-alpha?` | String/Char | 1 | New |
| 2 | `char-digit?` | String/Char | 1 | New |
| 3 | `char-whitespace?` | String/Char | 1 | New |
| 4 | `char-upper?` | String/Char | 1 | New |
| 5 | `char-lower?` | String/Char | 1 | New |
| 6 | `parse-int-radix` | Conversion | 2 | New |
| 7 | `int-to-string-radix` | Conversion | 2 | New |
| 8 | `string-ci=?` | String | 2 | New |
| 9 | `shell-exec` | Process | 2 | **Changed** (returns map) |
| 10 | `shell-exec-with-stdin` | Process | 3 | New |
| 11 | `get-cwd` | System | 0 | New |
| 12 | `temp-file` | File I/O | 1 | New |
| 13 | `temp-dir` | File I/O | 1 | New |
| 14 | `file-mtime` | File I/O | 1 | New |

Plus: `define` as a new language form (not a builtin).

## Testing Strategy

### Unit Tests (in-module `#[cfg(test)]`)

Each new `extern "C"` function gets tests in its source module:

**`string.rs` tests:**
- `char-alpha?`: ASCII letters, digits (false), Unicode letters (accented), empty string
- `char-digit?`: 0-9 (true), letters (false), Unicode digits
- `char-whitespace?`: space, tab, newline, CR, non-breaking space
- `char-upper?` / `char-lower?`: ASCII + Unicode case
- `string-ci=?`: equal strings, different case, Unicode case folding

**`misc.rs` tests:**
- `parse-int-radix`: hex, binary, octal, base-36, invalid input, base out of range
- `int-to-string-radix`: round-trip with parse-int-radix, negative numbers, zero
- `shell-exec`: verify map keys exist, stdout content, stderr capture, non-zero exit code
- `shell-exec-with-stdin`: stdin piped correctly, output captured
- `get-cwd`: returns non-empty string

**`io.rs` tests:**
- `temp-file`: file is created, path contains prefix
- `temp-dir`: directory is created
- `file-mtime`: returns positive epoch ms for existing file, -1 for missing

### Fixture Tests (`tests/fixtures/valid/`)

New fixture files:

**`define.airl`** — basic define, recursion, TCO (tail-recursive sum), multiple defines calling each other

**`char_classify.airl`** — all 5 classification builtins with true/false cases

**`radix.airl`** — hex/binary parsing, round-trip formatting, edge cases

**`shell_exec_enhanced.airl`** — map return structure, stderr capture, exit codes, stdin piping

### AOT Tests (`tests/aot/`)

- Update `round3_builtin_shell.airl` for new `shell-exec` return type
- Add `round1_bc_define.airl` for G3-compiled define test
- Clear AOT cache before running: `rm -rf tests/aot/cache`

### Coverage Checklist

- [ ] Every new `extern "C"` function has at least 3 unit tests
- [ ] Every new builtin has a fixture test in `tests/fixtures/valid/`
- [ ] `define` tested with: basic call, recursion, TCO (deep tail recursion), multiple defines, define calling define
- [ ] `shell-exec` breaking change: update `round3_builtin_shell.airl`
- [ ] All 5 char classification builtins tested with ASCII, Unicode, empty string, multi-char string
- [ ] Radix parsing tested with bases 2, 8, 10, 16, 36, invalid input, boundary values
- [ ] `shell-exec-with-stdin` tested with actual stdin piping
- [ ] `get-cwd`, `temp-file`, `temp-dir`, `file-mtime` tested
- [ ] `cargo test` passes all crates
- [ ] AOT test suite passes after G3 rebuild
- [ ] Bootstrap compiler changes verified via G3 self-compile

## Implementation Order

1. **Phase 1: Character classification + radix + string-ci=? + utility builtins** (no breaking changes, no bootstrap changes)
2. **Phase 2: Enhanced shell-exec + shell-exec-with-stdin** (breaking change, update test file)
3. **Phase 3: `define` — Rust side** (AST, parser, pipeline, type checker, agent)
4. **Phase 4: `define` — Bootstrap side** (parser.airl, bc_compiler.airl, G3 rebuild)
5. **Phase 5: Documentation** (AIRL-Header.md, CLAUDE.md, stdlib docs)

Phases 1-2 share a worktree. Phase 3-4 gets a separate worktree. Phase 5 can be done on either.

## Worktree Strategy

- **Worktree 1**: `feature/builtins-and-shell-exec` — Phases 1-2
- **Worktree 2**: `feature/define-form` — Phases 3-4
- Merge worktree 1 first (simpler, no bootstrap changes)
- Merge worktree 2 second (requires G3 rebuild which embeds worktree 1's changes)

## External Libraries

`../AIReqL` (HTTP client) and `../AirLift` (utility library) are available in adjacent directories. The features in this spec do not depend on or modify these libraries.
