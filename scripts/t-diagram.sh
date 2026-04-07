#!/bin/bash
# T-Diagram bootstrap verification.
# Proves that g3 (stage 1, compiled by airl-driver) and a binary produced by
# g3 compiling itself (stage 2) are sha256-identical.
#
# Usage:
#   bash scripts/t-diagram.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export AIRL_STDLIB="${AIRL_STDLIB:-$AIRL_ROOT/stdlib}"

STAGE1="$AIRL_ROOT/g3"
STAGE2="/tmp/g3-v2"

# ── Pre-flight ───────────────────────────────────────────────────────────────

if [ ! -x "$STAGE1" ]; then
    echo "[t-diagram] error: ./g3 not found or not executable — run bash scripts/build-g3.sh first" >&2
    exit 1
fi

for f in bootstrap/lexer.airl bootstrap/parser.airl bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl; do
    if [ ! -f "$AIRL_ROOT/$f" ]; then
        echo "[t-diagram] error: missing bootstrap file: $f" >&2
        exit 1
    fi
done

cd "$AIRL_ROOT"

HASH1=$(sha256sum "$STAGE1" | awk '{print $1}')
SIZE1=$(ls -lh "$STAGE1" | awk '{print $5}')
echo "[t-diagram] stage 1: $HASH1  $STAGE1 ($SIZE1)"

# ── Stage 2: g3 compiles itself ──────────────────────────────────────────────

echo "[t-diagram] running stage 2 (g3 compiles bootstrap chain)..."
"$STAGE1" -- \
    bootstrap/lexer.airl \
    bootstrap/parser.airl \
    bootstrap/bc_compiler.airl \
    bootstrap/g3_compiler.airl \
    -o "$STAGE2"

HASH2=$(sha256sum "$STAGE2" | awk '{print $1}')
SIZE2=$(ls -lh "$STAGE2" | awk '{print $5}')
echo "[t-diagram] stage 2: $HASH2  $STAGE2 ($SIZE2)"

# ── Compare ──────────────────────────────────────────────────────────────────

if [ "$HASH1" = "$HASH2" ]; then
    echo "[t-diagram] PASS: stage 1 == stage 2 ($HASH1)"
    rm -f "$STAGE2"
    exit 0
else
    echo "[t-diagram] FAIL: binaries differ"
    echo "[t-diagram]   stage 1: $HASH1 ($SIZE1)"
    echo "[t-diagram]   stage 2: $HASH2 ($SIZE2)"
    echo "[t-diagram]   stage 2 binary left at $STAGE2 for inspection"
    exit 1
fi
