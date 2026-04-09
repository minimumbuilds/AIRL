//! GPU tensor operations via CUDA.
//!
//! Uses NVRTC (runtime compilation) to compile CUDA C kernel strings to PTX,
//! then the CUDA driver API to load, launch, and manage device memory.

use cudarc::driver::{CudaContext, CudaFunction, CudaSlice, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// GPU backend context for CUDA tensor operations.
pub struct GpuContext {
    stream: Arc<CudaStream>,
    kernels: HashMap<&'static str, CudaFunction>,
    /// Free-list pool of device buffers, keyed by element count.
    /// Avoids per-call CUDA malloc/free by reusing previously allocated slices.
    buf_pool: Mutex<HashMap<usize, Vec<CudaSlice<f64>>>>,
    /// NVRTC target architecture string, queried at startup.
    compute_arch: &'static str,
}

// ─── CUDA C kernel sources ──────────────────────────────────────────────────

const KERNEL_ADD: &str = r#"
extern "C" __global__ void tensor_add(const double* a, const double* b, double* out, int len) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < len) out[i] = a[i] + b[i];
}
"#;

const KERNEL_MUL: &str = r#"
extern "C" __global__ void tensor_mul(const double* a, const double* b, double* out, int len) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < len) out[i] = a[i] * b[i];
}
"#;

/// Tiled 16×16 shared-memory matmul.  Replaces the naive triple-loop kernel.
/// Using shared memory tiles eliminates redundant global memory reads and
/// dramatically reduces latency for large matrices.
const KERNEL_MATMUL: &str = r#"
#define TILE 16
extern "C" __global__ void tensor_matmul(const double* a, const double* b, double* out,
                                          int m, int k, int n) {
    __shared__ double tileA[TILE][TILE];
    __shared__ double tileB[TILE][TILE];

    int row = blockIdx.y * TILE + threadIdx.y;
    int col = blockIdx.x * TILE + threadIdx.x;
    double sum = 0.0;

    int numTiles = (k + TILE - 1) / TILE;
    for (int t = 0; t < numTiles; t++) {
        int aCol = t * TILE + threadIdx.x;
        int bRow = t * TILE + threadIdx.y;

        tileA[threadIdx.y][threadIdx.x] = (row < m && aCol < k) ? a[row * k + aCol] : 0.0;
        tileB[threadIdx.y][threadIdx.x] = (bRow < k && col < n) ? b[bRow * n + col] : 0.0;

        __syncthreads();

        for (int i = 0; i < TILE; i++) {
            sum += tileA[threadIdx.y][i] * tileB[i][threadIdx.x];
        }

        __syncthreads();
    }

    if (row < m && col < n) out[row * n + col] = sum;
}
"#;

const KERNEL_SOFTMAX: &str = r#"
extern "C" __global__ void tensor_softmax_max(const double* input, double* block_maxes, int len) {
    extern __shared__ double sdata[];
    int tid = threadIdx.x;
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    sdata[tid] = (i < len) ? input[i] : -1e308;
    __syncthreads();
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s && sdata[tid] < sdata[tid + s]) sdata[tid] = sdata[tid + s];
        __syncthreads();
    }
    if (tid == 0) block_maxes[blockIdx.x] = sdata[0];
}

extern "C" __global__ void tensor_softmax_exp_sum(const double* input, double* output,
                                                   double max_val, double* sum_out, int len) {
    extern __shared__ double sdata[];
    int tid = threadIdx.x;
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    double val = 0.0;
    if (i < len) { val = exp(input[i] - max_val); output[i] = val; }
    sdata[tid] = val;
    __syncthreads();
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] += sdata[tid + s];
        __syncthreads();
    }
    if (tid == 0) atomicAdd(sum_out, sdata[0]);
}

extern "C" __global__ void tensor_softmax_div(double* output, double sum_val, int len) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < len) output[i] /= sum_val;
}
"#;

const KERNEL_TRANSPOSE: &str = r#"
extern "C" __global__ void tensor_transpose(const double* input, double* output, int rows, int cols) {
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    if (row < rows && col < cols) output[col * rows + row] = input[row * cols + col];
}
"#;

