# Full Cranelift Code Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the bytecode→Cranelift compiler to handle ALL bytecode opcodes (not just primitives), emitting calls to `airl-rt` runtime functions for non-primitive operations. After this, every AIRL function is compilable to native code.

**Architecture:** New file `bytecode_jit_full.rs` implements a "boxed mode" compiler where every value is a `*mut RtValue` (i64 pointer). All operations — arithmetic, comparisons, list construction, pattern matching, closures — emit `call` instructions to the corresponding `airl_*` runtime functions. The existing `bytecode_jit.rs` (unboxed primitives) is preserved as-is for the fast path. A new `BytecodeJitFull` struct mirrors `BytecodeJit` but compiles ALL functions, no eligibility check.

**Tech Stack:** Rust, Cranelift 0.130, `airl-rt` (runtime library from Step 1)

**Spec:** `docs/superpowers/specs/2026-03-23-self-hosting-design.md` (Step 2)

**Reference files:**
- Current primitive JIT: `crates/airl-runtime/src/bytecode_jit.rs` (~870 lines)
- Bytecode opcodes: `crates/airl-runtime/src/bytecode.rs` (Op enum, Instruction, BytecodeFunc)
- Bytecode VM (reference semantics): `crates/airl-runtime/src/bytecode_vm.rs`
- Runtime library: `crates/airl-rt/src/` (all `airl_*` symbols)
- Pipeline: `crates/airl-driver/src/pipeline.rs`
- CLI: `crates/airl-driver/src/main.rs`

**Key design decisions:**
- All values are `*mut RtValue` represented as Cranelift `I64` (pointer-sized)
- No type hints or eligibility checks — every function compiles
- Runtime functions declared as `Linkage::Import` in the JIT module, resolved via `JITBuilder::symbol`
- String constants emitted as data sections or constructed via `airl_str` calls
- `match_flag` for pattern matching tracked as a Cranelift variable (i64, 0 or 1)
- JumpIfFalse/JumpIfTrue need to extract the bool from a `*mut RtValue` — call a small helper or emit `load` from a known offset

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `crates/airl-runtime/src/bytecode_jit_full.rs` | `BytecodeJitFull`: boxed-value Cranelift compiler for all opcodes |
| Modify: `crates/airl-runtime/src/lib.rs` | Add `#[cfg(feature = "jit")] pub mod bytecode_jit_full;` |
| Modify: `crates/airl-runtime/Cargo.toml` | Add `airl-rt` as optional dependency under `jit` feature |
| Modify: `crates/airl-runtime/src/bytecode_vm.rs` | Add `new_with_full_jit()`, `jit_full` field, dispatch in Op::Call |
| Modify: `crates/airl-driver/src/pipeline.rs` | Add `run_source_jit_full()` |
| Modify: `crates/airl-driver/src/main.rs` | Add `--jit-full` flag |

---

### Task 1: Crate Wiring and Runtime Symbol Registration

**Files:**
- Modify: `crates/airl-runtime/Cargo.toml`
- Create: `crates/airl-runtime/src/bytecode_jit_full.rs`
- Modify: `crates/airl-runtime/src/lib.rs`

Set up the new module and register all `airl_*` symbols with the Cranelift JIT so they're callable.

- [ ] **Step 1: Add `airl-rt` dependency to `airl-runtime/Cargo.toml`**

Update the `jit` feature to include `airl-rt`:
```toml
jit = ["dep:cranelift-codegen", "dep:cranelift-frontend", "dep:cranelift-jit", "dep:cranelift-module", "dep:target-lexicon", "dep:airl-rt"]
```

Add to `[dependencies]`:
```toml
airl-rt = { path = "../airl-rt", optional = true }
```

- [ ] **Step 2: Create `bytecode_jit_full.rs` with struct and runtime symbol registration**

