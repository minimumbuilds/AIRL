# AIRL Performance Benchmark: Interpreted vs Compiled vs Python

**Date:** 2026-03-23
**Mode:** Release build (`cargo build --release -p airl-driver`)
**Machine:** Linux 6.8.0-60-generic
**Method:** Best of 2-3 runs, wall-clock time via `date +%s%N`

## Context

AIRL now has two execution modes:
- **Interpreted** — tree-walking interpreter (`eval.rs`), full contract checking
- **Compiled** — AST → IR nodes → Rust VM (`ir_vm.rs`), no contracts, self-TCO

This benchmark compares both against Python 3 on equivalent programs.

## Results: Original 5 Benchmark Tasks (Trivial Computation)

These programs from the original AIRL vs Python benchmark do minimal computation. Timings are dominated by process startup (~40ms Python, ~60-80ms AIRL).

| Task | Python | AIRL Interpreted | AIRL Compiled |
|------|--------|-----------------|---------------|
| safe-divide | 39ms | 83ms | 63ms |
| fibonacci(10) | 39ms | 79ms | 62ms |
| list-processing | 97ms | 72ms | 62ms |
| input-validation | 39ms | 72ms | 65ms |
| string-tokenizer | 39ms | 71ms | 65ms |

**Conclusion:** No meaningful performance difference — startup overhead dominates.

## Results: Compute-Heavy Stress Tests

Programs designed to stress the execution engine with enough work to measure.

| Benchmark | Description | Python | AIRL Interpreted | AIRL Compiled | Interp→Compiled |
|-----------|-------------|--------|-----------------|---------------|-----------------|
| fib(30) | Exponential recursion (~2.7M calls) | 291ms | 32,250ms | 6,031ms | **5.3x** |
| fact(12) x 10K | Tail-recursive loop, 10K iterations | 47ms | 987ms | 207ms | **4.7x** |
| sum-evens x 5K | map/filter/fold on 10-element list, 5K iterations | 43ms | 1,882ms | 973ms | **1.9x** |

### Analysis

**Compiled vs Interpreted:** 2-5x speedup. Biggest gains on recursive code (fib: 5.3x) where the IR VM's simpler dispatch loop eliminates AST metadata overhead (spans, types, contracts). Smaller gains on list operations (1.9x) because stdlib functions (`map`, `filter`, `fold`) are themselves recursive AIRL — the VM overhead is a smaller fraction.

**AIRL vs Python:** Python remains 20-100x faster. CPython's bytecode VM is a mature register-based C loop with decades of optimization. AIRL's IR VM is a first-generation Rust tree-walker on variant nodes. The gap can be narrowed via: flat bytecode encoding → register-based VM → Cranelift native codegen.

### Benchmark Programs

#### fib(30)
```clojure
;; AIRL
(defn fibonacci
  :sig [(n : i64) -> i64]
  :requires [(>= n 0)]
  :ensures [(>= result 0)]
  :body (if (<= n 1) n (+ (fibonacci (- n 1)) (fibonacci (- n 2)))))
(print (fibonacci 30))
```
```python
# Python
def fibonacci(n):
    assert n >= 0
    if n <= 1: return n
    return fibonacci(n - 1) + fibonacci(n - 2)
print(fibonacci(30))
```

#### fact(12) x 10K
```clojure
;; AIRL
(defn fact-iter
  :sig [(n : i64) (acc : i64) -> i64]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (if (<= n 1) acc (fact-iter (- n 1) (* acc n))))
(defn run-fact
  :sig [(i : i64) (limit : i64) (acc : i64) -> i64]
  :requires [(valid i)]
  :ensures [(valid result)]
  :body (if (>= i limit) acc (run-fact (+ i 1) limit (+ acc (fact-iter 12 1)))))
(print (run-fact 0 10000 0))
```
```python
# Python
def fact_iter(n, acc):
    while n > 1:
        acc *= n
        n -= 1
    return acc
acc = 0
for i in range(10000):
    acc += fact_iter(12, 1)
print(acc)
```

#### sum-evens x 5K
```clojure
;; AIRL
(defn sum-evens
  :sig [(xs : List) -> i64]
  :requires [(valid xs)]
  :ensures [(valid result)]
  :body (fold (fn [acc x] (+ acc x)) 0 (filter (fn [x] (= (% x 2) 0)) xs)))
(defn run-list
  :sig [(i : i64) (limit : i64) (acc : i64) -> i64]
  :requires [(valid i)]
  :ensures [(valid result)]
  :body (if (>= i limit) acc (run-list (+ i 1) limit (+ acc (sum-evens [1 2 3 4 5 6 7 8 9 10])))))
(print (run-list 0 5000 0))
```
```python
# Python
def sum_evens(xs):
    return sum(x for x in xs if x % 2 == 0)
acc = 0
for i in range(5000):
    acc += sum_evens([1,2,3,4,5,6,7,8,9,10])
print(acc)
```
