# Runtime Library (`libairl_rt`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a standalone Rust static library (`libairl_rt.a`) that exposes AIRL's ~48 builtin operations through a stable C ABI, enabling generated native code to call into the runtime.

**Architecture:** A new `crates/airl-rt/` crate containing a C-ABI `RtValue` struct (heap-allocated, refcounted), `extern "C"` wrappers around the existing builtin logic, and memory management functions (retain/release). The crate has zero workspace dependencies — it's a standalone runtime that generated code links against. Existing builtins in `airl-runtime/src/builtins.rs` are the reference implementation; `airl-rt` re-implements the same logic on the new `RtValue` type.

**Tech Stack:** Rust, `#[no_mangle] extern "C"`, `#[repr(C)]`, `cbindgen` (optional, for C header generation)

**Spec:** `docs/superpowers/specs/2026-03-23-self-hosting-design.md` (Step 1)

**Reference files:**
- Existing Value type: `crates/airl-runtime/src/value.rs` (23 variants — only ~10 needed for runtime)
- Existing builtins: `crates/airl-runtime/src/builtins.rs` (~1,591 lines, ~60 registered functions)
- Existing errors: `crates/airl-runtime/src/error.rs`
- Workspace: `Cargo.toml` (workspace members list)

**Design decisions:**
- `RtValue` is heap-allocated with a refcount (simple, uniform — optimize later)
- Every `extern "C"` function takes/returns `*mut RtValue`
- Errors are fatal: print message + `exit(1)` (matches current AIRL semantics)
- No workspace dependencies — `airl-rt` is self-contained so it can be linked independently
- Only builtins the bootstrap compiler actually uses are implemented (not tensors, not `run-ir`)

---

## File Structure

| File | Responsibility |
|------|---------------|
| Create: `crates/airl-rt/Cargo.toml` | Crate config, `crate-type = ["staticlib", "rlib"]` |
| Create: `crates/airl-rt/src/lib.rs` | Module declarations, public re-exports |
| Create: `crates/airl-rt/src/value.rs` | `RtValue` struct, tag constants, constructors, Display, PartialEq |
| Create: `crates/airl-rt/src/memory.rs` | `airl_value_retain`, `airl_value_release`, `airl_value_clone`, allocation |
| Create: `crates/airl-rt/src/arithmetic.rs` | `airl_add`, `airl_sub`, `airl_mul`, `airl_div`, `airl_mod` |
| Create: `crates/airl-rt/src/comparison.rs` | `airl_eq`, `airl_ne`, `airl_lt`, `airl_gt`, `airl_le`, `airl_ge` |
| Create: `crates/airl-rt/src/logic.rs` | `airl_not`, `airl_and`, `airl_or`, `airl_xor` |
| Create: `crates/airl-rt/src/list.rs` | `airl_head`, `airl_tail`, `airl_cons`, `airl_empty`, `airl_list_new`, `airl_length`, `airl_at`, `airl_append` |
| Create: `crates/airl-rt/src/string.rs` | 13 string builtins (`airl_char_at`, `airl_substring`, etc.) |
| Create: `crates/airl-rt/src/map.rs` | 10 map builtins (`airl_map_new`, `airl_map_get`, etc.) |
| Create: `crates/airl-rt/src/io.rs` | `airl_print`, `airl_type_of`, `airl_valid` |
| Create: `crates/airl-rt/src/variant.rs` | `airl_make_variant`, `airl_match_tag` |
| Create: `crates/airl-rt/src/closure.rs` | `airl_make_closure`, `airl_call_closure` |
| Create: `crates/airl-rt/src/error.rs` | `airl_runtime_error` — print + exit(1) |
| Modify: `Cargo.toml` | Add `crates/airl-rt` to workspace members |

---

### Task 1: Crate Skeleton and Value Type

**Files:**
- Create: `crates/airl-rt/Cargo.toml`
- Create: `crates/airl-rt/src/lib.rs`
- Create: `crates/airl-rt/src/value.rs`
- Create: `crates/airl-rt/src/error.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Create `crates/airl-rt/Cargo.toml`**

```toml
[package]
name = "airl-rt"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["staticlib", "rlib"]
```

- [ ] **Step 2: Add to workspace `Cargo.toml`**

Add `"crates/airl-rt"` to the `members` array.

- [ ] **Step 3: Create `crates/airl-rt/src/error.rs`**

```rust
use std::process;

/// Fatal runtime error — prints message and exits.
/// Generated code calls this for type mismatches, division by zero, etc.
#[no_mangle]
pub extern "C" fn airl_runtime_error(msg: *const u8, len: usize) -> ! {
    let slice = unsafe { std::slice::from_raw_parts(msg, len) };
    let s = std::str::from_utf8(slice).unwrap_or("<invalid utf8>");
    eprintln!("Runtime error: {}", s);
    process::exit(1);
}

/// Rust-side helper: abort with a &str message. Used by other airl-rt modules.
pub(crate) fn rt_error(msg: &str) -> ! {
    eprintln!("Runtime error: {}", msg);
    process::exit(1);
}
```

- [ ] **Step 4: Create `crates/airl-rt/src/value.rs`**

```rust
use std::collections::HashMap;
use std::fmt;

