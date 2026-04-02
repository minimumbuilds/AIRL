# Building AIRL with the Rust Toolchain (Legacy)

This document covers building AIRL from source using the Rust/Cargo toolchain. This produces the `airl` host binary which includes the AOT compiler and all builtins.

**Most users should use the pre-built `airl` binary or the self-hosted G3 compiler instead.** This guide is for:
- First-time bootstrap (building `airl` from source)
- Developing the Rust runtime or Cranelift backend
- Building for a new platform

## Prerequisites

- **Rust 1.85+** (install via [rustup](https://rustup.rs))
- **CMake** + **C++ compiler** (for Z3, first build only)
- **Python 3** (for Z3 build scripts)
- **System C linker** (`cc`) — present on all Linux/macOS

First build takes ~5-15 minutes due to Z3 compiling from C++ source.

## Build Commands

```bash
# Full AOT build (recommended)
cargo build --release --features aot

# Bytecode-only (no Cranelift, no native compilation — development/testing only)
cargo build --release
```

The binary is at `target/release/airl-driver`. Copy it to your PATH:

```bash
cp target/release/airl-driver /usr/local/bin/airl
```

## CLI Usage

```bash
airl run <file>              # AOT compile to temp binary, execute, clean up
airl compile <file> -o <out> # AOT compile to native binary
airl check <file>            # Type-check and verify contracts
airl agent <file>            # Run as agent worker
airl fmt <file>              # Pretty-print source
```

## Running Tests

```bash
# Rust test suite (~520 tests)
cargo test -p airl-syntax -p airl-types -p airl-contracts \
  -p airl-runtime -p airl-agent -p airl-driver

# G2 AOT test suite (58 tests)
bash tests/aot/run_aot_tests.sh

# Bootstrap compiler tests
cargo run --release --features aot -- run --load bootstrap/lexer.airl bootstrap/lexer_test.airl
cargo run --release --features aot -- run --load bootstrap/lexer.airl --load bootstrap/parser.airl bootstrap/parser_test.airl
```

## Feature Flags

| Flag | Effect |
|------|--------|
| `aot` | Cranelift AOT — enables `airl compile` for standalone native executables |
| `mlir` | MLIR/GPU compilation (requires LLVM 19+, see Dockerfile) |

Building without any features produces a bytecode-only binary (development/testing only).

## Crate Structure

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `airl-syntax` | Lexer, parser, AST | None |
| `airl-types` | Type checker, linearity | airl-syntax |
| `airl-contracts` | Contract types | airl-syntax, airl-types |
| `airl-rt` | Runtime library (libairl_rt.a) | None (standalone) |
| `airl-runtime` | AOT compiler | airl-syntax, airl-types, airl-contracts, cranelift |
| `airl-codegen` | Cranelift codegen | airl-syntax, airl-types, cranelift |
| `airl-solver` | Z3 verification | airl-syntax, z3 |
| `airl-agent` | Agent transport/protocol | airl-syntax, airl-runtime |
| `airl-driver` | CLI, pipeline | all crates |

## Embedded Runtime

The runtime library (`libairl_rt.a`) is gzip-compressed at build time and embedded in the `airl` binary via `include_bytes!`. During `airl compile`, it's extracted to a temp file for linking, then cleaned up. This makes the `airl` binary fully self-contained — no separate runtime installation needed.

## Debug Output

```bash
AIRL_AOT_DEBUG=1 airl compile program.airl -o out  # Cranelift IR dump
AIRL_BC_DUMP=1 airl compile program.airl -o out     # Bytecode instruction dump
```
