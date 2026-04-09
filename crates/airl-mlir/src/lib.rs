pub mod context;
#[cfg(feature = "cuda")]
pub mod gpu;
pub mod jit;
pub mod lower;
pub mod optimize;

use std::collections::HashMap;

use context::MlirContext;
use jit::CompiledModule;
use lower::ElementwiseOp;

/// Entry in the compiled-kernel cache, together with a last-access generation
/// for LRU eviction.
struct CacheEntry {
    module: CompiledModule,
    last_access: u64,
}

/// Cache key for compiled kernels: operation kind + shape signature.
///
/// Unlike Cranelift's `TensorJit` which compiles shape-generic loops (passing
/// length as a runtime parameter), MLIR benefits from shape-specialized
/// compilation for better optimization (tiling, vectorization). So each unique
/// shape gets its own compiled kernel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CacheKey {
    Elementwise { op: ElementwiseOp, len: usize },
    Matmul { m: usize, k: usize, n: usize },
    Softmax { len: usize },
    Transpose { rows: usize, cols: usize },
}

/// MLIR-based tensor JIT compiler.
///
/// Mirrors the API of `airl_codegen::TensorJit` — operates on raw `&[f64]`
/// slices with no dependency on runtime types (`Value`, `TensorValue`) to
/// avoid circular crate dependencies.
///
/// Compilation flow for each operation:
/// 1. Build MLIR IR (func/arith/memref/scf dialects)
/// 2. Run lowering pass pipeline → LLVM dialect
/// 3. JIT-compile via MLIR ExecutionEngine
/// 4. Cache the compiled module keyed by (op, shape)
pub struct MlirTensorJit {
    mlir_ctx: MlirContext,
    /// LRU-managed cache: each entry records the compiled module and the
    /// generation at which it was last accessed.
    cache: HashMap<CacheKey, CacheEntry>,
    /// Monotonically increasing access counter used for LRU eviction decisions.
    generation: u64,
    #[cfg(feature = "cuda")]
    gpu: Option<gpu::GpuContext>,
}

impl MlirTensorJit {
    /// Create a new MLIR tensor JIT with all dialects loaded.
    /// If CUDA is available, GPU acceleration is enabled automatically.
    pub fn new() -> Result<Self, String> {
        let mlir_ctx = MlirContext::new()?;
        Ok(Self {
            mlir_ctx,
            cache: HashMap::new(),
            generation: 0,
            #[cfg(feature = "cuda")]
            gpu: gpu::GpuContext::try_new(),
        })
    }

    /// Returns true if GPU acceleration is available.
    pub fn has_gpu(&self) -> bool {
        #[cfg(feature = "cuda")]
        { self.gpu.is_some() }
        #[cfg(not(feature = "cuda"))]
        { false }
    }

    /// Dispatch an operation to the GPU if available.
    /// Returns `Some(result)` if GPU ran it, `None` to fall through to MLIR JIT.
    #[cfg(feature = "cuda")]
    fn try_gpu<R, F: FnOnce(&gpu::GpuContext) -> Result<R, String>>(
        &self,
        f: F,
    ) -> Option<Result<R, String>> {
        self.gpu.as_ref().map(f)
    }