// ─── Implementation ─────────────────────────────────────────────────────────

impl GpuContext {
    /// Try to create a GPU context. Returns None if no CUDA GPU is available.
    pub fn try_new() -> Option<Self> {
        let ctx = CudaContext::new(0).ok()?;
        let compute_arch = Self::query_compute_arch(&ctx);
        let stream = ctx.default_stream();
        let mut gpu = Self {
            stream,
            kernels: HashMap::new(),
            buf_pool: Mutex::new(HashMap::new()),
            compute_arch,
        };
        // Pre-compile all kernels; log and bail if any fail.
        if gpu.compile_kernel("tensor_add", KERNEL_ADD).is_err()
            || gpu.compile_kernel("tensor_mul", KERNEL_MUL).is_err()
            || gpu.compile_kernel("tensor_matmul", KERNEL_MATMUL).is_err()
            || gpu.compile_kernel("tensor_transpose", KERNEL_TRANSPOSE).is_err()
            || gpu.compile_kernels_multi(
                &["tensor_softmax_max", "tensor_softmax_exp_sum", "tensor_softmax_div"],
                KERNEL_SOFTMAX,
            ).is_err()
        {
            return None;
        }
        Some(gpu)
    }

    fn ctx(&self) -> &Arc<CudaContext> {
        self.stream.context()
    }

