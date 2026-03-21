use melior::ir::Module;
use melior::ExecutionEngine;

/// A compiled MLIR module ready for JIT execution.
///
/// Wraps `melior::ExecutionEngine` and provides a safe interface to call
/// compiled tensor operations with raw `&[f64]` pointers.
pub struct CompiledModule {
    engine: ExecutionEngine,
}

/// Descriptor struct matching MLIR's `UnrankedMemRefDescriptor` ABI.
/// MLIR's default calling convention for memref arguments uses a struct:
///   { i64 rank, void* descriptor }
/// For ranked memrefs, the descriptor is:
///   { f64* allocated, f64* aligned, i64 offset, i64[rank] sizes, i64[rank] strides }
///
/// We use the "bare pointer" calling convention instead where possible,
/// which passes just `f64*` directly. This requires the pass pipeline to
/// enable bare-pointer memref calling convention.
#[repr(C)]
pub struct MemRefDescriptor1D {
    pub allocated: *mut f64,
    pub aligned: *mut f64,
    pub offset: i64,
    pub size: i64,
    pub stride: i64,
}

#[repr(C)]
pub struct MemRefDescriptor2D {
    pub allocated: *mut f64,
    pub aligned: *mut f64,
    pub offset: i64,
    pub sizes: [i64; 2],
    pub strides: [i64; 2],
}

impl MemRefDescriptor1D {
    /// Create a descriptor for a contiguous 1-D slice.
    pub fn from_slice(data: &[f64]) -> Self {
        Self {
            allocated: data.as_ptr() as *mut f64,
            aligned: data.as_ptr() as *mut f64,
            offset: 0,
            size: data.len() as i64,
            stride: 1,
        }
    }

    /// Create a descriptor for a mutable contiguous 1-D slice.
    pub fn from_mut_slice(data: &mut [f64]) -> Self {
        Self {
            allocated: data.as_mut_ptr(),
            aligned: data.as_mut_ptr(),
            offset: 0,
            size: data.len() as i64,
            stride: 1,
        }
    }
}

impl MemRefDescriptor2D {
    /// Create a descriptor for a contiguous row-major 2-D array.
    pub fn from_slice(data: &[f64], rows: usize, cols: usize) -> Self {
        Self {
            allocated: data.as_ptr() as *mut f64,
            aligned: data.as_ptr() as *mut f64,
            offset: 0,
            sizes: [rows as i64, cols as i64],
            strides: [cols as i64, 1],
        }
    }

    /// Create a descriptor for a mutable contiguous row-major 2-D array.
    pub fn from_mut_slice(data: &mut [f64], rows: usize, cols: usize) -> Self {
        Self {
            allocated: data.as_mut_ptr(),
            aligned: data.as_mut_ptr(),
            offset: 0,
            sizes: [rows as i64, cols as i64],
            strides: [cols as i64, 1],
        }
    }
}

impl CompiledModule {
    /// Create a JIT execution engine from a fully-lowered MLIR module.
    ///
    /// The module must contain only LLVM dialect ops (i.e., the lowering pipeline
    /// must have been run already).
    pub fn new(module: &Module) -> Result<Self, String> {
        // opt_level=2, shared_lib_paths=[], enable_object_dump=false
        let engine = ExecutionEngine::new(module, 2, &[], false);
        Ok(Self { engine })
    }

    /// Dump the module IR for debugging.
    #[allow(dead_code)]
    pub fn dump_module(module: &Module) -> String {
        module.as_operation().to_string()
    }

