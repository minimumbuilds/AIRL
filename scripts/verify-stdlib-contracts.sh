#!/usr/bin/env bash
# scripts/verify-stdlib-contracts.sh
#
# Verify all stdlib/*.airl contracts via g3 Z3 verification.
# Each stdlib file is compiled as a user file, which triggers Z3 verification
# (g3-compile-source-with-z3-strict) and fails with exit code 1 if any
# contract is disproven.
#
# Exit 0 if all contracts are proven/unknown/cached.
# Exit 1 if any contract is disproven or a compile error occurs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STDLIB_DIR="${AIRL_STDLIB:-$REPO_ROOT/stdlib}"

# Find the g3 binary — check arch-specific symlink, generic symlink, then PATH.
# In a worktree, the binary lives in the main repo (git common dir), not the worktree.
ARCH="$(uname -m)"
GIT_COMMON_DIR="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null || true)"
MAIN_REPO_ROOT=""
if [[ -n "$GIT_COMMON_DIR" ]]; then
    # GIT_COMMON_DIR may be absolute or relative; its parent is the main repo root.
    if [[ "$GIT_COMMON_DIR" = /* ]]; then
        MAIN_REPO_ROOT="$(cd "$GIT_COMMON_DIR/.." && pwd)"
    else
        MAIN_REPO_ROOT="$(cd "$REPO_ROOT/$GIT_COMMON_DIR/.." && pwd)"
    fi
fi

G3=""
for candidate in \
    "$REPO_ROOT/g3-$ARCH" \
    "$REPO_ROOT/g3" \
    "${MAIN_REPO_ROOT:+$MAIN_REPO_ROOT/g3-$ARCH}" \
    "${MAIN_REPO_ROOT:+$MAIN_REPO_ROOT/g3}" \
    "$(command -v g3 2>/dev/null || true)"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        G3="$(realpath "$candidate")"
        break
    fi
done

if [[ -z "$G3" ]]; then
    echo "ERROR: g3 not found at $REPO_ROOT/g3-$ARCH or g3-$ARCH — run scripts/build-g3.sh first" >&2
    exit 1
fi

echo "=== Stdlib Contract Verification ==="
echo "g3:      $G3"
echo "stdlib:  $STDLIB_DIR"
echo ""

FAIL=0
TOTAL=0
DISPROVEN=0
ERRORS=0

# Run from REPO_ROOT so .g3-z3-cache is found/updated in the right place.
cd "$REPO_ROOT"
export AIRL_STDLIB="$STDLIB_DIR"

for f in "$STDLIB_DIR"/*.airl; do
    name=$(basename "$f")
    TOTAL=$((TOTAL + 1))
    TMPOUT="/tmp/g3-stdlib-verify-$$-${name%.airl}"

    # Compile the stdlib file as a user input file.
    # g3 compiles user files with Z3 verification (g3-compile-source-with-z3-strict)
    # and exits 1 with "Compile error: Z3 contract violation: ..." on disproven contracts.
    if output=$("$G3" -- "$f" -o "$TMPOUT" 2>&1); then
        echo "OK:   $name"
        rm -f "$TMPOUT"
    else
        rm -f "$TMPOUT"
        if echo "$output" | grep -q "Z3 contract violation\|disproven contract"; then
            echo "FAIL: $name — disproven contracts:"
            echo "$output" | grep -E "Z3 contract violation|disproven" \
                | sed 's/^/        /'
            DISPROVEN=$((DISPROVEN + 1))
            FAIL=1
        else
            echo "ERR:  $name — compilation error:"
            echo "$output" | tail -5 | sed 's/^/        /'
            ERRORS=$((ERRORS + 1))
            FAIL=1
        fi
    fi
done

echo ""
echo "=== Results ==="
printf "Files checked: %d\n" "$TOTAL"
printf "Disproven:     %d\n" "$DISPROVEN"
printf "Other errors:  %d\n" "$ERRORS"
echo ""

if [[ $FAIL -eq 1 ]]; then
    echo "FAILED — fix disproven contracts before merging"
    exit 1
else
    echo "PASSED — all stdlib contracts proven/unknown/cached"
    exit 0
fi