```rust
//! Full bytecode→Cranelift compiler (boxed values).
//!
//! Unlike bytecode_jit.rs (primitive-only, unboxed i64), this compiles ALL
//! functions by representing every value as *mut RtValue (i64 pointer) and
//! emitting calls to airl-rt runtime functions for all operations.

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, FuncId};

use crate::bytecode::*;
use crate::value::Value;
use crate::error::RuntimeError;

/// Function IDs for all runtime imports.
struct RuntimeImports {
    // Constructors
    airl_int: FuncId,
    airl_float: FuncId,
    airl_bool: FuncId,
    airl_nil: FuncId,
    airl_unit: FuncId,
    airl_str: FuncId,
    // Arithmetic
    airl_add: FuncId,
    airl_sub: FuncId,
    airl_mul: FuncId,
    airl_div: FuncId,
    airl_mod: FuncId,
    // Comparison
    airl_eq: FuncId,
    airl_ne: FuncId,
    airl_lt: FuncId,
    airl_gt: FuncId,
    airl_le: FuncId,
    airl_ge: FuncId,
    // Logic
    airl_not: FuncId,
    // List
    airl_list_new: FuncId,
    airl_head: FuncId,
    airl_tail: FuncId,
    airl_cons: FuncId,
    airl_empty: FuncId,
    // Variant
    airl_make_variant: FuncId,
    airl_match_tag: FuncId,
    // Closure
    airl_make_closure: FuncId,
    airl_call_closure: FuncId,
    // Memory
    airl_value_retain: FuncId,
    airl_value_release: FuncId,
    // Builtins (generic dispatcher — used for CallBuiltin)
    // For CallBuiltin, we'll need to call the specific builtin by name.
    // Strategy: declare each builtin we encounter as an import.
}

pub struct BytecodeJitFull {
    module: JITModule,
    rt: RuntimeImports,
    /// Compiled function cache: name → function pointer
    compiled: HashMap<String, *const u8>,
    /// Additional runtime imports declared on-demand (for CallBuiltin)
    builtin_imports: HashMap<String, FuncId>,
}

impl BytecodeJitFull {
    pub fn new() -> Result<Self, String> {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;

        // Register all airl-rt symbols so Cranelift can resolve them
        Self::register_runtime_symbols(&mut builder);

        let mut module = JITModule::new(builder);
        let rt = Self::declare_runtime_imports(&mut module)?;

        Ok(Self {
            module,
            rt,
            compiled: HashMap::new(),
            builtin_imports: HashMap::new(),
        })
    }

    fn register_runtime_symbols(builder: &mut JITBuilder) {
        // Map symbol names to function pointers from airl-rt
        let symbols: Vec<(&str, *const u8)> = vec![
            ("airl_int", airl_rt::value::airl_int as *const u8),
            ("airl_float", airl_rt::value::airl_float as *const u8),
            ("airl_bool", airl_rt::value::airl_bool as *const u8),
            ("airl_nil", airl_rt::value::airl_nil as *const u8),
            ("airl_unit", airl_rt::value::airl_unit as *const u8),
            ("airl_str", airl_rt::value::airl_str as *const u8),
            ("airl_add", airl_rt::arithmetic::airl_add as *const u8),
            ("airl_sub", airl_rt::arithmetic::airl_sub as *const u8),
            ("airl_mul", airl_rt::arithmetic::airl_mul as *const u8),
            ("airl_div", airl_rt::arithmetic::airl_div as *const u8),
            ("airl_mod", airl_rt::arithmetic::airl_mod as *const u8),
            ("airl_eq", airl_rt::comparison::airl_eq as *const u8),
            ("airl_ne", airl_rt::comparison::airl_ne as *const u8),
            ("airl_lt", airl_rt::comparison::airl_lt as *const u8),
            ("airl_gt", airl_rt::comparison::airl_gt as *const u8),
            ("airl_le", airl_rt::comparison::airl_le as *const u8),
            ("airl_ge", airl_rt::comparison::airl_ge as *const u8),
            ("airl_not", airl_rt::logic::airl_not as *const u8),
            ("airl_list_new", airl_rt::list::airl_list_new as *const u8),
            ("airl_head", airl_rt::list::airl_head as *const u8),
            ("airl_tail", airl_rt::list::airl_tail as *const u8),
            ("airl_cons", airl_rt::list::airl_cons as *const u8),
            ("airl_empty", airl_rt::list::airl_empty as *const u8),
            ("airl_make_variant", airl_rt::variant::airl_make_variant as *const u8),
            ("airl_match_tag", airl_rt::variant::airl_match_tag as *const u8),
            ("airl_make_closure", airl_rt::closure::airl_make_closure as *const u8),
            ("airl_call_closure", airl_rt::closure::airl_call_closure as *const u8),
            ("airl_value_retain", airl_rt::memory::airl_value_retain as *const u8),
            ("airl_value_release", airl_rt::memory::airl_value_release as *const u8),
            // Builtins used by stdlib/bootstrap — register the most common ones
            ("airl_length", airl_rt::list::airl_length as *const u8),
            ("airl_at", airl_rt::list::airl_at as *const u8),
            ("airl_append", airl_rt::list::airl_append as *const u8),
            ("airl_print", airl_rt::io::airl_print as *const u8),
            ("airl_type_of", airl_rt::io::airl_type_of as *const u8),
            ("airl_valid", airl_rt::io::airl_valid as *const u8),
            ("airl_char_at", airl_rt::string::airl_char_at as *const u8),
            ("airl_substring", airl_rt::string::airl_substring as *const u8),
            ("airl_chars", airl_rt::string::airl_chars as *const u8),
            ("airl_split", airl_rt::string::airl_split as *const u8),
            ("airl_join", airl_rt::string::airl_join as *const u8),
            ("airl_contains", airl_rt::string::airl_contains as *const u8),
            ("airl_starts_with", airl_rt::string::airl_starts_with as *const u8),
            ("airl_ends_with", airl_rt::string::airl_ends_with as *const u8),
            ("airl_index_of", airl_rt::string::airl_index_of as *const u8),
            ("airl_trim", airl_rt::string::airl_trim as *const u8),
            ("airl_to_upper", airl_rt::string::airl_to_upper as *const u8),
            ("airl_to_lower", airl_rt::string::airl_to_lower as *const u8),
            ("airl_replace", airl_rt::string::airl_replace as *const u8),
            ("airl_map_new", airl_rt::map::airl_map_new as *const u8),
            ("airl_map_from", airl_rt::map::airl_map_from as *const u8),
            ("airl_map_get", airl_rt::map::airl_map_get as *const u8),
            ("airl_map_get_or", airl_rt::map::airl_map_get_or as *const u8),
            ("airl_map_set", airl_rt::map::airl_map_set as *const u8),
            ("airl_map_has", airl_rt::map::airl_map_has as *const u8),
            ("airl_map_remove", airl_rt::map::airl_map_remove as *const u8),
            ("airl_map_keys", airl_rt::map::airl_map_keys as *const u8),
            ("airl_map_values", airl_rt::map::airl_map_values as *const u8),
            ("airl_map_size", airl_rt::map::airl_map_size as *const u8),
            ("airl_and", airl_rt::logic::airl_and as *const u8),
            ("airl_or", airl_rt::logic::airl_or as *const u8),
            ("airl_xor", airl_rt::logic::airl_xor as *const u8),
        ];
        for (name, ptr) in symbols {
            builder.symbol(name, ptr);
        }
    }

    /// Declare all runtime functions as imports in the Cranelift module.
    fn declare_runtime_imports(module: &mut JITModule) -> Result<RuntimeImports, String> {
        // Helper: signature with N i64 params and 1 i64 return
        macro_rules! sig {
            ($module:expr, $n:expr) => {{
                let mut s = $module.make_signature();
                for _ in 0..$n { s.params.push(AbiParam::new(types::I64)); }
                s.returns.push(AbiParam::new(types::I64));
                s
            }};
        }
        // Helper: signature for void functions (retain/release)
        macro_rules! sig_void {
            ($module:expr, $n:expr) => {{
                let mut s = $module.make_signature();
                for _ in 0..$n { s.params.push(AbiParam::new(types::I64)); }
                s
            }};
        }

        let decl = |module: &mut JITModule, name: &str, sig: cranelift_codegen::ir::Signature| -> Result<FuncId, String> {
            module.declare_function(name, Linkage::Import, &sig)
                .map_err(|e| format!("declare {}: {}", name, e))
        };

        Ok(RuntimeImports {
            airl_int: decl(module, "airl_int", sig!(module, 1))?,
            airl_float: {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(types::F64)); // float param, not i64
                s.returns.push(AbiParam::new(types::I64));
                decl(module, "airl_float", s)?
            },
            airl_bool: decl(module, "airl_bool", sig!(module, 1))?,
            airl_nil: decl(module, "airl_nil", sig!(module, 0))?,
            airl_unit: decl(module, "airl_unit", sig!(module, 0))?,
            airl_str: decl(module, "airl_str", sig!(module, 2))?, // ptr + len
            airl_add: decl(module, "airl_add", sig!(module, 2))?,
            airl_sub: decl(module, "airl_sub", sig!(module, 2))?,
            airl_mul: decl(module, "airl_mul", sig!(module, 2))?,
            airl_div: decl(module, "airl_div", sig!(module, 2))?,
            airl_mod: decl(module, "airl_mod", sig!(module, 2))?,
            airl_eq: decl(module, "airl_eq", sig!(module, 2))?,
            airl_ne: decl(module, "airl_ne", sig!(module, 2))?,
            airl_lt: decl(module, "airl_lt", sig!(module, 2))?,
            airl_gt: decl(module, "airl_gt", sig!(module, 2))?,
            airl_le: decl(module, "airl_le", sig!(module, 2))?,
            airl_ge: decl(module, "airl_ge", sig!(module, 2))?,
            airl_not: decl(module, "airl_not", sig!(module, 1))?,
            airl_list_new: decl(module, "airl_list_new", sig!(module, 2))?, // ptr + count
            airl_head: decl(module, "airl_head", sig!(module, 1))?,
            airl_tail: decl(module, "airl_tail", sig!(module, 1))?,
            airl_cons: decl(module, "airl_cons", sig!(module, 2))?,
            airl_empty: decl(module, "airl_empty", sig!(module, 1))?,
            airl_make_variant: decl(module, "airl_make_variant", sig!(module, 2))?,
            airl_match_tag: decl(module, "airl_match_tag", sig!(module, 2))?,
            airl_make_closure: decl(module, "airl_make_closure", sig!(module, 3))?,
            airl_call_closure: decl(module, "airl_call_closure", sig!(module, 3))?,
            airl_value_retain: {
                decl(module, "airl_value_retain", sig_void!(module, 1))?
            },
            airl_value_release: {
                decl(module, "airl_value_release", sig_void!(module, 1))?
            },
        })
    }

    pub fn try_call_native(&self, name: &str, args: &[Value]) -> Option<Value> {
        let &ptr = self.compiled.get(name)?;
        // Marshal args: convert each Value to *mut RtValue
        let rt_args: Vec<*mut airl_rt::value::RtValue> = args.iter().map(|v| value_to_rt(v)).collect();
        // Call through function pointer (dispatch by arity)
        let result_ptr = unsafe { call_fn_ptr(ptr, &rt_args) }?;
        // Unmarshal result: convert *mut RtValue back to Value
        let result = rt_to_value(result_ptr);
        Some(result)
    }
}

/// Convert an airl-runtime Value to an airl-rt RtValue pointer.
fn value_to_rt(v: &Value) -> *mut airl_rt::value::RtValue {
    match v {
        Value::Int(n) => airl_rt::value::rt_int(*n),
        Value::Float(f) => airl_rt::value::rt_float(*f),
        Value::Bool(b) => airl_rt::value::rt_bool(*b),
        Value::Str(s) => airl_rt::value::rt_str(s.clone()),
        Value::Nil => airl_rt::value::rt_nil(),
        Value::Unit => airl_rt::value::rt_unit(),
        Value::List(items) => {
            let rt_items: Vec<*mut airl_rt::value::RtValue> = items.iter().map(|i| value_to_rt(i)).collect();
            airl_rt::value::rt_list(rt_items)
        }
        Value::Variant(tag, inner) => {
            let rt_inner = value_to_rt(inner);
            airl_rt::value::rt_variant(tag.clone(), rt_inner)
        }
        Value::Map(m) => {
            let rt_map: std::collections::HashMap<String, *mut airl_rt::value::RtValue> = m.iter()
                .map(|(k, v)| (k.clone(), value_to_rt(v)))
                .collect();
            airl_rt::value::rt_map(rt_map)
        }
        _ => airl_rt::value::rt_nil(), // Functions, closures, etc. → nil for now
    }
}

/// Convert an airl-rt RtValue pointer back to an airl-runtime Value.
fn rt_to_value(ptr: *mut airl_rt::value::RtValue) -> Value {
    if ptr.is_null() { return Value::Nil; }
    let v = unsafe { &*ptr };
    let result = match &v.data {
        airl_rt::value::RtData::Nil => Value::Nil,
        airl_rt::value::RtData::Unit => Value::Unit,
        airl_rt::value::RtData::Int(n) => Value::Int(*n),
        airl_rt::value::RtData::Float(f) => Value::Float(*f),
        airl_rt::value::RtData::Bool(b) => Value::Bool(*b),
        airl_rt::value::RtData::Str(s) => Value::Str(s.clone()),
        airl_rt::value::RtData::List(items) => {
            Value::List(items.iter().map(|&i| rt_to_value(i)).collect())
        }
        airl_rt::value::RtData::Variant { tag_name, inner } => {
            Value::Variant(tag_name.clone(), Box::new(rt_to_value(*inner)))
        }
        airl_rt::value::RtData::Map(m) => {
            Value::Map(m.iter().map(|(k, &v)| (k.clone(), rt_to_value(v))).collect())
        }
        _ => Value::Nil,
    };
    unsafe { airl_rt::memory::airl_value_release(ptr); }
    result
}

/// Call a function pointer with the given RtValue args. Returns None if arity unsupported.
unsafe fn call_fn_ptr(ptr: *const u8, args: &[*mut airl_rt::value::RtValue]) -> Option<*mut airl_rt::value::RtValue> {
    type P = *mut airl_rt::value::RtValue;
    let result = match args.len() {
        0 => { let f: fn() -> P = std::mem::transmute(ptr); f() }
        1 => { let f: fn(P) -> P = std::mem::transmute(ptr); f(args[0]) }
        2 => { let f: fn(P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1]) }
        3 => { let f: fn(P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2]) }
        4 => { let f: fn(P, P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2], args[3]) }
        5 => { let f: fn(P, P, P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2], args[3], args[4]) }
        6 => { let f: fn(P, P, P, P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2], args[3], args[4], args[5]) }
        7 => { let f: fn(P, P, P, P, P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2], args[3], args[4], args[5], args[6]) }
        8 => { let f: fn(P, P, P, P, P, P, P, P) -> P = std::mem::transmute(ptr); f(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]) }
        _ => return None,
    };
    Some(result)
}
```

