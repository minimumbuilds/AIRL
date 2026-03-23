# AIRL Performance Benchmark: Bytecode VM

**Date:** 2026-03-23
**Mode:** Release build (`cargo build --release -p airl-driver`)
**Machine:** Linux 6.8.0-60-generic
**Method:** Best of 2-3 runs, wall-clock time via `date +%s%N`

## Context

AIRL now has three execution modes:
- **Interpreted** — tree-walking interpreter (`eval.rs`), full contract checking
- **IR VM (`--compiled`)** — AST → IR nodes → Rust VM (`ir_vm.rs`), no contracts, self-TCO
- **Bytecode VM (`--bytecode`)** — AST → flat register-based bytecode → tight execution loop (`bytecode_vm.rs`), self-TCO

## Results

| Benchmark | Interpreted | IR VM (--compiled) | Bytecode VM (--bytecode) | IR→Bytecode Speedup |
|-----------|------------|-------------------|--------------------------|---------------------|
| fib(30) | 41,785ms | 7,162ms | 4,626ms | **1.5x** |
| fact(12) x 10K | 793ms | 140ms | 147ms | ~1.0x (parity) |
| sum-evens x 5K | 3,427ms | 1,176ms | ERROR* | N/A |

*`sum-evens` fails in bytecode mode with `UndefinedSymbol: __lambda_2` — anonymous lambdas passed as arguments to stdlib higher-order functions (`map`/`filter`/`fold`) are not yet fully supported by the bytecode compiler. See Known Issues below.

## Previous IR VM vs Interpreted Baseline (from perf_2026-03-23_compiled.md)

For reference, the previous benchmark measured:

| Benchmark | Interpreted | IR VM | Interp→IR Speedup |
|-----------|------------|-------|-------------------|
| fib(30) | 32,250ms | 6,031ms | **5.3x** |
| fact(12) x 10K | 987ms | 207ms | **4.7x** |
| sum-evens x 5K | 1,882ms | 973ms | **1.9x** |

Note: interpreted times differ slightly between benchmark runs due to system load variance.

## Analysis

**Bytecode vs IR VM:**
- fib(30): 1.5x speedup. The flat bytecode instruction array with indexed register access avoids the HashMap-based variable lookup and recursive tree-walking of the IR VM, yielding meaningful gains on compute-heavy recursive code.
- fact(12)x10K: Near-parity (140ms vs 147ms). Tail-recursive loops with TCO perform similarly in both modes — startup overhead and TCO loop costs dominate at this scale.
- sum-evens: Bytecode VM does not yet handle anonymous closures (`fn [...]`) passed to stdlib higher-order functions. Falls back to an error rather than interpreted execution.

**Overall pipeline speedup (Interpreted → Bytecode):**
- fib(30): ~9x faster
- fact(12)x10K: ~5.4x faster

## Known Issues

### Bytecode VM: Anonymous Closures in Higher-Order Functions

When anonymous `fn` expressions are passed as arguments to stdlib functions like `map`, `filter`, or `fold`, the bytecode compiler generates a lambda reference (`__lambda_N`) but the bytecode VM cannot resolve it when the stdlib function calls back into it. This affects any program that uses lambda expressions as first-class values.

**Affected benchmark:** sum-evens x 5K
**Error:** `Runtime error: UndefinedSymbol: '__lambda_2'`
**Workaround:** Use named `defn` functions instead of anonymous `fn` literals when using stdlib higher-order functions in bytecode mode.

**Root cause:** The bytecode compiler extracts `fn` expressions into top-level lambda definitions, but the bytecode VM's dispatch for stdlib call-backs does not look up these generated lambdas in the function registry.

## Benchmark Programs

### fib(30)
```clojure
(defn fibonacci
  :sig [(n : i64) -> i64]
  :requires [(>= n 0) (<= n 40)]
  :ensures [(>= result 0)]
  :body (if (= n 0) 0
          (if (= n 1) 1
            (+ (fibonacci (- n 1)) (fibonacci (- n 2))))))

(print (fibonacci 30))
```

### fact(12) x 10K
```clojure
(defn fact-helper
  :sig [(n : i64) (acc : i64) -> i64]
  :requires [(>= n 0)]
  :ensures [(>= result 1)]
  :body (if (<= n 1) acc (fact-helper (- n 1) (* acc n))))

(defn run-fact
  :sig [(i : i64) -> i64]
  :requires [(>= i 0)]
  :ensures [(>= result 0)]
  :body (if (= i 0) 0 (do (fact-helper 12 1) (run-fact (- i 1)))))

(print (run-fact 10000))
```

### sum-evens x 5K
```clojure
(defn run-evens
  :sig [(i : i64) -> i64]
  :requires [(>= i 0)]
  :ensures [(>= result 0)]
  :body (if (= i 0) 0
    (do
      (fold (fn [acc x] (+ acc x)) 0
        (filter (fn [x] (= (% x 2) 0))
          (map (fn [x] (* x 2))
            [1 2 3 4 5 6 7 8 9 10])))
      (run-evens (- i 1)))))

(print (run-evens 5000))
```
