# T-Diagram Bootstrap Verification Test

**Date:** 2026-04-07
**Status:** Approved
**Scope:** Single shell script proving the self-hosted G3 compiler reproduces itself binary-identically

## Overview

The T-diagram test is the gold-standard bootstrap correctness proof: stage 1 (the interpreter-compiled binary) and stage 2 (the self-compiled binary) must be binary identical. If they diverge, the compiler's self-hosted code path produces different output than the Rust-hosted path — a sign of non-determinism or a bug in the self-hosted codegen.

This is distinct from the fixpoint tests, which only prove `bc-compile-program` is deterministic within a single interpreter run. The T-diagram test crosses the boundary between the Rust VM and the native binary.

## Design

**File:** `scripts/t-diagram.sh`

Follows the style of `scripts/build-g3.sh` — standalone bash script, `[t-diagram]` output prefix, `set -euo pipefail`.

### Pre-flight

- Verify `./g3` exists and is executable (stage 1 binary — produced by `build-g3.sh`)
- Verify all 4 bootstrap source files are present:
  `bootstrap/lexer.airl`, `bootstrap/parser.airl`, `bootstrap/bc_compiler.airl`, `bootstrap/g3_compiler.airl`
- Set `AIRL_STDLIB` if not already set (mirrors `build-g3.sh`)
- Compute and print sha256 of `./g3`

### Stage 2

```bash
./g3 -- bootstrap/lexer.airl bootstrap/parser.airl \
        bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl \
        -o /tmp/g3-v2
```

`./g3` is the stage 1 binary. It compiles the same bootstrap source it was built from, producing `/tmp/g3-v2`.

### Comparison

- `sha256sum` both binaries
- **PASS:** hashes match → print shared hash, clean up `/tmp/g3-v2`, exit 0
- **FAIL:** hashes differ → print both hashes and file sizes, leave `/tmp/g3-v2` for inspection, exit 1

## Non-Goals

- Rebuilding stage 1 from `airl-driver` (use `build-g3.sh` for that — reusing existing `./g3` is intentional)
- Cross-compiler equivalence between `airl-driver compile` and `./g3` (different front-ends, not expected to match)
- CI integration (can be added later; script exits 1 on failure for easy hooking)
