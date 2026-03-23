# BytecodeJitFull Bug Report — 9 Fixture Failures

**Date:** 2026-03-23
**Context:** `--jit-full` mode compiles ALL bytecode functions to native x86-64 via `BytecodeJitFull` in `crates/airl-runtime/src/bytecode_jit_full.rs`. 17/26 fixture tests pass. This document describes the 9 failures for a debugging agent.

**How to reproduce:** Build with `cargo build --release --features jit -p airl-driver`, then:
```bash
RUST_MIN_STACK=67108864 target/release/airl-driver run --jit-full tests/fixtures/valid/<test>.airl
```

**Key diagnostic:** Every failing test prints `[JIT-full] __main__ compile error: define: Compilation error: Verifier errors` — this means the `__main__` function (which calls stdlib functions like `fold`, `sort`, etc.) failed Cranelift's IR verifier and fell back to bytecode interpretation. The user-defined and stdlib functions that DID compile to native code may have bugs.

**Reference files:**
- JIT compiler: `crates/airl-runtime/src/bytecode_jit_full.rs`
- Runtime library: `crates/airl-rt/src/`
- Bytecode VM (correct reference): `crates/airl-runtime/src/bytecode_vm.rs`
- Bytecode types: `crates/airl-runtime/src/bytecode.rs`

---

## Bug 1: Variant Tag String Corruption

**Fixture:** `safe_divide`
**Symptom:** `(Ok 3)` → `(   3)` — variant tag becomes spaces/garbage
**Exit:** 0 (no crash)

**Root cause hypothesis:** When `MakeVariant` emits a tag string via `airl_str(ptr, len)`, the string pointer comes from `func.constants[idx]` — the bytecode constant pool. The constant pool lives in the `BytecodeFunc` which is stored in `BytecodeVm.functions`. However, when the function is JIT-compiled, the constant pool's `Value::Str` may be a `String` whose backing buffer is on the Rust heap. If the `BytecodeFunc` or its constants Vec gets dropped, moved, or reallocated after compilation, the pointer becomes dangling.

**Debugging approach:**
1. Add `AIRL_JIT_DEBUG=1` output in `compile_func` when emitting `MakeVariant` — print the tag string, its pointer, and its length
2. Check whether the string pointer is still valid at call time by adding a test: compile a function that creates `(Ok 42)`, call it, verify the tag is "Ok"
3. **Fix:** Instead of using `s.as_ptr()` from the constant pool, copy the string bytes into the JIT module's memory (via a Cranelift data section) or allocate a stable copy that outlives the JIT compilation

**Affected opcodes:** `MakeVariant`, `MakeVariant0`, `MatchTag`, `CallBuiltin` — any opcode that reads a string constant from the pool and passes it to a runtime function

---

## Bug 2: Cranelift Verifier Errors on `__main__`

**Fixture:** ALL 9 failing tests
**Symptom:** `[JIT-full] __main__ compile error: define: Compilation error: Verifier errors`
**Exit:** Falls back to bytecode (not a crash)

**Root cause hypothesis:** The `__main__` function in every AIRL program is the top-level expression sequence. It likely contains opcodes or patterns that trigger Cranelift's IR verifier. Common causes:
- **Unreachable code after terminators:** If a `Return` or `TailCall` is followed by more instructions that aren't at a block boundary, the verifier rejects them. (This was partially fixed — Return/TailCall were added to the block boundary scanner, but may be incomplete.)
- **Block with no terminator:** If a block doesn't end with jump/return/brif, the verifier complains.
- **Variable used before defined in a block:** If a Cranelift variable is used in a block where it wasn't defined and the block has multiple predecessors with different definitions.
- **Type mismatch:** If `airl_float` is declared with wrong param type (F64 vs I64).

**Debugging approach:**
1. Set `AIRL_JIT_DEBUG=1` and print the verifier error message (not just "Verifier errors" — the full Cranelift verifier output lists which instruction/block failed)
2. In `compile_func`, before `self.module.define_function(func_id, &mut ctx)`, add:
   ```rust
   if let Err(errors) = cranelift_codegen::verify_function(&ctx.func, self.module.isa()) {
       eprintln!("[JIT-full] verifier: {}", errors);
   }
   ```
3. This will show exactly which Cranelift IR instruction is invalid
4. Common fixes: ensure every block ends with a terminator, add missing block boundaries for code after Return/TailCall

---

## Bug 3: Closure Dispatch Failure — "not a Closure"

**Fixtures:** `stdlib_fold_reverse`, `stdlib_map_filter`, `stdlib_search`, `stdlib_sort`
**Symptom:** `Runtime error: airl_call_closure: not a Closure`
**Exit:** 1

**Root cause hypothesis:** These all use higher-order stdlib functions (`fold`, `filter`, `any`, `sort`) that take lambda arguments. The `CallReg` opcode dispatches to `airl_call_closure`, but the value in the callee register isn't an `RtValue` with `TAG_CLOSURE`. Possible causes:

