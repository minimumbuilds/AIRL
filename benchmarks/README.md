# AIRL vs Python: AI Code Generation Benchmark

This benchmark compares how well AI (Claude) generates code in AIRL versus Python across a set of programming tasks. It measures whether a language designed for AI systems actually produces better results when AI writes the code.

## What It Tests

The benchmark asks Claude to generate implementations for 5 tasks in both AIRL and Python, then runs each generated program and measures the results.

**Tasks:**
1. **Safe Divide** -- Integer division with error handling for division by zero
2. **Fibonacci** -- Nth Fibonacci number with input validation
3. **List Processing** -- Filter, map, and fold over a list
4. **Input Validation** -- Range-check an integer and return Result/error
5. **String Tokenizer** -- Split a string by whitespace

## Hypotheses

### 1. Token Efficiency
AIRL programs with full contracts should be comparable in size (or smaller) than Python programs with equivalent assert-based validation. AIRL's mandatory contracts replace what would be informal comments or forgotten checks in Python.

### 2. Correctness
AI-generated AIRL should compile and run correctly at similar or higher rates than Python. The AIRL compiler's type checker and contract verifier catch errors at compile time that would be runtime errors in Python.

### 3. Contract Safety
AIRL's mandatory contracts provide stronger safety guarantees than Python's optional asserts. The AI must write contracts in AIRL (the compiler enforces it), while Python asserts are easily omitted.

## Prerequisites

- **Rust toolchain** with `cargo` (for building AIRL)
- **Python 3** (for running generated Python programs)
- **Claude CLI** (`claude` command) -- install from https://docs.anthropic.com/claude-code
- First build takes 5-15 minutes (Z3 C++ compilation)

## Running

From the repository root:

```bash
# Build AIRL first (if not already built)
cargo build

# Run the full benchmark
./benchmarks/run.sh
```

The script will:
1. Build the AIRL compiler
2. Generate AIRL and Python code for each task using Claude
3. Measure code size (chars, words, lines)
4. Execute both versions and check for success/failure
5. Print a summary table
6. Save detailed results to `benchmarks/results/run_YYYY-MM-DD_HHMMSS.md`

If the `claude` CLI is not installed, the script skips generation and runs any pre-existing output files.

## Analyzing Results

After running the benchmark:

```bash
./benchmarks/analyze.sh
```

This reads the most recent results file and computes:
- Average AIRL vs Python code size
- Size ratio (AIRL/Python)
- Correctness rate for each language
- Edge case coverage (informational)

## Interpreting Results

The results markdown file (in `benchmarks/results/`) contains:

- **Size comparison table** -- chars, words, and lines for each task in both languages
- **Correctness** -- whether each generated program ran successfully (exit code 0)
- **Generated code** -- the actual AIRL and Python code Claude produced
- **Output** -- what each program printed when run

Key metrics to look at:
- **Char ratio < 1.0** means AIRL is more concise than Python
- **Char ratio > 1.0** means Python is more concise
- **AIRL correct / total** vs **Python correct / total** shows relative generation reliability

## Directory Structure

```
benchmarks/
  run.sh              # Main benchmark runner
  analyze.sh           # Post-run analysis script
  tasks/               # Task specifications (what to implement)
    01_safe_divide.md
    02_fibonacci.md
    03_list_processing.md
    04_input_validation.md
    05_string_tokenizer.md
  prompts/             # System prompts for code generation
    airl_system.md     # AIRL language reference + generation instructions
    python_system.md   # Python generation instructions
  edge_cases/          # Edge case descriptions for each task
    01_edge.txt
    02_edge.txt
    03_edge.txt
    04_edge.txt
    05_edge.txt
  output/              # Generated code (gitignored)
    airl/              # Generated .airl files
    python/            # Generated .py files
  results/             # Benchmark results (committable)
    run_*.md
```

## Limitations

- Results vary between runs since Claude's output is non-deterministic
- The benchmark measures single-shot generation (no iterative refinement)
- Edge case testing is manual in v1 (automated testing planned for v2)
- Python "contracts" are just asserts -- they can be bypassed with `python -O`
- AIRL contracts are compiler-enforced and cannot be disabled