- [ ] **Step 3: Add module to `lib.rs`**

Add after the existing `bytecode_jit` line:
```rust
#[cfg(feature = "jit")]
pub mod bytecode_jit_full;
```

- [ ] **Step 4: Build**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-runtime --features jit 2>&1 | tail -5`
Expected: Build succeeds

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/Cargo.toml crates/airl-runtime/src/bytecode_jit_full.rs crates/airl-runtime/src/lib.rs
git commit -m "feat(jit-full): create BytecodeJitFull with runtime symbol registration and marshaling"
```

---

### Task 2: Core Compiler — Literals, Arithmetic, Comparisons, Control Flow

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit_full.rs`

Add `compile_all()` and `compile_func()` to `BytecodeJitFull`. This task handles: LoadConst, LoadNil, LoadTrue, LoadFalse, Move, Add/Sub/Mul/Div/Mod, Neg, Eq/Ne/Lt/Le/Gt/Ge, Not, Jump, JumpIfFalse, JumpIfTrue, Return, Call, TailCall.

The structure follows `bytecode_jit.rs:compile_func` closely (2-pass: block scanning then emission), but:
- Every value is i64 (pointer to RtValue)
- Arithmetic: `call airl_add(a, b)` instead of `iadd`
- Comparisons: `call airl_eq(a, b)` → returns `*mut RtValue` (a boxed Bool)
- JumpIfFalse/JumpIfTrue: need to extract the bool from the RtValue. Strategy: load the `tag` byte at offset 0, then load the bool data. **Simpler: call a helper `airl_is_truthy` that returns i64 0 or 1.** Actually simplest: read the `RtData::Bool` value. The `RtValue` layout is `{ tag: u8, rc: u32, data: ... }`. For Bool, data offset depends on enum layout. **Safest: add `airl_as_bool_raw(v: *mut RtValue) -> i64` to airl-rt that returns 0 or 1 as a plain i64 (not a RtValue).** Register it as a symbol.
- LoadConst for strings: emit `airl_str(ptr, len)` where ptr is a Cranelift data section containing the string bytes
- LoadConst for ints/floats/bools: emit `airl_int(n)` / `airl_float(f)` / `airl_bool(b)`
- Call: same as existing JIT (declare target, emit `call`), but all params are i64 pointers
- TailCall: jump back to entry (same loop pattern)

**New airl-rt helper needed:** `airl_as_bool_raw(*mut RtValue) -> i64` — extracts bool as plain 0/1 for branch conditions. Add to `airl-rt/src/logic.rs`.

- [ ] **Step 1: Add `airl_as_bool_raw` to airl-rt**

In `crates/airl-rt/src/logic.rs`, add:
```rust
/// Extract bool as raw i64 (0 or 1) for use in branch conditions.
/// Unlike airl_not etc., this does NOT return a *mut RtValue — it returns a plain integer.
#[no_mangle]
pub extern "C" fn airl_as_bool_raw(v: *mut crate::value::RtValue) -> i64 {
    let val = unsafe { &*v };
    match &val.data {
        crate::value::RtData::Bool(b) => *b as i64,
        crate::value::RtData::Nil => 0,
        _ => 1, // non-nil non-bool values are truthy
    }
}
```

Register this in `bytecode_jit_full.rs` symbol list and declare it with signature `(I64) -> I64`.

- [ ] **Step 2: Implement `compile_all` and `compile_func` for all opcodes listed above**

The `compile_func` method:
1. Build signature: all params I64, return I64
2. Declare function as `Linkage::Local`
3. Pre-declare call targets (same as existing JIT)
4. **Pre-emit string constants:** scan all `LoadConst` instructions, for any `Value::Str` constant, create a Cranelift data section with the string bytes. Store the `DataId` for later use.
5. Pass 1: find block boundaries (same as existing)
6. Create blocks, declare variables, bind params
7. Pass 2: emit IR:
   - `LoadConst(Int)`: `call airl_int(iconst n)`
   - `LoadConst(Float)`: `call airl_float(f64const f)`
   - `LoadConst(Bool)`: `call airl_bool(iconst b)`
   - `LoadConst(Str)`: `call airl_str(data_ptr, iconst len)` using pre-emitted data section
   - `LoadNil`: `call airl_nil()`
   - `LoadTrue`: `call airl_bool(iconst 1)`
   - `LoadFalse`: `call airl_bool(iconst 0)`
   - `Move`: `def_var(dst, use_var(src))`
   - `Add`: `call airl_add(a, b)` → def_var(dst)
   - `Sub/Mul/Div/Mod`: same pattern with corresponding rt function
   - `Neg`: `call airl_sub(airl_int(0), a)` (or add `airl_neg` to rt)
   - `Eq/Ne/Lt/Le/Gt/Ge`: `call airl_eq(a, b)` etc.
   - `Not`: `call airl_not(a)`
   - `Jump`: same as existing
   - `JumpIfFalse`: `call airl_as_bool_raw(cond_var)` → i64, then `brif`
   - `JumpIfTrue`: same but inverted
   - `Return`: `return val`
   - `Call`: declare target, emit `call`, store result
   - `TailCall`: move args to param vars, jump to loop block
8. Seal blocks, finalize, define, finalize_definitions, get pointer

- [ ] **Step 3: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    #[test]
    fn test_full_jit_add() {
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("add", &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));

        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert!(jit.compiled.contains_key("add"));

        let result = jit.try_call_native("add", &[Value::Int(3), Value::Int(4)]);
        assert_eq!(result, Some(Value::Int(7)));
    }

    #[test]
    fn test_full_jit_string_concat() {
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("greet", &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));

        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);

        let result = jit.try_call_native("greet", &[Value::Str("hello ".into()), Value::Str("world".into())]);
        assert_eq!(result, Some(Value::Str("hello world".into())));
    }

    #[test]
    fn test_full_jit_if_branch() {
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("max2", &["a".into(), "b".into()],
            &IRNode::If(
                Box::new(IRNode::Call(">".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())])),
                Box::new(IRNode::Load("a".into())),
                Box::new(IRNode::Load("b".into())),
            ));

        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);

        assert_eq!(jit.try_call_native("max2", &[Value::Int(10), Value::Int(3)]), Some(Value::Int(10)));
        assert_eq!(jit.try_call_native("max2", &[Value::Int(2), Value::Int(8)]), Some(Value::Int(8)));
    }

    #[test]
    fn test_full_jit_list_creation() {
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("make", &[],
            &IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]));

        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);

        let result = jit.try_call_native("make", &[]);
        assert_eq!(result, Some(Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])));
    }
}
```