    /// Element-wise add: `out[i] = a[i] + b[i]` for all `i` in `0..len`.
    /// All three slices must have the same length.
    pub fn add(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        if b.len() != len || out.len() != len {
            return Err("MLIR tensor add: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }

        #[cfg(feature = "cuda")]
        if let Some(result) = self.try_gpu(|gpu| gpu.add(a, b, out)) {
            return result;
        }

        let key = CacheKey::Elementwise {
            op: ElementwiseOp::Add,
            len,
        };
        self.ensure_compiled(&key)?;
        let compiled = &self.cache.get(&key).unwrap().module;
        compiled.call_elementwise("tensor_add", a, b, out)
    }

    /// Element-wise mul: `out[i] = a[i] * b[i]` for all `i` in `0..len`.
    /// All three slices must have the same length.
    pub fn mul(&mut self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        if b.len() != len || out.len() != len {
            return Err("MLIR tensor mul: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }

        #[cfg(feature = "cuda")]
        if let Some(result) = self.try_gpu(|gpu| gpu.mul(a, b, out)) {
            return result;
        }

        let key = CacheKey::Elementwise {
            op: ElementwiseOp::Mul,
            len,
        };
        self.ensure_compiled(&key)?;
        let compiled = &self.cache.get(&key).unwrap().module;
        compiled.call_elementwise("tensor_mul", a, b, out)
    }

    /// Matrix multiply: `out[i,j] = sum_p a[i,p] * b[p,j]`.
    /// `a` is `m×k`, `b` is `k×n`, `out` is `m×n` (all row-major).
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
            return Err(format!(
                "MLIR matmul: a.len()={} but expected m*k={}",
                a.len(),
                m * k
            ));
        }
        if b.len() != k * n {
            return Err(format!(
                "MLIR matmul: b.len()={} but expected k*n={}",
                b.len(),
                k * n
            ));
        }
        if out.len() != m * n {
            return Err(format!(
                "MLIR matmul: out.len()={} but expected m*n={}",
                out.len(),
                m * n
            ));
        }

        #[cfg(feature = "cuda")]
        if let Some(result) = self.try_gpu(|gpu| gpu.matmul(a, b, out, m, k, n)) {
            return result;
        }

        let key = CacheKey::Matmul { m, k, n };
        self.ensure_compiled(&key)?;
        let compiled = &self.cache.get(&key).unwrap().module;
        compiled.call_matmul("tensor_matmul", a, b, out, m, k, n)
    }

    /// Softmax: numerically-stable softmax over a 1-D f64 slice.
    /// `out[i] = exp(input[i] - max(input)) / sum(exp(input[j] - max(input)))`
    pub fn softmax(
        &mut self,
        input: &[f64],
        out: &mut [f64],
    ) -> Result<(), String> {
        let len = input.len();
        if out.len() != len {
            return Err("MLIR softmax: length mismatch".into());
        }
        if len == 0 {
            return Ok(());
        }

        #[cfg(feature = "cuda")]
        if let Some(result) = self.try_gpu(|gpu| gpu.softmax(input, out)) {
            return result;
        }

        let key = CacheKey::Softmax { len };
        self.ensure_compiled(&key)?;
        let compiled = &self.cache.get(&key).unwrap().module;
        compiled.call_softmax("tensor_softmax", input, out)
    }

    /// Transpose a 2-D row-major matrix: `out[j,i] = input[i,j]`.
    /// `input` is `rows×cols`, `out` is `cols×rows`.
    pub fn transpose(
        &mut self,
        input: &[f64],
        out: &mut [f64],
        rows: usize,
        cols: usize,
    ) -> Result<(), String> {
        if input.len() != rows * cols {
            return Err(format!(
                "MLIR transpose: input.len()={} but expected rows*cols={}",
                input.len(), rows * cols
            ));
        }
        if out.len() != rows * cols {
            return Err(format!(
                "MLIR transpose: out.len()={} but expected rows*cols={}",
                out.len(), rows * cols
            ));
        }

        #[cfg(feature = "cuda")]
        if let Some(result) = self.try_gpu(|gpu| gpu.transpose(input, out, rows, cols)) {
            return result;
        }

        let key = CacheKey::Transpose { rows, cols };
        self.ensure_compiled(&key)?;
        let compiled = &self.cache.get(&key).unwrap().module;
        compiled.call_transpose("tensor_transpose", input, out, rows, cols)
    }

    /// Maximum number of cached compiled kernels before LRU eviction kicks in.
    const MAX_CACHE_SIZE: usize = 256;

    /// Compile and cache a kernel if not already present.
    ///
    /// Access tracking: bumps `self.generation` and updates the entry's
    /// `last_access` on every hit and insert.  On overflow, evicts only the
    /// least-recently-used entry rather than clearing the entire cache.
    fn ensure_compiled(&mut self, key: &CacheKey) -> Result<(), String> {
        let gen = self.generation;
        self.generation = self.generation.wrapping_add(1);

        if let Some(entry) = self.cache.get_mut(key) {
            entry.last_access = gen;
            return Ok(());
        }

        // Evict the single LRU entry when at capacity.
        if self.cache.len() >= Self::MAX_CACHE_SIZE {
            if let Some(lru_key) = self.cache
                .iter()
                .min_by_key(|(_, e)| e.last_access)
                .map(|(k, _)| k.clone())
            {
                self.cache.remove(&lru_key);
            }
        }

        let mut module = self.mlir_ctx.new_module();

        match key {
            CacheKey::Elementwise { op, .. } => {
                let fn_name = match op {
                    ElementwiseOp::Add => "tensor_add",
                    ElementwiseOp::Mul => "tensor_mul",
                };
                lower::lower_elementwise(self.mlir_ctx.context(), &module, *op, fn_name)?;
            }
            CacheKey::Matmul { .. } => {
                lower::lower_matmul(self.mlir_ctx.context(), &module, "tensor_matmul")?;
            }
            CacheKey::Softmax { .. } => {
                lower::lower_softmax(self.mlir_ctx.context(), &module, "tensor_softmax")?;
            }
            CacheKey::Transpose { .. } => {
                lower::lower_transpose(self.mlir_ctx.context(), &module, "tensor_transpose")?;
            }
        }

        optimize::run_lowering_pipeline(self.mlir_ctx.context(), &mut module)?;
        let compiled = CompiledModule::new(&module)?;
        self.cache.insert(key.clone(), CacheEntry { module: compiled, last_access: gen });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlir_tensor_add() {
        let mut jit = MlirTensorJit::new().unwrap();
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 20.0, 30.0, 40.0, 50.0];
        let mut out = [0.0f64; 5];
        jit.add(&a, &b, &mut out).unwrap();
        assert_eq!(out, [11.0, 22.0, 33.0, 44.0, 55.0]);
    }

    #[test]
    fn test_mlir_tensor_mul() {
        let mut jit = MlirTensorJit::new().unwrap();
        let a = [2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0];
        let mut out = [0.0f64; 3];
        jit.mul(&a, &b, &mut out).unwrap();
        assert_eq!(out, [10.0, 18.0, 28.0]);
    }

    #[test]
    fn test_mlir_tensor_add_empty() {
        let mut jit = MlirTensorJit::new().unwrap();
        let mut out = [];
        jit.add(&[], &[], &mut out).unwrap();
    }

    #[test]
    fn test_mlir_tensor_add_cache_hit() {
        let mut jit = MlirTensorJit::new().unwrap();
        // First call compiles
        let mut out1 = [0.0f64; 3];
        jit.add(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0], &mut out1)
            .unwrap();
        assert_eq!(out1, [5.0, 7.0, 9.0]);

        // Second call with same length hits cache
        let mut out2 = [0.0f64; 3];
        jit.add(&[10.0, 20.0, 30.0], &[1.0, 2.0, 3.0], &mut out2)
            .unwrap();
        assert_eq!(out2, [11.0, 22.0, 33.0]);
    }

    #[test]
    fn test_mlir_matmul_2x2() {
        let mut jit = MlirTensorJit::new().unwrap();
        // [[1, 2], [3, 4]] × [[5, 6], [7, 8]] = [[19, 22], [43, 50]]
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0, 8.0];
        let mut out = [0.0f64; 4];
        jit.matmul(&a, &b, &mut out, 2, 2, 2).unwrap();
        assert_eq!(out, [19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    fn test_mlir_length_mismatch() {
        let mut jit = MlirTensorJit::new().unwrap();
        let mut out = [0.0f64; 3];
        let err = jit.add(&[1.0, 2.0], &[3.0, 4.0, 5.0], &mut out);
        assert!(err.is_err());
    }

    #[test]
    fn test_mlir_softmax() {
        let mut jit = MlirTensorJit::new().unwrap();
        let input = [1.0, 2.0, 3.0];
        let mut out = [0.0f64; 3];
        jit.softmax(&input, &mut out).unwrap();

        // Verify: softmax([1,2,3]) = [exp(1-3)/sum, exp(2-3)/sum, exp(3-3)/sum]
        let max_v = 3.0f64;
        let exps: Vec<f64> = input.iter().map(|x| (x - max_v).exp()).collect();
        let sum: f64 = exps.iter().sum();
        let expected: Vec<f64> = exps.iter().map(|e| e / sum).collect();

        for i in 0..3 {
            assert!(
                (out[i] - expected[i]).abs() < 1e-10,
                "softmax[{}]: expected {} got {}", i, expected[i], out[i]
            );
        }
        // Sum of softmax should be ~1.0
        let total: f64 = out.iter().sum();
        assert!((total - 1.0).abs() < 1e-10, "softmax sum should be 1.0, got {}", total);
    }

    #[test]
    fn test_mlir_softmax_uniform() {
        let mut jit = MlirTensorJit::new().unwrap();
        let input = [5.0, 5.0, 5.0, 5.0];
        let mut out = [0.0f64; 4];
        jit.softmax(&input, &mut out).unwrap();
        // Uniform input → uniform output (each 0.25)
        for i in 0..4 {
            assert!((out[i] - 0.25).abs() < 1e-10, "softmax[{}] should be 0.25, got {}", i, out[i]);
        }
    }

    #[test]
    fn test_mlir_transpose_2x3() {
        let mut jit = MlirTensorJit::new().unwrap();
        // [[1, 2, 3], [4, 5, 6]] → [[1, 4], [2, 5], [3, 6]]
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut out = [0.0f64; 6];
        jit.transpose(&input, &mut out, 2, 3).unwrap();
        assert_eq!(out, [1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }

    #[test]
    fn test_mlir_transpose_3x3() {
        let mut jit = MlirTensorJit::new().unwrap();
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let mut out = [0.0f64; 9];
        jit.transpose(&input, &mut out, 3, 3).unwrap();
        assert_eq!(out, [1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0]);
    }
}
