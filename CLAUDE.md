# AIRL — Project Instructions for Claude

## Project Overview

AIRL (AI Intermediate Representation Language) is a programming language for AI systems. Rust Cargo workspace, 10 crates, self-hosted (G3 compiler written in AIRL produces native binaries).

**Key docs:** `AIRL-Header.md` (LLM reference — read before writing AIRL), `AIRL-LLM-Guide.md` (verbose guide), `stdlib/*.md` (stdlib docs), `docs/superpowers/specs/` (design specs)

## Pre-Flight (BLOCKING)

**Before writing ANY `.airl` file:** Read `AIRL-Header.md` in full. No exceptions.

## Fresh clone setup

```bash
bash scripts/git-hooks/install.sh    # Activate tracked git hooks (pre-push fixpoint gate)
```

One-shot, idempotent. Points `core.hooksPath` at `scripts/git-hooks/`. See `scripts/git-hooks/README.md`.

## Build & Test

```bash
cargo build -p airl-rt                                 # Build runtime library FIRST (fresh build)
cargo clean -p airl-runtime                            # Force build.rs re-run to find libairl_rt.a
cargo build --features aot                         # Full build (embeds libairl_rt.a)
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
cargo run --features aot -- run <file.airl>        # Run (AOT compile → execute)
cargo run --features aot -- compile <file.airl> -o <binary>  # AOT compile
cargo run --features aot -- check <file.airl>          # Type-check only
bash scripts/build-g3.sh                               # Rebuild G3 (~23 min)
bash tests/aot/run_aot_tests.sh                        # G3 AOT test suite (68 tests)
```

**Fresh build order:** `airl-runtime` embeds a compressed `libairl_rt.a` at build time. On a fresh checkout: (1) `cargo build -p airl-rt` to produce the `.a`, (2) `cargo clean -p airl-runtime` to invalidate the cached "not found" result, (3) full build. If you see `libairl_rt.a not found`, repeat all three steps.

**First build:** Z3 compiles from C++ source (~5-15 min). Requires CMake, C++ compiler, Python 3.

**macOS prerequisites:** `xcode-select --install` (provides C/C++ compiler and linker). `brew install cmake z3`. Python 3 is required for Z3's build system. Set `export LIBRARY_PATH="$(brew --prefix z3)/lib"` so the linker finds Z3.

## Architecture

```
airl-syntax → airl-types → airl-contracts → airl-runtime ← airl-codegen (Cranelift)
                                                ↓
                                            airl-agent → airl-driver ← airl-solver (Z3)

airl-rt: extern "C" runtime (builtins for AOT)
```

**Execution:** `airl run` AOT-compiles to temp binary. `airl compile` produces standalone executable. `./g3` is the self-hosted compiler (AIRL front-end → Cranelift AOT). All paths share `libairl_rt.a` builtins.

**Critical constraint:** `airl-runtime` depends on `airl-codegen` — no reverse dependency allowed.

## Conventions

- Zero external deps for core crates (only `airl-codegen`/Cranelift and `airl-solver`/Z3)
- Tests: inline `#[cfg(test)]` + fixture E2E tests in `crates/airl-driver/tests/fixtures.rs`
- Fixtures: `tests/fixtures/{valid,type_errors,contract_errors,linearity_errors,agent}/`
- Multi-binding `let` preferred: `(let (x : T v1) (y : T v2) body)`
- Builtins: `extern "C"` in `crates/airl-rt/`, dispatched via `CallBuiltin` opcode
- **Default verification level is `:verify proven`.** Modules without an explicit `:verify` annotation require provable contracts on every `:pub defn`. Grandfathered modules are listed in `.airl-verify-baseline.toml`; run `airl verify-policy` to check the baseline, `airl verify-policy --list-uncovered` to see which `:pub defn`s need `:ensures`.

## Module System

```clojure
(import "lib/math.airl")             ;; prefix: (math.abs -5)
(import "lib/math.airl" :as m)      ;; alias: (m.abs -5)
(import "lib/math.airl" :only [abs]) ;; bare: (abs -5)
```

