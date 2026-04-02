#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
G3="${G3:-$HOME/repos/AIRL/g3}"
AT_ROOT="${AT_ROOT:-$HOME/repos/AirTraffic}"

if [[ ! -x "$G3" ]]; then
    echo "Error: g3 compiler not found at $G3"
    echo "Build it: cd ~/repos/AIRL && bash scripts/build-g3.sh"
    exit 1
fi

# patch-prompts-list.airl is listed BEFORE airtraffic.airl so the local
# airtraffic-handle-prompts-list fix (first-def-wins) takes effect.
# mynameisairl.airl must be LAST since its top-level code references
# functions from airtraffic.airl.
"$G3" -- \
    "$AT_ROOT/src/transport.airl" \
    "$AT_ROOT/src/jsonrpc.airl" \
    "$AT_ROOT/src/schema.airl" \
    "$SCRIPT_DIR/patch-prompts-list.airl" \
    "$AT_ROOT/src/airtraffic.airl" \
    "$SCRIPT_DIR/mynameisairl.airl" \
    -o "${1:-$SCRIPT_DIR/mynameisairl}"
