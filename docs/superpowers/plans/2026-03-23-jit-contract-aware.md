# JIT Contract-Aware Compilation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Cranelift JIT compile functions that have contract assertions, emitting native conditional branches for the happy path and calling a runtime helper on contract violation.

**Architecture:** Contract assertion opcodes (`AssertRequires`, `AssertEnsures`, `AssertInvariant`) are currently in the JIT's disqualification list. Since every AIRL function has mandatory contracts, this means the JIT compiles nothing. The fix: remove these opcodes from the disqualification list and emit Cranelift IR that checks the boolean register and, on failure, calls an extern C function `airl_contract_fail` that constructs the error and aborts. The happy path is one native compare-and-branch — essentially free.

**Tech Stack:** Cranelift IR, Rust `extern "C"` functions, existing bytecode_jit.rs infrastructure

---

## Current State

The bytecode compiler emits this for a function with `:requires [(> x 0)]`:

```
LoadConst  r2, const[0]    ;; load 0
Gt         r3, r0, r2      ;; r3 = x > 0
AssertRequires fn_name_idx, r3, clause_src_idx  ;; check r3, error if false
... body ...
AssertEnsures  fn_name_idx, r4, clause_src_idx  ;; check ensures
Return     _, r_result, _
```

The JIT sees `AssertRequires` → bails. We want it to emit:

```nasm
;; Native x86-64 (via Cranelift)
cmp   r3, 0
jne   .continue          ;; happy path: 1 branch (predicted taken)
mov   rdi, fn_name_ptr   ;; sad path: call runtime helper
mov   rsi, clause_ptr
mov   rdx, 0             ;; kind = Requires
call  airl_contract_fail ;; never returns (calls longjmp or sets error flag)
.continue:
... body ...
```

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/bytecode_jit.rs` | Modify | Remove contract opcodes from disqualification list, add IR emission for assertions |
| `crates/airl-runtime/src/bytecode_vm.rs` | Modify | Add `airl_contract_fail` extern C function, add error propagation from JIT |
| `crates/airl-runtime/src/bytecode.rs` | No change | Opcode definitions stay the same |
| `crates/airl-runtime/src/bytecode_compiler.rs` | No change | Contract compilation stays the same |

---

### Task 1: Add the runtime helper function

The JIT needs a C-ABI function it can call when a contract fails. This function receives raw data (pointers/integers) and sets an error flag that the VM checks after JIT execution returns.

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit.rs` (add static error cell and extern C helper)

- [ ] **Step 1: Write the failing test**

Add a test in `bytecode_jit.rs` that compiles a function WITH contract assertions and expects it to be eligible:

```rust
#[test]
fn test_contract_function_eligible() {
    use crate::bytecode::*;
    use crate::value::Value;
    // A function: AssertRequires(check r0 > 0), then return r0
    let func = BytecodeFunc {
        name: "positive".into(),
        arity: 1,
        register_count: 4,
        capture_count: 0,
        instructions: vec![
            // r1 = 0
            Instruction::new(Op::LoadConst, 1, 0, 0),
            // r2 = r0 > r1
            Instruction::new(Op::Gt, 2, 0, 1),
            // AssertRequires: fn_name=const[1], bool=r2, clause=const[2]
            Instruction::new(Op::AssertRequires, 1, 2, 2),
            // return r0
            Instruction::new(Op::Return, 0, 0, 0),
        ],
        constants: vec![
            Value::Int(0),                          // const[0]: the zero literal
            Value::Str("positive".into()),          // const[1]: function name
            Value::Str("(> x 0)".into()),           // const[2]: clause source
        ],
    };
    let mut all = std::collections::HashMap::new();
    all.insert("positive".into(), func.clone());
    assert!(BytecodeJit::is_eligible(&func, &all, &std::collections::HashMap::new(), &std::collections::HashSet::new()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-runtime test_contract_function_eligible`
Expected: FAIL — is_eligible returns false because AssertRequires is disqualified

