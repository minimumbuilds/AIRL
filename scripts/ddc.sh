#!/bin/bash
# DDC (Diverse Double-Compiling) test: behavioral comparison between
# x86_64 g3 (local) and aarch64 g3 (Pi4).
#
# Compiles each AOT test with both compilers, runs each binary on its native
# hardware, and asserts stdout is identical — proving both compilers produce
# semantically equivalent programs despite different architectures and toolchains.
#
# Usage:
#   bash scripts/ddc.sh [pi-host] [pi-airl-root]
#   bash scripts/ddc.sh jbarnes@192.168.0.109 ~/repos/AIRL
#
# Prerequisites:
#   - Local:  ./g3 built (bash scripts/build-g3.sh)
#   - Remote: ~/repos/AIRL/g3 built on Pi (bash scripts/build-g3.sh)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export AIRL_STDLIB="${AIRL_STDLIB:-$AIRL_ROOT/stdlib}"

PI_HOST="${1:-jbarnes@192.168.0.109}"
PI_AIRL_ROOT="${2:-~/repos/AIRL}"
LOCAL_G3="$AIRL_ROOT/g3"

TMPDIR_LOCAL="$(mktemp -d /tmp/ddc-local-XXXXXX)"
trap 'rm -rf "$TMPDIR_LOCAL"' EXIT

# ── Pre-flight ───────────────────────────────────────────────────────────────

if [ ! -x "$LOCAL_G3" ]; then
    echo "[ddc] error: ./g3 not found — run bash scripts/build-g3.sh first" >&2
    exit 1
fi

echo "[ddc] checking remote g3..."
if ! ssh "$PI_HOST" "test -x $PI_AIRL_ROOT/g3" 2>/dev/null; then
    echo "[ddc] error: g3 not found on $PI_HOST at $PI_AIRL_ROOT/g3" >&2
    echo "[ddc] run: bash scripts/build-g3.sh on the Pi first" >&2
    exit 1
fi

LOCAL_ARCH=$(uname -m)
REMOTE_ARCH=$(ssh "$PI_HOST" "uname -m")
echo "[ddc] local:  $LOCAL_ARCH  ($LOCAL_G3)"
echo "[ddc] remote: $REMOTE_ARCH ($PI_HOST:$PI_AIRL_ROOT/g3)"

# ── Sync test corpus and bootstrap to Pi ────────────────────────────────────