/// Tag byte identifying the value variant.
pub const TAG_NIL: u8 = 0;
pub const TAG_INT: u8 = 1;
pub const TAG_FLOAT: u8 = 2;
pub const TAG_BOOL: u8 = 3;
pub const TAG_STR: u8 = 4;
pub const TAG_LIST: u8 = 5;
pub const TAG_MAP: u8 = 6;
pub const TAG_VARIANT: u8 = 7;
pub const TAG_CLOSURE: u8 = 8;
pub const TAG_UNIT: u8 = 9;

/// Heap-allocated, refcounted runtime value.
///
/// Every AIRL value at runtime is a `*mut RtValue`.
/// Generated code and runtime builtins pass these pointers around.
/// Memory is managed by retain/release (reference counting).
pub struct RtValue {
    pub tag: u8,
    pub rc: u32,
    pub data: RtData,
}

pub enum RtData {
    Nil,
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<*mut RtValue>),
    Map(HashMap<String, *mut RtValue>),
    Variant { tag_name: String, inner: *mut RtValue },
    Closure { func_ptr: *const u8, captures: Vec<*mut RtValue> },
}

impl RtValue {
    /// Allocate a new RtValue on the heap with refcount 1.
    pub fn alloc(tag: u8, data: RtData) -> *mut RtValue {
        let val = Box::new(RtValue { tag, rc: 1, data });
        Box::into_raw(val)
    }
}

// ── Constructors (Rust-side helpers) ──────────────────────

pub fn rt_nil() -> *mut RtValue {
    RtValue::alloc(TAG_NIL, RtData::Nil)
}

pub fn rt_unit() -> *mut RtValue {
    RtValue::alloc(TAG_UNIT, RtData::Unit)
}

pub fn rt_int(n: i64) -> *mut RtValue {
    RtValue::alloc(TAG_INT, RtData::Int(n))
}

pub fn rt_float(f: f64) -> *mut RtValue {
    RtValue::alloc(TAG_FLOAT, RtData::Float(f))
}

pub fn rt_bool(b: bool) -> *mut RtValue {
    RtValue::alloc(TAG_BOOL, RtData::Bool(b))
}

pub fn rt_str(s: String) -> *mut RtValue {
    RtValue::alloc(TAG_STR, RtData::Str(s))
}

pub fn rt_list(items: Vec<*mut RtValue>) -> *mut RtValue {
    RtValue::alloc(TAG_LIST, RtData::List(items))
}

pub fn rt_map(m: HashMap<String, *mut RtValue>) -> *mut RtValue {
    RtValue::alloc(TAG_MAP, RtData::Map(m))
}

pub fn rt_variant(tag_name: String, inner: *mut RtValue) -> *mut RtValue {
    RtValue::alloc(TAG_VARIANT, RtData::Variant { tag_name, inner })
}

// ── C-ABI constructors ──────────────────────────────────

#[no_mangle]
pub extern "C" fn airl_int(n: i64) -> *mut RtValue { rt_int(n) }

#[no_mangle]
pub extern "C" fn airl_float(f: f64) -> *mut RtValue { rt_float(f) }

#[no_mangle]
pub extern "C" fn airl_bool(b: bool) -> *mut RtValue { rt_bool(b) }

#[no_mangle]
pub extern "C" fn airl_nil() -> *mut RtValue { rt_nil() }

#[no_mangle]
pub extern "C" fn airl_unit() -> *mut RtValue { rt_unit() }

/// Create a string from a pointer + length (no null terminator required).
#[no_mangle]
pub extern "C" fn airl_str(ptr: *const u8, len: usize) -> *mut RtValue {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let s = std::str::from_utf8(slice).unwrap_or("").to_string();
    rt_str(s)
}

// ── Accessors (Rust-side helpers) ────────────────────────

/// Extract i64 from an RtValue, or abort.
pub(crate) fn as_int(v: *mut RtValue) -> i64 {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Int(n) => *n,
        _ => crate::error::rt_error(&format!("expected Int, got tag {}", v.tag)),
    }
}

pub(crate) fn as_float(v: *mut RtValue) -> f64 {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Float(f) => *f,
        _ => crate::error::rt_error(&format!("expected Float, got tag {}", v.tag)),
    }
}

pub(crate) fn as_bool(v: *mut RtValue) -> bool {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Bool(b) => *b,
        _ => crate::error::rt_error(&format!("expected Bool, got tag {}", v.tag)),
    }
}

pub(crate) fn as_str(v: *mut RtValue) -> &'static str {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Str(s) => {
            // SAFETY: the RtValue is heap-allocated and lives as long as it's retained
            unsafe { std::mem::transmute::<&str, &'static str>(s.as_str()) }
        }
        _ => crate::error::rt_error(&format!("expected Str, got tag {}", v.tag)),
    }
}

pub(crate) fn as_str_owned(v: *mut RtValue) -> String {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Str(s) => s.clone(),
        _ => crate::error::rt_error(&format!("expected Str, got tag {}", v.tag)),
    }
}

pub(crate) fn as_list(v: *mut RtValue) -> &'static Vec<*mut RtValue> {
    let v = unsafe { &*v };
    match &v.data {
        RtData::List(items) => unsafe { std::mem::transmute(items) },
        _ => crate::error::rt_error(&format!("expected List, got tag {}", v.tag)),
    }
}

