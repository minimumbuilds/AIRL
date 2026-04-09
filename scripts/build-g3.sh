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

# Ensure AIRL_STDLIB is set — g3 needs it to find prelude.airl
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export AIRL_STDLIB="${AIRL_STDLIB:-$AIRL_ROOT/stdlib}"

# Arch normalization (same as workflow's lib/user.sh)
_build_arch="$(uname -m)"
case "$_build_arch" in
    x86_64)           G3_ARCH="x86_64" ;;
    arm64|aarch64)    G3_ARCH="arm64"  ;;
    *)                G3_ARCH="$_build_arch" ;;
esac

# Isolate build artifacts per arch so Linux x86_64 and macOS arm64
# don't stomp each other on a shared filesystem.
export CARGO_TARGET_DIR="${AIRL_ROOT}/target-${G3_ARCH}"

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
BUILD_NAME="g3-${G3_ARCH}-${COMMIT}-${TIMESTAMP}"
BUILD_PATH="${BUILDS_DIR}/${BUILD_NAME}"

AIRL_BIN="${AIRL_BIN:-cargo run --release --features aot --}"

echo "[build-g3] Building host binary..."
cargo build --release --features aot

echo "[build-g3] Compiling G3 -> ${BUILD_PATH}..."
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

# Arch-specific symlink — safe to coexist with the other arch on shared filesystem
ln -sf "builds/$(basename "$BUILD_PATH")" "${AIRL_ROOT}/g3-${G3_ARCH}"

# Keep unadorned g3 symlink pointing to the arch-specific one for local convenience
# (scripts that haven't been updated yet will use this as fallback)
ln -sf "g3-${G3_ARCH}" "${AIRL_ROOT}/g3"

SIZE=$(ls -lh "$BUILD_PATH" | awk '{print $5}')
echo "[build-g3] Done: ${SIZE} -> ${BUILD_PATH}"
echo "Arch: ${G3_ARCH} — symlink: g3-${G3_ARCH} -> builds/$(basename "$BUILD_PATH")"
echo "CARGO_TARGET_DIR: $CARGO_TARGET_DIR"
./"$BUILD_PATH" -- --version 2>/dev/null || true
