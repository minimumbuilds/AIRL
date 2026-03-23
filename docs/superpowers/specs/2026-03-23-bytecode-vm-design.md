# Register-Based Bytecode VM Design

**Date:** 2026-03-23
**Status:** Draft
**Scope:** Flat bytecode instruction set, register-based VM, IRNode-to-bytecode compiler — targeting 3-5x speedup over the current IR VM

## Overview

A register-based bytecode VM that executes flat instruction arrays instead of tree-walking IR variant nodes. The bytecode compiler takes existing `IRNode` trees and flattens them to compact instructions with indexed register operands. This eliminates the three main bottlenecks of the current IR VM: HashMap-based variable lookup, per-call frame allocation, and pointer-chasing through `Box<IRNode>` trees.

## Goals

1. **3-5x speedup over IR VM** — fib(30) from ~6s to ~1-2s in release mode
2. **Third execution mode** — `--bytecode` flag alongside existing `--compiled` (IR VM) and default (interpreted)
3. **Compile from IRNode** — reuse the existing IR compilation pipeline, add bytecode as a lowering pass
4. **No new external dependencies** — pure Rust, consistent with project philosophy
5. **Full documentation** — all new types, opcodes, compilation strategy, and VM semantics documented

## Non-Goals

- Replacing the IR VM (it stays as `--compiled` for correctness reference)
- Replacing the interpreter (it stays as default with full contract checking)
- Garbage collection (values are owned/cloned, same as IR VM)
- JIT compilation to native code (future Step 3)
- Self-hosted bytecode compiler in AIRL (future work)

## Architecture

### Pipeline Position

```
Source → Lex → Parse → [Type Check] → IR Compile → Bytecode Compile → Bytecode VM
                                         ↑              ↑                  ↑
                                      existing        new (Rust)        new (Rust)
                                    (pipeline.rs)  (bytecode_compiler)  (bytecode_vm)
```

The bytecode compiler takes `IRNode` trees as input. This means both the Rust-side IR compiler (`pipeline.rs`) and the self-hosted AIRL compiler (`compiler.airl`) feed into it. The existing pipeline stays intact — bytecode is an additional lowering pass.

### Why compile from IRNode, not AST?

The IR compiler already strips contracts, source spans, type annotations, and resolves `defn` to `IRFunc`. The bytecode compiler only handles ~15 node types instead of 30+ AST variants. Both the Rust-side and AIRL-side compilers produce `IRNode`, so bytecode benefits both paths automatically.

## Instruction Format

Each instruction is a fixed-size struct:

```rust
#[derive(Debug, Clone, Copy)]
struct Instruction {
    op: Op,       // opcode (u8-sized enum)
    dst: u16,     // destination register
    a: u16,       // operand A (register index or constant index)
    b: u16,       // operand B (register index, constant index, or offset)
}
```

Fixed size ensures cache-friendly sequential access. The VM's inner loop indexes into a `Vec<Instruction>` without pointer indirection.

## Instruction Set

### Literals and Moves

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `LoadConst` | `dst, const_idx, _` | `registers[dst] = constants[const_idx]` |
| `LoadNil` | `dst, _, _` | `registers[dst] = Nil` |
| `LoadTrue` | `dst, _, _` | `registers[dst] = Bool(true)` |
| `LoadFalse` | `dst, _, _` | `registers[dst] = Bool(false)` |
| `Move` | `dst, src, _` | `registers[dst] = registers[src].clone()` |

### Arithmetic

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `Add` | `dst, a, b` | `registers[dst] = registers[a] + registers[b]` |
| `Sub` | `dst, a, b` | `registers[dst] = registers[a] - registers[b]` |
| `Mul` | `dst, a, b` | `registers[dst] = registers[a] * registers[b]` |
| `Div` | `dst, a, b` | `registers[dst] = registers[a] / registers[b]` |
| `Mod` | `dst, a, b` | `registers[dst] = registers[a] % registers[b]` |
| `Neg` | `dst, a, _` | `registers[dst] = -registers[a]` |

