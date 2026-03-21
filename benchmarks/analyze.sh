#!/usr/bin/env bash
#
# Analyze the most recent AIRL vs Python benchmark results.
#
# Usage: ./benchmarks/analyze.sh  (from repo root)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
OUTPUT_DIR="$SCRIPT_DIR/output"

# ---------------------------------------------------------------------------
# Find the most recent results file
# ---------------------------------------------------------------------------
LATEST=$(ls -t "$RESULTS_DIR"/run_*.md 2>/dev/null | head -1)

if [ -z "$LATEST" ]; then
    echo "No results files found in $RESULTS_DIR/"
    echo "Run ./benchmarks/run.sh first."
    exit 1
fi

echo "Analyzing: $(basename "$LATEST")"
echo ""

# ---------------------------------------------------------------------------
# Compute stats from the output files
# ---------------------------------------------------------------------------
AIRL_DIR="$OUTPUT_DIR/airl"
PY_DIR="$OUTPUT_DIR/python"

if [ ! -d "$AIRL_DIR" ] || [ ! -d "$PY_DIR" ]; then
    echo "No output files found. Run ./benchmarks/run.sh first."
    exit 1
fi

total_airl_chars=0
total_py_chars=0
total_airl_words=0
total_py_words=0
airl_count=0
py_count=0
task_count=0

airl_correct=0
py_correct=0

AIRL_BIN="$SCRIPT_DIR/../target/debug/airl-driver"

echo "=== Per-Task Analysis ==="
echo ""

for num in 01 02 03 04 05; do
    airl_file="$AIRL_DIR/${num}.airl"
    py_file="$PY_DIR/${num}.py"

    if [ ! -f "$airl_file" ] || [ ! -f "$py_file" ]; then
        continue
    fi

    ((task_count++)) || true

    ac=$(wc -c < "$airl_file")
    aw=$(wc -w < "$airl_file")
    pc=$(wc -c < "$py_file")
    pw=$(wc -w < "$py_file")

    total_airl_chars=$((total_airl_chars + ac))
    total_py_chars=$((total_py_chars + pc))
    total_airl_words=$((total_airl_words + aw))
    total_py_words=$((total_py_words + pw))

    # Test AIRL execution
    set +e
    "$AIRL_BIN" run "$airl_file" &>/dev/null
    airl_ok=$?
    set -e
    [ "$airl_ok" -eq 0 ] && ((airl_correct++)) || true

    # Test Python execution
    set +e
    python3 "$py_file" &>/dev/null
    py_ok=$?
    set -e
    [ "$py_ok" -eq 0 ] && ((py_correct++)) || true

    if [ "$pc" -gt 0 ]; then
        ratio=$(awk "BEGIN { printf \"%.2f\", $ac / $pc }")
    else
        ratio="N/A"
    fi

    airl_status="FAIL"
    py_status="FAIL"
    [ "$airl_ok" -eq 0 ] && airl_status="PASS"
    [ "$py_ok" -eq 0 ] && py_status="PASS"

    printf "Task %s: AIRL %5d chars (%s)  Python %5d chars (%s)  ratio=%s\n" \
        "$num" "$ac" "$airl_status" "$pc" "$py_status" "$ratio"
done

echo ""
echo "=== Summary ==="
echo ""

# Averages
if [ "$task_count" -gt 0 ]; then
    avg_airl=$(awk "BEGIN { printf \"%.0f\", $total_airl_chars / $task_count }")
    avg_py=$(awk "BEGIN { printf \"%.0f\", $total_py_chars / $task_count }")
    avg_ratio=$(awk "BEGIN { printf \"%.2f\", $total_airl_chars / $total_py_chars }")

    echo "Average AIRL chars:   $avg_airl"
    echo "Average Python chars: $avg_py"
    echo "Average ratio (AIRL/Python): $avg_ratio"
    echo ""
fi

echo "AIRL correct:   $airl_correct / $task_count"
echo "Python correct: $py_correct / $task_count"
echo ""

# Edge case analysis (informational)
echo "=== Edge Cases (manual testing) ==="
echo ""
echo "Edge case test files are in benchmarks/edge_cases/."
echo "To test edge cases, modify the generated code to use the edge inputs"
echo "and re-run. This will be automated in v2."
echo ""

for num in 01 02 03 04 05; do
    edge_file="$SCRIPT_DIR/edge_cases/${num}_edge.txt"
    if [ -f "$edge_file" ]; then
        printf "Task %s edge: %s\n" "$num" "$(cat "$edge_file" | tr -d '\n')"
    fi
done

echo ""
echo "Results file: $(basename "$LATEST")"
