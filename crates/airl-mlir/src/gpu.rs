//! GPU tensor operations via CUDA.
//!
//! Uses NVRTC (runtime compilation) to compile CUDA C kernel strings to PTX,
//! then the CUDA driver API to load, launch, and manage device memory.

use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions};
use std::collections::HashMap;
use std::sync::Arc;

/// GPU backend context for CUDA tensor operations.
pub struct GpuContext {
    stream: Arc<CudaStream>,
    kernels: HashMap<&'static str, CudaFunction>,
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

const KERNEL_MATMUL: &str = r#"
extern "C" __global__ void tensor_matmul(const double* a, const double* b, double* out,
                                          int m, int k, int n) {
    int row = blockIdx.y * blockDim.y + threadIdx.y;
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    if (row < m && col < n) {
        double sum = 0.0;
        for (int p = 0; p < k; p++) {
            sum += a[row * k + p] * b[p * n + col];
        }
        out[row * n + col] = sum;
    }
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
        let stream = ctx.default_stream();
        let mut gpu = Self {
            stream,
            kernels: HashMap::new(),
        };
        // Pre-compile all kernels
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

    fn nvrtc_opts() -> CompileOptions {
        // TODO: query the GPU's compute capability at runtime via cuDeviceGetAttribute
        // instead of hardcoding. compute_86 targets Ampere (RTX 3xxx / A-series).
        CompileOptions {
            arch: Some("compute_86"),
            ..Default::default()
        }
    }

    fn compile_kernel(&mut self, name: &'static str, source: &str) -> Result<(), String> {
        let ptx = compile_ptx_with_opts(source, Self::nvrtc_opts())
            .map_err(|e| format!("NVRTC compile '{}': {:?}", name, e))?;
        let module = self.ctx().load_module(ptx)
            .map_err(|e| format!("Load module '{}': {:?}", name, e))?;
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
        let ptx = compile_ptx_with_opts(source, Self::nvrtc_opts())
            .map_err(|e| format!("NVRTC compile multi: {:?}", e))?;
        let module = self.ctx().load_module(ptx)
            .map_err(|e| format!("Load module multi: {:?}", e))?;
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

    /// Element-wise add on GPU.
    pub fn add(&self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        let d_a = self.stream.clone_htod(a).map_err(|e| format!("{:?}", e))?;
        let d_b = self.stream.clone_htod(b).map_err(|e| format!("{:?}", e))?;
        let mut d_out = self.stream.alloc_zeros::<f64>(len).map_err(|e| format!("{:?}", e))?;

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
        Ok(())
    }

    /// Element-wise mul on GPU.
    pub fn mul(&self, a: &[f64], b: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = a.len();
        let d_a = self.stream.clone_htod(a).map_err(|e| format!("{:?}", e))?;
        let d_b = self.stream.clone_htod(b).map_err(|e| format!("{:?}", e))?;
        let mut d_out = self.stream.alloc_zeros::<f64>(len).map_err(|e| format!("{:?}", e))?;

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
        Ok(())
    }

    /// Matrix multiply on GPU.
    pub fn matmul(
        &self, a: &[f64], b: &[f64], out: &mut [f64],
        m: usize, k: usize, n: usize,
    ) -> Result<(), String> {
        let d_a = self.stream.clone_htod(a).map_err(|e| format!("{:?}", e))?;
        let d_b = self.stream.clone_htod(b).map_err(|e| format!("{:?}", e))?;
        let mut d_out = self.stream.alloc_zeros::<f64>(m * n).map_err(|e| format!("{:?}", e))?;

        let func = self.kernels.get("tensor_matmul").ok_or("tensor_matmul not loaded")?;
        unsafe {
            self.stream.launch_builder(func)
                .arg(&d_a).arg(&d_b).arg(&mut d_out)
                .arg(&(m as i32)).arg(&(k as i32)).arg(&(n as i32))
                .launch(Self::cfg_2d(m, n))
                .map_err(|e| format!("GPU matmul: {:?}", e))?;
        }
        self.stream.synchronize().map_err(|e| format!("{:?}", e))?;
        let result = self.stream.clone_dtoh(&d_out).map_err(|e| format!("{:?}", e))?;
        out.copy_from_slice(&result);
        Ok(())
    }

    /// Numerically-stable softmax on GPU.
    pub fn softmax(&self, input: &[f64], out: &mut [f64]) -> Result<(), String> {
        let len = input.len();
        let block_size = 256usize;
        let grid_size = (len + block_size - 1) / block_size;

        let d_input = self.stream.clone_htod(input).map_err(|e| format!("{:?}", e))?;
        let mut d_output = self.stream.alloc_zeros::<f64>(len).map_err(|e| format!("{:?}", e))?;
        let mut d_scratch = self.stream.alloc_zeros::<f64>(grid_size).map_err(|e| format!("{:?}", e))?;
        let mut d_sum = self.stream.alloc_zeros::<f64>(1).map_err(|e| format!("{:?}", e))?;

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
        Ok(())
    }

    /// 2-D transpose on GPU.
    pub fn transpose(
        &self, input: &[f64], out: &mut [f64],
        rows: usize, cols: usize,
    ) -> Result<(), String> {
        let d_input = self.stream.clone_htod(input).map_err(|e| format!("{:?}", e))?;
        let mut d_out = self.stream.alloc_zeros::<f64>(rows * cols).map_err(|e| format!("{:?}", e))?;

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
}