    /// Query the GPU's compute capability and return the matching arch string.
    /// Falls back to `compute_86` (Ampere) on any error.
    fn query_compute_arch(ctx: &Arc<CudaContext>) -> &'static str {
        let cap = ctx.compute_capability().unwrap_or((8, 6));
        match cap {
            (7, 0) => "compute_70",
            (7, 5) => "compute_75",
            (8, 0) => "compute_80",
            (8, 6) => "compute_86",
            (8, 9) => "compute_89",
            (9, 0) => "compute_90",
            (9, _) => "compute_90",  // Ada/Hopper variants
            _ => "compute_86",        // conservative fallback
        }
    }

    fn nvrtc_opts(&self) -> CompileOptions {
        CompileOptions {
            arch: Some(self.compute_arch),
            ..Default::default()
        }
    }

    fn compile_kernel(&mut self, name: &'static str, source: &str) -> Result<(), String> {
        let ptx = compile_ptx_with_opts(source, self.nvrtc_opts())
            .map_err(|e| {
                eprintln!("NVRTC compile error for '{}': {:?}\nPTX source:\n{}", name, e, source);
                format!("NVRTC compile '{}': {:?}", name, e)
            })?;
        let module = self.ctx().load_module(ptx)
            .map_err(|e| {
                eprintln!("CUDA load module error for '{}': {:?}", name, e);
                format!("Load module '{}': {:?}", name, e)
            })?;
        let func = module.load_function(name)
            .map_err(|e| format!("Load function '{}': {:?}", name, e))?;
        self.kernels.insert(name, func);
        Ok(())
    }

    fn compile_kernels_multi(
        &mut self,
        names: &[&'static str],
        source: &str,
    ) -> Result<(), String> {
        let ptx = compile_ptx_with_opts(source, self.nvrtc_opts())
            .map_err(|e| {
                eprintln!("NVRTC multi-compile error: {:?}\nPTX source:\n{}", e, source);
                format!("NVRTC compile multi: {:?}", e)
            })?;
        let module = self.ctx().load_module(ptx)
            .map_err(|e| {
                eprintln!("CUDA load module error (multi): {:?}", e);
                format!("Load module multi: {:?}", e)
            })?;
        for &name in names {
            let func = module.load_function(name)
                .map_err(|e| format!("Load function '{}': {:?}", name, e))?;
            self.kernels.insert(name, func);
        }
        Ok(())
    }

    fn cfg_1d(len: usize) -> LaunchConfig {
        let block = 256u32;
        let grid = ((len as u32) + block - 1) / block;
        LaunchConfig {
            grid_dim: (grid, 1, 1),
            block_dim: (block, 1, 1),
            shared_mem_bytes: 0,
        }
    }

    fn cfg_2d(rows: usize, cols: usize) -> LaunchConfig {
        let bx = 16u32;
        let by = 16u32;
        LaunchConfig {
            grid_dim: (((cols as u32) + bx - 1) / bx, ((rows as u32) + by - 1) / by, 1),
            block_dim: (bx, by, 1),
            shared_mem_bytes: 0,
        }
    }

    /// Launch config for tiled matmul: 16×16 blocks with shared memory for two tiles.
    fn cfg_matmul(m: usize, n: usize) -> LaunchConfig {
        const TILE: u32 = 16;
        // Two TILE×TILE tiles of f64 (8 bytes each)
        let shared_bytes = 2 * TILE * TILE * 8;
        LaunchConfig {
            grid_dim: (((n as u32) + TILE - 1) / TILE, ((m as u32) + TILE - 1) / TILE, 1),
            block_dim: (TILE, TILE, 1),
            shared_mem_bytes: shared_bytes,
        }
    }

    // ─── Buffer pool ─────────────────────────────────────────────────────────

    /// Acquire a device buffer of `n` f64 elements from the pool, or allocate
    /// a fresh one if the pool is empty for that size.
    fn acquire_buf(&self, n: usize) -> Result<CudaSlice<f64>, String> {
        let mut pool = self.buf_pool.lock().expect("buf_pool lock poisoned");
        if let Some(bufs) = pool.get_mut(&n) {
            if let Some(buf) = bufs.pop() {
                return Ok(buf);
            }
        }
        drop(pool);
        self.stream.alloc_zeros::<f64>(n).map_err(|e| format!("GPU alloc({}): {:?}", n, e))
    }

    /// Return a device buffer back to the pool for reuse.
    fn release_buf(&self, buf: CudaSlice<f64>, n: usize) {
        let mut pool = self.buf_pool.lock().expect("buf_pool lock poisoned");
        pool.entry(n).or_default().push(buf);
    }

    // ─── Operations ──────────────────────────────────────────────────────────

    /// Element-wise add on GPU.
    pub fn add(&self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        let mut d_a = self.acquire_buf(len)?;
        self.stream.memcpy_htod(a, &mut d_a).map_err(|e| format!("H2D a: {:?}", e))?;
        let mut d_b = self.acquire_buf(len)?;
        self.stream.memcpy_htod(b, &mut d_b).map_err(|e| format!("H2D b: {:?}", e))?;
        let mut d_out = self.acquire_buf(len)?;

        let func = self.kernels.get("tensor_add").ok_or("tensor_add not loaded")?;
        unsafe {
            self.stream.launch_builder(func)
                .arg(&d_a).arg(&d_b).arg(&mut d_out).arg(&(len as i32))
                .launch(Self::cfg_1d(len))
                .map_err(|e| format!("GPU add: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;
        let result = self.stream.clone_dtoh(&d_out).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);

        self.release_buf(d_a, len);
        self.release_buf(d_b, len);
        self.release_buf(d_out, len);
        Ok(())
    }

    /// Element-wise mul on GPU.
    pub fn mul(&self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        let mut d_a = self.acquire_buf(len)?;
        self.stream.memcpy_htod(a, &mut d_a).map_err(|e| format!("H2D a: {:?}", e))?;
        let mut d_b = self.acquire_buf(len)?;
        self.stream.memcpy_htod(b, &mut d_b).map_err(|e| format!("H2D b: {:?}", e))?;
        let mut d_out = self.acquire_buf(len)?;

        let func = self.kernels.get("tensor_mul").ok_or("tensor_mul not loaded")?;
        unsafe {
            self.stream.launch_builder(func)
                .arg(&d_a).arg(&d_b).arg(&mut d_out).arg(&(len as i32))
                .launch(Self::cfg_1d(len))
                .map_err(|e| format!("GPU mul: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;
        let result = self.stream.clone_dtoh(&d_out).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);

        self.release_buf(d_a, len);
        self.release_buf(d_b, len);
        self.release_buf(d_out, len);
        Ok(())
    }

    /// Matrix multiply on GPU using tiled shared-memory kernel.
    pub fn matmul(
        &self, a: &[f64], b: &[f64], out: &mut [f64],
        m: usize, k: usize, n: usize,
    ) -> Result<(), String> {
        let mut d_a = self.acquire_buf(m * k)?;
        self.stream.memcpy_htod(a, &mut d_a).map_err(|e| format!("H2D a: {:?}", e))?;
        let mut d_b = self.acquire_buf(k * n)?;
        self.stream.memcpy_htod(b, &mut d_b).map_err(|e| format!("H2D b: {:?}", e))?;
        let mut d_out = self.acquire_buf(m * n)?;

        let func = self.kernels.get("tensor_matmul").ok_or("tensor_matmul not loaded")?;
        unsafe {
            self.stream.launch_builder(func)
                .arg(&d_a).arg(&d_b).arg(&mut d_out)
                .arg(&(m as i32)).arg(&(k as i32)).arg(&(n as i32))
                .launch(Self::cfg_matmul(m, n))
                .map_err(|e| format!("GPU matmul: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;
        let result = self.stream.clone_dtoh(&d_out).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);

        self.release_buf(d_a, m * k);
        self.release_buf(d_b, k * n);
        self.release_buf(d_out, m * n);
        Ok(())
    }

    /// Numerically-stable softmax on GPU.
    pub fn softmax(&self, input: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = input.len();
        let block_size = 256usize;
        let grid_size = (len + block_size - 1) / block_size;

        let mut d_input = self.acquire_buf(len)?;
        self.stream.memcpy_htod(input, &mut d_input).map_err(|e| format!("H2D input: {:?}", e))?;
        let mut d_output = self.acquire_buf(len)?;
        let mut d_scratch = self.acquire_buf(grid_size)?;
        let mut d_sum = self.acquire_buf(1)?;

        let shared_bytes = (block_size * std::mem::size_of::<f64>()) as u32;

        // Pass 1: per-block max reduction
        let max_func = self.kernels.get("tensor_softmax_max").ok_or("softmax_max not loaded")?;
        let max_cfg = LaunchConfig {
            grid_dim: (grid_size as u32, 1, 1),
            block_dim: (block_size as u32, 1, 1),
            shared_mem_bytes: shared_bytes,
        };
        unsafe {
            self.stream.launch_builder(max_func)
                .arg(&d_input).arg(&mut d_scratch).arg(&(len as i32))
                .launch(max_cfg)
                .map_err(|e| format!("GPU softmax max: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;  // Wait for all block maxes to be written before reading d_scratch

        // Find global max on host
        let scratch_host = self.stream.clone_dtoh(&d_scratch).map_err(|e| format!("{:?}", e))?;
        let max_val = scratch_host.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Pass 2: exp(x - max) + sum accumulation
        let exp_func = self.kernels.get("tensor_softmax_exp_sum").ok_or("softmax_exp not loaded")?;
        let exp_cfg = LaunchConfig {
            grid_dim: (grid_size as u32, 1, 1),
            block_dim: (block_size as u32, 1, 1),
            shared_mem_bytes: shared_bytes,
        };
        unsafe {
            self.stream.launch_builder(exp_func)
                .arg(&d_input).arg(&mut d_output).arg(&max_val).arg(&mut d_sum).arg(&(len as i32))
                .launch(exp_cfg)
                .map_err(|e| format!("GPU softmax exp: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;  // Ensure all atomicAdd calls complete before division

        let sum_host = self.stream.clone_dtoh(&d_sum).map_err(|e| format!("{:?}", e))?;
        let sum_val = sum_host[0];

        // Pass 3: divide by sum
        let div_func = self.kernels.get("tensor_softmax_div").ok_or("softmax_div not loaded")?;
        unsafe {
            self.stream.launch_builder(div_func)
                .arg(&mut d_output).arg(&sum_val).arg(&(len as i32))
                .launch(Self::cfg_1d(len))
                .map_err(|e| format!("GPU softmax div: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;

        let result = self.stream.clone_dtoh(&d_output).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);

        self.release_buf(d_input, len);
        self.release_buf(d_output, len);
        self.release_buf(d_scratch, grid_size);
        self.release_buf(d_sum, 1);
        Ok(())
    }

    /// 2-D transpose on GPU.
    pub fn transpose(
        &self, input: &[f64], out: &mut [f64],
        rows: usize, cols: usize,
    ) -> Result<(), String> {
        let n = rows * cols;
        let mut d_input = self.acquire_buf(n)?;
        self.stream.memcpy_htod(input, &mut d_input).map_err(|e| format!("H2D input: {:?}", e))?;
        let mut d_out = self.acquire_buf(n)?;

        let func = self.kernels.get("tensor_transpose").ok_or("transpose not loaded")?;
        unsafe {
            self.stream.launch_builder(func)
                .arg(&d_input).arg(&mut d_out).arg(&(rows as i32)).arg(&(cols as i32))
                .launch(Self::cfg_2d(rows, cols))
                .map_err(|e| format!("GPU transpose: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;

        let result = self.stream.clone_dtoh(&d_out).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);

        self.release_buf(d_input, n);
        self.release_buf(d_out, n);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu() -> GpuContext {
        GpuContext::try_new().expect("CUDA GPU required for these tests")
    }

    #[test]
    fn test_gpu_add() {
        let ctx = gpu();
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [10.0, 20.0, 30.0, 40.0, 50.0];
        let mut out = [0.0f64; 5];
        ctx.add(&a, &b, &mut out).unwrap();
        assert_eq!(out, [11.0, 22.0, 33.0, 44.0, 55.0]);
    }

    #[test]
    fn test_gpu_mul() {
        let ctx = gpu();
        let a = [2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0];
        let mut out = [0.0f64; 3];
        ctx.mul(&a, &b, &mut out).unwrap();
        assert_eq!(out, [10.0, 18.0, 28.0]);
    }

    #[test]
    fn test_gpu_matmul_2x2() {
        let ctx = gpu();
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [5.0, 6.0, 7.0, 8.0];
        let mut out = [0.0f64; 4];
        ctx.matmul(&a, &b, &mut out, 2, 2, 2).unwrap();
        assert_eq!(out, [19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    fn test_gpu_softmax() {
        let ctx = gpu();
        let input = [1.0, 2.0, 3.0];
        let mut out = [0.0f64; 3];
        ctx.softmax(&input, &mut out).unwrap();

        let max_v = 3.0f64;
        let exps: Vec<f64> = input.iter().map(|x| (x - max_v).exp()).collect();
        let sum: f64 = exps.iter().sum();
        for i in 0..3 {
            assert!((out[i] - exps[i] / sum).abs() < 1e-10,
                "softmax[{}]: expected {} got {}", i, exps[i] / sum, out[i]);
        }
    }

    #[test]
    fn test_gpu_transpose_2x3() {
        let ctx = gpu();
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut out = [0.0f64; 6];
        ctx.transpose(&input, &mut out, 2, 3).unwrap();
        assert_eq!(out, [1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }

    #[test]
    fn test_gpu_large_add() {
        let ctx = gpu();
        let n = 10_000;
        let a: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let b: Vec<f64> = (0..n).map(|i| (n - i) as f64).collect();
        let mut out = vec![0.0f64; n];
        ctx.add(&a, &b, &mut out).unwrap();
        for i in 0..n {
            assert_eq!(out[i], n as f64);
        }
    }

    #[test]
    fn test_buf_pool_reuse() {
        // Call add twice with same size; second call should reuse pooled buffers.
        let ctx = gpu();
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let mut out = [0.0f64; 3];
        ctx.add(&a, &b, &mut out).unwrap();
        assert_eq!(out, [5.0, 7.0, 9.0]);
        // Pool should now have buffers; second call reuses them.
        let mut out2 = [0.0f64; 3];
        ctx.add(&a, &b, &mut out2).unwrap();
        assert_eq!(out2, [5.0, 7.0, 9.0]);
        // Verify pool is non-empty after operations
        let pool = ctx.buf_pool.lock().unwrap();
        assert!(!pool.is_empty(), "pool should contain reusable buffers");
    }
}
