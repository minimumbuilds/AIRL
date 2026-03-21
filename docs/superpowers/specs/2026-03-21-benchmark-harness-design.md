# AIRL Benchmark Harness — Design Spec

## Problem

AIRL claims three advantages over existing languages for AI code generation:
1. **Token efficiency** — S-expression syntax is more token-dense
2. **First-attempt correctness** — Unambiguous grammar leads to fewer parse/semantic errors
3. **Contract-driven safety** — Mandatory contracts catch bugs that optional asserts miss

These claims are unvalidated. This benchmark harness tests them empirically.

## Approach

Use Claude Code CLI (`claude --print`) to generate equivalent implementations in AIRL and Python for 5 programming tasks. Measure code size, correctness, and contract effectiveness.

## Directory Structure

```
benchmarks/
├── README.md                    # Usage and interpretation guide
├── run.sh                       # Main harness: generates code, runs tests, collects metrics
├── analyze.sh                   # Scores and compares results, outputs summary table
├── tasks/                       # Task specifications (natural language)
│   ├── 01_safe_divide.md
│   ├── 02_fibonacci.md
│   ├── 03_list_processing.md
│   ├── 04_input_validation.md
│   └── 05_string_tokenizer.md
├── prompts/
│   ├── airl_system.md           # System prompt for AIRL generation
│   └── python_system.md         # System prompt for Python generation
├── edge_cases/                  # Bug injection inputs per task
│   ├── 01_edge.txt
│   ├── 02_edge.txt
│   ├── 03_edge.txt
│   ├── 04_edge.txt
│   └── 05_edge.txt
├── output/                      # Generated code (gitignored)
│   ├── airl/
│   └── python/
└── results/                     # Run results (committed)
```

## Tasks

### 01: Safe Division
Divide two integers, return Result. Handle division by zero.
- **Edge case:** denominator = 0
- **Contract test:** `:requires [(not (= b 0))]`

### 02: Fibonacci with Bounds
Compute nth Fibonacci number. Input must be 0-30.
- **Edge case:** n = -1
- **Contract test:** `:requires [(>= n 0) (<= n 30)]`

### 03: List Processing Pipeline
Take a list of integers, filter evens, double them, return the sum.
- **Edge case:** empty list (should return 0, not crash)
- **Contract test:** `:ensures [(>= result 0)]`

### 04: Input Validation
Validate an "age" integer: must be 0-150. Return Ok(age) or Err(reason).
- **Edge case:** age = -5
- **Contract test:** `:requires [(>= age 0) (<= age 150)]`

### 05: String Tokenizer
Split string by whitespace, filter empty tokens, return list and count.
- **Edge case:** empty string ""
- **Contract test:** `:ensures [(>= (length result) 0)]`

## Prompts

### AIRL System Prompt
Include the AIRL LLM Guide (or a condensed version) plus:
> "Generate a complete, runnable AIRL program for the following task. Output ONLY the AIRL code, no explanation. The program must include proper contracts (:requires/:ensures) and print the result."

### Python System Prompt
> "Generate a complete, runnable Python program for the following task. Include assert statements or input validation equivalent to what you would put in a contract (preconditions and postconditions). Output ONLY the Python code, no explanation. The program must print the result."

## Execution Flow (run.sh)

```bash
for each task in tasks/*.md:
  # 1. Generate AIRL
  cat prompts/airl_system.md task | claude --print > output/airl/task_N.airl

  # 2. Generate Python
  cat prompts/python_system.md task | claude --print > output/python/task_N.py

  # 3. Measure size
  airl_chars=$(wc -c < output/airl/task_N.airl)
  python_chars=$(wc -c < output/python/task_N.py)

  # 4. Test correctness (normal input)
  cargo run -- run output/airl/task_N.airl → pass/fail
  python3 output/python/task_N.py → pass/fail

  # 5. Test edge case (inject bug)
  # Modify input per edge_cases/N_edge.txt, re-run
  # Record: did AIRL contract catch it? Did Python assert catch it?
```

## Metrics

| Metric | Method | Unit |
|--------|--------|------|
| Code size (chars) | `wc -c` | characters |
| Code size (words) | `wc -w` | words |
| Code size (lines) | `wc -l` | lines |
| Parse success | exit code of parser/interpreter | bool |
| Execution success | correct output | bool |
| Contract catch | AIRL rejects edge case with ContractViolation | bool |
| Assert catch | Python raises AssertionError on edge case | bool |

## Output Format (results/)

Markdown table per run:

```
| Task | AIRL chars | Py chars | Ratio | AIRL correct | Py correct | AIRL catches edge | Py catches edge |
|------|-----------|----------|-------|-------------|-----------|-------------------|-----------------|
| 01   | 234       | 189      | 1.24  | ✓           | ✓         | ✓                 | ✗               |
```

## Constraints

- No API keys — uses `claude --print` CLI
- AIRL binary must be built first (`cargo build`)
- Python 3 must be available
- Results are committed to git for historical tracking
- `output/` is gitignored (regenerated each run)

## Edge Case Design

Each edge case is a specific input that should be rejected. The task spec describes the "happy path" — the edge case tests whether contracts/asserts catch the unhappy path. The edge case file contains the specific invocation to test.

For AIRL, edge cases are tested by creating a wrapper .airl file that calls the function with the bad input. For Python, by calling the function with the bad argument.
