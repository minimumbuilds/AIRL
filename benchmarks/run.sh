#!/usr/bin/env bash
#
# AIRL vs Python AI Code Generation Benchmark
#
# Usage: ./benchmarks/run.sh  (from repo root)
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve paths relative to this script (so it works from any cwd)
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TASKS_DIR="$SCRIPT_DIR/tasks"
PROMPTS_DIR="$SCRIPT_DIR/prompts"
OUTPUT_DIR="$SCRIPT_DIR/output"
RESULTS_DIR="$SCRIPT_DIR/results"

# ---------------------------------------------------------------------------
# Color helpers (disabled if not a terminal)
# ---------------------------------------------------------------------------
if [ -t 1 ]; then
    GREEN='\033[0;32m'
    RED='\033[0;31m'
    YELLOW='\033[0;33m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    RESET='\033[0m'
else
    GREEN='' RED='' YELLOW='' CYAN='' BOLD='' RESET=''
fi

pass() { printf "${GREEN}PASS${RESET}"; }
fail() { printf "${RED}FAIL${RESET}"; }
info() { printf "${CYAN}>>>${RESET} %s\n" "$*"; }
warn() { printf "${YELLOW}WARNING:${RESET} %s\n" "$*"; }

# ---------------------------------------------------------------------------
# Prerequisite checks
# ---------------------------------------------------------------------------
info "Checking prerequisites..."

HAVE_CLAUDE=true
if ! command -v claude &>/dev/null; then
    warn "claude CLI not found. Install it from https://docs.anthropic.com/claude-code"
    warn "Skipping code generation -- will only run pre-existing output files."
    HAVE_CLAUDE=false
fi

if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 is required but not found." >&2
    exit 1
fi

# Build the AIRL binary
info "Building AIRL compiler..."
(cd "$REPO_ROOT" && cargo build -p airl-driver --quiet 2>&1) || {
    echo "ERROR: cargo build failed." >&2
    exit 1
}

AIRL_BIN="$REPO_ROOT/target/debug/airl-driver"
if [ ! -x "$AIRL_BIN" ]; then
    # Try finding it via cargo
    AIRL_BIN="$(cd "$REPO_ROOT" && cargo build --message-format=json 2>/dev/null \
        | jq -r 'select(.executable != null) | .executable' | head -1)"
fi

# ---------------------------------------------------------------------------
# Prepare output directories
# ---------------------------------------------------------------------------
mkdir -p "$OUTPUT_DIR/airl" "$OUTPUT_DIR/python" "$RESULTS_DIR"

# ---------------------------------------------------------------------------
# Task list
# ---------------------------------------------------------------------------
TASKS=(
    "01_safe_divide"
    "02_fibonacci"
    "03_list_processing"
    "04_input_validation"
    "05_string_tokenizer"
    "06_absolute_value"
    "07_gcd"
    "08_power"
    "09_reverse_list"
    "10_find_max"
    "11_remove_duplicates"
    "12_zip_lists"
    "13_palindrome_check"
    "14_count_vowels"
    "15_caesar_cipher"
    "16_safe_sqrt"
    "17_parse_int"
    "18_bounded_access"
    "19_flatten_list"
    "20_group_by_parity"
    "21_running_sum"
    "22_word_frequency"
    "23_matrix_transpose"
    "24_merge_sorted"
    "25_pipeline"
)

TASK_NAMES=(
    "Safe Divide"
    "Fibonacci"
    "List Processing"
    "Input Validation"
    "String Tokenizer"
    "Absolute Value"
    "GCD"
    "Power"
    "Reverse List"
    "Find Max"
    "Remove Duplicates"
    "Zip Lists"
    "Palindrome Check"
    "Count Vowels"
    "Caesar Cipher"
    "Safe Sqrt"
    "Parse Int"
    "Bounded Access"
    "Flatten List"
    "Group by Parity"
    "Running Sum"
    "Word Frequency"
    "Matrix Transpose"
    "Merge Sorted"
    "Pipeline"
)

# ---------------------------------------------------------------------------
# Result accumulators
# ---------------------------------------------------------------------------
declare -a AIRL_CHARS AIRL_WORDS AIRL_LINES
declare -a PY_CHARS PY_WORDS PY_LINES
declare -a AIRL_EXIT AIRL_OUTPUT
declare -a PY_EXIT PY_OUTPUT

TIMESTAMP="$(date +%Y-%m-%d_%H%M%S)"

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------
for i in "${!TASKS[@]}"; do
    task="${TASKS[$i]}"
    name="${TASK_NAMES[$i]}"
    num="${task:0:2}"

    printf "\n${BOLD}=== Task %s: %s ===${RESET}\n" "$num" "$name"

    # -- Generate AIRL -------------------------------------------------------
    airl_file="$OUTPUT_DIR/airl/${num}.airl"
    if [ "$HAVE_CLAUDE" = true ]; then
        info "Generating AIRL for task $num..."
        claude -p "$(cat "$PROMPTS_DIR/airl_system.md")

$(cat "$TASKS_DIR/${task}.md")" --output-format text 2>/dev/null \
            | sed '/^```/d' \
            | sed -n '/^[[:space:]]*(defn\|^[[:space:]]*(let\|^[[:space:]]*(do\|^[[:space:]]*(print\|^[[:space:]]*(match\|^[[:space:]]*(if\|^;;/,$p' \
            | sed '/^`/,$d' \
            > "$airl_file" || {
            warn "claude failed for AIRL task $num"
            echo ";; generation failed" > "$airl_file"
        }
    else
        if [ ! -f "$airl_file" ]; then
            warn "No pre-existing AIRL output for task $num"
            echo ";; no output" > "$airl_file"
        fi
    fi

    # -- Generate Python -----------------------------------------------------
    py_file="$OUTPUT_DIR/python/${num}.py"
    if [ "$HAVE_CLAUDE" = true ]; then
        info "Generating Python for task $num..."
        claude -p "$(cat "$PROMPTS_DIR/python_system.md")

