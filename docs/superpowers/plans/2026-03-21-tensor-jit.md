# Tensor JIT Compilation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compile tensor.add, tensor.mul, and tensor.matmul to native loops via Cranelift, replacing interpreted Rust implementations for these hot-path operations.

**Architecture:** New `tensor_ops.rs` in `airl-codegen` generates Cranelift IR loops operating on raw `f64` pointers. `TensorJit` struct compiles lazily and caches. Interpreter checks TensorJit before falling back to interpreted builtins.

**Tech Stack:** Rust, existing Cranelift dependencies in airl-codegen.

**Spec:** `docs/superpowers/specs/2026-03-21-tensor-jit-design.md`

---

## File Map

```
crates/
├── airl-codegen/src/
│   ├── tensor_ops.rs       # NEW — TensorJit, loop codegen, calling
│   └── lib.rs              # MODIFY — export tensor_ops
│
├── airl-runtime/src/
│   └── eval.rs             # MODIFY — add tensor_jit, check in FnCall
```

---

## Task 1: Element-wise Loop Codegen (`tensor_ops.rs`)

**Files:**
- Create: `crates/airl-codegen/src/tensor_ops.rs`
- Modify: `crates/airl-codegen/src/lib.rs`

This task implements the TensorJit struct and element-wise operations (add, mul). Matmul comes in Task 2.

- [ ] **Step 1: Create TensorJit struct and element-wise compilation**

