#!/bin/bash
# Build the G3 self-hosted AIRL compiler.
# Outputs to builds/g3-<commit>-<timestamp>, symlinks g3 → latest build.
# Previous builds are preserved for rollback and A/B testing.
#
# Usage:
#   bash scripts/build-g3.sh          # build + cache + symlink
#   bash scripts/build-g3.sh --list   # list cached builds
#   bash scripts/build-g3.sh --use <path>  # switch g3 symlink to a cached build
set -euo pipefail

BUILDS_DIR="builds"
mkdir -p "$BUILDS_DIR"

# --- Subcommands ---

if [ "${1:-}" = "--list" ]; then
    echo "Cached G3 builds:"
    for f in "$BUILDS_DIR"/g3-*; do
        [ -f "$f" ] || continue
        size=$(ls -lh "$f" | awk '{print $5}')
        current=""
        if [ -L g3 ] && [ "$(readlink g3)" = "$f" ]; then
            current=" <- current"
        fi
        echo "  $f ($size)$current"
    done
    exit 0
fi

if [ "${1:-}" = "--use" ]; then
    target="${2:?Usage: build-g3.sh --use <path>}"
    if [ ! -f "$target" ]; then
        echo "error: $target not found" >&2
        exit 1
    fi
    ln -sf "$target" g3
    echo "g3 -> $target"
    ./g3 -- --version 2>/dev/null || true
    exit 0
fi

# --- Build ---

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BUILD_NAME="g3-${COMMIT}-${TIMESTAMP}"
BUILD_PATH="${BUILDS_DIR}/${BUILD_NAME}"

AIRL_BIN="${AIRL_BIN:-cargo run --release --features jit,aot --}"

echo "[build-g3] Building host binary..."
cargo build --release --features jit,aot

echo "[build-g3] Compiling G3 -> ${BUILD_PATH} (this takes ~23 minutes)..."
$AIRL_BIN run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl -- \
  bootstrap/lexer.airl \
  bootstrap/parser.airl \
  bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl \
  -o "$BUILD_PATH"

# Symlink g3 → latest build
ln -sf "$BUILD_PATH" g3

SIZE=$(ls -lh "$BUILD_PATH" | awk '{print $5}')
echo "[build-g3] Done: ${SIZE} -> ${BUILD_PATH}"
echo "[build-g3] g3 -> ${BUILD_PATH}"
./"$BUILD_PATH" -- --version 2>/dev/null || true
