# AIRL Self-Hosting Compiler Design

**Date:** 2026-03-23
**Status:** Draft
**Scope:** Complete path from current bootstrap compiler to a self-hosting AIRL compiler that reads its own source and produces a native binary

## Problem Statement

The AIRL bootstrap compiler (lexer, parser, type checker, evaluator, IR compiler) is implemented in AIRL but runs on the Rust runtime. ~48 primitive operations (arithmetic, string/map/list operations, I/O) are Rust builtins. The compiler cannot produce a standalone executable. The goal is a native AIRL compiler binary that compiles its own source code.

## Current State

```
AIRL Source → [Rust Lexer/Parser] → AST → IR → Bytecode → Bytecode VM
                                                              ↓ (--jit, primitives only)
                                                         Cranelift → native x86-64

Bootstrap compiler (in AIRL): lexer, parser, typechecker, evaluator, IR compiler
  → runs ON the Rust runtime
  → delegates ~48 operations to Rust builtins
  → determinism verified: interpreter and IR VM produce identical output
```

**What works:**
- Bytecode→Cranelift JIT for primitive-typed functions (int/float/bool only)
- Bootstrap compiler can lex, parse, type-check, and compile AIRL programs
- Full pipeline tested (pipeline_test.airl)
- Determinism verified (equivalence_test.airl, fixpoint_test.airl)

**What's missing:**
- Native code generation for non-primitive operations (lists, maps, strings, closures, variants, pattern matching)
- AOT compilation (emit object files, not just JIT in memory)
- A runtime library with C ABI for the ~48 builtins
- A bootstrap driver that wires it all together

## Target State

```
AIRL Source → [AIRL Compiler Binary] → native executable
                    ↑
            compiled from its own source (fixpoint-verified)
```

## Architecture Overview

Five steps, each building on the previous:

### Step 1: Runtime Library (`libairl_rt`)

A Rust static library exposing ~48 builtin operations with a C ABI. This is the "floor" — generated native code calls into it for operations that can't be expressed as machine instructions (string manipulation, map operations, memory allocation, I/O).

**Key design decision: Value representation.** Generated code must interoperate with the runtime library through a shared `Value` type. Two options:

**Option A: Pointer-tagged values (NaN-boxing)**
- Every value is a 64-bit word
- Small ints and bools are stored inline (tagged)
- Heap objects (strings, lists, maps, closures) are pointers with tag bits
- Pro: No indirection for common int/bool operations
- Con: Complex tagging scheme, limited inline int range

**Option B: Heap-allocated values with refcounting**
- Every value is a pointer to a heap-allocated `Value` struct
- `Value` has a tag byte + union payload + refcount
- Pro: Simple, uniform — every value is `*mut Value`
- Con: Every integer operation allocates/deallocates
- Mitigation: Arena allocator for function-scoped values, free-list for common sizes

**Recommendation: Option B.** Simplicity matters more than micro-optimization at this stage. The compiler itself is not performance-critical — it just needs to work correctly. Option A can be a future optimization pass.

```rust
// libairl_rt/src/value.rs
#[repr(C)]
pub struct Value {
    tag: u8,
    rc: u32,
    data: ValueData,
}

#[repr(C)]
pub union ValueData {
    int_val: i64,
    float_val: f64,
    bool_val: bool,
    str_val: *mut AirlString,
    list_val: *mut AirlList,
    map_val: *mut AirlMap,
    variant_val: *mut AirlVariant,
    closure_val: *mut AirlClosure,
    nil: (),
}
```

**Exported functions (~48, all `extern "C"`):**

Arithmetic (5): `airl_add`, `airl_sub`, `airl_mul`, `airl_div`, `airl_mod`
Comparison (6): `airl_eq`, `airl_ne`, `airl_lt`, `airl_gt`, `airl_le`, `airl_ge`
Logic (4): `airl_not`, `airl_and`, `airl_or`, `airl_xor`
List (5): `airl_head`, `airl_tail`, `airl_cons`, `airl_empty`, `airl_list_new`
String (14): `airl_char_at`, `airl_substring`, `airl_chars`, `airl_split`, `airl_join`, `airl_contains`, `airl_starts_with`, `airl_ends_with`, `airl_index_of`, `airl_trim`, `airl_to_upper`, `airl_to_lower`, `airl_replace`, `airl_length`
Map (10): `airl_map_new`, `airl_map_from`, `airl_map_get`, `airl_map_get_or`, `airl_map_has`, `airl_map_size`, `airl_map_set`, `airl_map_remove`, `airl_map_keys`, `airl_map_values`
I/O (3): `airl_print`, `airl_type_of`, `airl_append`
Memory (3): `airl_value_retain`, `airl_value_release`, `airl_value_clone`
Variant (2): `airl_make_variant`, `airl_match_tag`
Closure (1): `airl_call_closure`

