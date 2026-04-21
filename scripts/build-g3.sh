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
#
# Redirect CARGO_TARGET_DIR to /tmp when:
#   (a) path contains colons — Cargo treats them as LD_LIBRARY_PATH separators, or
#   (b) running on macOS — repo is likely NFS-mounted; writing Rust artifacts
#       over NFS causes spurious "No such file or directory" errors on rmeta writes.
# macOS ships without GNU timeout; use gtimeout (brew install coreutils) or a fallback.
_build_os="$(uname -s)"
if [[ "$_build_os" == "Darwin" ]]; then
    if command -v gtimeout >/dev/null 2>&1; then
        timeout() { gtimeout "$@"; }
    else
        # No timeout available — define a no-op passthrough.
        # Runaway processes won't be killed, but the build won't fail on a missing command.
        timeout() { local _t="$1"; shift; "$@"; }
    fi
fi

if [[ "$AIRL_ROOT" == *:* || "$_build_os" == "Darwin" ]]; then
    export CARGO_TARGET_DIR="/tmp/g3-build-${G3_ARCH}"
    _G3_COLON_PATH=1
else
    export CARGO_TARGET_DIR="${AIRL_ROOT}/target-${G3_ARCH}"
    _G3_COLON_PATH=0
fi

BUILDS_DIR="builds"
mkdir -p "$BUILDS_DIR"

# --- Subcommands ---
# These run on the host — no Docker needed for listing or switching symlinks.

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

# --- Docker sandbox ---
# Re-exec inside a resource-limited container unless on macOS or already inside one.
# exec replaces the current process so exit code and stdio pass through cleanly.
_in_container=0
if [[ -f /.dockerenv ]] || grep -q 'docker\|lxc' /proc/1/cgroup 2>/dev/null; then
    _in_container=1
fi

if [[ "$_build_os" != "Darwin" && "$_in_container" -eq 0 ]]; then
    if [[ "$AIRL_ROOT" == *:* ]]; then
        echo "error: worktree path contains colon — use a dash-named worktree for Docker builds" >&2
        exit 1
    fi
    exec docker run --rm \
        --memory=6g --memory-swap=6g --cpus=2 \
        -v "${AIRL_ROOT}:${AIRL_ROOT}" \
        -w "${AIRL_ROOT}" \
        -e "AIRL_STDLIB=${AIRL_STDLIB}" \
        -e "CARGO_TARGET_DIR=${CARGO_TARGET_DIR}" \
        -e "AIRL_BIN=${AIRL_BIN:-cargo run --release --features aot --}" \
        -e "AIRL_ROOT=${AIRL_ROOT}" \
        -e "AIRL_MEM_TRACE=${AIRL_MEM_TRACE:-}" \
        -e "AIRL_NO_Z3_CACHE=${AIRL_NO_Z3_CACHE:-}" \
        -e "AIRL_RT_TRACE=${AIRL_RT_TRACE:-}" \
        rust:slim \
        bash scripts/build-g3.sh "$@"
fi

# --- Container build dependencies ---
# rust:slim is minimal: install every system library the build pipeline needs.
#   cmake + build-essential + python3  → z3-sys compiles Z3 from C++
#   libclang-dev                       → bindgen generates Rust FFI for z3-sys
#   libsqlite3-dev                     → libsqlite3-sys links against -lsqlite3
#   libcurl4-openssl-dev               → airl-rt HTTP builtins link against -lcurl
#   libz3-dev                          → G3's final link step pulls in -lz3
#                                       (the airl-driver binary statically links
#                                        z3-sys, but G3-emitted binaries link
#                                        against the system -lz3)
# Apt output is suppressed to keep the build log readable; failures still
# surface via the non-zero exit.
if [[ "$_in_container" -eq 1 ]] && ! command -v cmake >/dev/null 2>&1; then
    echo "[build-g3] Installing cmake + build-essential + python3 + libclang-dev + libsqlite3-dev + libcurl4-openssl-dev + libz3-dev..."
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        cmake build-essential python3 libclang-dev libsqlite3-dev \
        libcurl4-openssl-dev libz3-dev >/dev/null
fi

# --- Build ---

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BUILD_NAME="g3-${G3_ARCH}-${COMMIT}-${TIMESTAMP}"
BUILD_PATH="${BUILDS_DIR}/${BUILD_NAME}"

AIRL_BIN="${AIRL_BIN:-cargo run --release --features aot --}"

echo "[build-g3] Building host binary..."
if [[ "${_G3_COLON_PATH:-0}" -eq 1 ]]; then
    # Fresh build order: airl-rt must be compiled before airl-runtime embeds it.
    # Required when CARGO_TARGET_DIR is new (e.g. /tmp path for colon-path worktrees).
    echo "[build-g3] Fresh build order (colon worktree path — CARGO_TARGET_DIR=$CARGO_TARGET_DIR)..."
    cargo build --release -p airl-rt
    cargo clean -p airl-runtime
fi
cargo build --release --features aot