- [ ] **Step 3: Add the contract fail helper and error cell**

At the top of `bytecode_jit.rs`, add a thread-local error cell and an extern C helper:

```rust
use std::cell::RefCell;

/// Thread-local error cell for JIT contract violations.
/// The JIT calls airl_jit_contract_fail which stores the error here.
/// The VM checks this after JIT execution returns.
thread_local! {
    static JIT_CONTRACT_ERROR: RefCell<Option<(u8, u64, u64)>> = RefCell::new(None);
    // (kind: 0=Requires 1=Ensures 2=Invariant, fn_name_ptr, clause_ptr)
}

/// C-ABI function called by JIT-compiled code when a contract assertion fails.
/// Stores error info in thread-local cell. The caller (VM) checks after return.
/// We use a sentinel return value (u64::MAX) that the JIT propagates as the return.
#[no_mangle]
pub extern "C" fn airl_jit_contract_fail(kind: u64, fn_name_ptr: u64, clause_ptr: u64) -> u64 {
    JIT_CONTRACT_ERROR.with(|cell| {
        *cell.borrow_mut() = Some((kind as u8, fn_name_ptr, clause_ptr));
    });
    u64::MAX // sentinel return value
}

/// Check if a JIT contract error occurred and extract it.
pub fn take_jit_contract_error() -> Option<(u8, String, String)> {
    JIT_CONTRACT_ERROR.with(|cell| {
        cell.borrow_mut().take().map(|(kind, fn_ptr, clause_ptr)| {
            // SAFETY: these pointers were created from &str references to BytecodeFunc.constants
            // which are alive for the duration of execution.
            let fn_name = unsafe {
                let (ptr, len) = (fn_ptr as *const u8, ((fn_ptr >> 48) & 0xFFFF) as usize);
                // We use a simpler approach: store the actual string index, not a pointer
                String::new() // placeholder — see Step 5 for the real implementation
            };
            (kind, fn_name, String::new())
        })
    })
}
```

Actually, passing string pointers through the JIT is fragile. A simpler approach: pass the constant indices (u16 values packed into u64) and have the VM look them up in the BytecodeFunc constants table after the JIT returns.

Revised helper:

```rust
/// C-ABI: called by JIT code on contract failure.
/// kind: 0=Requires, 1=Ensures, 2=Invariant
/// fn_name_idx: index into BytecodeFunc.constants for function name
/// clause_idx: index into BytecodeFunc.constants for clause source
#[no_mangle]
pub extern "C" fn airl_jit_contract_fail(kind: u64, fn_name_idx: u64, clause_idx: u64) -> u64 {
    JIT_CONTRACT_ERROR.with(|cell| {
        *cell.borrow_mut() = Some((kind as u8, fn_name_idx as u16, clause_idx as u16));
    });
    u64::MAX
}

pub fn take_jit_contract_error() -> Option<(u8, u16, u16)> {
    JIT_CONTRACT_ERROR.with(|cell| cell.borrow_mut().take())
}
```

- [ ] **Step 4: Remove contract opcodes from disqualification list**

In `is_eligible()`, remove `AssertRequires`, `AssertEnsures`, `AssertInvariant` from the disqualifying match arm:

