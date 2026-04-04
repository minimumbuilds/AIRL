#!/usr/bin/env bash
# Integration tests for mynameisAIRL MCP prompt server
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVER_DIR="$(dirname "$SCRIPT_DIR")"

PASS=0
FAIL=0

assert_contains() {
    local label="$1" output="$2" expected="$3"
    if echo "$output" | grep -qF "$expected"; then
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $label"
        echo "    expected to contain: $expected"
        echo "    got: $(echo "$output" | head -c 200)"
        FAIL=$((FAIL + 1))
    fi
}

assert_not_contains() {
    local label="$1" output="$2" unexpected="$3"
    if echo "$output" | grep -qF "$unexpected"; then
        echo "  FAIL: $label"
        echo "    expected NOT to contain: $unexpected"
        echo "    got: $(echo "$output" | head -c 200)"
        FAIL=$((FAIL + 1))
    else
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    fi
}

echo "=== mynameisAIRL Integration Tests ==="

# Build the server (build.sh handles g3 check, guide embedding, etc.)
echo "Building mynameisairl..."
BIN="/tmp/mynameisairl-test-$$"
if ! bash "$SERVER_DIR/build.sh" "$BIN" 2>/tmp/mynameisairl-build.log; then
    echo "FAIL: build failed"
    cat /tmp/mynameisairl-build.log
    exit 1
fi
echo "Build succeeded."
echo ""

run_server() {
    local input="$1"
    echo "$input" | AIRL_ALLOW_EXEC="*" "$BIN" 2>/dev/null
}

# ─── Test 1: Initialize ──────────────────────────────────────────────
echo "Test 1: Initialize"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}')
assert_contains "response has server name" "$OUTPUT" "mynameisAIRL"
assert_contains "response has prompts capability" "$OUTPUT" "prompts"
echo ""

# ─── Test 2: Prompts list ────────────────────────────────────────────
echo "Test 2: Prompts list"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"prompts/list","params":{}}')
assert_contains "has teach_airl prompt" "$OUTPUT" "teach_airl"
assert_contains "has description" "$OUTPUT" "Comprehensive AIRL language guide"
echo ""

# ─── Test 3: Prompts get ─────────────────────────────────────────────
echo "Test 3: Prompts get"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"teach_airl"}}')
assert_contains "has messages" "$OUTPUT" "messages"
assert_contains "contains AIRL guide content (defn keyword)" "$OUTPUT" "defn"
assert_contains "contains AIRL guide content (S-expression)" "$OUTPUT" "S-expression"
assert_contains "has kung-fu response" "$OUTPUT" "I know kung-fu"
echo ""

# ─── Test 4: Unknown prompt ──────────────────────────────────────────
echo "Test 4: Unknown prompt"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"bogus"}}')
assert_contains "error for unknown prompt" "$OUTPUT" "not found"
echo ""

# ─── Test 5: Empty tools list ────────────────────────────────────────
echo "Test 5: Empty tools list"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')
assert_contains "has tools key" "$OUTPUT" "tools"
echo ""

# ─── Test 6: Unknown method ──────────────────────────────────────────
echo "Test 6: Unknown method"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"bogus/method","params":{}}')
assert_contains "method not found error" "$OUTPUT" "Method not found"
echo ""

# ─── Test 7: Version from Cargo.toml ────────────────────────────────
echo "Test 7: Version"
OUTPUT=$(run_server '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}')
assert_contains "version matches Cargo.toml" "$OUTPUT" "0.11.0"
echo ""

# Cleanup
rm -f "$BIN"

# Summary
echo "=== Results: $PASS passed, $FAIL failed ==="
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
