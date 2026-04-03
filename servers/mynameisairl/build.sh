#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_DIR="${AIRL_DIR:-$HOME/repos/AIRL}"
G3="${G3:-$AIRL_DIR/g3}"
AT_ROOT="${AT_ROOT:-$HOME/repos/AirTraffic}"
export AIRL_STDLIB="${AIRL_STDLIB:-$AIRL_DIR/stdlib}"

if [[ ! -x "$G3" ]]; then
    echo "Error: g3 compiler not found at $G3"
    echo "Build it: cd ~/repos/AIRL && bash scripts/build-g3.sh"
    exit 1
fi

# json.airl provides json-parse/json-stringify (needed by jsonrpc.airl).
# patch-prompts-list.airl is listed BEFORE airtraffic.airl so the local
# airtraffic-handle-prompts-list fix (first-def-wins) takes effect.
# mynameisairl.airl must be LAST since its top-level code is the entry point.
"$G3" -- \
    "$AIRL_DIR/stdlib/json.airl" \
    "$AT_ROOT/src/transport.airl" \
    "$AT_ROOT/src/jsonrpc.airl" \
    "$AT_ROOT/src/schema.airl" \
    "$SCRIPT_DIR/patch-prompts-list.airl" \
    "$AT_ROOT/src/airtraffic.airl" \
    "$SCRIPT_DIR/mynameisairl.airl" \
    -o "${1:-$SCRIPT_DIR/mynameisairl}"