```rust
// Before:
Op::MatchWild | Op::TryUnwrap | Op::CallBuiltin | Op::CallReg |
Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
    return false;
}

// After:
Op::MatchWild | Op::TryUnwrap | Op::CallBuiltin | Op::CallReg => {
    return false;
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p airl-runtime test_contract_function_eligible`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/bytecode_jit.rs
git commit -m "feat(jit): add contract fail helper and make contracted functions eligible"
```

---

### Task 2: Emit Cranelift IR for contract assertions

Teach `compile_func()` to handle `AssertRequires`, `AssertEnsures`, and `AssertInvariant` opcodes by emitting a conditional branch + call to the runtime helper.

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit.rs` (add opcode handling in compile_func)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_jit_contract_pass() {
    // positive(5) should return 5 (contract passes)
    let func = BytecodeFunc {
        name: "positive".into(),
        arity: 1,
        register_count: 4,
        capture_count: 0,
        instructions: vec![
            Instruction::new(Op::LoadConst, 1, 0, 0),   // r1 = 0
            Instruction::new(Op::Gt, 2, 0, 1),          // r2 = r0 > 0
            Instruction::new(Op::AssertRequires, 1, 2, 2), // assert r2
            Instruction::new(Op::Return, 0, 0, 0),      // return r0
        ],
        constants: vec![
            Value::Int(0),
            Value::Str("positive".into()),
            Value::Str("(> x 0)".into()),
        ],
    };
    let mut all = HashMap::new();
    all.insert("positive".into(), func.clone());
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all);
    let result = jit.try_call_native("positive", &[Value::Int(5)]);
    assert_eq!(result, Some(Value::Int(5)));
}