# find_lib() in airl-driver (find_airl_libs) looks for target/release/lib*.a
# first, then falls back to -lairl_rt. When CARGO_TARGET_DIR points elsewhere
# (target-x86_64, /tmp/g3-build-*), stale pre-existing `target/release/lib*.a`
# can shadow the freshly-built libraries. Symlink to the current CARGO_TARGET_DIR
# whenever we've redirected it, regardless of whether the redirect reason was a
# colon-path workaround or plain arch-suffix isolation.
if [[ "${CARGO_TARGET_DIR}" != "${AIRL_ROOT}/target" ]]; then
    mkdir -p "${AIRL_ROOT}/target/release"
    ln -sfn "${CARGO_TARGET_DIR}/release/libairl_rt.a" "${AIRL_ROOT}/target/release/libairl_rt.a"
    ln -sfn "${CARGO_TARGET_DIR}/release/libairl_runtime.a" "${AIRL_ROOT}/target/release/libairl_runtime.a"
fi

echo "[build-g3] Compiling G3 -> ${BUILD_PATH}..."
$AIRL_BIN run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/bc_compiler.airl \
  --load bootstrap/z3_bridge_g3.airl \
  --load bootstrap/z3_cache.airl \
  --load bootstrap/linearity.airl \
  bootstrap/g3_compiler.airl -- \
  bootstrap/lexer.airl \
  bootstrap/parser.airl \
  bootstrap/bc_compiler.airl \
  bootstrap/z3_bridge_g3.airl \
  bootstrap/z3_cache.airl \
  bootstrap/linearity.airl \
  bootstrap/g3_compiler.airl \
  -o "$BUILD_PATH"

# Arch-specific symlink — safe to coexist with the other arch on shared filesystem
ln -sf "builds/$(basename "$BUILD_PATH")" "${AIRL_ROOT}/g3-${G3_ARCH}"

# Update unadorned g3 symlink only if:
# (a) it doesn't exist yet, OR
# (b) it already points to this arch's binary (we're updating our own)
# Never clobber a symlink pointing to a different arch — multi-host shared FS safety.
CURRENT_G3_TARGET="$(readlink "${AIRL_ROOT}/g3" 2>/dev/null || true)"
if [[ -z "$CURRENT_G3_TARGET" || "$CURRENT_G3_TARGET" == "g3-${G3_ARCH}" || "$CURRENT_G3_TARGET" == g3-${G3_ARCH}-* ]]; then
    ln -sf "g3-${G3_ARCH}" "${AIRL_ROOT}/g3"
    echo "[build-g3] Updated g3 symlink -> g3-${G3_ARCH}"
else
    echo "[build-g3] Skipping g3 symlink update (currently -> ${CURRENT_G3_TARGET}, preserving other arch)"
fi

SIZE=$(ls -lh "$BUILD_PATH" | awk '{print $5}')
echo "[build-g3] Done: ${SIZE} -> ${BUILD_PATH}"
echo "Arch: ${G3_ARCH} — symlink: g3-${G3_ARCH} -> builds/$(basename "$BUILD_PATH")"
echo "CARGO_TARGET_DIR: $CARGO_TARGET_DIR"
./"$BUILD_PATH" -- --version 2>/dev/null || true

# --- Stage 3: Fixpoint test ---
# Use the new g3 binary (not cargo/Rust interpreter) to compile and run a
# non-trivial AIRL test. Catches OOM/TCO regressions that AOT tests miss
# because those tests still run via the Rust interpreter.
echo "=== Stage 3: Fixpoint test ==="
FIXPOINT_SRC="${AIRL_ROOT}/tests/fixpoint/fixpoint_smoke.airl"
FIXPOINT_BIN="/tmp/g3-fixpoint-smoke-$$"
FIXPOINT_EXPECTED="sum:55|rev:cba"

if [[ ! -f "$FIXPOINT_SRC" ]]; then
    echo "FIXPOINT FAIL: test source not found: $FIXPOINT_SRC"
    exit 1
fi

timeout 60 "$BUILD_PATH" -- "$FIXPOINT_SRC" -o "$FIXPOINT_BIN" \
    > /tmp/g3-fixpoint-compile.log 2>&1 \
    || {
        rc=$?
        if [[ $rc -eq 124 ]]; then
            echo "FIXPOINT FAIL: g3 compile timed out after 60s (likely OOM or infinite loop)"
        else
            echo "FIXPOINT FAIL: g3 could not compile fixpoint smoke test (exit $rc)"
        fi
        cat /tmp/g3-fixpoint-compile.log
        exit 1
    }

ACTUAL=$(timeout 10 "$FIXPOINT_BIN" 2>&1) || {
    echo "FIXPOINT FAIL: compiled binary crashed or timed out"
    exit 1
}

if [[ "$ACTUAL" != "$FIXPOINT_EXPECTED" ]]; then
    echo "FIXPOINT FAIL: expected '$FIXPOINT_EXPECTED' got '$ACTUAL'"
    exit 1
fi

echo "Stage 3 OK — fixpoint smoke: $ACTUAL"
rm -f "$FIXPOINT_BIN" /tmp/g3-fixpoint-compile.log