- [ ] **Step 4: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-runtime --features jit bytecode_jit_full -- --nocapture 2>&1 | tail -15`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-rt/src/logic.rs crates/airl-runtime/src/bytecode_jit_full.rs
git commit -m "feat(jit-full): core compiler — literals, arithmetic, comparisons, control flow, lists"
```

---

### Task 3: Data Operations — MakeVariant, MakeClosure, CallBuiltin, CallReg

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit_full.rs`

Add handling for the remaining opcodes in `compile_func`:

- `MakeList`: already handled in Task 2 via `airl_list_new`
- `MakeVariant(dst, tag_idx, inner_reg)`: load tag string constant, `call airl_make_variant(tag_str, inner_val)`
- `MakeVariant0(dst, tag_idx)`: `call airl_make_variant(tag_str, airl_unit())`
- `MakeClosure(dst, func_idx, capture_start)`: lookup func name from constants, get its compiled pointer, build captures array on stack, `call airl_make_closure(func_ptr, captures_ptr, count)`
- `CallBuiltin(dst, name_idx, argc)`: lookup builtin name from constants, map it to the corresponding `airl_*` function, emit a direct call. For this, maintain a `HashMap<String, FuncId>` of builtin name → Cranelift FuncId. Declare on first encounter.
- `CallReg(dst, callee_reg, argc)`: `call airl_call_closure(callee, args_ptr, argc)`

**CallBuiltin mapping:** The bytecode `CallBuiltin` stores the builtin name as a string constant (e.g., "+", "head", "map-get"). Map each name to the corresponding `airl_*` symbol:
```
"+" → airl_add, "-" → airl_sub, "*" → airl_mul, "/" → airl_div, "%" → airl_mod,
"=" → airl_eq, "!=" → airl_ne, "<" → airl_lt, ">" → airl_gt, "<=" → airl_le, ">=" → airl_ge,
"not" → airl_not, "and" → airl_and, "or" → airl_or, "xor" → airl_xor,
"head" → airl_head, "tail" → airl_tail, "cons" → airl_cons, "empty?" → airl_empty,
"length" → airl_length, "at" → airl_at, "append" → airl_append,
"print" → airl_print, "type-of" → airl_type_of, "valid" → airl_valid,
"char-at" → airl_char_at, "substring" → airl_substring, "chars" → airl_chars,
"split" → airl_split, "join" → airl_join, "contains" → airl_contains,
"starts-with" → airl_starts_with, "ends-with" → airl_ends_with,
"index-of" → airl_index_of, "trim" → airl_trim, "to-upper" → airl_to_upper,
"to-lower" → airl_to_lower, "replace" → airl_replace,
"map-new" → airl_map_new, "map-from" → airl_map_from, "map-get" → airl_map_get,
"map-get-or" → airl_map_get_or, "map-set" → airl_map_set, "map-has" → airl_map_has,
"map-remove" → airl_map_remove, "map-keys" → airl_map_keys,
"map-values" → airl_map_values, "map-size" → airl_map_size,
```

- [ ] **Step 1: Implement MakeVariant, MakeVariant0, MakeClosure**
- [ ] **Step 2: Implement CallBuiltin with name→symbol mapping**
- [ ] **Step 3: Implement CallReg (closure dispatch)**
- [ ] **Step 4: Add tests for variant creation, CallBuiltin, list operations**

Test: create a function that builds a list, calls `head` on it, returns the result. This exercises MakeList + CallBuiltin.

- [ ] **Step 5: Run tests and commit**

```bash
git add crates/airl-runtime/src/bytecode_jit_full.rs
git commit -m "feat(jit-full): MakeVariant, MakeClosure, CallBuiltin, CallReg support"
```

---

### Task 4: Pattern Matching — MatchTag, JumpIfNoMatch, MatchWild, TryUnwrap

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_jit_full.rs`

