# Tensor JIT Compilation Design

**Date:** 2026-03-21
**Status:** Approved
**Depends on:** Cranelift JIT (scalar ops working, 412 tests)

---

## Overview

Extend the Cranelift JIT to compile tensor operations. Three builtins — `tensor.add`, `tensor.mul`, `tensor.matmul` — get native loop implementations instead of interpreted Rust code. The compiled loops operate on raw `f64` pointers with runtime shape parameters (generic, not shape-specialized).

---

## 1. What Gets Compiled

| Builtin | Loop Structure | Cranelift IR |
|---|---|---|
| `tensor.add(a, b)` | Flat: `out[i] = a[i] + b[i]` | Counted loop, load/fadd/store |
| `tensor.mul(a, b)` | Flat: `out[i] = a[i] * b[i]` | Counted loop, load/fmul/store |
| `tensor.matmul(a, b)` | Triple: `out[i][j] += a[i][k] * b[k][j]` | Three nested loops, load/fmul/fadd/store |

All other tensor ops stay interpreted.

---

## 2. Calling Convention

Compiled functions take raw pointers and dimensions:

**Element-wise (add, mul):**
```
fn tensor_add(a_ptr: i64, b_ptr: i64, out_ptr: i64, len: i64)
```
- `a_ptr`, `b_ptr`: input data pointers (`Vec<f64>::as_ptr() as i64`)
- `out_ptr`: output data pointer (pre-allocated `Vec<f64>::as_mut_ptr() as i64`)
- `len`: number of elements

**Matmul:**
```
fn tensor_matmul(a_ptr: i64, b_ptr: i64, out_ptr: i64, m: i64, k: i64, n: i64)
```
- `a_ptr`: [M,K] matrix, row-major
- `b_ptr`: [K,N] matrix, row-major
- `out_ptr`: [M,N] output, pre-allocated and zeroed
- `m`, `k`, `n`: dimensions

---

## 3. Cranelift IR Structure

### Element-wise loop
```
entry(a_ptr, b_ptr, out_ptr, len):
    i = iconst 0
    jump loop_header(i)

loop_header(i):
    cmp = icmp sge i, len
    brif cmp, exit, loop_body

loop_body:
    offset = imul i, iconst(8)
    a_addr = iadd a_ptr, offset
    b_addr = iadd b_ptr, offset
    out_addr = iadd out_ptr, offset
    a_val = load.f64 a_addr
    b_val = load.f64 b_addr
    result = fadd a_val, b_val    ;; or fmul for tensor.mul
    store result, out_addr
    i_next = iadd i, iconst(1)
    jump loop_header(i_next)

exit:
    return
```

### Matmul triple loop
```
entry(a_ptr, b_ptr, out_ptr, m, k, n):
    ;; Zero output (m*n elements)
    ;; ... zeroing loop ...

    i = 0
    i_loop:
        if i >= m: exit
        j = 0
        j_loop:
            if j >= n: i_next
            sum = f64const(0.0)
            p = 0
            k_loop:
                if p >= k: store_result
                a_offset = (i*k + p) * 8
                b_offset = (p*n + j) * 8
                a_val = load.f64 [a_ptr + a_offset]
                b_val = load.f64 [b_ptr + b_offset]
                sum = fadd(sum, fmul(a_val, b_val))
                p = p + 1
                jump k_loop
            store_result:
                out_offset = (i*n + j) * 8
                store sum, [out_ptr + out_offset]
                j = j + 1
                jump j_loop
        i_next:
            i = i + 1
            jump i_loop
    exit:
        return
```

---

## 4. TensorJit Struct

New file: `crates/airl-codegen/src/tensor_ops.rs`

```rust
pub struct TensorJit {
    module: JITModule,
    add_fn: Option<*const u8>,
    mul_fn: Option<*const u8>,
    matmul_fn: Option<*const u8>,
}
```

Each op is compiled lazily on first use. Function pointers are cached.

### Public API

```rust
impl TensorJit {
    pub fn new() -> Result<Self, String>;

    /// Try to execute a tensor op via JIT.
    /// Returns Ok(Some(result)) if JIT succeeded.
    /// Returns Ok(None) if op not supported by JIT.
    pub fn try_call(&mut self, op: &str, args: &[Value]) -> Result<Option<Value>, RuntimeError>;
}
```

### Wrapper flow (e.g., tensor.add)

1. Extract `TensorValue` from `args[0]` and `args[1]`
2. Validate shapes match
3. Allocate output `Vec<f64>` of same length
4. Get pointers: `a.data.as_ptr() as i64`, `b.data.as_ptr() as i64`, `out.as_mut_ptr() as i64`
5. Call compiled `tensor_add(a_ptr, b_ptr, out_ptr, len as i64)`
6. Wrap output in `TensorValue` with same shape/dtype

---

## 5. Integration with Interpreter

### Changes to eval.rs

Add `tensor_jit: Option<TensorJit>` to `Interpreter`, initialized in `new()`.

In the `FnCall` arm, when the callee is a tensor builtin, try JIT first:

```rust
Value::BuiltinFn(ref name) if name.starts_with("tensor.") => {
    // Try JIT path
    if let Some(ref mut tjit) = self.tensor_jit {
        if let Some(result) = tjit.try_call(name, &arg_vals)? {
            return Ok(result); // or set result variable
        }
    }
    // Fall back to interpreted builtin
    let f = self.builtins.get(name).ok_or_else(|| ...)?;
    f(&arg_vals)
}
```

---

## 6. Testing

### Unit tests (tensor_ops.rs)

- Compile and call `tensor_add` with known values → verify element-wise sum
- Compile and call `tensor_mul` → verify element-wise product
- Compile and call `tensor_matmul` with 2×3 × 3×2 → verify against known result
- Edge cases: empty tensors (len=0), single element, large tensors

### Integration tests

- `tensor.add` through full AIRL pipeline produces same result with/without JIT
- `tensor.matmul` through full pipeline matches interpreted result
- Interpreter with `tensor_jit: None` still works (fallback)

### Correctness fixture

`tests/fixtures/valid/tensor_jit.airl`:
```clojure
;; EXPECT: tensor output
;; Tests tensor ops go through JIT transparently
(let (a : tensor (tensor.ones f32 [3 3]))
  (let (b : tensor (tensor.ones f32 [3 3]))
    (tensor.add a b)))
```

---

## 7. Files

| File | Change |
|---|---|
| `crates/airl-codegen/src/tensor_ops.rs` | **NEW** — TensorJit, loop generation, compilation |
| `crates/airl-codegen/src/lib.rs` | Export tensor_ops module |
| `crates/airl-runtime/src/eval.rs` | Add tensor_jit field, check in FnCall |

---

## 8. Not In Scope

- SIMD vectorization
- Shape-specialized compilation
- Loop tiling/blocking for cache efficiency
- Tensor ops beyond add/mul/matmul
- GPU compilation