1. **MakeClosure emits wrong data:** The compiled closure's `func_ptr` field may be wrong (null, or pointing to the wrong function). Or the `capture_count` from the target function's `BytecodeFunc.capture_count` may be wrong.
2. **The closure value is a bytecode-fallback `Value::BytecodeClosure`** that was marshaled via `value_to_rt` but didn't create a proper `RtData::Closure` — the marshaling function maps `BytecodeClosure` to `rt_nil()` (the "anything else" case in `value_to_rt`).
3. **Lambda functions weren't compiled:** If the lambda function (e.g., `__lambda_collections_0`) wasn't JIT-compiled, `MakeClosure` can't get a native function pointer for it. The lambda might have failed compilation (verifier errors) and fallen through.

**Debugging approach:**
1. In `compile_func`'s `MakeClosure` handling, print: the function name being closed over, whether it was found in `self.compiled`, and its pointer
2. Check that lambda functions (names like `__lambda_collections_0`) are being compiled successfully
3. In `airl_call_closure`, print the tag of the value it receives (is it TAG_CLOSURE or something else?)
4. **Fix:** If the lambda wasn't compiled, either: (a) compile it first (dependency ordering), or (b) create a trampoline that calls back into the bytecode VM

---

## Bug 4: Invalid UTF-8 in String Construction

**Fixture:** `stdlib_result`
**Symptom:** `Runtime error: airl_str: invalid utf8` (after partial output)
**Exit:** 1

**Root cause:** Same as Bug 1. A string constant pointer is passed to `airl_str(ptr, len)` but the bytes at that address are no longer the original string. The UTF-8 validation in `airl_str` catches the corruption.

**Fix:** Same as Bug 1 — use stable string storage instead of raw pointers to the constant pool.

---

## Bug 5: Segfault After Partial Success

**Fixtures:** `stdlib_map`, `stdlib_math`, `stdlib_string`
**Symptom:** Produces correct output for many operations, then segfaults (exit 139)
**Exit:** 139 (SIGSEGV)

**Root cause hypothesis:** These tests produce a LOT of correct output before crashing, suggesting most operations work but a specific operation triggers use-after-free or null dereference. Likely causes:

1. **Refcount bug:** A value is released too early, then accessed later. The `try_call_native` method retains the result before releasing args (this was already fixed), but there may be similar issues in the generated code itself — e.g., when a function returns a value that's part of a list, the list is released but the inner value is still referenced.
2. **Stack overflow from deep recursion:** stdlib functions like `sum-list` and `product-list` use recursive `fold`. If the JIT'd version doesn't have TCO and the list is large, it could overflow the stack.
3. **Null pointer from failed compilation:** A function that failed to compile (verifier errors) returns `None` from `try_call_native`, but the bytecode VM fallback may interact poorly with JIT-compiled callers.

**Debugging approach:**
1. Run under Valgrind: `valgrind --tool=memcheck target/release/airl-driver run --jit-full tests/fixtures/valid/stdlib_math.airl`
2. Identify which specific operation causes the segfault by adding print statements or bisecting the fixture
3. Check if the segfault happens in JIT'd code or in the runtime library

---

## Priority Order for Fixes

1. **Bug 2 (Verifier errors)** — Fix first. This causes `__main__` to fall back to bytecode, which means mixed execution (some functions JIT, some bytecode). Many other bugs may be artifacts of this mixed mode. Print the full verifier error to understand what's wrong.

2. **Bug 1/4 (String constant corruption)** — Fix second. This affects MakeVariant, MatchTag, CallBuiltin, and any string constant. Solution: copy string bytes to stable storage during compilation.

3. **Bug 3 (Closure dispatch)** — Fix third. This blocks all higher-order stdlib (fold, map, filter, sort). Need to ensure lambda functions are compiled before their closures are created.

4. **Bug 5 (Segfault)** — Fix last. May resolve itself once the above bugs are fixed, since segfaults often cascade from earlier corruption.

---

## How to Run Tests

```bash
# Build
source "$HOME/.cargo/env" && cargo build --release --features jit -p airl-driver

# Run a single fixture
RUST_MIN_STACK=67108864 target/release/airl-driver run --jit-full tests/fixtures/valid/<name>.airl

# Run with JIT debug output
AIRL_JIT_DEBUG=1 RUST_MIN_STACK=67108864 target/release/airl-driver run --jit-full tests/fixtures/valid/<name>.airl

# Run all fixtures and compare
for f in tests/fixtures/valid/*.airl; do
  name=$(basename "$f" .airl)
  case "$name" in execute_on_gpu|mlir_tensor|jit_arithmetic|lexer_bootstrap|contracts|invariant|float_contract|forall_contract|forall_expr|exists_expr|proven_contracts|quantifier_proven) continue ;; esac
  interp=$(RUST_MIN_STACK=67108864 timeout 10 target/release/airl-driver run "$f" 2>/dev/null) || continue
  jitfull=$(RUST_MIN_STACK=67108864 timeout 10 target/release/airl-driver run --jit-full "$f" 2>/dev/null)
  [ "$interp" = "$jitfull" ] && echo "PASS: $name" || echo "FAIL: $name"
done

# Run unit tests
source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode_jit_full -- --nocapture
```