```rust
use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, MemFlags};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use airl_runtime::value::Value;
use airl_runtime::tensor::TensorValue;
use airl_runtime::error::RuntimeError;
use airl_types::ty::PrimTy;

pub struct TensorJit {
    module: JITModule,
    add_fn: Option<*const u8>,
    mul_fn: Option<*const u8>,
    matmul_fn: Option<*const u8>,
}

impl TensorJit {
    pub fn new() -> Result<Self, String> {
        let builder = cranelift_jit::JITBuilder::new(
            cranelift_module::default_libcall_names()
        ).map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            add_fn: None,
            mul_fn: None,
            matmul_fn: None,
        })
    }

    pub fn try_call(&mut self, op: &str, args: &[Value]) -> Result<Option<Value>, RuntimeError> {
        match op {
            "tensor.add" => self.call_elementwise(args, false),
            "tensor.mul" => self.call_elementwise(args, true),
            "tensor.matmul" => self.call_matmul(args),
            _ => Ok(None),
        }
    }

    fn call_elementwise(&mut self, args: &[Value], is_mul: bool) -> Result<Option<Value>, RuntimeError> {
        if args.len() != 2 { return Ok(None); }

        let (a, b) = match (&args[0], &args[1]) {
            (Value::Tensor(a), Value::Tensor(b)) => (a.as_ref(), b.as_ref()),
            _ => return Ok(None),
        };

        if a.shape != b.shape {
            return Err(RuntimeError::ShapeMismatch {
                expected: a.shape.clone(),
                got: b.shape.clone(),
            });
        }

        let len = a.data.len();
        let mut out_data = vec![0.0f64; len];

        // Compile if needed
        let fn_ptr = if is_mul {
            if self.mul_fn.is_none() {
                self.mul_fn = Some(self.compile_elementwise("tensor_mul", true)?);
            }
            self.mul_fn.unwrap()
        } else {
            if self.add_fn.is_none() {
                self.add_fn = Some(self.compile_elementwise("tensor_add", false)?);
            }
            self.add_fn.unwrap()
        };

        // Call native
        unsafe {
            let f: fn(i64, i64, i64, i64) = std::mem::transmute(fn_ptr);
            f(
                a.data.as_ptr() as i64,
                b.data.as_ptr() as i64,
                out_data.as_mut_ptr() as i64,
                len as i64,
            );
        }

        Ok(Some(Value::Tensor(Box::new(TensorValue {
            dtype: a.dtype,
            shape: a.shape.clone(),
            data: out_data,
        }))))
    }

    fn compile_elementwise(&mut self, name: &str, is_mul: bool) -> Result<*const u8, RuntimeError> {
        // Signature: fn(a_ptr: i64, b_ptr: i64, out_ptr: i64, len: i64)
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64)); // a_ptr
        sig.params.push(AbiParam::new(types::I64)); // b_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // len

        let func_id = self.module.declare_function(name, Linkage::Local, &sig)
            .map_err(|e| RuntimeError::Custom(format!("declare: {}", e)))?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);

            let a_ptr = builder.block_params(entry)[0];
            let b_ptr = builder.block_params(entry)[1];
            let out_ptr = builder.block_params(entry)[2];
            let len = builder.block_params(entry)[3];

            let loop_header = builder.create_block();
            let loop_body = builder.create_block();
            let exit = builder.create_block();

            // i = 0
            let zero = builder.ins().iconst(types::I64, 0);
            let eight = builder.ins().iconst(types::I64, 8);
            builder.ins().jump(loop_header, &[zero]);

            // loop_header(i): if i >= len → exit
            builder.append_block_param(loop_header, types::I64);
            builder.switch_to_block(loop_header);
            let i = builder.block_params(loop_header)[0];
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
            builder.ins().brif(cmp, exit, &[], loop_body, &[]);

            // loop_body: load, op, store
            builder.switch_to_block(loop_body);
            builder.seal_block(loop_body);

            let offset = builder.ins().imul(i, eight);
            let a_addr = builder.ins().iadd(a_ptr, offset);
            let b_addr = builder.ins().iadd(b_ptr, offset);
            let out_addr = builder.ins().iadd(out_ptr, offset);

            let a_val = builder.ins().load(types::F64, MemFlags::trusted(), a_addr, 0);
            let b_val = builder.ins().load(types::F64, MemFlags::trusted(), b_addr, 0);

            let result = if is_mul {
                builder.ins().fmul(a_val, b_val)
            } else {
                builder.ins().fadd(a_val, b_val)
            };

            builder.ins().store(MemFlags::trusted(), result, out_addr, 0);

            let i_next = builder.ins().iadd_imm(i, 1);
            builder.ins().jump(loop_header, &[i_next]);

            // exit
            builder.switch_to_block(exit);
            builder.seal_block(exit);
            builder.seal_block(loop_header);
            builder.ins().return_(&[]);

            builder.finalize();
        }

        self.module.define_function(func_id, &mut ctx)
            .map_err(|e| RuntimeError::Custom(format!("define: {}", e)))?;
        self.module.clear_context(&mut ctx);
        self.module.finalize_definitions()
            .map_err(|e| RuntimeError::Custom(format!("finalize: {}", e)))?;

        Ok(self.module.get_finalized_function(func_id))
    }

    fn call_matmul(&mut self, _args: &[Value]) -> Result<Option<Value>, RuntimeError> {
        Ok(None) // Stub — implemented in Task 2
    }
}
```

- [ ] **Step 2: Add airl-runtime dependency to airl-codegen**

In `crates/airl-codegen/Cargo.toml`, add:
```toml
airl-runtime = { path = "../airl-runtime" }
```

Wait — this creates a circular dependency! `airl-runtime` already depends on `airl-codegen`. We need `Value` and `TensorValue` from runtime, but runtime depends on codegen.

**Solution:** TensorJit should NOT depend on airl-runtime. Instead, it takes raw pointers and lengths as arguments, and the caller (in eval.rs) handles the Value/TensorValue extraction and wrapping.

Revise: `TensorJit` exposes a lower-level API:

```rust
pub struct TensorJit { ... }

impl TensorJit {
    pub fn new() -> Result<Self, String>;

    /// Element-wise add: out[i] = a[i] + b[i]
    /// All pointers are f64 arrays, len is element count.
    pub fn add(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String>;

    /// Element-wise mul: out[i] = a[i] * b[i]
    pub fn mul(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String>;

    /// Matrix multiply: a[M,K] * b[K,N] = out[M,N] (row-major)
    pub fn matmul(&mut self, a: &[f64], b: &[f64], out: &mut [f64], m: usize, k: usize, n: usize) -> Result<(), String>;
}
```

