# AOT Reference Counting: Move Semantics

**Date:** 2026-04-06  
**Status:** Proposed  
**Scope:** `crates/airl-runtime/src/bytecode_aot.rs`, `bootstrap/bc_compiler.airl`

---

## Problem

AOT-compiled AIRL programs leak every heap-allocated value. Every `Map`, `List`, `String`, and other `RtValue` is allocated with `rc=1` and that count never reaches zero during execution. Memory grows monotonically until OOM.

**Root cause:** Two missing pieces:

1. `Op::Move` in the AOT backend copies a pointer without retaining or zeroing — ownership semantics are undefined.
2. `Op::Release` was added to the bytecode VM but left as a no-op in the AOT backend, because without consistent ownership semantics, emitting Release would cause double-frees.

The interpreter VM path (used during `g3` compilation of AIRL source) got the Release fix in commit `e47fa5f`. The AOT path (used for all compiled native binaries) did not.

---

## Invariant

> **Every Cranelift variable holding a heap-allocated value has exactly one refcount reference attributed to it.**

Concretely:
- A variable is either **live** (holds a pointer with one attributed rc reference) or **dead** (holds null/0, no attributed reference).
- Overwriting a live variable **must** first release the old value.
- Transferring a value to another variable **moves** it: the source becomes dead (zeroed), the destination becomes live.
- Releasing a dead variable is a no-op (null-safe via `airl_value_release`).

This is **move semantics** — the same model as Rust's `Box<T>` and AIRL's own ownership system.

---

## Ownership Flow Per Opcode

### Variable initialization

All Cranelift variables must be initialized to `0` (null) at function entry before any use. This ensures Release on an unwritten variable is a no-op and prevents Cranelift use-before-def errors.

```
for each var in vars:
    def_var(var, iconst(I64, 0))
```

### `Op::Move dst src`

Ownership transfers from `src` to `dst`. After the move, `src` is dead.

```
old_dst = use_var(vars[dst])
airl_value_release(old_dst)          // release old owner of dst
val = use_var(vars[src])
def_var(vars[dst], val)              // dst takes ownership
def_var(vars[src], iconst(I64, 0))  // src is now dead
```

**Why release old_dst?** Registers are reused. Before a Move overwrites `dst`, whatever was there (from a previous expression in the same function) must be released.

### `Op::Release reg`

Explicitly releases a value and marks the register dead.

```
val = use_var(vars[reg])
airl_value_release(val)
def_var(vars[reg], iconst(I64, 0))
```

This is already implemented as a no-op. After this spec is implemented, it becomes active.

### `Op::Call dst func argc`

The argument slots `dst+1..=dst+argc` were populated by preceding `Op::Move` instructions, which zeroed their sources. Ownership now resides in the arg slots.

The callee receives the arg pointers. Within the callee (also AOT-compiled), args are parameters — they arrive as live variables with rc=1. The callee's own `Op::Release` opcodes will release them when they go out of scope.

