#!/bin/bash
# Build the G3 self-hosted AIRL compiler.
# Requires: cargo, Rust toolchain, ~25GB RAM, ~23 minutes.
set -euo pipefail

AIRL_BIN="${AIRL_BIN:-cargo run --release --features jit,aot --}"
OUTPUT="${1:-g3}"

echo "[build-g3] Building host binary..."
cargo build --release --features jit,aot

echo "[build-g3] Compiling G3 -> ${OUTPUT} (this takes ~23 minutes)..."
$AIRL_BIN run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl -- \
  bootstrap/lexer.airl \
  bootstrap/parser.airl \
  bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl \
  -o "$OUTPUT"

echo "[build-g3] Done: $(ls -lh "$OUTPUT" | awk '{print $5}') -> $OUTPUT"
./"$OUTPUT" -- --version 2>/dev/null || true
