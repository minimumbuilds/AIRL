# airtest — AIRL Testing Framework

**Date:** 2026-04-01
**Status:** Approved design
**Scope:** stdlib/test.airl (AIRL repo) + airtest runner (new repo)

## Problem

Every AIRL ecosystem project independently reinvents the same test infrastructure:
- Custom `check` or `assert-eq` helper functions with PASS/FAIL printing
- A `main` function that runs assertions sequentially, `(exit 1)` on first failure
- A `test.sh` script that compiles source + test files with g3 and runs the binary

There is no shared assertion library, no structured output, no test discovery, no summary statistics, and no machine-readable results. LLM agents — the primary consumers of test output in the AIRL workflow — must parse ad-hoc text to determine pass/fail status.

## Design Principles

1. **Agents are first-class consumers.** JSON output by default, not human-readable text. LLM agents parse JSON natively; ad-hoc text is ambiguous and lossy.
2. **Assertions in stdlib, runner as standalone project.** Assertions are pure functions with zero dependencies — they belong where every project can use them without a build dependency. The runner has opinions about CLI, discovery, and reporting — that's a separate project.
3. **Manifest-first, convention-fallback.** Projects declare test configuration in `airtest.sexp`. If absent, the runner applies directory conventions (`src/*.airl`, `tests/test-*.airl`). Explicit is better than implicit for automated systems.
4. **No language changes required.** Test discovery is file-level (scan `tests/` for `test-*.airl`), not function-level. Individual test names come from assertion output at runtime. Works with g3 today.

## Architecture

```
stdlib/test.airl              ← shared assertions, emits JSON lines (AIRL repo)
    ↑ used by
airtest                       ← runner: discovery, compilation, execution, reporting (new repo)
    ↑ invoked by
g3 test                       ← compiler subcommand, thin wrapper (future, AIRL repo)
```

---

## Component 1: stdlib/test.airl

**Location:** `repos/AIRL/stdlib/test.airl`

Shared assertion functions imported by all test files. Each assertion emits one JSON line to stdout.

### Functions

| Function | Signature | Behavior |
|----------|-----------|----------|
| `assert-eq` | `(a : Any) (b : Any) (msg : String) -> Unit` | Pass if `(= a b)`, fail otherwise |
| `assert-ne` | `(a : Any) (b : Any) (msg : String) -> Unit` | Pass if `(!= a b)`, fail otherwise |
| `assert-ok` | `(r : Result) (msg : String) -> Unit` | Pass if `(Ok _)`, fail if `(Err _)` |
| `assert-err` | `(r : Result) (msg : String) -> Unit` | Pass if `(Err _)`, fail if `(Ok _)` |
| `assert-contains` | `(haystack : String) (needle : String) (msg : String) -> Unit` | Pass if needle found in haystack |
| `assert-true` | `(cond : Bool) (msg : String) -> Unit` | Pass if true, fail if false |

### Output Protocol

**On pass:**
```json
{"test": "head of non-empty list", "status": "pass"}
```

**On fail:**
```json
{"test": "head of non-empty list", "status": "fail", "expected": "1", "actual": "2"}
```
Then `(exit 1)`.

Each assertion emits exactly one JSON line to stdout. Tests fail-fast on first failure (existing ecosystem behavior preserved). The `expected` and `actual` fields are included where applicable (`assert-eq`, `assert-ne`, `assert-ok`, `assert-err`).

### Contracts

All functions follow AIRL conventions:
```clojure
(defn assert-eq
  :sig [(a : Any) (b : Any) (msg : String) -> Unit]
  :requires [(valid msg)]
  :ensures [(valid result)]
  :body ...)
```

---

## Component 2: airtest.sexp — Manifest Format

**Location:** Project root (e.g., `repos/CairLI/airtest.sexp`)

Optional S-expression configuration file. When present, the runner reads it for test configuration. When absent, directory conventions apply.

### Example