Pattern matching in the bytecode VM uses a `match_flag` variable. In the JIT, track this as a Cranelift variable (i64, 0 or 1).

- `MatchTag(dst, scrutinee, tag_idx)`: call `airl_match_tag(scrutinee, tag_str)`. If result is non-null, set `match_flag_var = 1` and `dst = result`. If null, set `match_flag_var = 0`.
  - Emit: `result = call airl_match_tag(scr, tag); is_null = icmp eq result, 0; brif is_null, no_match_block, match_block`
  - In match_block: `def_var(dst, result); def_var(match_flag, iconst 1); jump continue`
  - In no_match_block: `def_var(match_flag, iconst 0); jump continue`

- `JumpIfNoMatch(offset)`: `val = use_var(match_flag); brif val, fallthrough, target` (if match_flag is 0, jump)

- `MatchWild(dst, scrutinee)`: `def_var(dst, use_var(scrutinee)); def_var(match_flag, iconst 1)`

- `TryUnwrap(dst, src, err_offset)`: call `airl_match_tag(src, "Ok" string)`. If non-null, store inner in dst. If null, it's an error — for now, call `airl_runtime_error`. (The err_offset jumping to an error handler is complex; simplest initial approach: abort on Err.)

- [ ] **Step 1: Add `match_flag` variable to compile_func**
- [ ] **Step 2: Implement MatchTag with branch-on-null pattern**
- [ ] **Step 3: Implement JumpIfNoMatch, MatchWild**
- [ ] **Step 4: Implement TryUnwrap**
- [ ] **Step 5: Add tests for pattern matching**