pub(crate) fn as_map(v: *mut RtValue) -> &'static HashMap<String, *mut RtValue> {
    let v = unsafe { &*v };
    match &v.data {
        RtData::Map(m) => unsafe { std::mem::transmute(m) },
        _ => crate::error::rt_error(&format!("expected Map, got tag {}", v.tag)),
    }
}

impl fmt::Display for RtValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            RtData::Nil => write!(f, "nil"),
            RtData::Unit => write!(f, "()"),
            RtData::Int(n) => write!(f, "{}", n),
            RtData::Float(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    write!(f, "{}.0", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            RtData::Bool(b) => write!(f, "{}", b),
            RtData::Str(s) => write!(f, "\"{}\"", s),
            RtData::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    let v = unsafe { &**item };
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            RtData::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                write!(f, "{{")?;
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    let v = unsafe { &*m[*k] };
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            RtData::Variant { tag_name, inner } => {
                let inner_v = unsafe { &**inner };
                match &inner_v.data {
                    RtData::Unit => write!(f, "({})", tag_name),
                    _ => write!(f, "({} {})", tag_name, inner_v),
                }
            }
            RtData::Closure { .. } => write!(f, "<closure>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{airl_value_release};

    #[test]
    fn int_roundtrip() {
        let v = rt_int(42);
        assert_eq!(as_int(v), 42);
        assert_eq!(format!("{}", unsafe { &*v }), "42");
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn float_roundtrip() {
        let v = rt_float(3.14);
        assert_eq!(as_float(v), 3.14);
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn bool_roundtrip() {
        let v = rt_bool(true);
        assert_eq!(as_bool(v), true);
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn str_roundtrip() {
        let v = rt_str("hello".into());
        assert_eq!(as_str_owned(v), "hello");
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn nil_display() {
        let v = rt_nil();
        assert_eq!(format!("{}", unsafe { &*v }), "nil");
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn list_display() {
        let items = vec![rt_int(1), rt_int(2), rt_int(3)];
        let v = rt_list(items);
        assert_eq!(format!("{}", unsafe { &*v }), "[1 2 3]");
        unsafe { airl_value_release(v); }
    }

    #[test]
    fn variant_display() {
        let inner = rt_int(42);
        let v = rt_variant("Ok".into(), inner);
        assert_eq!(format!("{}", unsafe { &*v }), "(Ok 42)");
        unsafe { airl_value_release(v); }
    }
}
```

- [ ] **Step 5: Create `crates/airl-rt/src/lib.rs`**

```rust
pub mod value;
pub mod memory;
pub mod error;
pub mod arithmetic;
pub mod comparison;
pub mod logic;
pub mod list;
pub mod string;
pub mod map;
pub mod io;
pub mod variant;
pub mod closure;
```

Note: modules `memory` through `closure` don't exist yet. Create them as empty files with `// TODO` so `lib.rs` compiles. They'll be filled in subsequent tasks.

- [ ] **Step 6: Create stub files for remaining modules**

Create each of these as empty module files:
- `crates/airl-rt/src/memory.rs`
- `crates/airl-rt/src/arithmetic.rs`
- `crates/airl-rt/src/comparison.rs`
- `crates/airl-rt/src/logic.rs`
- `crates/airl-rt/src/list.rs`
- `crates/airl-rt/src/string.rs`
- `crates/airl-rt/src/map.rs`
- `crates/airl-rt/src/io.rs`
- `crates/airl-rt/src/variant.rs`
- `crates/airl-rt/src/closure.rs`

Each one needs at minimum a stub for `airl_value_release` (used in value.rs tests) — but that goes in `memory.rs` (Step 7).

- [ ] **Step 7: Create `crates/airl-rt/src/memory.rs`**

```rust
use crate::value::*;

/// Increment refcount.
#[no_mangle]
pub extern "C" fn airl_value_retain(v: *mut RtValue) {
    if v.is_null() { return; }
    let val = unsafe { &mut *v };
    val.rc += 1;
}

/// Decrement refcount. Frees when it reaches zero.
/// Recursively releases nested values (list items, map values, variant inners).
#[no_mangle]
pub extern "C" fn airl_value_release(v: *mut RtValue) {
    if v.is_null() { return; }
    let val = unsafe { &mut *v };
    if val.rc == 0 {
        return; // already freed or static — don't double-free
    }
    val.rc -= 1;
    if val.rc > 0 {
        return; // still referenced
    }
    // Refcount hit zero — release nested values and free
    match &val.data {
        RtData::List(items) => {
            for &item in items.iter() {
                airl_value_release(item);
            }
        }
        RtData::Map(m) => {
            for (_, &v) in m.iter() {
                airl_value_release(v);
            }
        }
        RtData::Variant { inner, .. } => {
            airl_value_release(*inner);
        }
        RtData::Closure { captures, .. } => {
            for &cap in captures.iter() {
                airl_value_release(cap);
            }
        }
        _ => {} // primitives, strings — no nested values
    }
    // Reclaim memory
    unsafe { drop(Box::from_raw(v)); }
}

/// Clone a value (deep copy with fresh refcount of 1).
#[no_mangle]
pub extern "C" fn airl_value_clone(v: *mut RtValue) -> *mut RtValue {
    if v.is_null() { return rt_nil(); }
    let val = unsafe { &*v };
    match &val.data {
        RtData::Nil => rt_nil(),
        RtData::Unit => rt_unit(),
        RtData::Int(n) => rt_int(*n),
        RtData::Float(f) => rt_float(*f),
        RtData::Bool(b) => rt_bool(*b),
        RtData::Str(s) => rt_str(s.clone()),
        RtData::List(items) => {
            let cloned: Vec<*mut RtValue> = items.iter().map(|&item| {
                airl_value_retain(item);
                item
            }).collect();
            rt_list(cloned)
        }
        RtData::Map(m) => {
            let cloned: std::collections::HashMap<String, *mut RtValue> = m.iter().map(|(k, &v)| {
                airl_value_retain(v);
                (k.clone(), v)
            }).collect();
            rt_map(cloned)
        }
        RtData::Variant { tag_name, inner } => {
            airl_value_retain(*inner);
            rt_variant(tag_name.clone(), *inner)
        }
        RtData::Closure { func_ptr, captures } => {
            let cloned: Vec<*mut RtValue> = captures.iter().map(|&cap| {
                airl_value_retain(cap);
                cap
            }).collect();
            RtValue::alloc(TAG_CLOSURE, RtData::Closure { func_ptr: *func_ptr, captures: cloned })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retain_release_basic() {
        let v = rt_int(42);
        assert_eq!(unsafe { (*v).rc }, 1);
        airl_value_retain(v);
        assert_eq!(unsafe { (*v).rc }, 2);
        airl_value_release(v);
        assert_eq!(unsafe { (*v).rc }, 1);
        airl_value_release(v); // frees
    }

    #[test]
    fn clone_int() {
        let v = rt_int(99);
        let v2 = airl_value_clone(v);
        assert_eq!(crate::value::as_int(v2), 99);
        assert_eq!(unsafe { (*v2).rc }, 1); // independent
        airl_value_release(v);
        airl_value_release(v2);
    }

    #[test]
    fn release_null_safe() {
        airl_value_release(std::ptr::null_mut()); // should not crash
    }

    #[test]
    fn clone_list_retains_items() {
        let a = rt_int(1);
        let b = rt_int(2);
        let list = rt_list(vec![a, b]);
        let list2 = airl_value_clone(list);
        // Items are shared (retained), not deep-copied
        assert_eq!(unsafe { (*a).rc }, 2); // list + list2
        airl_value_release(list);
        assert_eq!(unsafe { (*a).rc }, 1); // only list2
        airl_value_release(list2);
    }
}
```

- [ ] **Step 8: Build the crate**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-rt 2>&1 | tail -5`
Expected: Build succeeds

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add crates/airl-rt/ Cargo.toml
git commit -m "feat(rt): create airl-rt crate with RtValue, refcounting, and error handling"
```

---

### Task 2: Arithmetic and Comparison

**Files:**
- Modify: `crates/airl-rt/src/arithmetic.rs`
- Modify: `crates/airl-rt/src/comparison.rs`

- [ ] **Step 1: Implement `arithmetic.rs`**

Port from `builtins.rs:builtin_add` etc. Key difference: operates on `*mut RtValue` not `&Value`.

```rust
use crate::value::*;
use crate::error::rt_error;

#[no_mangle]
pub extern "C" fn airl_add(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_add(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x + y),
        (RtData::Str(x), RtData::Str(y)) => rt_str(format!("{}{}", x, y)),
        _ => rt_error(&format!("+ type mismatch: tags {} and {}", va.tag, vb.tag)),
    }
}

#[no_mangle]
pub extern "C" fn airl_sub(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_sub(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x - y),
        _ => rt_error(&format!("- type mismatch: tags {} and {}", va.tag, vb.tag)),
    }
}

#[no_mangle]
pub extern "C" fn airl_mul(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_mul(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x * y),
        _ => rt_error(&format!("* type mismatch: tags {} and {}", va.tag, vb.tag)),
    }
}

#[no_mangle]
pub extern "C" fn airl_div(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(_, ), RtData::Int(0)) => rt_error("division by zero"),
        (RtData::Int(x), RtData::Int(y)) => rt_int(x / y),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x / y),
        _ => rt_error(&format!("/ type mismatch: tags {} and {}", va.tag, vb.tag)),
    }
}

#[no_mangle]
pub extern "C" fn airl_mod(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(_), RtData::Int(0)) => rt_error("modulo by zero"),
        (RtData::Int(x), RtData::Int(y)) => rt_int(x % y),
        _ => rt_error(&format!("% type mismatch: tags {} and {}", va.tag, vb.tag)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;

    #[test]
    fn add_ints() {
        let r = airl_add(rt_int(3), rt_int(4));
        assert_eq!(as_int(r), 7);
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn add_floats() {
        let r = airl_add(rt_float(1.5), rt_float(2.5));
        assert_eq!(as_float(r), 4.0);
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn add_strings() {
        let r = airl_add(rt_str("hello".into()), rt_str(" world".into()));
        assert_eq!(as_str_owned(r), "hello world");
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn sub_ints() {
        let r = airl_sub(rt_int(10), rt_int(3));
        assert_eq!(as_int(r), 7);
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn mul_ints() {
        let r = airl_mul(rt_int(6), rt_int(7));
        assert_eq!(as_int(r), 42);
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn div_ints() {
        let r = airl_div(rt_int(10), rt_int(3));
        assert_eq!(as_int(r), 3);
        unsafe { airl_value_release(r); }
    }

    #[test]
    fn mod_ints() {
        let r = airl_mod(rt_int(10), rt_int(3));
        assert_eq!(as_int(r), 1);
        unsafe { airl_value_release(r); }
    }
}
```

- [ ] **Step 2: Implement `comparison.rs`**

```rust
use crate::value::*;
use crate::error::rt_error;

#[no_mangle]
pub extern "C" fn airl_eq(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(rt_values_equal(a, b))
}

#[no_mangle]
pub extern "C" fn airl_ne(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(!rt_values_equal(a, b))
}

fn rt_values_equal(a: *mut RtValue, b: *mut RtValue) -> bool {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => x == y,
        (RtData::Float(x), RtData::Float(y)) => x.to_bits() == y.to_bits(),
        (RtData::Bool(x), RtData::Bool(y)) => x == y,
        (RtData::Str(x), RtData::Str(y)) => x == y,
        (RtData::Nil, RtData::Nil) => true,
        (RtData::Unit, RtData::Unit) => true,
        (RtData::List(xs), RtData::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(&x, &y)| rt_values_equal(x, y))
        }
        (RtData::Variant { tag_name: t1, inner: i1 }, RtData::Variant { tag_name: t2, inner: i2 }) => {
            t1 == t2 && rt_values_equal(*i1, *i2)
        }
        _ => false,
    }
}

#[no_mangle]
pub extern "C" fn airl_lt(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    let result = match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => x < y,
        (RtData::Float(x), RtData::Float(y)) => x < y,
        (RtData::Str(x), RtData::Str(y)) => x < y,
        _ => rt_error(&format!("< type mismatch: tags {} and {}", va.tag, vb.tag)),
    };
    rt_bool(result)
}

#[no_mangle]
pub extern "C" fn airl_gt(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    let result = match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => x > y,
        (RtData::Float(x), RtData::Float(y)) => x > y,
        (RtData::Str(x), RtData::Str(y)) => x > y,
        _ => rt_error(&format!("> type mismatch: tags {} and {}", va.tag, vb.tag)),
    };
    rt_bool(result)
}

#[no_mangle]
pub extern "C" fn airl_le(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    let result = match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => x <= y,
        (RtData::Float(x), RtData::Float(y)) => x <= y,
        _ => rt_error(&format!("<= type mismatch: tags {} and {}", va.tag, vb.tag)),
    };
    rt_bool(result)
}

#[no_mangle]
pub extern "C" fn airl_ge(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    let result = match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => x >= y,
        (RtData::Float(x), RtData::Float(y)) => x >= y,
        _ => rt_error(&format!(">= type mismatch: tags {} and {}", va.tag, vb.tag)),
    };
    rt_bool(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;

    #[test]
    fn eq_ints() { assert_eq!(as_bool(airl_eq(rt_int(1), rt_int(1))), true); }

    #[test]
    fn ne_ints() { assert_eq!(as_bool(airl_ne(rt_int(1), rt_int(2))), true); }

    #[test]
    fn lt_ints() { assert_eq!(as_bool(airl_lt(rt_int(1), rt_int(2))), true); }

    #[test]
    fn ge_ints() { assert_eq!(as_bool(airl_ge(rt_int(2), rt_int(2))), true); }

    #[test]
    fn eq_strings() { assert_eq!(as_bool(airl_eq(rt_str("a".into()), rt_str("a".into()))), true); }

    #[test]
    fn eq_lists() {
        let a = rt_list(vec![rt_int(1), rt_int(2)]);
        let b = rt_list(vec![rt_int(1), rt_int(2)]);
        assert_eq!(as_bool(airl_eq(a, b)), true);
    }

    #[test]
    fn eq_variants() {
        let a = rt_variant("Ok".into(), rt_int(1));
        let b = rt_variant("Ok".into(), rt_int(1));
        assert_eq!(as_bool(airl_eq(a, b)), true);

        let c = rt_variant("Err".into(), rt_int(1));
        assert_eq!(as_bool(airl_eq(a, c)), false);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-rt/src/arithmetic.rs crates/airl-rt/src/comparison.rs
git commit -m "feat(rt): arithmetic and comparison builtins with C ABI"
```

---

### Task 3: Logic, List, and I/O

**Files:**
- Modify: `crates/airl-rt/src/logic.rs`
- Modify: `crates/airl-rt/src/list.rs`
- Modify: `crates/airl-rt/src/io.rs`

- [ ] **Step 1: Implement `logic.rs`**

```rust
use crate::value::*;

#[no_mangle]
pub extern "C" fn airl_not(a: *mut RtValue) -> *mut RtValue {
    rt_bool(!as_bool(a))
}

#[no_mangle]
pub extern "C" fn airl_and(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) && as_bool(b))
}

#[no_mangle]
pub extern "C" fn airl_or(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) || as_bool(b))
}

#[no_mangle]
pub extern "C" fn airl_xor(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) ^ as_bool(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_true() { assert_eq!(as_bool(airl_not(rt_bool(true))), false); }

    #[test]
    fn and_tt() { assert_eq!(as_bool(airl_and(rt_bool(true), rt_bool(true))), true); }

    #[test]
    fn or_tf() { assert_eq!(as_bool(airl_or(rt_bool(true), rt_bool(false))), true); }

    #[test]
    fn xor_tf() { assert_eq!(as_bool(airl_xor(rt_bool(true), rt_bool(false))), true); }
}
```

- [ ] **Step 2: Implement `list.rs`**

```rust
use crate::value::*;
use crate::memory::*;
use crate::error::rt_error;

#[no_mangle]
pub extern "C" fn airl_head(list: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    if items.is_empty() {
        rt_error("head: empty list");
    }
    airl_value_retain(items[0]);
    items[0]
}

#[no_mangle]
pub extern "C" fn airl_tail(list: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    if items.is_empty() {
        rt_error("tail: empty list");
    }
    let tail: Vec<*mut RtValue> = items[1..].iter().map(|&item| {
        airl_value_retain(item);
        item
    }).collect();
    rt_list(tail)
}

#[no_mangle]
pub extern "C" fn airl_cons(elem: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    let mut new_items = Vec::with_capacity(items.len() + 1);
    airl_value_retain(elem);
    new_items.push(elem);
    for &item in items.iter() {
        airl_value_retain(item);
        new_items.push(item);
    }
    rt_list(new_items)
}

#[no_mangle]
pub extern "C" fn airl_empty(list: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    rt_bool(items.is_empty())
}

/// Create a list from a pointer to N `*mut RtValue`s.
/// Called by generated code for list literals: `[1 2 3]` → `airl_list_new(ptr, 3)`
#[no_mangle]
pub extern "C" fn airl_list_new(items: *const *mut RtValue, count: usize) -> *mut RtValue {
    let slice = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(items, count) }
    };
    let owned: Vec<*mut RtValue> = slice.iter().map(|&item| {
        airl_value_retain(item);
        item
    }).collect();
    rt_list(owned)
}

#[no_mangle]
pub extern "C" fn airl_length(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::List(items) => rt_int(items.len() as i64),
        RtData::Str(s) => rt_int(s.chars().count() as i64),
        RtData::Map(m) => rt_int(m.len() as i64),
        _ => rt_error(&format!("length: unsupported type tag {}", val.tag)),
    }
}

#[no_mangle]
pub extern "C" fn airl_at(list: *mut RtValue, index: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    let i = as_int(index) as usize;
    if i >= items.len() {
        rt_error(&format!("at: index {} out of bounds (len {})", i, items.len()));
    }
    airl_value_retain(items[i]);
    items[i]
}

#[no_mangle]
pub extern "C" fn airl_append(list: *mut RtValue, elem: *mut RtValue) -> *mut RtValue {
    let items = as_list(list);
    let mut new_items: Vec<*mut RtValue> = items.iter().map(|&item| {
        airl_value_retain(item);
        item
    }).collect();
    airl_value_retain(elem);
    new_items.push(elem);
    rt_list(new_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_tail() {
        let list = rt_list(vec![rt_int(1), rt_int(2), rt_int(3)]);
        assert_eq!(as_int(airl_head(list)), 1);
        let t = airl_tail(list);
        assert_eq!(as_int(airl_head(t)), 2);
    }

    #[test]
    fn cons_prepends() {
        let list = rt_list(vec![rt_int(2), rt_int(3)]);
        let r = airl_cons(rt_int(1), list);
        assert_eq!(as_int(airl_head(r)), 1);
    }

    #[test]
    fn empty_check() {
        assert_eq!(as_bool(airl_empty(rt_list(vec![]))), true);
        assert_eq!(as_bool(airl_empty(rt_list(vec![rt_int(1)]))), false);
    }

    #[test]
    fn length_list() {
        let list = rt_list(vec![rt_int(1), rt_int(2)]);
        assert_eq!(as_int(airl_length(list)), 2);
    }

    #[test]
    fn length_str() {
        assert_eq!(as_int(airl_length(rt_str("hello".into()))), 5);
    }

    #[test]
    fn at_index() {
        let list = rt_list(vec![rt_int(10), rt_int(20), rt_int(30)]);
        assert_eq!(as_int(airl_at(list, rt_int(1))), 20);
    }

    #[test]
    fn append_elem() {
        let list = rt_list(vec![rt_int(1)]);
        let r = airl_append(list, rt_int(2));
        assert_eq!(as_int(airl_at(r, rt_int(1))), 2);
    }
}
```

- [ ] **Step 3: Implement `io.rs`**

```rust
use crate::value::*;

#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    // Print without quotes for strings (matches AIRL print semantics)
    match &val.data {
        RtData::Str(s) => println!("{}", s),
        _ => println!("{}", val),
    }
    rt_nil()
}

#[no_mangle]
pub extern "C" fn airl_type_of(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let name = match &val.data {
        RtData::Nil => "nil",
        RtData::Unit => "unit",
        RtData::Int(_) => "int",
        RtData::Float(_) => "float",
        RtData::Bool(_) => "bool",
        RtData::Str(_) => "string",
        RtData::List(_) => "list",
        RtData::Map(_) => "map",
        RtData::Variant { .. } => "variant",
        RtData::Closure { .. } => "closure",
    };
    rt_str(name.to_string())
}

#[no_mangle]
pub extern "C" fn airl_valid(v: *mut RtValue) -> *mut RtValue {
    // `valid` returns true for any non-nil value
    let val = unsafe { &*v };
    rt_bool(!matches!(&val.data, RtData::Nil))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_of_int() {
        assert_eq!(as_str_owned(airl_type_of(rt_int(1))), "int");
    }

    #[test]
    fn type_of_str() {
        assert_eq!(as_str_owned(airl_type_of(rt_str("hi".into()))), "string");
    }

    #[test]
    fn valid_non_nil() {
        assert_eq!(as_bool(airl_valid(rt_int(1))), true);
    }

    #[test]
    fn valid_nil() {
        assert_eq!(as_bool(airl_valid(rt_nil())), false);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-rt/src/logic.rs crates/airl-rt/src/list.rs crates/airl-rt/src/io.rs
git commit -m "feat(rt): logic, list, and I/O builtins"
```

---

### Task 4: String Builtins

**Files:**
- Modify: `crates/airl-rt/src/string.rs`

Port all 13 string builtins from `builtins.rs`. Each takes/returns `*mut RtValue`.

- [ ] **Step 1: Implement all 13 string functions**

Implement: `airl_char_at`, `airl_substring`, `airl_chars`, `airl_split`, `airl_join`, `airl_contains`, `airl_starts_with`, `airl_ends_with`, `airl_index_of`, `airl_trim`, `airl_to_upper`, `airl_to_lower`, `airl_replace`.

Follow the pattern from `builtins.rs` (lines ~600-900), using `as_str_owned()` / `as_int()` to extract args and `rt_str()` / `rt_int()` / `rt_bool()` / `rt_list()` to construct results. Each function is `#[no_mangle] pub extern "C"`.

Reference: `crates/airl-runtime/src/builtins.rs` lines 600-900 for exact semantics (Unicode char indexing, not byte indexing).

- [ ] **Step 2: Add tests for each function**

At minimum: `char_at` with ASCII and Unicode, `substring`, `split`+`join` roundtrip, `contains`, `starts_with`, `ends_with`, `trim`, `to_upper`, `to_lower`, `replace`, `index_of`, `chars`.

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt string 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-rt/src/string.rs
git commit -m "feat(rt): 13 string builtins with C ABI"
```

---

### Task 5: Map Builtins

**Files:**
- Modify: `crates/airl-rt/src/map.rs`

Port all 10 map builtins.

- [ ] **Step 1: Implement all 10 map functions**

Implement: `airl_map_new`, `airl_map_from`, `airl_map_get`, `airl_map_get_or`, `airl_map_set`, `airl_map_has`, `airl_map_remove`, `airl_map_keys`, `airl_map_values`, `airl_map_size`.

`airl_map_from` takes a list of `[key value]` pairs (list of 2-element lists).
`airl_map_set`/`airl_map_remove` return new maps (immutable semantics — clone + modify).
Keys are always strings (extracted via `as_str_owned`).

Reference: `crates/airl-runtime/src/builtins.rs` lines ~900-1100.

- [ ] **Step 2: Add tests**

At minimum: create empty map, set/get roundtrip, has/size, remove, keys/values, map_from pairs.

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt map 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-rt/src/map.rs
git commit -m "feat(rt): 10 map builtins with C ABI"
```

---

### Task 6: Variant and Closure

**Files:**
- Modify: `crates/airl-rt/src/variant.rs`
- Modify: `crates/airl-rt/src/closure.rs`

- [ ] **Step 1: Implement `variant.rs`**

```rust
use crate::value::*;
use crate::memory::*;

/// Create a variant value: (Ok 42) → airl_make_variant("Ok", rt_int(42))
/// `tag` is a string RtValue. `inner` is the wrapped value.
#[no_mangle]
pub extern "C" fn airl_make_variant(tag: *mut RtValue, inner: *mut RtValue) -> *mut RtValue {
    let tag_name = as_str_owned(tag);
    airl_value_retain(inner);
    rt_variant(tag_name, inner)
}

/// Match a variant's tag. Returns the inner value if tag matches, null if not.
/// Used by generated code for pattern matching: check tag, extract inner.
#[no_mangle]
pub extern "C" fn airl_match_tag(val: *mut RtValue, expected_tag: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*val };
    let expected = as_str_owned(expected_tag);
    match &v.data {
        RtData::Variant { tag_name, inner } if tag_name == &expected => {
            airl_value_retain(*inner);
            *inner
        }
        _ => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_and_match() {
        let v = airl_make_variant(rt_str("Ok".into()), rt_int(42));
        let inner = airl_match_tag(v, rt_str("Ok".into()));
        assert!(!inner.is_null());
        assert_eq!(as_int(inner), 42);
    }

    #[test]
    fn match_wrong_tag() {
        let v = airl_make_variant(rt_str("Ok".into()), rt_int(42));
        let inner = airl_match_tag(v, rt_str("Err".into()));
        assert!(inner.is_null());
    }

    #[test]
    fn nullary_variant() {
        let v = airl_make_variant(rt_str("None".into()), rt_unit());
        assert_eq!(format!("{}", unsafe { &*v }), "(None)");
    }
}
```

- [ ] **Step 2: Implement `closure.rs`**

```rust
use crate::value::*;
use crate::memory::*;
use crate::error::rt_error;

/// Create a closure: function pointer + captured values.
/// `func_ptr` is the native code address of the compiled function.
/// `captures` is a pointer to N captured `*mut RtValue`s.
#[no_mangle]
pub extern "C" fn airl_make_closure(
    func_ptr: *const u8,
    captures: *const *mut RtValue,
    capture_count: usize,
) -> *mut RtValue {
    let caps = if capture_count == 0 {
        vec![]
    } else {
        let slice = unsafe { std::slice::from_raw_parts(captures, capture_count) };
        slice.iter().map(|&cap| {
            airl_value_retain(cap);
            cap
        }).collect()
    };
    RtValue::alloc(TAG_CLOSURE, RtData::Closure { func_ptr, captures: caps })
}

/// Call a closure with arguments.
/// The closure's function expects (captures..., args...) as parameters.
/// This function extracts the captures and calls through the function pointer.
///
/// For now, this supports closures with up to 8 total parameters (captures + args).
/// The calling convention: all parameters are *mut RtValue, return is *mut RtValue.
#[no_mangle]
pub extern "C" fn airl_call_closure(
    closure: *mut RtValue,
    args: *const *mut RtValue,
    argc: usize,
) -> *mut RtValue {
    let val = unsafe { &*closure };
    let (func_ptr, captures) = match &val.data {
        RtData::Closure { func_ptr, captures } => (*func_ptr, captures),
        _ => rt_error("call_closure: not a closure"),
    };

    // Build full parameter list: captures first, then args
    let arg_slice = if argc == 0 { &[] } else {
        unsafe { std::slice::from_raw_parts(args, argc) }
    };
    let total = captures.len() + argc;
    let mut all_args: Vec<*mut RtValue> = Vec::with_capacity(total);
    all_args.extend_from_slice(captures);
    all_args.extend_from_slice(arg_slice);

    // Dispatch by arity (same pattern as bytecode_jit try_call_native)
    type F0 = fn() -> *mut RtValue;
    type F1 = fn(*mut RtValue) -> *mut RtValue;
    type F2 = fn(*mut RtValue, *mut RtValue) -> *mut RtValue;
    type F3 = fn(*mut RtValue, *mut RtValue, *mut RtValue) -> *mut RtValue;
    type F4 = fn(*mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue) -> *mut RtValue;
    // ... extend as needed

    unsafe {
        match total {
            0 => { let f: F0 = std::mem::transmute(func_ptr); f() }
            1 => { let f: F1 = std::mem::transmute(func_ptr); f(all_args[0]) }
            2 => { let f: F2 = std::mem::transmute(func_ptr); f(all_args[0], all_args[1]) }
            3 => { let f: F3 = std::mem::transmute(func_ptr); f(all_args[0], all_args[1], all_args[2]) }
            4 => { let f: F4 = std::mem::transmute(func_ptr); f(all_args[0], all_args[1], all_args[2], all_args[3]) }
            _ => rt_error(&format!("call_closure: arity {} not yet supported", total)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Closure tests require real function pointers, so we test the construction path
    #[test]
    fn make_closure_no_captures() {
        let c = airl_make_closure(std::ptr::null(), std::ptr::null(), 0);
        let val = unsafe { &*c };
        assert_eq!(val.tag, TAG_CLOSURE);
        airl_value_release(c);
    }

    #[test]
    fn make_closure_with_captures() {
        let cap = rt_int(42);
        let caps = [cap];
        let c = airl_make_closure(std::ptr::null(), caps.as_ptr(), 1);
        // Captured value should be retained
        assert_eq!(unsafe { (*cap).rc }, 2);
        airl_value_release(c);
        assert_eq!(unsafe { (*cap).rc }, 1);
        airl_value_release(cap);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-rt/src/variant.rs crates/airl-rt/src/closure.rs
git commit -m "feat(rt): variant construction/matching and closure support"
```

---

### Task 7: Build Static Library and Integration Test

**Files:**
- No new files — verify the static library builds and all exports are present

- [ ] **Step 1: Build the static library**

Run: `source "$HOME/.cargo/env" && cargo build -p airl-rt --release 2>&1 | tail -5`
Expected: Build succeeds

Verify the .a file exists:
Run: `ls -lh target/release/libairl_rt.a`
Expected: File exists (size ~500KB-2MB)

- [ ] **Step 2: Verify all exported symbols**

Run: `nm target/release/libairl_rt.a 2>/dev/null | grep ' T airl_' | sort`
Expected: All `airl_*` symbols are present (add, sub, mul, div, mod, eq, ne, lt, gt, le, ge, not, and, or, xor, head, tail, cons, empty, list_new, length, at, append, char_at, substring, chars, split, join, contains, starts_with, ends_with, index_of, trim, to_upper, to_lower, replace, map_new, map_from, map_get, map_get_or, map_set, map_has, map_remove, map_keys, map_values, map_size, print, type_of, valid, make_variant, match_tag, make_closure, call_closure, value_retain, value_release, value_clone, int, float, bool, nil, unit, str, runtime_error)

- [ ] **Step 3: Run full test suite**

Run: `source "$HOME/.cargo/env" && cargo test -p airl-rt 2>&1 | grep "test result"`
Expected: All tests pass

Also verify workspace still passes:
Run: `source "$HOME/.cargo/env" && cargo test --workspace --exclude airl-mlir 2>&1 | grep -E "FAILED|^test result" | tail -5`
Expected: No failures

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(rt): complete libairl_rt runtime library with all builtins"
```

Plan complete and saved to `docs/superpowers/plans/2026-03-23-runtime-library.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?