`:pub` on `defn`/`deftype` exports to importers. Private by default. Stdlib always available without import. Paths relative to importing file. No `..` or absolute paths. Circular deps rejected.

## Builtins Summary

All signatures in `AIRL-Header.md`. Key categories:

| Category | Count | Examples |
|----------|-------|---------|
| List | 7 | `head`, `tail`, `empty?`, `cons`, `at-or`, `set-at`, `list-contains?` |
| String | 23 | `str`, `char-at`, `split`, `join`, `contains`, `replace`, `char-alpha?`, `char-digit?`, `string-ci=?` |
| Map | 10 | `map-new`, `map-from`, `map-get`, `map-set`, `map-keys`, `map-values` |
| File I/O | 14 | `read-file`, `write-file`, `file-exists?`, `read-dir`, `temp-file`, `temp-dir`, `file-mtime` |
| Float math | 15 | `sqrt`, `sin`, `cos`, `floor`, `ceil`, `int-to-float` |
| Bitwise | 5 | `bitwise-and`, `bitwise-or`, `bitwise-xor`, `bitwise-shl`, `bitwise-shr` |
| TCP | 9 | `tcp-listen`, `tcp-accept`, `tcp-connect`, `tcp-connect-tls`, `tcp-send`, `tcp-recv` |
| Threads | 10 | `thread-spawn`, `thread-join`, `channel-new`, `channel-send`, `channel-recv` |
| Crypto | 13 | `sha256`, `sha512`, `hmac-*`, `pbkdf2-*`, `base64-*`, `random-bytes` |
| Compression | 8 | `gzip-*`, `snappy-*`, `lz4-*`, `zstd-*` (all Bytes in/out) |
| Bytes | 11 | `bytes-from-int{16,32,64}`, `bytes-to-*`, `bytes-concat`, `crc32c` |
| Stdio | 4 | `read-line`, `read-stdin`, `eprint`, `eprintln` |
| System | 10 | `shell-exec`, `shell-exec-with-stdin`, `time-now`, `sleep`, `getenv`, `get-args`, `cpu-count`, `get-cwd` |
| Conversion | 4 | `parse-int-radix`, `int-to-string-radix`, `int-to-string`, `string-to-int` |
| Stdlib (AIRL) | 68 | `map`, `filter`, `fold`, `sort`, `abs`, `min`, `is-ok?`, `words`, `set-*` |
| HTTP (AIReqL) | 20+ | `aireql-get`, `aireql-post-with-opts`, `aireql-request`, `aireql-json` (lib: `../AIReqL`) |

## Bootstrap Compiler

Lives in `bootstrap/`. AIRL compiler phases written in AIRL. G3 build:

```bash
bash scripts/build-g3.sh    # Builds G3 with all 4 bootstrap files as input
```

**G3 build requires all 4 files as input:** `lexer.airl parser.airl bc_compiler.airl g3_compiler.airl`. Omitting any causes segfault (unresolved function pointers).

**AIRL constraints for bootstrap code:**
- `and`/`or` are eager — use nested `if` for short-circuit
- No mixed int/float — use `int-to-float`
- Test files must contain all function definitions (no imports in bootstrap tests)

## Known Issues

1. **MLIR requires system libs:** `libzstd-dev`, LLVM 19+. Excluded from default build. Use `--features mlir`.
2. **AOT extern "C" errors are non-recoverable:** `process::exit(1)` on type mismatch. Prevented by type checker.
3. **Type checker incomplete for builtins:** 45+ builtins registered as `TypeVar("builtin")` — no compile-time type checking. Runtime panics catch errors. See `docs/superpowers/specs/2026-03-28-verification-gaps-assessment.md`.
4. **Z3 verification informational only:** Disproven contracts print warnings but don't block execution. Runtime assertions are the enforcement mechanism.
5. **G3 rebuild after runtime changes:** After modifying `crates/airl-rt/`, rebuild G3 via `bash scripts/build-g3.sh` to pick up the new embedded runtime. The `build.rs` tracks `libairl_rt.a` automatically, but G3 is a separate binary that needs explicit rebuilding.