echo "[ddc] syncing test corpus to $PI_HOST..."
rsync -az tests/aot/*.airl "$PI_HOST:$PI_AIRL_ROOT/tests/aot/"
rsync -az bootstrap/         "$PI_HOST:$PI_AIRL_ROOT/bootstrap/"
rsync -az stdlib/            "$PI_HOST:$PI_AIRL_ROOT/stdlib/"

# ── Build remote runner script ───────────────────────────────────────────────
# Uploaded to Pi and executed in one SSH connection (avoids per-test overhead).
# Outputs lines: PASS|||name|||output  or  FAIL|||name|||reason

REMOTE_RUNNER="$(mktemp /tmp/ddc-runner-XXXXXX.sh)"
trap 'rm -rf "$TMPDIR_LOCAL" "$REMOTE_RUNNER"' EXIT

cat > "$REMOTE_RUNNER" << 'RUNNER_EOF'
#!/bin/bash
set -uo pipefail
AIRL_ROOT="$1"
export AIRL_STDLIB="$AIRL_ROOT/stdlib"
G3="$AIRL_ROOT/g3"
TMPDIR_REMOTE="$(mktemp -d /tmp/ddc-remote-XXXXXX)"
trap 'rm -rf "$TMPDIR_REMOTE"' EXIT

for test in "$AIRL_ROOT"/tests/aot/round*.airl; do
    name=$(basename "$test" .airl)

    if grep -q '^;; SKIP_AOT:' "$test"; then
        echo "SKIP|||$name|||skipped"
        continue
    fi

    deps_line=$(sed -n '2p' "$test")
    deps=""
    if echo "$deps_line" | grep -q '^;; DEPS:'; then
        deps=$(echo "$deps_line" | sed 's/^;; DEPS: //')
    fi

    bin="$TMPDIR_REMOTE/$name"
    cmd="$G3 --"
    for dep in $deps; do cmd="$cmd $AIRL_ROOT/$dep"; done
    cmd="$cmd $test -o $bin"

    if ! eval "$cmd" > /dev/null 2>&1; then
        echo "COMPILE_FAIL|||$name|||compile failed"
        continue
    fi

    output=$("$bin" 2>&1) || true
    # Escape newlines so each result is one line
    output_escaped=$(printf '%s' "$output" | tr '\n' '\x1E')
    echo "OK|||$name|||$output_escaped"
done
RUNNER_EOF

chmod +x "$REMOTE_RUNNER"
scp -q "$REMOTE_RUNNER" "$PI_HOST:/tmp/ddc-runner.sh"

# ── Run tests locally (x86_64) ───────────────────────────────────────────────

echo "[ddc] compiling and running tests locally ($LOCAL_ARCH)..."
declare -A LOCAL_RESULTS

for test in "$AIRL_ROOT"/tests/aot/round*.airl; do
    name=$(basename "$test" .airl)

    if grep -q '^;; SKIP_AOT:' "$test"; then
        LOCAL_RESULTS[$name]="SKIP"
        continue
    fi

    deps_line=$(sed -n '2p' "$test")
    deps=""
    if echo "$deps_line" | grep -q '^;; DEPS:'; then
        deps=$(echo "$deps_line" | sed 's/^;; DEPS: //')
    fi

    bin="$TMPDIR_LOCAL/$name"
    cmd="$LOCAL_G3 --"
    for dep in $deps; do cmd="$cmd $AIRL_ROOT/$dep"; done
    cmd="$cmd $test -o $bin"

    if ! eval "$cmd" > /dev/null 2>&1; then
        LOCAL_RESULTS[$name]="COMPILE_FAIL"
        continue
    fi

    output=$("$bin" 2>&1) || true
    LOCAL_RESULTS[$name]="$output"
done

# ── Run tests remotely (aarch64) ─────────────────────────────────────────────

echo "[ddc] compiling and running tests remotely ($REMOTE_ARCH)..."
declare -A REMOTE_RESULTS

while IFS='|||' read -r status name output_escaped; do
    output=$(printf '%s' "$output_escaped" | tr '\x1E' '\n')
    case "$status" in
        OK)           REMOTE_RESULTS[$name]="$output" ;;
        COMPILE_FAIL) REMOTE_RESULTS[$name]="COMPILE_FAIL" ;;
        SKIP)         REMOTE_RESULTS[$name]="SKIP" ;;
    esac
done < <(ssh "$PI_HOST" "bash /tmp/ddc-runner.sh $PI_AIRL_ROOT")

# ── Compare ───────────────────────────────────────────────────────────────────

echo "[ddc] comparing results..."
echo ""

PASS=0
FAIL=0
SKIP=0
DISAGREE=0
ERRORS=""

for test in "$AIRL_ROOT"/tests/aot/round*.airl; do
    name=$(basename "$test" .airl)
    expected=$(head -1 "$test" | sed 's/^;; EXPECT: //')

    local_out="${LOCAL_RESULTS[$name]:-MISSING}"
    remote_out="${REMOTE_RESULTS[$name]:-MISSING}"

    if [ "$local_out" = "SKIP" ] && [ "$remote_out" = "SKIP" ]; then
        SKIP=$((SKIP + 1))
        continue
    fi

    if [ "$local_out" = "COMPILE_FAIL" ] || [ "$remote_out" = "COMPILE_FAIL" ]; then
        echo "COMPILE_FAIL: $name (local=$local_out remote=$remote_out)"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  COMPILE_FAIL: $name"
        continue
    fi

    if [ "$local_out" != "$remote_out" ]; then
        echo "DISAGREE: $name"
        echo "  local  ($LOCAL_ARCH): $(printf '%s' "$local_out" | head -1)"
        echo "  remote ($REMOTE_ARCH): $(printf '%s' "$remote_out" | head -1)"
        DISAGREE=$((DISAGREE + 1))
        ERRORS="$ERRORS\n  DISAGREE: $name"
        continue
    fi

    if [ "$local_out" = "$expected" ]; then
        echo "PASS: $name"
        PASS=$((PASS + 1))
    else
        echo "WRONG_OUTPUT: $name"
        echo "  expected: '$expected'"
        echo "  both got: '$local_out'"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  WRONG_OUTPUT: $name"
    fi
done

echo ""
echo "[ddc] Results: $PASS passed, $DISAGREE disagree, $FAIL wrong/compile-fail, $SKIP skipped"
echo "[ddc] local:  $LOCAL_ARCH | remote: $REMOTE_ARCH"

if [ $DISAGREE -gt 0 ] || [ $FAIL -gt 0 ]; then
    printf "[ddc] Failures:%b\n" "$ERRORS"
    exit 1
fi

echo "[ddc] PASS: both compilers produce identical behavior across all tests"
