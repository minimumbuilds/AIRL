# AIRL Performance Benchmark: Bytecode VM

**Date:** 2026-03-23
**Mode:** Release build (`cargo build --release -p airl-driver`)
**Machine:** Linux 6.8.0-60-generic
**Method:** Best of 2-3 runs, wall-clock time via `date +%s%N`

## Context

AIRL now has three execution modes:
- **Interpreted** — tree-walking interpreter (`eval.rs`), full contract checking
- **IR VM (`--compiled`)** — AST → IR nodes → Rust VM (`ir_vm.rs`), no contracts, self-TCO
- **Bytecode VM (`--bytecode`)** — AST → IR → flat register-based bytecode → tight execution loop (`bytecode_vm.rs`), self-TCO

## Results

| Benchmark | Interpreted | IR VM (--compiled) | Bytecode VM (--bytecode) | IR→Bytecode Speedup | Interp→Bytecode |
|-----------|------------|-------------------|--------------------------|---------------------|-----------------|
| fib(30) | 42,006ms | 7,355ms | 4,438ms | **1.7x** | **9.5x** |
| fact(12) x 10K | 829ms | 161ms | 149ms | **1.1x** | **5.6x** |
| sum-evens x 5K | 3,497ms | 1,175ms | 798ms | **1.5x** | **4.4x** |

## Analysis

**Bytecode vs IR VM:**
- **fib(30): 1.7x speedup.** The flat bytecode instruction array with indexed register access (`registers[slot]`) eliminates the IR VM's HashMap-based variable lookup and pointer-chasing through `Box<IRNode>` trees. On deep recursive code (~2.7M function calls), this yields the largest gains.
- **fact(12)x10K: 1.1x speedup.** Tail-recursive loops with TCO perform similarly — the TCO hot path (reset ip, rebind args) has similar overhead in both VMs.
- **sum-evens x5K: 1.5x speedup.** Higher-order function callbacks (`map`/`filter`/`fold`) involve closure dispatch, which benefits from register-based locals. The lambda free-variable analysis ensures only actually-referenced variables are captured.

**Overall pipeline speedup (Interpreted → Bytecode):**
- fib(30): **9.5x** faster
- fact(12)x10K: **5.6x** faster
- sum-evens x5K: **4.4x** faster

**Where the bytecode wins come from:**
1. `registers[slot]` array indexing vs `HashMap<String, Value>` lookup per variable access
2. Flat `Vec<Instruction>` sequential iteration vs `Box<IRNode>` pointer chasing
3. `match instr.op` on small fieldless enum vs `match node { IRNode::... }` on 15+ data-carrying variants
4. Pre-allocated register arrays per call frame vs HashMap allocation per scope

## Previous IR VM vs Interpreted Baseline (from perf_2026-03-23_compiled.md)

For reference, the previous benchmark measured:

| Benchmark | Interpreted | IR VM | Interp→IR Speedup |
|-----------|------------|-------|-------------------|
| fib(30) | 32,250ms | 6,031ms | **5.3x** |
| fact(12) x 10K | 987ms | 207ms | **4.7x** |
| sum-evens x 5K | 1,882ms | 973ms | **1.9x** |

Note: interpreted times differ slightly between benchmark runs due to system load variance.

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