$(cat "$TASKS_DIR/${task}.md")" --output-format text 2>/dev/null \
            | sed '/^```/d' \
            | sed -n '/^def \|^import \|^from \|^#/,$p' \
            | sed '/^`/,$d' \
            > "$py_file" || {
            warn "claude failed for Python task $num"
            echo "# generation failed" > "$py_file"
        }
    else
        if [ ! -f "$py_file" ]; then
            warn "No pre-existing Python output for task $num"
            echo "# no output" > "$py_file"
        fi
    fi

    # -- Measure sizes -------------------------------------------------------
    AIRL_CHARS[$i]=$(wc -c < "$airl_file")
    AIRL_WORDS[$i]=$(wc -w < "$airl_file")
    AIRL_LINES[$i]=$(wc -l < "$airl_file")

    PY_CHARS[$i]=$(wc -c < "$py_file")
    PY_WORDS[$i]=$(wc -w < "$py_file")
    PY_LINES[$i]=$(wc -l < "$py_file")

    # -- Run AIRL ------------------------------------------------------------
    info "Running AIRL task $num..."
    set +e
    airl_out=$("$AIRL_BIN" run "$airl_file" 2>&1)
    AIRL_EXIT[$i]=$?
    set -e
    AIRL_OUTPUT[$i]="$airl_out"

    if [ "${AIRL_EXIT[$i]}" -eq 0 ]; then
        printf "  AIRL: $(pass)  output: %s\n" "$(echo "$airl_out" | head -1)"
    else
        printf "  AIRL: $(fail)  exit=%d  output: %s\n" "${AIRL_EXIT[$i]}" "$(echo "$airl_out" | head -1)"
    fi

    # -- Run Python ----------------------------------------------------------
    info "Running Python task $num..."
    set +e
    py_out=$(python3 "$py_file" 2>&1)
    PY_EXIT[$i]=$?
    set -e
    PY_OUTPUT[$i]="$py_out"

    if [ "${PY_EXIT[$i]}" -eq 0 ]; then
        printf "  Python: $(pass)  output: %s\n" "$(echo "$py_out" | head -1)"
    else
        printf "  Python: $(fail)  exit=%d  output: %s\n" "${PY_EXIT[$i]}" "$(echo "$py_out" | head -1)"
    fi
done

# ---------------------------------------------------------------------------
# Intent Recovery Pass (Hypothesis 4)
# ---------------------------------------------------------------------------
declare -a AIRL_INTENT_SCORES PY_INTENT_SCORES