#[test]
fn test_jit_contract_fail() {
    // positive(-1) should trigger contract violation
    // (the JIT returns u64::MAX sentinel, VM converts to error)
    let func = BytecodeFunc { /* same as above */ };
    let mut all = HashMap::new();
    all.insert("positive".into(), func.clone());
    let mut jit = BytecodeJit::new().unwrap();
    jit.try_compile(&func, &all);
    // After JIT call with -1, check the error cell
    let result = jit.try_call_native("positive", &[Value::Int(-1)]);
    // Result will be the sentinel value — VM must check error cell
    let err = crate::bytecode_jit::take_jit_contract_error();
    assert!(err.is_some());
    let (kind, fn_idx, clause_idx) = err.unwrap();
    assert_eq!(kind, 0); // Requires
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p airl-runtime test_jit_contract`
Expected: Both FAIL — compile_func doesn't handle AssertRequires yet

- [ ] **Step 3: Declare the runtime helper in Cranelift**

In `compile_func()`, before the main instruction loop, declare `airl_jit_contract_fail` as an imported function:

```rust
// Declare contract fail helper
let mut contract_fail_sig = self.module.make_signature();
contract_fail_sig.params.push(AbiParam::new(types::I64)); // kind
contract_fail_sig.params.push(AbiParam::new(types::I64)); // fn_name_idx
contract_fail_sig.params.push(AbiParam::new(types::I64)); // clause_idx
contract_fail_sig.returns.push(AbiParam::new(types::I64)); // sentinel return

let contract_fail_id = self.module.declare_function(
    "airl_jit_contract_fail",
    cranelift_module::Linkage::Import,
    &contract_fail_sig,
)?;
```

- [ ] **Step 4: Emit IR for assertion opcodes**

In the main opcode match in `compile_func()`, add handling for the three assertion opcodes:

```rust
Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
    let fn_name_idx = instr.dst;    // constant index for function name
    let bool_reg = instr.a as usize;
    let clause_idx = instr.b;       // constant index for clause source

    let bool_val = builder.use_var(vars[bool_reg]);

    // Create two blocks: fail path and continue path
    let fail_block = builder.create_block();
    let cont_block = builder.create_block();

    // Branch: if bool_val != 0 (true) → continue, else → fail
    builder.ins().brif(bool_val, cont_block, &[], fail_block, &[]);

    // Fail block: call runtime helper
    builder.switch_to_block(fail_block);
    let kind_val = match instr.op {
        Op::AssertRequires => builder.ins().iconst(types::I64, 0),
        Op::AssertEnsures => builder.ins().iconst(types::I64, 1),
        _ => builder.ins().iconst(types::I64, 2), // Invariant
    };
    let fn_idx_val = builder.ins().iconst(types::I64, fn_name_idx as i64);
    let clause_val = builder.ins().iconst(types::I64, clause_idx as i64);
    let fail_ref = self.module.declare_func_in_func(contract_fail_id, builder.func);
    let call = builder.ins().call(fail_ref, &[kind_val, fn_idx_val, clause_val]);
    let sentinel = builder.inst_results(call)[0];
    builder.ins().return_(&[sentinel]);

    // Continue block: contract passed, proceed
    builder.switch_to_block(cont_block);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p airl-runtime test_jit_contract`
Expected: PASS — both the passing and failing contract cases work

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/bytecode_jit.rs
git commit -m "feat(jit): emit native contract assertion checks with runtime fail helper"
```

---

### Task 3: Wire up error propagation in the bytecode VM

When the JIT returns a sentinel value after calling `airl_jit_contract_fail`, the VM must check the thread-local error cell and convert it to a proper `RuntimeError::ContractViolation`.

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_vm.rs` (check error cell after JIT calls)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_jit_contract_violation_e2e() {
    // Full end-to-end: compile with contracts, run via JIT, expect contract error
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    let body = IRNode::Load("x".into());
    let requires = vec![(
        IRNode::Call(">".into(), vec![IRNode::Load("x".into()), IRNode::Int(0)]),
        "(> x 0)".to_string(),
    )];
    let ensures = vec![];
    let invariants = vec![];

    let mut compiler = BytecodeCompiler::new();
    let func = compiler.compile_function_with_contracts(
        "positive", &["x".into()], &body, &requires, &ensures, &invariants,
    );

    let mut vm = BytecodeVm::new_with_jit();
    vm.load_function(func);

    // Load a __main__ that calls positive(-1)
    let main_body = IRNode::Call("positive".into(), vec![IRNode::Int(-1)]);
    let main_func = compiler.compile_function("__main__", &[], &main_body);
    vm.load_function(main_func);
    vm.jit_compile_all();

    let result = vm.exec_main();
    assert!(result.is_err());
    match result.unwrap_err() {
        RuntimeError::ContractViolation(cv) => {
            assert_eq!(cv.contract_kind, airl_contracts::violation::ContractKind::Requires);
            assert!(cv.clause_source.contains("> x 0"));
        }
        other => panic!("expected ContractViolation, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-runtime test_jit_contract_violation_e2e --features jit`
Expected: FAIL — VM doesn't check error cell after JIT return

- [ ] **Step 3: Add error checking after JIT calls**

In `bytecode_vm.rs`, find where `try_call_native` is used in the `Op::Call` handler. After a successful JIT call, check the error cell:

```rust
// After JIT call returns a value:
if let Some(result) = jit_result {
    // Check if a contract violation was signaled
    if let Some((kind, fn_name_idx, clause_idx)) = crate::bytecode_jit::take_jit_contract_error() {
        let f = self.functions.get(&callee_name).unwrap();
        let fn_name = match &f.constants.get(fn_name_idx as usize) {
            Some(Value::Str(s)) => s.clone(),
            _ => callee_name.clone(),
        };
        let clause_source = match &f.constants.get(clause_idx as usize) {
            Some(Value::Str(s)) => s.clone(),
            _ => "?".into(),
        };
        let contract_kind = match kind {
            0 => airl_contracts::violation::ContractKind::Requires,
            1 => airl_contracts::violation::ContractKind::Ensures,
            _ => airl_contracts::violation::ContractKind::Invariant,
        };
        return Err(RuntimeError::ContractViolation(
            airl_contracts::violation::ContractViolation {
                function: fn_name,
                contract_kind,
                clause_source,
                bindings: vec![],
                evaluated: "false".into(),
                span: airl_syntax::Span::dummy(),
            }
        ));
    }
    // Normal happy path: store result in caller's frame
    ...
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p airl-runtime test_jit_contract_violation_e2e --features jit`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/bytecode_vm.rs
git commit -m "feat(jit): propagate contract violations from JIT to VM error handling"
```

---

### Task 4: End-to-end validation

Verify that the full pipeline (source → parse → bytecode with contracts → JIT → execute) works correctly for both passing and failing contracts, and measure the performance impact.

**Files:**
- No new files — integration testing via existing fixtures and benchmarks

- [ ] **Step 1: Run all fixture tests with JIT**

```bash
cargo test -p airl-driver --test fixtures --features jit
```

Expected: All 5 fixture test suites pass (valid, type_errors, contract_errors, linearity_errors, check_type_errors). Contract error fixtures must still produce ContractViolation errors.

- [ ] **Step 2: Run the 25-task benchmark with JIT**

```bash
# Build release
cargo build -p airl-driver --features jit --release

# Run benchmark (reuse the existing benchmark script pattern)
AIRL_BIN=target/release/airl-driver
for num in $(seq -w 1 25); do
    $AIRL_BIN run --jit benchmarks/output/airl/${num}.airl 2>/dev/null && echo "PASS $num" || echo "FAIL $num"
done
```

Expected: 24/25 pass (same as default — task 21 has a pre-existing recursive-let issue)

- [ ] **Step 3: Benchmark fib(30) — the critical test**

```bash
echo '(defn fib :sig [(n : i64) -> i64] :requires [(>= n 0)] :ensures [(>= result 0)] :body (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))) (print (fib 30))' > /tmp/fib30.airl

# Default (bytecode)
time target/release/airl-driver run /tmp/fib30.airl

# JIT
time target/release/airl-driver run --jit /tmp/fib30.airl
```

Expected: JIT should be dramatically faster than bytecode (target: <50ms, vs ~5800ms bytecode).
The contract check (>= n 0) is one branch per call. fib(30) makes ~2.7M calls, so ~2.7M extra branches — but these are perfectly predicted (always taken on the happy path) and cost essentially nothing.

- [ ] **Step 4: Cache the new artifact**

```bash
cp target/release/airl-driver benchmarks/artifacts/v0.2/airl-driver-release-jit-contracts
```

- [ ] **Step 5: Update performance results**

Update `benchmarks/results/perf_2026-03-23_v0.2.md` with the new JIT numbers.

- [ ] **Step 6: Commit**

```bash
git add benchmarks/
git commit -m "perf: JIT now compiles contracted functions — fib(30) benchmark restored"
```

---

## Important Notes for the Implementing Agent

1. **The `airl_jit_contract_fail` function must be `#[no_mangle] pub extern "C"`** — Cranelift resolves it by symbol name at JIT link time via `Module::declare_function` with `Linkage::Import`.

2. **The symbol must be registered in the JIT module's symbol table.** Check how existing external calls (like cross-function calls) are resolved. You may need to manually register the symbol pointer via `module.define_function_bytes` or the JIT module's symbol lookup mechanism. Look for how the existing `Call` opcode resolves function pointers — the pattern should be similar.

3. **Thread-local error cell is essential** because the JIT's calling convention returns a u64, not a Result. The error must be communicated out-of-band. The sentinel value (u64::MAX) tells the VM "check the error cell." This is the same pattern V8 uses for deoptimization.

4. **The bool_val in Cranelift is I64, not I8.** The bytecode JIT represents all values as I64. Boolean `true` is 1, `false` is 0. The `brif` instruction treats 0 as false and non-zero as true — this is correct for our use case.

5. **Contract clauses compile to multiple bytecode instructions** (the clause expression) followed by one assertion opcode. The JIT must handle ALL the clause expression opcodes too — but these are just arithmetic, comparisons, and loads which the JIT already handles. The only new opcode is the assertion itself.

6. **The `ensures` clause references `result`** — this is bound to the body's return register by the bytecode compiler. The JIT handles this naturally since it's just another register variable.

## Dependency Graph

```
Task 1 (runtime helper + eligibility)
    │
    └─→ Task 2 (Cranelift IR emission)
            │
            └─→ Task 3 (VM error propagation)
                    │
                    └─→ Task 4 (integration test + benchmark)
```

All tasks are sequential — each depends on the previous.