### Comparison

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `Eq` | `dst, a, b` | `registers[dst] = Bool(registers[a] == registers[b])` |
| `Ne` | `dst, a, b` | `registers[dst] = Bool(registers[a] != registers[b])` |
| `Lt` | `dst, a, b` | `registers[dst] = Bool(registers[a] < registers[b])` |
| `Le` | `dst, a, b` | `registers[dst] = Bool(registers[a] <= registers[b])` |
| `Gt` | `dst, a, b` | `registers[dst] = Bool(registers[a] > registers[b])` |
| `Ge` | `dst, a, b` | `registers[dst] = Bool(registers[a] >= registers[b])` |

### Logic

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `Not` | `dst, a, _` | `registers[dst] = Bool(!registers[a].as_bool())` |

### Control Flow

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `Jump` | `_, offset, _` | `ip += offset` (signed, relative) |
| `JumpIfFalse` | `_, a, offset` | `if !registers[a].as_bool() { ip += offset }` |
| `JumpIfTrue` | `_, a, offset` | `if registers[a].as_bool() { ip += offset }` |

Offsets are signed i16 encoded as u16. Forward jumps are positive, backward jumps for loops are negative.

### Function Calls

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `Call` | `dst, func_idx, argc` | Call function `func_idx`, args in registers `[dst+1..dst+1+argc]`, result in `dst` |
| `CallBuiltin` | `dst, name_idx, argc` | Call builtin by constant pool index, args in `[dst+1..dst+1+argc]` |
| `CallReg` | `dst, callee_reg, argc` | Call closure/funcref in register, args in `[dst+1..dst+1+argc]` |
| `TailCall` | `_, func_idx, argc` | Self-TCO: rebind arg registers, reset ip to 0 |
| `Return` | `_, src, _` | Return `registers[src]` to caller |

**Calling convention:** Before a `Call`, the compiler arranges arguments in consecutive registers starting at `dst+1`. The callee receives them as its first N registers. The result goes into `dst`.

### Data Construction

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `MakeList` | `dst, start, count` | `registers[dst] = List(registers[start..start+count])` |
| `MakeVariant` | `dst, tag_idx, a` | `registers[dst] = Variant(constants[tag_idx], registers[a])` |
| `MakeVariant0` | `dst, tag_idx, _` | `registers[dst] = Variant(constants[tag_idx], Nil)` — 0-arg variant |
| `MakeClosure` | `dst, func_idx, capture_start` | Create closure capturing registers `[capture_start..]` |

### Pattern Matching

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `MatchTag` | `dst, scrutinee, tag_idx` | If `registers[scrutinee]` is `Variant(tag, inner)` where `tag == constants[tag_idx]`, store `inner` in `dst` and set match flag; else clear flag |
| `JumpIfNoMatch` | `_, offset, _` | Jump if last `MatchTag` failed |
| `MatchWild` | `dst, scrutinee, _` | Always matches, binds `registers[scrutinee]` to `dst` |

Match compilation emits a sequence of `MatchTag`/`JumpIfNoMatch` pairs for each arm, with the body code between jumps.

### Try

| Opcode | Operands | Semantics |
|--------|----------|-----------|
| `TryUnwrap` | `dst, src, err_offset` | If `registers[src]` is `Ok(v)`, `dst = v`. If `Err(e)`, jump to error handler at `ip + err_offset` |

## Constant Pool

Each compiled function has a `Vec<Value>` constant pool. String literals, integer constants, float constants, function names, and variant tags are stored here. Instructions reference constants by index.

```rust
struct BytecodeFunc {
    name: String,
    arity: u16,                     // number of parameters
    register_count: u16,            // total registers needed (params + locals + temps)
    instructions: Vec<Instruction>,
    constants: Vec<Value>,          // per-function constant pool
}
```

## Register Allocation

Simple linear allocation — AIRL has no mutable variables, so register liveness is straightforward:

1. Function parameters occupy registers `r0..rN` (N = arity)
2. Each `let` binding gets the next available register
3. Temporary values (sub-expression results) get the next available register
4. When a scope ends (let body evaluated), temporaries are freed (watermark reset)
5. The compiler tracks the high-water mark as `register_count`

No graph coloring or interference analysis needed. The lack of mutation means no variable has conflicting live ranges.

### Example

```clojure
(defn example :sig [(x : i64) (y : i64) -> i64] ...
  :body (let (z : i64 (+ x y))
          (* z 2)))
```

Register assignment:
- `r0` = x (parameter)
- `r1` = y (parameter)
- `r2` = z (let binding = x + y)
- `r3` = temporary (constant 2)
- `r0` = result (reuse, since x is dead after z is computed)

```
  ADD         r2, r0, r1      ; z = x + y
  LOAD_CONST  r3, 0           ; r3 = 2
  MUL         r0, r2, r3      ; result = z * 2
  RETURN      r0
```

`register_count = 4`

## Bytecode VM

### Structure

```rust
struct BytecodeVm {
    functions: HashMap<String, BytecodeFunc>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
    match_flag: bool,               // set by MatchTag
}

struct CallFrame {
    registers: Vec<Value>,          // pre-allocated to register_count
    func_name: String,              // for TCO detection
    ip: usize,                      // instruction pointer
    return_reg: u16,                // caller's destination register
}
```

### Execution Loop

```rust
fn run(&mut self) -> Result<Value, RuntimeError> {
    loop {
        let frame = self.call_stack.last_mut().unwrap();
        let func = &self.functions[&frame.func_name];
        let instr = func.instructions[frame.ip];
        frame.ip += 1;

        match instr.op {
            Op::LoadConst => {
                frame.registers[instr.dst as usize] = func.constants[instr.a as usize].clone();
            }
            Op::Add => {
                let a = &frame.registers[instr.a as usize];
                let b = &frame.registers[instr.b as usize];
                frame.registers[instr.dst as usize] = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                    // ... other numeric types
                };
            }
            Op::JumpIfFalse => {
                if !frame.registers[instr.a as usize].as_bool() {
                    frame.ip = (frame.ip as i32 + instr.b as i16 as i32) as usize;
                }
            }
            Op::Call => {
                // Push new frame, transfer args, continue
            }
            Op::TailCall => {
                // Rebind arg registers in current frame, reset ip
            }
            Op::Return => {
                let result = frame.registers[instr.a as usize].clone();
                self.call_stack.pop();
                if self.call_stack.is_empty() {
                    return Ok(result);
                }
                let caller = self.call_stack.last_mut().unwrap();
                caller.registers[/* return_reg */] = result;
            }
            // ... remaining opcodes
        }
    }
}
```

### Performance Characteristics vs IR VM

| IR VM bottleneck | Bytecode VM solution | Expected impact |
|-----------------|---------------------|-----------------|
| `HashMap<String, Value>` env lookup per variable | Indexed register: `registers[slot]` — O(1), no hashing | Major — variable access is the hottest path |
| `HashMap::insert` on every `env_bind` | `registers[slot] = value` — direct array write | Major — eliminates allocation per binding |
| `push_frame()` allocates new HashMap | `Vec<Value>` pre-sized to `register_count` | Moderate — one allocation per call instead of per scope |
| `Box<IRNode>` pointer chasing through tree | Flat `Vec<Instruction>` — sequential cache lines | Major — L1 cache stays hot |
| `match node { IRNode::... }` — 15+ variants with data | `match instr.op` — small fieldless enum | Moderate — better branch prediction |
| `Value::clone()` on every variable read | Values stay in registers; clone only on `Move`/`Call` | Moderate — fewer allocations |

## File Structure

### New Files

| File | Responsibility | Est. Lines |
|------|---------------|-----------|
| `crates/airl-runtime/src/bytecode.rs` | `Op` enum, `Instruction`, `BytecodeFunc` types, constant pool | ~100 |
| `crates/airl-runtime/src/bytecode_compiler.rs` | `BytecodeCompiler`: IRNode → bytecode, register allocation | ~400 |
| `crates/airl-runtime/src/bytecode_vm.rs` | `BytecodeVm`: execution loop, call frames, pattern matching | ~500 |