if [ "$HAVE_CLAUDE" = true ]; then
    printf "\n${BOLD}=== Intent Recovery Analysis ===${RESET}\n"

    for i in "${!TASKS[@]}"; do
        task="${TASKS[$i]}"
        name="${TASK_NAMES[$i]}"
        num="${task:0:2}"

        airl_file="$OUTPUT_DIR/airl/${num}.airl"
        py_file="$OUTPUT_DIR/python/${num}.py"
        task_file="$TASKS_DIR/${task}.md"

        # Skip if generation failed
        if grep -q "generation failed\|no output" "$airl_file" 2>/dev/null; then
            AIRL_INTENT_SCORES[$i]="N/A"
            PY_INTENT_SCORES[$i]="N/A"
            continue
        fi

        info "Recovering intent from AIRL task $num..."
        airl_intent=$(claude -p "$(cat "$PROMPTS_DIR/intent_recovery.md")
$(cat "$airl_file")" --output-format text 2>/dev/null) || airl_intent="(recovery failed)"

        info "Recovering intent from Python task $num..."
        py_intent=$(claude -p "$(cat "$PROMPTS_DIR/intent_recovery.md")
$(cat "$py_file")" --output-format text 2>/dev/null) || py_intent="(recovery failed)"

        # Save recovered intents
        echo "$airl_intent" > "$OUTPUT_DIR/airl/${num}_intent.txt"
        echo "$py_intent" > "$OUTPUT_DIR/python/${num}_intent.txt"

        # Judge AIRL intent
        info "Scoring AIRL intent for task $num..."
        task_spec=$(cat "$task_file")
        airl_scores=$(claude -p "$(cat "$PROMPTS_DIR/intent_judge.md")
${task_spec}

RECONSTRUCTED DESCRIPTION:
${airl_intent}" --output-format text 2>/dev/null) || airl_scores="0 0 0 0"

        # Judge Python intent
        info "Scoring Python intent for task $num..."
        py_scores=$(claude -p "$(cat "$PROMPTS_DIR/intent_judge.md")
${task_spec}

RECONSTRUCTED DESCRIPTION:
${py_intent}" --output-format text 2>/dev/null) || py_scores="0 0 0 0"

        # Parse scores and compute average (extract first 4 numbers)
        airl_avg=$(echo "$airl_scores" | grep -oE '[0-9]+' | head -4 | awk '{s+=$1; n++} END {if(n>0) printf "%.1f", s/n; else print "N/A"}')
        py_avg=$(echo "$py_scores" | grep -oE '[0-9]+' | head -4 | awk '{s+=$1; n++} END {if(n>0) printf "%.1f", s/n; else print "N/A"}')

        AIRL_INTENT_SCORES[$i]="$airl_avg"
        PY_INTENT_SCORES[$i]="$py_avg"

        # Save raw scores
        echo "$airl_scores" > "$OUTPUT_DIR/airl/${num}_score.txt"
        echo "$py_scores" > "$OUTPUT_DIR/python/${num}_score.txt"

        printf "  %s — AIRL intent: %s/5  Python intent: %s/5\n" "$name" "$airl_avg" "$py_avg"
    done
else
    for i in "${!TASKS[@]}"; do
        AIRL_INTENT_SCORES[$i]="N/A"
        PY_INTENT_SCORES[$i]="N/A"
    done
fi

# ---------------------------------------------------------------------------
# Summary table
# ---------------------------------------------------------------------------
printf "\n${BOLD}========================================${RESET}\n"
printf "${BOLD}         BENCHMARK RESULTS SUMMARY${RESET}\n"
printf "${BOLD}========================================${RESET}\n\n"

# Header
printf "%-20s | %-8s %-8s %-6s | %-8s %-8s %-6s | %-6s %-6s | %-7s %-7s\n" \
    "Task" "AI chars" "AI words" "AI ln" "Py chars" "Py words" "Py ln" "AIRL" "Python" "AI int" "Py int"
printf "%s\n" "--------------------+---------------------------+---------------------------+-------------+----------------"

AIRL_PASS=0
PY_PASS=0

for i in "${!TASKS[@]}"; do
    name="${TASK_NAMES[$i]}"

    airl_status="FAIL"
    py_status="FAIL"
    [ "${AIRL_EXIT[$i]}" -eq 0 ] && { airl_status="PASS"; ((AIRL_PASS++)) || true; }
    [ "${PY_EXIT[$i]}" -eq 0 ] && { py_status="PASS"; ((PY_PASS++)) || true; }

    printf "%-20s | %8d %8d %6d | %8d %8d %6d | %-6s %-6s | %-7s %-7s\n" \
        "$name" \
        "${AIRL_CHARS[$i]}" "${AIRL_WORDS[$i]}" "${AIRL_LINES[$i]}" \
        "${PY_CHARS[$i]}" "${PY_WORDS[$i]}" "${PY_LINES[$i]}" \
        "$airl_status" "$py_status" \
        "${AIRL_INTENT_SCORES[$i]}" "${PY_INTENT_SCORES[$i]}"
done

printf "\n"
printf "AIRL correct: %d / %d\n" "$AIRL_PASS" "${#TASKS[@]}"
printf "Python correct: %d / %d\n" "$PY_PASS" "${#TASKS[@]}"

# Compute average char ratio
total_airl_chars=0
total_py_chars=0
for i in "${!TASKS[@]}"; do
    total_airl_chars=$((total_airl_chars + AIRL_CHARS[$i]))
    total_py_chars=$((total_py_chars + PY_CHARS[$i]))
done

if [ "$total_py_chars" -gt 0 ]; then
    # Use awk for float division
    ratio=$(awk "BEGIN { printf \"%.2f\", $total_airl_chars / $total_py_chars }")
    printf "Total AIRL chars: %d  |  Total Python chars: %d  |  Ratio (AIRL/Python): %s\n" \
        "$total_airl_chars" "$total_py_chars" "$ratio"
fi

# ---------------------------------------------------------------------------
# Write results to markdown file
# ---------------------------------------------------------------------------
RESULTS_FILE="$RESULTS_DIR/run_${TIMESTAMP}.md"

cat > "$RESULTS_FILE" <<RESULTS_EOF
# AIRL vs Python Benchmark Results

**Date:** $(date '+%Y-%m-%d %H:%M:%S')
**Tasks:** ${#TASKS[@]}

## Hypothesis

1. **Token Efficiency:** AIRL programs are more concise than equivalent Python with contracts.
2. **Correctness:** AI-generated AIRL compiles and runs correctly as often as Python.
3. **Contract Safety:** AIRL's mandatory contracts catch errors that Python asserts miss.
4. **Intent Recoverability:** An AI can reconstruct the original specification more accurately from AIRL than from Python.

## Results

| Task | AIRL chars | AIRL words | AIRL lines | Python chars | Python words | Python lines | AIRL runs | Python runs | AIRL intent | Python intent |
|------|-----------|-----------|-----------|-------------|-------------|-------------|-----------|-------------|-------------|---------------|
RESULTS_EOF

for i in "${!TASKS[@]}"; do
    name="${TASK_NAMES[$i]}"
    airl_status="FAIL"
    py_status="FAIL"
    [ "${AIRL_EXIT[$i]}" -eq 0 ] && airl_status="PASS"
    [ "${PY_EXIT[$i]}" -eq 0 ] && py_status="PASS"

    printf "| %-18s | %9d | %9d | %9d | %11d | %11d | %11d | %-9s | %-11s | %-11s | %-13s |\n" \
        "$name" \
        "${AIRL_CHARS[$i]}" "${AIRL_WORDS[$i]}" "${AIRL_LINES[$i]}" \
        "${PY_CHARS[$i]}" "${PY_WORDS[$i]}" "${PY_LINES[$i]}" \
        "$airl_status" "$py_status" \
        "${AIRL_INTENT_SCORES[$i]}" "${PY_INTENT_SCORES[$i]}" >> "$RESULTS_FILE"
done

cat >> "$RESULTS_FILE" <<RESULTS_EOF

## Totals

- **AIRL correct:** $AIRL_PASS / ${#TASKS[@]}
- **Python correct:** $PY_PASS / ${#TASKS[@]}
- **Total AIRL chars:** $total_airl_chars
- **Total Python chars:** $total_py_chars
- **Char ratio (AIRL/Python):** $ratio

## Generated Code

### AIRL Programs

RESULTS_EOF

for i in "${!TASKS[@]}"; do
    num="${TASKS[$i]:0:2}"
    name="${TASK_NAMES[$i]}"
    airl_file="$OUTPUT_DIR/airl/${num}.airl"
    cat >> "$RESULTS_FILE" <<TASK_EOF
#### Task $num: $name

\`\`\`lisp
$(cat "$airl_file")
\`\`\`

**Exit code:** ${AIRL_EXIT[$i]}
**Output:** \`${AIRL_OUTPUT[$i]}\`

TASK_EOF
done

cat >> "$RESULTS_FILE" <<RESULTS_EOF
### Python Programs

RESULTS_EOF

for i in "${!TASKS[@]}"; do
    num="${TASKS[$i]:0:2}"
    name="${TASK_NAMES[$i]}"
    py_file="$OUTPUT_DIR/python/${num}.py"
    cat >> "$RESULTS_FILE" <<TASK_EOF
#### Task $num: $name

\`\`\`python
$(cat "$py_file")
\`\`\`

**Exit code:** ${PY_EXIT[$i]}
**Output:** \`${PY_OUTPUT[$i]}\`

TASK_EOF
done

printf "\nResults saved to: %s\n" "$RESULTS_FILE"