**Memory management:** Reference counting. `airl_value_retain` increments refcount, `airl_value_release` decrements and frees at zero. Generated code emits retain/release around value usage. The runtime owns all heap allocation.

**Error handling:** Runtime errors (division by zero, index out of bounds, type mismatch) call `airl_runtime_error(msg)` which prints and calls `exit(1)`. No exception unwinding in generated code — matches current AIRL semantics where runtime errors are fatal.

### Step 2: Full Cranelift Code Generation

Extend `bytecode_jit.rs` to handle every bytecode opcode. Currently ineligible opcodes emit calls to runtime helper functions from Step 1.

**Value calling convention:** All function parameters and return values are `i64` (pointer to `Value`). This replaces the current scheme where primitives are unboxed `i64`s. The uniformity simplifies code generation at the cost of boxing overhead. (Future optimization: unbox primitives within a function body when type is statically known.)

**Opcode translations for currently ineligible opcodes:**

| Opcode | Cranelift emission |
|--------|-------------------|
| `MakeList` | Emit N value args to stack, `call airl_list_new(ptr, count)` |
| `MakeVariant(tag, inner)` | `call airl_make_variant(tag_str_ptr, inner_val)` |
| `MakeVariant0(tag)` | `call airl_make_variant(tag_str_ptr, null)` |
| `MakeClosure(func_idx, captures)` | Allocate closure struct with func pointer + captured values |
| `MatchTag(dst, scrutinee, tag)` | `call airl_match_tag(scrutinee, tag_str_ptr)`, result is inner value or null |
| `JumpIfNoMatch` | `brif` on null result from `airl_match_tag` |
| `MatchWild` | Move scrutinee to dst (always matches) |
| `CallBuiltin(name, args)` | Dispatch: lookup name in builtin table, `call` the corresponding `airl_*` function |
| `CallReg(callee, args)` | `call airl_call_closure(callee_val, args_ptr, argc)` |
| `TryUnwrap` | `call airl_match_tag(val, "Ok")`, jump to error handler if null |

**Refcount insertion:** After each function call that returns a `Value*`, emit `call airl_value_retain`. Before overwriting a register that holds a `Value*`, emit `call airl_value_release` on the old value. At function exit, release all live locals.

**Eligibility removal:** With all opcodes handled, the eligibility check is no longer needed. Every function is compilable. The `is_eligible` / `ineligible` set can be removed or kept as a "fast path" optimization hint.

### Step 3: AOT Object File Emission

Replace `cranelift-jit` (`JITModule`) with `cranelift-object` (`ObjectModule`) for ahead-of-time compilation.

```rust
// New crate or module: airl-aot
use cranelift_object::{ObjectBuilder, ObjectModule};

pub fn compile_to_object(funcs: &[BytecodeFunc], output: &Path) -> Result<(), String> {
    let mut module = ObjectModule::new(ObjectBuilder::new(...)?);

    // Declare all runtime imports (airl_add, airl_map_get, etc.)
    declare_runtime_imports(&mut module)?;

    // Compile all functions
    for func in funcs {
        compile_func(&mut module, func)?;
    }

    // Emit object file
    let obj = module.finish();
    std::fs::write(output, obj.emit()?)?;
    Ok(())
}
```

**Linking:** The emitted `.o` file is linked with `libairl_rt.a` via `cc`:
```
cc -o output program.o -lairl_rt -L/path/to/runtime
```

**Entry point:** The AOT compiler emits a `main` function that:
1. Calls `airl_runtime_init()` (initializes allocator, string interning)
2. Calls `__airl_main()` (the compiled `__main__` function)
3. Calls `airl_runtime_shutdown()` (cleanup)

### Step 4: Bootstrap Driver

A new AIRL source file (`bootstrap/driver.airl`) that wires the bootstrap compiler into a command-line tool:

```clojure
(defn main []
  ;; Read source file from argv
  ;; Lex → parse → type-check → compile to IR → compile to bytecode
  ;; Invoke AOT emission (via new builtin: emit-object)
  ;; Invoke linker (via new builtin: run-command)
  ...)
```