```clojure
;; airtest.sexp — test configuration for CairLI
(airtest
  (sources ["src/cairli.airl"])
  (tests   "tests/")
  (stdlib  true)
  (g3      "../AIRL/g3"))
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `sources` | List of strings | No | `src/*.airl` | Source files to compile with each test |
| `tests` | String | No | `tests/` | Directory containing test files |
| `test-pattern` | String | No | `test-*.airl` or `test_*.airl` | Glob pattern for test file discovery |
| `stdlib` | Bool | No | `true` | Include AIRL stdlib in compilation |
| `g3` | String | No | `$AIRL_DIR/g3` or `../AIRL/g3` | Path to g3 compiler |
| `link-flags` | List of strings | No | `[]` | Extra linker flags (e.g., `["-lm" "-lpthread"]`) |
| `timeout` | Int | No | `30` | Per-test timeout in seconds |

### Convention Fallback (no manifest)

When `airtest.sexp` does not exist:
- **Sources:** all `*.airl` files in `src/`
- **Tests:** all files matching `test-*.airl` or `test_*.airl` in `tests/`
- **Stdlib:** included
- **g3:** resolved from `$AIRL_DIR/g3`, then `../AIRL/g3`

### Why S-expressions

AIRL already parses S-expressions. The manifest can be read using the same lexer/parser that AIRL programs use. No new parser needed.

---

## Component 3: airtest — The Test Runner

**Location:** `repos/airtest` (new project)
**Language:** AIRL
**Dependencies:** CairLI (CLI parsing), stdlib (including test.airl)
**Output:** Native binary via g3

### CLI Interface

```bash
# Run all tests (reads airtest.sexp or falls back to convention)
airtest

# Run tests matching a pattern
airtest --filter "test-binary*"

# Run a single test file
airtest --file tests/test-lexer.airl

# Override g3 path
airtest --g3 /path/to/g3

# Output control
airtest --quiet          # summary only to stderr, JSON to stdout
airtest --verbose        # per-test JSON to stderr too
```

### Execution Flow

```
1. Read airtest.sexp (or apply convention defaults)
2. Discover test files matching pattern in tests/
3. For each test file:
   a. Compile: g3 -- [sources...] [stdlib...] test-file.airl -o /tmp/airtest-{hash}
   b. Execute with timeout
   c. Capture stdout (JSON lines) and exit code
   d. Parse JSON lines into test results
   e. Clean up binary
4. Aggregate results
5. Emit final JSON report to stdout
6. Print summary line to stderr
7. Exit 0 if all pass, exit 1 if any fail
```

### Output Format (stdout)

The runner emits a single JSON object wrapping all results:

```json
{
  "version": 1,
  "project": "CairLI",
  "timestamp": "2026-04-01T16:00:00Z",
  "results": [
    {"file": "tests/test-parsing.airl", "test": "full parse", "status": "pass", "duration_ms": 3},
    {"file": "tests/test-parsing.airl", "test": "error recovery", "status": "pass", "duration_ms": 1},
    {"file": "tests/test-builders.airl", "test": "flag builder", "status": "fail", "expected": "true", "actual": "false", "duration_ms": 2}
  ],
  "summary": {
    "total": 3,
    "passed": 2,
    "failed": 1,
    "skipped": 0,
    "duration_ms": 48
  }
}
```

### Summary Line (stderr)

```
3 tests, 2 passed, 1 failed — 48ms
```

Human-readable one-liner for developers who glance at terminal output.

### Error Handling

| Condition | Result entry |
|-----------|-------------|
| Compile failure | `{"file": "...", "status": "error", "message": "compile failed: ..."}` |
| Timeout | `{"file": "...", "status": "timeout", "timeout_s": 30}` |
| Crash (non-zero exit, no JSON) | `{"file": "...", "status": "crash", "exit_code": 139}` |

### Test Discovery

Test files are discovered by scanning the `tests` directory for files matching `test-pattern`. Within each file, individual test names come from the JSON lines emitted by `stdlib/test.airl` assertions at runtime — the runner does not parse AIRL source to find `defn test-*` names. Discovery is file-level; naming is assertion-level.

---

## Component 4: g3 test — Compiler Integration (Future)

**Not part of the initial implementation.** Noted here for forward compatibility.

`g3 test` would be a thin wrapper that:
1. Looks for `airtest.sexp` in the current directory
2. Invokes the `airtest` binary (must be on PATH)
3. Passes through any flags

This becomes useful once airlDelivery (the package manager) can manage tool dependencies. Until then, projects invoke `airtest` directly or from their Makefile.

---

## Migration Path

### Phase 1: stdlib assertions (AIRL repo)

- Add `stdlib/test.airl` with the 6 assertion functions
- Each emits JSON lines to stdout
- Projects adopt by replacing ad-hoc `check`/`assert-eq` helpers with `(import test)`
- Existing `test.sh` scripts continue to work — assertions are backward-compatible

### Phase 2: airtest runner (new repo)

- Build the runner as a standalone AIRL project
- Projects add `airtest.sexp` manifests (or rely on convention)
- Projects replace `test.sh` invocations with `airtest` in their Makefile

### Phase 3: ecosystem rollout

- Update each project's Makefile: `test: airtest`
- Delete `test.sh` scripts
- Workflow agents (`airl-workflow`) use `airtest --quiet` and parse stdout JSON

No big-bang migration. Phases are independently useful. Projects adopt at their own pace.

---

## Files Changed

| Location | Change |
|----------|--------|
| `repos/AIRL/stdlib/test.airl` | New file — 6 assertion functions |
| `repos/airtest/` | New project — runner, manifest parser, CLI |
| `repos/airtest/src/main.airl` | Entry point, CairLI CLI setup |
| `repos/airtest/src/discovery.airl` | File discovery and manifest parsing |
| `repos/airtest/src/compiler.airl` | g3 invocation and binary management |
| `repos/airtest/src/runner.airl` | Test execution, timeout, output capture |
| `repos/airtest/src/reporter.airl` | JSON aggregation and stderr summary |
| `repos/airtest/airtest.sexp` | Self-testing manifest |
| `repos/airtest/tests/` | Tests for the runner itself |
| `repos/airtest/Makefile` | Build and test targets |
| `repos/airtest/CLAUDE.md` | Project instructions |
| `repos/airtest/README.md` | Documentation |

## Ecosystem Registration

Register `airtest` in:
- `repos/AIRL/ECOSYSTEM.md` — add to Libraries section
- `airl-workflow/CLAUDE.md` — add to Project Registry