Test: a function that takes a variant and matches on it (match arm with tag check).

- [ ] **Step 6: Run tests and commit**

```bash
git add crates/airl-runtime/src/bytecode_jit_full.rs
git commit -m "feat(jit-full): pattern matching — MatchTag, JumpIfNoMatch, MatchWild, TryUnwrap"
```

---

### Task 5: VM and Pipeline Integration

**Files:**
- Modify: `crates/airl-runtime/src/bytecode_vm.rs`
- Modify: `crates/airl-driver/src/pipeline.rs`
- Modify: `crates/airl-driver/src/main.rs`

Wire `BytecodeJitFull` into the VM and add `--jit-full` CLI flag.

- [ ] **Step 1: Add `jit_full` field to BytecodeVm**

```rust
#[cfg(feature = "jit")]
jit_full: Option<crate::bytecode_jit_full::BytecodeJitFull>,
```

Add `new_with_full_jit()`, `jit_full_compile_all()`, and dispatch in `Op::Call`.

- [ ] **Step 2: Add `run_source_jit_full()` to pipeline.rs**

Follow `run_source_jit()` pattern but use `new_with_full_jit()` and `jit_full_compile_all()`.

- [ ] **Step 3: Add `--jit-full` flag to main.rs**

- [ ] **Step 4: Smoke test**

```bash
echo '(+ 21 21)' > /tmp/jit_full_test.airl
source "$HOME/.cargo/env" && cargo run --features jit -p airl-driver -- run --jit-full /tmp/jit_full_test.airl
```
Expected: `42`

Test with list operations:
```bash
echo '(head [10 20 30])' > /tmp/jit_full_list.airl
source "$HOME/.cargo/env" && cargo run --features jit -p airl-driver -- run --jit-full /tmp/jit_full_list.airl
```
Expected: `10`

- [ ] **Step 5: Run fixture compatibility**

Compare `--jit-full` output against interpreted for all valid fixtures.

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/bytecode_vm.rs crates/airl-driver/src/pipeline.rs crates/airl-driver/src/main.rs
git commit -m "feat(driver): add --jit-full execution mode (all functions compiled via airl-rt)"
```

---

### Task 6: Full Test Suite and Documentation

- [ ] **Step 1: Run all fixture tests through --jit-full**
- [ ] **Step 2: Run workspace tests**
- [ ] **Step 3: Update CLAUDE.md**
- [ ] **Step 4: Commit**