**New builtins needed:**
- `read-file` — read a file to string (currently AIRL has no file I/O)
- `argv` — access command-line arguments
- `emit-object` — invoke the AOT compiler on bytecode functions, write .o file
- `run-command` — exec a subprocess (for the linker invocation)
- `exit` — exit with status code

These are added to `libairl_rt` and exposed as builtins.

**The bootstrap sequence:**

```
1. Rust toolchain builds: libairl_rt.a + airl-driver (with AOT mode)

2. airl-driver --aot bootstrap/driver.airl → stage0.o
   cc stage0.o -lairl_rt -o airl-stage1

3. airl-stage1 bootstrap/driver.airl → stage1.o
   cc stage1.o -lairl_rt -o airl-stage2

4. airl-stage2 bootstrap/driver.airl → stage2.o
   cc stage2.o -lairl_rt -o airl-stage3

5. diff stage2.o stage3.o → identical (fixpoint)
```

After step 5, `airl-stage2` is a self-hosting compiler. It reads AIRL source, produces a native binary (linked against `libairl_rt.a`), and can compile its own source to produce an identical binary.

### Step 5: Fixpoint Verification

**What "fixpoint" means here:** stage2.o == stage3.o (byte-identical object files). This proves the compiler is a fixed point of itself — it faithfully reproduces its own compilation.

**What it does NOT mean:** The compiler is independent of all Rust code. `libairl_rt.a` is still compiled by Rust. The self-hosting claim is: "the compiler binary is produced by compiling AIRL source, not by compiling Rust source." The runtime library is a separate artifact, analogous to libc for a C compiler.

**Verification test:**
```bash
#!/bin/bash
# Build stage1 from Rust-hosted compiler
airl-driver --aot bootstrap/driver.airl -o stage1.o
cc stage1.o -lairl_rt -o stage1

# Build stage2 from stage1
./stage1 bootstrap/driver.airl -o stage2.o
cc stage2.o -lairl_rt -o stage2

# Build stage3 from stage2
./stage2 bootstrap/driver.airl -o stage3.o

# Fixpoint check
diff stage2.o stage3.o && echo "FIXPOINT VERIFIED" || echo "FIXPOINT FAILED"
```

## Implementation Order

| Step | Description | Depends On | Estimated Scope |
|------|-------------|-----------|-----------------|
| 1 | Runtime library (`libairl_rt`) | Nothing — can start now | New crate, ~1,500 lines |
| 2 | Full Cranelift codegen (all opcodes) | Step 1 (needs runtime ABI) | Extend `bytecode_jit.rs`, ~800 lines |
| 3 | AOT object emission | Step 2 (needs full codegen) | New module, ~300 lines |
| 4 | Bootstrap driver | Steps 1-3 + new builtins | ~200 lines AIRL + ~100 lines Rust |
| 5 | Fixpoint verification | Step 4 | Test script + CI |

## Non-Goals

- Optimizing compiled code performance (correctness first)
- Removing the Rust runtime library (it's analogous to libc — expected)
- Self-hosting the runtime library in AIRL (the runtime is a platform, not part of the compiler)
- Cross-compilation (x86-64 Linux only for now)
- Incremental compilation, caching, or build system integration

## Risks

1. **Value representation mismatch.** The runtime's `Value` struct must exactly match what generated code assumes. A single field offset error produces silent corruption. Mitigation: generate the struct layout from a shared definition; test with AddressSanitizer.

2. **Refcount correctness.** Incorrect retain/release causes leaks or use-after-free. Mitigation: conservative strategy (retain on every copy, release on every overwrite); verify with Valgrind on the fixpoint test.

3. **Bootstrap compiler performance.** The bootstrap compiler uses maps heavily (environment frames). With heap-allocated values and refcounting, compilation of large programs may be slow. Mitigation: not a blocker for correctness; optimize later.

4. **Closure calling convention.** Closures capture values from enclosing scopes. The generated code must allocate a closure struct, copy captured values, and dispatch calls through a function pointer + environment pair. This is the most complex code generation pattern. Mitigation: test thoroughly with the bootstrap compiler's own closure usage.

5. **String constants.** The compiler uses many string constants (keyword names, error messages). These must be emitted as data sections in the object file, not allocated at runtime. Cranelift's `ObjectModule` supports data sections for this.