After the call returns:
- `vars[dst+1..=dst+argc]` are **dead** (zeroed by the callee's release of its params? No — see below).
- `vars[dst]` holds the return value (rc=1, live).

**Problem:** The callee's parameters are separate from the caller's arg slots. The callee releases its own parameter registers; the caller's arg slot variables still hold the original (now-freed!) pointers.

**Solution:** The caller must zero its arg slots after the call returns. This is done in `bc_compiler.airl` by emitting `Op::Release` for `dst+1..=dst+argc` immediately after `Op::Call`. Since the callee owns the values and will release them, the caller's Release must be on zeroed slots — which means the caller must NOT retain the args (they were moved, not borrowed).

Wait — there's a race: the callee releases its params when? When the callee's Release opcodes fire (emitted by `cs-free-reg-to` in bc_compiler). But the callee's parameter registers are separate from the caller's arg slot variables. The callee's release of its `reg_1` does NOT zero the caller's `vars[dst+1]`.

**Revised model:** The function call boundary is a **borrow**:

- Caller retains each arg before the call (rc goes from 1 to 2).
- Callee receives args with rc=2. Callee's Release opcodes decrement to rc=1.
- After the call, caller Releases its arg slots (rc goes from 1 to 0, freed).

This is the **retain-before-call** model:

```
// Emit before Call:
for slot in dst+1..=dst+argc:
    airl_value_retain(vars[slot])

// Emit after Call (via bc_compiler Release opcodes):
for slot in dst+1..=dst+argc:
    airl_value_release(vars[slot])
    vars[slot] = 0
```

This requires **retain-on-call-args** in the AOT backend, plus **Release-after-call** in the bc_compiler.

### `Op::Return src`

```
val = use_var(vars[src])
return val
```

The return value is moved out to the caller. All other live variables should have been released by prior `Op::Release` opcodes (emitted by `cs-free-reg-to` as the let scopes unwind). If any live variable reaches Return without being released, it leaks.

**Note:** The bc_compiler must ensure that the return register (`src`, typically reg 0) is NOT released by `cs-free-reg-to` before Return. The calling convention: reg 0 is the return value, its ownership is transferred to the caller.

---

## Changes Required

### 1. `bytecode_aot.rs` — Variable Initialization

In `compile_func` (boxed path), after creating the Cranelift variables, initialize all to `0`:

```rust
// After: let vars: Vec<Variable> = (0..reg_count).map(...).collect();
let zero = builder.ins().iconst(types::I64, 0);
for &var in &vars {
    builder.declare_var(var, types::I64);
    builder.def_var(var, zero);
}
```

### 2. `bytecode_aot.rs` — Op::Move

Replace current no-retain Move with release-old + copy + zero-src:

```rust
Op::Move => {
    let dst = instr.dst as usize;
    let src = instr.a as usize;
    type_hints[dst] = type_hints[src];

    // Release old owner of dst
    let old_dst = builder.use_var(vars[dst]);
    let release_ref = self.module.declare_func_in_func(self.rt.value_release, builder.func);
    builder.ins().call(release_ref, &[old_dst]);

    // Move: transfer ownership from src to dst
    let v = builder.use_var(vars[src]);
    builder.def_var(vars[dst], v);

    // Zero src — it no longer owns the value
    let zero = builder.ins().iconst(types::I64, 0);
    builder.def_var(vars[src], zero);

    last_was_terminator = false;
}
```

### 3. `bytecode_aot.rs` — Op::Call (retain args before call)

Before emitting the native call, retain each arg slot:

```rust
// Before the call instruction:
let retain_ref = self.module.declare_func_in_func(self.rt.value_retain, builder.func);
for i in 1..=argc {
    let arg_val = builder.use_var(vars[dst + i]);
    builder.ins().call(retain_ref, &[arg_val]);
}
// ... existing call emission ...
```

### 4. `bytecode_aot.rs` — Op::Release (activate)

Replace the current no-op with the actual implementation:

```rust
Op::Release => {
    let reg_idx = instr.a as usize;
    if reg_idx < vars.len() {
        let val = builder.use_var(vars[reg_idx]);
        let release_ref = self.module.declare_func_in_func(self.rt.value_release, builder.func);
        builder.ins().call(release_ref, &[val]);
        let zero = builder.ins().iconst(types::I64, 0);
        builder.def_var(vars[reg_idx], zero);
    }
}
```

### 5. `bc_compiler.airl` — Release call slots after Call

In `bc-compile-call-named`, after emitting `op-call`, emit Release for each arg slot before `cs-free-reg-to`:

```airl
(defn bc-release-call-slots
  :sig [(st : _) (dst : Int) (argc : Int) -> _]
  :intent "Emit Release for call arg slots dst+1..dst+argc after a Call"
  :requires [(valid st) (>= argc 0)]
  :ensures [(valid result)]
  :body
    (if (<= argc 0) st
      (let (slot : Int (+ dst argc))
        (let (st2 : _ (cs-emit st (op-release) 0 slot 0))
          (bc-release-call-slots st2 dst (- argc 1))))))
```

Call this after `op-call` is emitted:
```airl
(let (st5 : _ (cs-emit st4a (op-call) dst name-idx argc))
  (let (st5b : _ (bc-release-call-slots st5 dst argc))
    (let (free-to : Int (if (> save (+ dst 1)) save (+ dst 1)))
      (Ok (cs-free-reg-to st5b free-to)))))
```

Same treatment for `op-call-reg` in `bc-compile-call-local` and `bc-compile-call-reg`.

### 6. `bc_compiler.airl` — Do not Release reg 0 before Return

`cs-free-reg-to` currently releases all registers from `r` to `next-reg`. In a function whose body result is in reg 0, the pattern before Return is:
- Body expr compiled to reg 0
- `cs-free-reg-to st 1` (releases regs 1..next-reg, NOT reg 0)
- `Return reg 0`

Verify that all call sites of `cs-free-reg-to` before Return always pass `r >= 1`, not `r = 0`. If any pass 0, add a guard:

```airl
(defn cs-free-reg-to
  :sig [(st : _) (r : Int) -> _]
  :body
    (let (current : Int (cs-next-reg st))
      (let (safe-r : Int (max r 1))  ;; never release reg 0 (return register)
        (let (st2 : _ (bc-emit-releases-loop st current safe-r))
          (map-set st2 "next-reg" safe-r)))))
```

---

## TailCall (Follow-on)

`Op::TailCall` in the AOT backend currently jumps to `loop_block` — the start of the function. For this to be memory-safe under move semantics:

1. The new args must be moved into the parameter registers (already done by preceding `Op::Move` instructions via `bc-move-to-slots`).
2. The old parameter registers (from the previous iteration) are overwritten by the Move instructions, which now release their old values (per spec item 2 above).
3. All other live temps are released by `cs-free-reg-to` before the TailCall.

TailCall is NOT required for memory safety (Release handles heap; stack depth grows but is bounded by modern stack limits). It is required for infinite-loop programs. Treat as follow-on once RC correctness is verified.

---

## Testing

### Unit tests
- Add a test to `bytecode_aot.rs` tests section: compile a function that allocates and releases 10,000 maps in a loop (via recursion with TailCall), check RSS stays flat.
- Existing 75 AOT tests must all pass — regressions indicate double-free or use-after-free.

### Canopy interactive demo
```bash
cd /home/jbarnes/repos/canopy
bash run-interactive.sh
# Press 500+ keys — no OOM, no slowdown
```

### Valgrind / AddressSanitizer
```bash
cd /home/jbarnes/repos/AIRL
RUSTFLAGS="-Z sanitizer=address" cargo build --release --features aot --target x86_64-unknown-linux-gnu
./g3 -- examples/01-hello-world/hello_world.airl -o /tmp/hw && /tmp/hw
```

---

## Risk

**Double-free:** If retain-before-call is missing but Release-after-call is present, args freed once by callee and once by caller = double-free = crash/heap corruption. The retain-before-call (spec item 3) and Release-after-call (spec item 5) must be implemented atomically.

**Uninitialized variables:** If variable init (spec item 1) is skipped, Release on unwritten vars reads garbage pointers. Must be done first.

**Unboxed path:** `compile_func_unboxed` in `bytecode_aot.rs` is a separate codegen path for integer-only functions. It does not use `RtValue` heap pointers; it can remain unchanged. Verify that unboxed functions never receive or return heap values before skipping this path.

---

## Implementation Order

1. Variable initialization (spec item 1) — prerequisite for all others
2. `Op::Release` activation (spec item 4)
3. `Op::Move` release-old + zero-src (spec item 2)
4. Retain-before-call in AOT (spec item 3)
5. Release-call-slots in bc_compiler (spec item 5)
6. Guard reg 0 in `cs-free-reg-to` (spec item 6)
7. Run full test suite; fix any double-free / use-after-free
8. TailCall memory safety (follow-on)
