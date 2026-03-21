use cranelift_codegen::ir::{types, AbiParam, BlockArg, InstBuilder, MemFlags};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

/// JIT-compiled tensor operations. Compiles element-wise and matmul loops
/// to native code via Cranelift. Each operation is lazily compiled on first
/// call and cached thereafter.
///
/// This struct operates on raw `&[f64]` slices — it has no dependency on
/// runtime types (`Value`, `TensorValue`, etc.) to avoid circular crate
/// dependencies.
pub struct TensorJit {
    module: JITModule,
    add_fn: Option<*const u8>,
    mul_fn: Option<*const u8>,
    matmul_fn: Option<*const u8>,
}

impl TensorJit {
    /// Create a new TensorJit with a fresh Cranelift JIT module.
    pub fn new() -> Result<Self, String> {
        let builder = cranelift_jit::JITBuilder::new(
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            add_fn: None,
            mul_fn: None,
            matmul_fn: None,
        })
    }

    /// Element-wise add: `out[i] = a[i] + b[i]` for all `i` in `0..len`.
    /// All three slices must have the same length.
    pub fn add(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        if b.len() != len || out.len() != len {
            return Err("tensor add: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }
        if self.add_fn.is_none() {
            self.add_fn = Some(self.compile_elementwise("tensor_add", false)?);
        }
        let fn_ptr = self.add_fn.unwrap();
        unsafe {
            let f: fn(i64, i64, i64, i64) = std::mem::transmute(fn_ptr);
            f(
                a.as_ptr() as i64,
                b.as_ptr() as i64,
                out.as_mut_ptr() as i64,
                len as i64,
            );
        }
        Ok(())
    }

    /// Element-wise mul: `out[i] = a[i] * b[i]` for all `i` in `0..len`.
    /// All three slices must have the same length.
    pub fn mul(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        if b.len() != len || out.len() != len {
            return Err("tensor mul: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }
        if self.mul_fn.is_none() {
            self.mul_fn = Some(self.compile_elementwise("tensor_mul", true)?);
        }
        let fn_ptr = self.mul_fn.unwrap();
        unsafe {
            let f: fn(i64, i64, i64, i64) = std::mem::transmute(fn_ptr);
            f(
                a.as_ptr() as i64,
                b.as_ptr() as i64,
                out.as_mut_ptr() as i64,
                len as i64,
            );
        }
        Ok(())
    }

    /// Matrix multiply: `a[M,K] * b[K,N] = out[M,N]` (row-major layout).
    /// Stub — implemented in Task 2.
    pub fn matmul(
        &mut self,
        _a: &[f64],
        _b: &[f64],
        _out: &mut [f64],
        _m: usize,
        _k: usize,
        _n: usize,
    ) -> Result<(), String> {
        Err("matmul not yet implemented".into())
    }

    /// Compile an element-wise loop (add or mul) to native code.
    /// Signature: `fn(a_ptr: i64, b_ptr: i64, out_ptr: i64, len: i64)`
    fn compile_elementwise(&mut self, name: &str, is_mul: bool) -> Result<*const u8, String> {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64)); // a_ptr
        sig.params.push(AbiParam::new(types::I64)); // b_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // len

        let func_id = self
            .module
            .declare_function(name, Linkage::Local, &sig)
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
            let len = builder.block_params(entry)[3];

            let loop_header = builder.create_block();
            let loop_body = builder.create_block();
            let exit = builder.create_block();

            // i = 0
            let zero = builder.ins().iconst(types::I64, 0);
            let eight = builder.ins().iconst(types::I64, 8);
            builder.ins().jump(loop_header, &[BlockArg::Value(zero)]);

            // loop_header(i): if i >= len -> exit
            builder.append_block_param(loop_header, types::I64);
            builder.switch_to_block(loop_header);
            let i = builder.block_params(loop_header)[0];
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, len);
            builder.ins().brif(cmp, exit, &[] as &[BlockArg], loop_body, &[] as &[BlockArg]);

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
            builder.ins().jump(loop_header, &[BlockArg::Value(i_next)]);

            // exit
            builder.switch_to_block(exit);
            builder.seal_block(exit);
            builder.seal_block(loop_header);
            builder.ins().return_(&[]);

            builder.finalize();
        }

        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("define: {}", e))?;
        self.module.clear_context(&mut ctx);
        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize: {}", e))?;

        Ok(self.module.get_finalized_function(func_id))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Element-wise tests ──────────────────────────────────────────

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