### Modified Files

| File | Change |
|------|--------|
| `crates/airl-runtime/src/lib.rs` | Add `pub mod bytecode; pub mod bytecode_compiler; pub mod bytecode_vm;` |
| `crates/airl-driver/src/pipeline.rs` | Add `run_source_bytecode()`, `run_file_bytecode()`, `compile_to_bytecode()` |
| `crates/airl-driver/src/main.rs` | Add `--bytecode` flag to `cmd_run` |
| `CLAUDE.md` | Document bytecode VM, add to completed tasks |
| `README.md` | Update architecture diagram and CLI section |

## Compilation Examples

### If expression

```clojure
(if (< x 10) (+ x 1) (* x 2))
```

```
  LT          r2, r0, r1       ; r2 = x < 10  (r1 has const 10)
  JUMP_IF_FALSE _, r2, +3       ; if false, jump to else
  ; then branch
  ADD         r3, r0, r4       ; r3 = x + 1  (r4 has const 1)
  MOVE        r_result, r3
  JUMP        _, +2, _         ; jump to end
  ; else branch
  MUL         r3, r0, r5       ; r3 = x * 2  (r5 has const 2)
  MOVE        r_result, r3
  ; end
```

### Match expression

```clojure
(match val (Ok v) (+ v 1) (Err e) e _ 0)
```

```
  ; arm 1: (Ok v)
  MATCH_TAG    r2, r1, "Ok"     ; extract inner to r2 if Ok
  JUMP_IF_NO_MATCH _, +3        ; skip to arm 2
  ADD          r0, r2, r3       ; result = v + 1
  JUMP         _, +5, _         ; jump to end
  ; arm 2: (Err e)
  MATCH_TAG    r2, r1, "Err"
  JUMP_IF_NO_MATCH _, +2
  MOVE         r0, r2           ; result = e
  JUMP         _, +1, _
  ; arm 3: wildcard
  LOAD_CONST   r0, idx_0        ; result = 0
  ; end
```

### Tail-recursive function

```clojure
(defn count-down :sig [(n : i64) -> i64] ...
  :body (if (= n 0) 0 (count-down (- n 1))))
```

```
  LOAD_CONST   r1, idx_0        ; r1 = 0
  EQ           r2, r0, r1       ; r2 = (n == 0)
  JUMP_IF_FALSE _, r2, +2       ; if false, skip to else
  LOAD_CONST   r0, idx_0        ; return 0
  RETURN       _, r0, _
  ; else: tail call
  SUB          r0, r0, r3       ; r0 = n - 1 (r3 has const 1)
  TAIL_CALL    _, func_self, 1  ; rebind r0, jump to ip 0
```

The `TAIL_CALL` instruction resets `ip` to 0 without pushing a new frame, achieving O(1) stack space for self-recursive tail calls.

## Testing Strategy

### Unit tests (in each new file)

- `bytecode.rs`: instruction construction, constant pool indexing
- `bytecode_compiler.rs`: compile simple expressions, verify instruction output
- `bytecode_vm.rs`: execute compiled bytecode for literals, arithmetic, if, let, do, functions, recursion, match, lambda, TCO (mirror IR VM tests)

### Equivalence testing

Run the existing 32-program equivalence test suite through `--bytecode` and verify identical output to `--compiled` and interpreted modes. This can be done by extending `bootstrap/equivalence_test.airl` or with a shell script that runs all three modes and diffs output.

### Benchmark

Re-run the three stress tests (fib30, fact12x10K, sum-evens5K) with all three modes and Python. Save results to `benchmarks/results/perf_bytecode.md`.

### Success criteria

- fib(30) under 2s in release mode (currently 6s with IR VM)
- All 32 equivalence tests pass
- All 485 workspace tests still pass
- TCO test (100K recursions) completes in constant stack space
