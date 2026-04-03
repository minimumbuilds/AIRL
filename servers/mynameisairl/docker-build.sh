#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_DIR="${AIRL_DIR:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

# Build locally if binary doesn't exist
if [ ! -f "$SCRIPT_DIR/mynameisairl" ]; then
    echo "Building mynameisairl binary..."
    cd "$AIRL_DIR" && AIRL_STDLIB=./stdlib ./g3 -- \
        "$SCRIPT_DIR/patch-json-result.airl" \
        "$AIRL_DIR/stdlib/json.airl" \
        ~/repos/AirTraffic/src/transport.airl \
        ~/repos/AirTraffic/src/jsonrpc.airl \
        ~/repos/AirTraffic/src/schema.airl \
        "$SCRIPT_DIR/patch-prompts-list.airl" \
        ~/repos/AirTraffic/src/airtraffic.airl \
        "$SCRIPT_DIR/mynameisairl.airl" \
        -o "$SCRIPT_DIR/mynameisairl"
fi

# Build minimal Docker image — just the binary + guide
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

cp "$SCRIPT_DIR/mynameisairl" "$TMPDIR/"
cp "$SCRIPT_DIR/mcp-stdio-wrapper.sh" "$TMPDIR/"
cp "$AIRL_DIR/AIRL-LLM-Guide.md" "$TMPDIR/"
cp "$SCRIPT_DIR/Dockerfile.prebuilt" "$TMPDIR/Dockerfile"

cd "$TMPDIR"
docker build -t mynameisairl .
echo "Done. Run with: docker run -i --rm mynameisairl"
