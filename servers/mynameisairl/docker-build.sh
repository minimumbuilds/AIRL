#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Copy repos into build context
cp -a ~/repos/AIRL "$TMPDIR/airl"
cp -a ~/repos/AirTraffic "$TMPDIR/airtraffic"
cp -a "$SCRIPT_DIR" "$TMPDIR/mynameisairl"
cp "$SCRIPT_DIR/Dockerfile" "$TMPDIR/Dockerfile"

cd "$TMPDIR"
docker build -t mynameisairl .
