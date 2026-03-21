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
    /// - `a` must have `m * k` elements
    /// - `b` must have `k * n` elements
    /// - `out` must have `m * n` elements
    pub fn matmul(
        &mut self,
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<(), String> {
        if a.len() != m * k {
            return Err(format!("matmul: a has {} elements, expected {}", a.len(), m * k));
        }
        if b.len() != k * n {
            return Err(format!("matmul: b has {} elements, expected {}", b.len(), k * n));
        }
        if out.len() != m * n {
            return Err(format!("matmul: out has {} elements, expected {}", out.len(), m * n));
        }
        if m == 0 || k == 0 || n == 0 {
            return Ok(());
        }
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

    /// Compile a triple-nested matmul loop to native code.
    /// Signature: `fn(a_ptr: i64, b_ptr: i64, out_ptr: i64, m: i64, k: i64, n: i64)`
    ///
    /// Computes C[i,j] = sum_p A[i*k+p] * B[p*n+j] for all i in 0..M, j in 0..N.
    fn compile_matmul(&mut self) -> Result<*const u8, String> {
        let mut sig = self.module.make_signature();
        for _ in 0..6 {
            sig.params.push(AbiParam::new(types::I64));
        }

        let func_id = self
            .module
            .declare_function("tensor_matmul", Linkage::Local, &sig)
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

            // Entry -> i_header(0)
            builder.ins().jump(i_header, &[BlockArg::Value(zero_i)]);

            // i_header(i): if i >= m -> exit
            builder.append_block_param(i_header, types::I64);
            builder.switch_to_block(i_header);
            let i = builder.block_params(i_header)[0];
            let i_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, m);
            builder.ins().brif(
                i_done,
                exit,
                &[] as &[BlockArg],
                j_header,
                &[BlockArg::Value(zero_i)],
            );

            // j_header(j): if j >= n -> i_next
            builder.append_block_param(j_header, types::I64);
            builder.switch_to_block(j_header);
            let j = builder.block_params(j_header)[0];
            let j_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, j, n);
            builder.ins().brif(
                j_done,
                i_next,
                &[] as &[BlockArg],
                k_header,
                &[BlockArg::Value(zero_i), BlockArg::Value(zero_f)],
            );

            // k_header(p, sum): if p >= k -> j_store(sum)
            builder.append_block_param(k_header, types::I64); // p
            builder.append_block_param(k_header, types::F64); // sum
            builder.switch_to_block(k_header);
            let p = builder.block_params(k_header)[0];
            let sum = builder.block_params(k_header)[1];
            let k_done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, p, k);
            builder.ins().brif(
                k_done,
                j_store,
                &[BlockArg::Value(sum)],
                k_body,
                &[] as &[BlockArg],
            );

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
            builder.ins().jump(
                k_header,
                &[BlockArg::Value(p_next), BlockArg::Value(new_sum)],
            );

            // j_store(sum): out[i*n+j] = sum, then j_next
            builder.append_block_param(j_store, types::F64);
            builder.switch_to_block(j_store);
            builder.seal_block(j_store);
            let final_sum = builder.block_params(j_store)[0];

            let in_ = builder.ins().imul(i, n);
            let inj = builder.ins().iadd(in_, j);
            let out_off = builder.ins().imul(inj, eight);
            let out_addr = builder.ins().iadd(out_ptr, out_off);
            builder
                .ins()
                .store(MemFlags::trusted(), final_sum, out_addr, 0);

            let j_inc = builder.ins().iadd_imm(j, 1);
            builder.ins().jump(j_next, &[] as &[BlockArg]);

            // j_next -> j_header(j+1)
            builder.switch_to_block(j_next);
            builder.seal_block(j_next);
            builder.ins().jump(j_header, &[BlockArg::Value(j_inc)]);

            // i_next -> i_header(i+1)
            builder.switch_to_block(i_next);
            builder.seal_block(i_next);
            let i_inc = builder.ins().iadd_imm(i, 1);
            builder.ins().jump(i_header, &[BlockArg::Value(i_inc)]);

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

    // ── Matmul tests ────────────────────────────────────────────────

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
}