    /// Call a compiled element-wise function (add or mul) on 1-D f64 slices.
    ///
    /// The function must have signature:
    ///   (memref<?xf64>, memref<?xf64>, memref<?xf64>, index) -> ()
    pub fn call_elementwise(
        &self,
        fn_name: &str,
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
    ) -> Result<(), String> {
        let len = a.len();
        if b.len() != len || out.len() != len {
            return Err("elementwise: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }

        let mut desc_a = MemRefDescriptor1D::from_slice(a);
        let mut desc_b = MemRefDescriptor1D::from_slice(b);
        let mut desc_out = MemRefDescriptor1D::from_mut_slice(out);
        let mut idx_len = len as i64;

        // invoke_packed adds one level of indirection: each array element is
        // a void* pointing to the argument value. Since the _mlir_ciface_*
        // wrapper takes memref descriptors by pointer (!llvm.ptr), we pass
        // &mut ptr_to_descriptor (pointer-to-pointer) for memref args, and
        // &mut i64 for scalar args.
        let mut ptr_a: *mut MemRefDescriptor1D = &mut desc_a;
        let mut ptr_b: *mut MemRefDescriptor1D = &mut desc_b;
        let mut ptr_out: *mut MemRefDescriptor1D = &mut desc_out;

        unsafe {
            self.engine
                .invoke_packed(
                    fn_name,
                    &mut [
                        &mut ptr_a as *mut _ as *mut _,
                        &mut ptr_b as *mut _ as *mut _,
                        &mut ptr_out as *mut _ as *mut _,
                        &mut idx_len as *mut _ as *mut _,
                    ],
                )
                .map_err(|_| format!("MLIR JIT invocation of '{}' failed", fn_name))?;
        }

        Ok(())
    }

    /// Call a compiled matmul function on 2-D f64 arrays.
    ///
    /// The function must have signature:
    ///   (memref<?x?xf64>, memref<?x?xf64>, memref<?x?xf64>, index, index, index) -> ()
    pub fn call_matmul(
        &self,
        fn_name: &str,
        a: &[f64],
        b: &[f64],
        out: &mut [f64],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<(), String> {
        let mut desc_a = MemRefDescriptor2D::from_slice(a, m, k);
        let mut desc_b = MemRefDescriptor2D::from_slice(b, k, n);
        let mut desc_out = MemRefDescriptor2D::from_mut_slice(out, m, n);
        let mut idx_m = m as i64;
        let mut idx_k = k as i64;
        let mut idx_n = n as i64;

        let mut ptr_a: *mut MemRefDescriptor2D = &mut desc_a;
        let mut ptr_b: *mut MemRefDescriptor2D = &mut desc_b;
        let mut ptr_out: *mut MemRefDescriptor2D = &mut desc_out;

        unsafe {
            self.engine
                .invoke_packed(
                    fn_name,
                    &mut [
                        &mut ptr_a as *mut _ as *mut _,
                        &mut ptr_b as *mut _ as *mut _,
                        &mut ptr_out as *mut _ as *mut _,
                        &mut idx_m as *mut _ as *mut _,
                        &mut idx_k as *mut _ as *mut _,
                        &mut idx_n as *mut _ as *mut _,
                    ],
                )
                .map_err(|_| format!("MLIR JIT invocation of '{}' failed", fn_name))?;
        }

        Ok(())
    }
    /// Call a compiled softmax function on 1-D f64 slices.
    ///
    /// The function must have signature:
    ///   (memref<?xf64>, memref<?xf64>, index) -> ()
    pub fn call_softmax(
        &self,
        fn_name: &str,
        input: &[f64],
        out: &mut [f64],
    ) -> Result<(), String> {
        let len = input.len();
        if out.len() != len {
            return Err("softmax: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }

        let mut desc_in = MemRefDescriptor1D::from_slice(input);
        let mut desc_out = MemRefDescriptor1D::from_mut_slice(out);
        let mut idx_len = len as i64;

        let mut ptr_in: *mut MemRefDescriptor1D = &mut desc_in;
        let mut ptr_out: *mut MemRefDescriptor1D = &mut desc_out;

        unsafe {
            self.engine
                .invoke_packed(
                    fn_name,
                    &mut [
                        &mut ptr_in as *mut _ as *mut _,
                        &mut ptr_out as *mut _ as *mut _,
                        &mut idx_len as *mut _ as *mut _,
                    ],
                )
                .map_err(|_| format!("MLIR JIT invocation of '{}' failed", fn_name))?;
        }

        Ok(())
    }

    /// Call a compiled transpose function on 2-D f64 arrays.
    ///
    /// The function must have signature:
    ///   (memref<?x?xf64>, memref<?x?xf64>, index, index) -> ()
    pub fn call_transpose(
        &self,
        fn_name: &str,
        input: &[f64],
        out: &mut [f64],
        rows: usize,
        cols: usize,
    ) -> Result<(), String> {
        let mut desc_in = MemRefDescriptor2D::from_slice(input, rows, cols);
        let mut desc_out = MemRefDescriptor2D::from_mut_slice(out, cols, rows);
        let mut idx_rows = rows as i64;
        let mut idx_cols = cols as i64;

        let mut ptr_in: *mut MemRefDescriptor2D = &mut desc_in;
        let mut ptr_out: *mut MemRefDescriptor2D = &mut desc_out;

        unsafe {
            self.engine
                .invoke_packed(
                    fn_name,
                    &mut [
                        &mut ptr_in as *mut _ as *mut _,
                        &mut ptr_out as *mut _ as *mut _,
                        &mut idx_rows as *mut _ as *mut _,
                        &mut idx_cols as *mut _ as *mut _,
                    ],
                )
                .map_err(|_| format!("MLIR JIT invocation of '{}' failed", fn_name))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::MlirContext;
    use crate::lower::{lower_elementwise, ElementwiseOp};
    use crate::optimize::run_lowering_pipeline;

    fn compile_elementwise(op: ElementwiseOp, name: &str) -> CompiledModule {
        let mlir_ctx = MlirContext::new().unwrap();
        let mut module = mlir_ctx.new_module();
        lower_elementwise(mlir_ctx.context(), &module, op, name).unwrap();
        run_lowering_pipeline(mlir_ctx.context(), &mut module).unwrap();
        CompiledModule::new(&module).unwrap()
    }

    #[test]
    fn test_jit_add() {
        let compiled = compile_elementwise(ElementwiseOp::Add, "tensor_add");
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [10.0, 20.0, 30.0, 40.0];
        let mut out = [0.0f64; 4];
        compiled
            .call_elementwise("tensor_add", &a, &b, &mut out)
            .unwrap();
        assert_eq!(out, [11.0, 22.0, 33.0, 44.0]);
    }

    #[test]
    fn test_jit_mul() {
        let compiled = compile_elementwise(ElementwiseOp::Mul, "tensor_mul");
        let a = [2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0];
        let mut out = [0.0f64; 3];
        compiled
            .call_elementwise("tensor_mul", &a, &b, &mut out)
            .unwrap();
        assert_eq!(out, [10.0, 18.0, 28.0]);
    }
}
