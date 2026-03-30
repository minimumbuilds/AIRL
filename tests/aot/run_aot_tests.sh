#!/bin/bash
# G2 AOT Test Runner
# Compiles each .airl test file via G2 pipeline to a native binary, runs it, compares output.
#
# Test files use a header to declare dependencies:
#   ;; EXPECT: expected output
#   ;; DEPS: bootstrap/lexer.airl bootstrap/parser.airl
# If no DEPS line, the test is standalone (just stdlib).
#
# Compiled binaries are cached in tests/aot/cache/. To force recompile, delete the cache dir.
set -euo pipefail

CACHE_DIR="tests/aot/cache"
mkdir -p "$CACHE_DIR"

PASS=0
FAIL=0
COMPILE_FAIL=0
SKIP=0
ERRORS=""

for test in tests/aot/round*.airl; do
  name=$(basename "$test" .airl)
  expected=$(head -1 "$test" | sed 's/^;; EXPECT: //')

  # Skip tests marked SKIP_AOT
  if grep -q '^;; SKIP_AOT:' "$test"; then
    echo "SKIP: $name ($(grep '^;; SKIP_AOT:' "$test" | sed 's/^;; SKIP_AOT: //'))"
    SKIP=$((SKIP + 1))
    continue
  fi

  # Extract DEPS from second line if present
  deps_line=$(sed -n '2p' "$test")
  deps=""
  if echo "$deps_line" | grep -q '^;; DEPS:'; then
    deps=$(echo "$deps_line" | sed 's/^;; DEPS: //')
  fi

  bin="${CACHE_DIR}/${name}"

  # Check if cached binary exists and is newer than source
  need_compile=1
  if [ -x "$bin" ] && [ "$bin" -nt "$test" ]; then
    # Check deps too
    stale=0
    for dep in $deps; do
      if [ -n "$dep" ] && [ "$dep" -nt "$bin" ]; then
        stale=1
        break
      fi
    done
    if [ "$stale" -eq 0 ]; then
      need_compile=0
    fi
  fi

  if [ "$need_compile" -eq 1 ]; then
    # Build command
    cmd="cargo run --release --features aot -- run"
    cmd="$cmd --load bootstrap/lexer.airl"
    cmd="$cmd --load bootstrap/parser.airl"
    cmd="$cmd --load bootstrap/bc_compiler.airl"
    cmd="$cmd bootstrap/g3_compiler.airl --"
    if [ -n "$deps" ]; then
      cmd="$cmd $deps"
    fi
    cmd="$cmd $test -o $bin"

    # Compile via G2 pipeline
    if ! eval "$cmd" > /dev/null 2>&1; then
      echo "COMPILE_FAIL: $name"
      COMPILE_FAIL=$((COMPILE_FAIL + 1))
      ERRORS="$ERRORS\n  COMPILE_FAIL: $name"
      continue
    fi
  fi

  # Run the native binary
  actual=$("$bin" 2>&1) || true

  if [ "$actual" = "$expected" ]; then
    echo "PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $name"
    echo "  expected: '$expected'"
    echo "  actual:   '$actual'"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  FAIL: $name (expected '$expected', got '$actual')"
  fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed, $COMPILE_FAIL compile errors, $SKIP skipped"
if [ $FAIL -gt 0 ] || [ $COMPILE_FAIL -gt 0 ]; then
  echo -e "Failures:$ERRORS"
  exit 1
fi