The eval.rs code extracts slices from `TensorValue`, calls these methods, and wraps results. No circular dependency.

- [ ] **Step 3: Write tests for element-wise ops**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_add_basic() {
        let mut jit = TensorJit::new().unwrap();
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![5.0, 6.0, 7.0, 8.0];
        let mut out = vec![0.0; 4];
        jit.add(&a, &b, &mut out).unwrap();
        assert_eq!(out, vec![6.0, 8.0, 10.0, 12.0]);
    }

    #[test]
    fn tensor_mul_basic() {
        let mut jit = TensorJit::new().unwrap();
        let a = vec![2.0, 3.0, 4.0];
        let b = vec![5.0, 6.0, 7.0];
        let mut out = vec![0.0; 3];
        jit.mul(&a, &b, &mut out).unwrap();
        assert_eq!(out, vec![10.0, 18.0, 28.0]);
    }

    #[test]
    fn tensor_add_empty() {
        let mut jit = TensorJit::new().unwrap();
        let a: Vec<f64> = vec![];
        let b: Vec<f64> = vec![];
        let mut out: Vec<f64> = vec![];
        jit.add(&a, &b, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn tensor_add_single() {
        let mut jit = TensorJit::new().unwrap();
        let a = vec![42.0];
        let b = vec![8.0];
        let mut out = vec![0.0];
        jit.add(&a, &b, &mut out).unwrap();
        assert_eq!(out, vec![50.0]);
    }

    #[test]
    fn tensor_add_large() {
        let mut jit = TensorJit::new().unwrap();
        let n = 10000;
        let a: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let b: Vec<f64> = (0..n).map(|i| (n - i) as f64).collect();
        let mut out = vec![0.0; n];
        jit.add(&a, &b, &mut out).unwrap();
        for val in &out {
            assert_eq!(*val, n as f64);
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

Add `pub mod tensor_ops;` and `pub use tensor_ops::TensorJit;` to `crates/airl-codegen/src/lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p airl-codegen -- tensor`
Expected: all tensor tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/airl-codegen/src/tensor_ops.rs crates/airl-codegen/src/lib.rs
git commit -m "feat(codegen): add JIT-compiled element-wise tensor ops (add, mul)"
```

---

## Task 2: Matmul Loop Codegen

**Files:**
- Modify: `crates/airl-codegen/src/tensor_ops.rs`

- [ ] **Step 1: Write failing matmul test**

```rust
#[test]
fn tensor_matmul_2x3_3x2() {
    let mut jit = TensorJit::new().unwrap();
    // A = [[1,2,3],[4,5,6]] (2x3)
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    // B = [[7,8],[9,10],[11,12]] (3x2)
    let b = vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
    // Expected: [[58,64],[139,154]]
    let mut out = vec![0.0; 4];
    jit.matmul(&a, &b, &mut out, 2, 3, 2).unwrap();
    assert_eq!(out, vec![58.0, 64.0, 139.0, 154.0]);
}

#[test]
fn tensor_matmul_identity() {
    let mut jit = TensorJit::new().unwrap();
    // A = [[1,0],[0,1]] (2x2 identity)
    let a = vec![1.0, 0.0, 0.0, 1.0];
    // B = [[5,6],[7,8]]
    let b = vec![5.0, 6.0, 7.0, 8.0];
    let mut out = vec![0.0; 4];
    jit.matmul(&a, &b, &mut out, 2, 2, 2).unwrap();
    assert_eq!(out, vec![5.0, 6.0, 7.0, 8.0]);
}

#[test]
fn tensor_matmul_1x1() {
    let mut jit = TensorJit::new().unwrap();
    let a = vec![3.0];
    let b = vec![4.0];
    let mut out = vec![0.0];
    jit.matmul(&a, &b, &mut out, 1, 1, 1).unwrap();
    assert_eq!(out, vec![12.0]);
}
```

- [ ] **Step 2: Implement compile_matmul**

The matmul uses three nested loops. In Cranelift IR:

```rust
fn compile_matmul(&mut self) -> Result<*const u8, String> {
    // Signature: fn(a_ptr: i64, b_ptr: i64, out_ptr: i64, m: i64, k: i64, n: i64)
    let mut sig = self.module.make_signature();
    for _ in 0..6 { sig.params.push(AbiParam::new(types::I64)); }

    let func_id = self.module.declare_function("tensor_matmul", Linkage::Local, &sig)
        .map_err(|e| format!("declare: {}", e))?;

    let mut ctx = self.module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let a_ptr = builder.block_params(entry)[0];
        let b_ptr = builder.block_params(entry)[1];
        let out_ptr = builder.block_params(entry)[2];
        let m = builder.block_params(entry)[3];
        let k = builder.block_params(entry)[4];
        let n = builder.block_params(entry)[5];

        let eight = builder.ins().iconst(types::I64, 8);
        let zero_i = builder.ins().iconst(types::I64, 0);
        let zero_f = builder.ins().f64const(0.0);

        // Blocks for triple loop
        let i_header = builder.create_block();
        let j_header = builder.create_block();
        let k_header = builder.create_block();
        let k_body = builder.create_block();
        let j_store = builder.create_block();
        let j_next = builder.create_block();
        let i_next = builder.create_block();
        let exit = builder.create_block();

        // Entry → i_header(0)
        builder.ins().jump(i_header, &[zero_i]);

        // i_header(i): if i >= m → exit
        builder.append_block_param(i_header, types::I64);
        builder.switch_to_block(i_header);
        let i = builder.block_params(i_header)[0];
        let i_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, m);
        builder.ins().brif(i_done, exit, &[], j_header, &[zero_i]);

        // j_header(j): if j >= n → i_next
        builder.append_block_param(j_header, types::I64);
        builder.switch_to_block(j_header);
        let j = builder.block_params(j_header)[0];
        let j_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, j, n);
        builder.ins().brif(j_done, i_next, &[], k_header, &[zero_i, zero_f]);

        // k_header(p, sum): if p >= k → j_store
        builder.append_block_param(k_header, types::I64); // p
        builder.append_block_param(k_header, types::F64); // sum
        builder.switch_to_block(k_header);
        let p = builder.block_params(k_header)[0];
        let sum = builder.block_params(k_header)[1];
        let k_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, p, k);
        builder.ins().brif(k_done, j_store, &[sum], k_body, &[]);

        // k_body: sum += a[i*k+p] * b[p*n+j]
        builder.switch_to_block(k_body);
        builder.seal_block(k_body);

        let ik = builder.ins().imul(i, k);
        let ikp = builder.ins().iadd(ik, p);
        let a_off = builder.ins().imul(ikp, eight);
        let a_addr = builder.ins().iadd(a_ptr, a_off);

        let pn = builder.ins().imul(p, n);
        let pnj = builder.ins().iadd(pn, j);
        let b_off = builder.ins().imul(pnj, eight);
        let b_addr = builder.ins().iadd(b_ptr, b_off);

        let a_val = builder.ins().load(types::F64, MemFlags::trusted(), a_addr, 0);
        let b_val = builder.ins().load(types::F64, MemFlags::trusted(), b_addr, 0);
        let prod = builder.ins().fmul(a_val, b_val);
        let new_sum = builder.ins().fadd(sum, prod);
        let p_next = builder.ins().iadd_imm(p, 1);
        builder.ins().jump(k_header, &[p_next, new_sum]);

        // j_store(sum): out[i*n+j] = sum
        builder.append_block_param(j_store, types::F64);
        builder.switch_to_block(j_store);
        builder.seal_block(j_store);
        let final_sum = builder.block_params(j_store)[0];

        let in_ = builder.ins().imul(i, n);
        let inj = builder.ins().iadd(in_, j);
        let out_off = builder.ins().imul(inj, eight);
        let out_addr = builder.ins().iadd(out_ptr, out_off);
        builder.ins().store(MemFlags::trusted(), final_sum, out_addr, 0);

        let j_inc = builder.ins().iadd_imm(j, 1);
        builder.ins().jump(j_next, &[]);

        // j_next → j_header(j+1)
        builder.switch_to_block(j_next);
        builder.seal_block(j_next);
        builder.ins().jump(j_header, &[j_inc]);

        // i_next → i_header(i+1)
        builder.switch_to_block(i_next);
        builder.seal_block(i_next);
        let i_inc = builder.ins().iadd_imm(i, 1);
        builder.ins().jump(i_header, &[i_inc]);

        // Seal remaining blocks
        builder.seal_block(k_header);
        builder.seal_block(j_header);
        builder.seal_block(i_header);

        // exit
        builder.switch_to_block(exit);
        builder.seal_block(exit);
        builder.ins().return_(&[]);

        builder.finalize();
    }

    self.module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("define: {}", e))?;
    self.module.clear_context(&mut ctx);
    self.module.finalize_definitions()
        .map_err(|e| format!("finalize: {}", e))?;

    Ok(self.module.get_finalized_function(func_id))
}
```

Update `call_matmul` to extract dimensions, allocate output, call native:

```rust
pub fn matmul(&mut self, a: &[f64], b: &[f64], out: &mut [f64], m: usize, k: usize, n: usize) -> Result<(), String> {
    if self.matmul_fn.is_none() {
        self.matmul_fn = Some(self.compile_matmul()?);
    }
    let fn_ptr = self.matmul_fn.unwrap();
    unsafe {
        let f: fn(i64, i64, i64, i64, i64, i64) = std::mem::transmute(fn_ptr);
        f(
            a.as_ptr() as i64,
            b.as_ptr() as i64,
            out.as_mut_ptr() as i64,
            m as i64,
            k as i64,
            n as i64,
        );
    }
    Ok(())
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-codegen -- tensor`
Expected: all tensor tests pass (element-wise + matmul)

- [ ] **Step 4: Commit**

```bash
git add crates/airl-codegen/src/tensor_ops.rs
git commit -m "feat(codegen): add JIT-compiled tensor matmul"
```

---

## Task 3: Integrate TensorJit into Interpreter

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs`

- [ ] **Step 1: Add tensor_jit to Interpreter**

```rust
pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    pub jit: Option<airl_codegen::JitCache>,
    pub tensor_jit: Option<airl_codegen::TensorJit>,  // NEW
}
```

In `Interpreter::new()`:
```rust
tensor_jit: airl_codegen::TensorJit::new().ok(),
```

- [ ] **Step 2: Add JIT check for tensor builtins in FnCall**

In the `FnCall` arm, where `Value::BuiltinFn(ref name)` is matched, add tensor JIT dispatch before the regular builtin call:

```rust
Value::BuiltinFn(ref name) => {
    // Try tensor JIT for supported ops
    if matches!(name.as_str(), "tensor.add" | "tensor.mul" | "tensor.matmul") {
        if let Some(ref mut tjit) = self.tensor_jit {
            if let Some(result) = try_tensor_jit(tjit, name, &arg_vals)? {
                return Ok(result); // or set result in outer scope
            }
        }
    }
    // Fall back to interpreted builtin
    let f = self.builtins.get(name).ok_or_else(|| {
        RuntimeError::UndefinedSymbol(name.clone())
    })?;
    f(&arg_vals)
}
```

Add the helper function:

```rust
fn try_tensor_jit(
    tjit: &mut airl_codegen::TensorJit,
    op: &str,
    args: &[Value],
) -> Result<Option<Value>, RuntimeError> {
    match op {
        "tensor.add" | "tensor.mul" => {
            if args.len() != 2 { return Ok(None); }
            let (a, b) = match (&args[0], &args[1]) {
                (Value::Tensor(a), Value::Tensor(b)) => (a.as_ref(), b.as_ref()),
                _ => return Ok(None),
            };
            if a.shape != b.shape {
                return Err(RuntimeError::ShapeMismatch {
                    expected: a.shape.clone(), got: b.shape.clone(),
                });
            }
            let mut out_data = vec![0.0f64; a.data.len()];
            let result = if op == "tensor.add" {
                tjit.add(&a.data, &b.data, &mut out_data)
            } else {
                tjit.mul(&a.data, &b.data, &mut out_data)
            };
            result.map_err(|e| RuntimeError::Custom(e))?;
            Ok(Some(Value::Tensor(Box::new(TensorValue {
                dtype: a.dtype, shape: a.shape.clone(), data: out_data,
            }))))
        }
        "tensor.matmul" => {
            if args.len() != 2 { return Ok(None); }
            let (a, b) = match (&args[0], &args[1]) {
                (Value::Tensor(a), Value::Tensor(b)) => (a.as_ref(), b.as_ref()),
                _ => return Ok(None),
            };
            if a.shape.len() != 2 || b.shape.len() != 2 {
                return Ok(None); // fall back to interpreted
            }
            let (m, k1) = (a.shape[0], a.shape[1]);
            let (k2, n) = (b.shape[0], b.shape[1]);
            if k1 != k2 {
                return Err(RuntimeError::ShapeMismatch {
                    expected: vec![m, k1], got: vec![k2, n],
                });
            }
            let mut out_data = vec![0.0f64; m * n];
            tjit.matmul(&a.data, &b.data, &mut out_data, m, k1, n)
                .map_err(|e| RuntimeError::Custom(e))?;
            Ok(Some(Value::Tensor(Box::new(TensorValue {
                dtype: a.dtype, shape: vec![m, n], data: out_data,
            }))))
        }
        _ => Ok(None),
    }
}
```

You'll need to add `use crate::tensor::TensorValue;` at the top of eval.rs.

- [ ] **Step 3: Write integration test**

```rust
#[test]
fn tensor_jit_add_transparent() {
    let input = r#"
        (let (a : tensor (tensor.ones f32 [4]))
          (let (b : tensor (tensor.ones f32 [4]))
            (tensor.add a b)))
    "#;
    let result = eval_str(input);
    // Result should be a tensor with all 2.0s
    if let Value::Tensor(t) = result {
        assert_eq!(t.data, vec![2.0, 2.0, 2.0, 2.0]);
    } else {
        panic!("expected Tensor, got {:?}", result);
    }
}

#[test]
fn tensor_jit_matmul_transparent() {
    let input = r#"
        (let (a : tensor (tensor.identity f32 3))
          (let (b : tensor (tensor.identity f32 3))
            (tensor.matmul a b)))
    "#;
    let result = eval_str(input);
    if let Value::Tensor(t) = result {
        // identity * identity = identity
        assert_eq!(t.shape, vec![3, 3]);
        assert_eq!(t.data[0], 1.0);
        assert_eq!(t.data[4], 1.0);
        assert_eq!(t.data[8], 1.0);
        assert_eq!(t.data[1], 0.0);
    } else {
        panic!("expected Tensor, got {:?}", result);
    }
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test --workspace`
Expected: all 412+ existing tests pass, plus new tensor JIT tests

- [ ] **Step 5: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): integrate TensorJit for transparent tensor op compilation"
```

---

## Task 4: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Verify tensor ops work end-to-end**

Run: `cargo run -- run tests/fixtures/valid/tensor_ops.airl` (if this fixture uses tensor.add)
Expected: correct output

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: tensor JIT complete — add, mul, matmul compiled to native loops"
```
