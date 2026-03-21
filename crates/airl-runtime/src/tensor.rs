use airl_types::ty::PrimTy;
use crate::error::RuntimeError;

/// Runtime tensor value — stores all data as f64 for Phase 1 simplicity.
#[derive(Debug, Clone)]
pub struct TensorValue {
    pub dtype: PrimTy,
    pub shape: Vec<usize>,
    pub data: Vec<f64>,
}

impl TensorValue {
    /// Total number of elements for a given shape.
    fn num_elements(shape: &[usize]) -> usize {
        shape.iter().product()
    }

    /// Create a tensor of zeros.
    pub fn zeros(dtype: PrimTy, shape: Vec<usize>) -> Self {
        let n = Self::num_elements(&shape);
        Self { dtype, shape, data: vec![0.0; n] }
    }

    /// Create a tensor of ones.
    pub fn ones(dtype: PrimTy, shape: Vec<usize>) -> Self {
        let n = Self::num_elements(&shape);
        Self { dtype, shape, data: vec![1.0; n] }
    }

    /// Create a tensor filled with pseudo-random values in [0, 1) using a simple LCG.
    pub fn rand(dtype: PrimTy, shape: Vec<usize>, seed: u64) -> Self {
        let n = Self::num_elements(&shape);
        let mut data = Vec::with_capacity(n);
        let mut state = seed.wrapping_add(1); // avoid zero state
        for _ in 0..n {
            // LCG parameters from Numerical Recipes
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let val = (state >> 33) as f64 / (1u64 << 31) as f64;
            data.push(val);
        }
        Self { dtype, shape, data }
    }

    /// Create an NxN identity matrix.
    pub fn identity(dtype: PrimTy, n: usize) -> Self {
        let mut data = vec![0.0; n * n];
        for i in 0..n {
            data[i * n + i] = 1.0;
        }
        Self { dtype, shape: vec![n, n], data }
    }

    /// Element-wise addition.
    pub fn add(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> {
        if self.shape != other.shape {
            return Err(RuntimeError::ShapeMismatch {
                expected: self.shape.clone(),
                got: other.shape.clone(),
            });
        }
        let data: Vec<f64> = self.data.iter().zip(&other.data).map(|(a, b)| a + b).collect();
        Ok(TensorValue { dtype: self.dtype, shape: self.shape.clone(), data })
    }

    /// Element-wise multiplication.
    pub fn mul(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> {
        if self.shape != other.shape {
            return Err(RuntimeError::ShapeMismatch {
                expected: self.shape.clone(),
                got: other.shape.clone(),
            });
        }
        let data: Vec<f64> = self.data.iter().zip(&other.data).map(|(a, b)| a * b).collect();
        Ok(TensorValue { dtype: self.dtype, shape: self.shape.clone(), data })
    }

    /// Matrix multiplication (2D only): [M, K] x [K, N] -> [M, N].
    pub fn matmul(&self, other: &TensorValue) -> Result<TensorValue, RuntimeError> {
        if self.shape.len() != 2 || other.shape.len() != 2 {
            return Err(RuntimeError::TypeError(
                "matmul requires 2D tensors".into(),
            ));
        }
        let (m, k1) = (self.shape[0], self.shape[1]);
        let (k2, n) = (other.shape[0], other.shape[1]);
        if k1 != k2 {
            return Err(RuntimeError::ShapeMismatch {
                expected: vec![m, k1],
                got: vec![k2, n],
            });
        }
        let mut data = vec![0.0; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for p in 0..k1 {
                    sum += self.data[i * k1 + p] * other.data[p * n + j];
                }
                data[i * n + j] = sum;
            }
        }
        Ok(TensorValue { dtype: self.dtype, shape: vec![m, n], data })
    }

    /// Reshape tensor to a new shape. Total element count must match.
    pub fn reshape(&self, new_shape: Vec<usize>) -> Result<TensorValue, RuntimeError> {
        let old_count = Self::num_elements(&self.shape);
        let new_count = Self::num_elements(&new_shape);
        if old_count != new_count {
            return Err(RuntimeError::ShapeMismatch {
                expected: self.shape.clone(),
                got: new_shape,
            });
        }
        Ok(TensorValue {
            dtype: self.dtype,
            shape: new_shape,
            data: self.data.clone(),
        })
    }

    /// Transpose a 2D tensor.
    pub fn transpose(&self) -> Result<TensorValue, RuntimeError> {
        if self.shape.len() != 2 {
            return Err(RuntimeError::TypeError(
                "transpose requires a 2D tensor".into(),
            ));
        }
        let (rows, cols) = (self.shape[0], self.shape[1]);
        let mut data = vec![0.0; rows * cols];
        for i in 0..rows {
            for j in 0..cols {
                data[j * rows + i] = self.data[i * cols + j];
            }
        }
        Ok(TensorValue {
            dtype: self.dtype,
            shape: vec![cols, rows],
            data,
        })
    }

    /// Softmax over the last axis (flattened if 1D, row-wise if 2D).
    pub fn softmax(&self) -> TensorValue {
        if self.data.is_empty() {
            return self.clone();
        }

        // Determine the size of the last axis
        let last_dim = *self.shape.last().unwrap_or(&1);
        let num_rows = self.data.len() / last_dim;

        let mut data = vec![0.0; self.data.len()];
        for row in 0..num_rows {
            let start = row * last_dim;
            let end = start + last_dim;
            let row_data = &self.data[start..end];

            // Numerical stability: subtract max
            let max_val = row_data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let exps: Vec<f64> = row_data.iter().map(|x| (x - max_val).exp()).collect();
            let sum: f64 = exps.iter().sum();
            for (i, e) in exps.iter().enumerate() {
                data[start + i] = e / sum;
            }
        }

        TensorValue { dtype: self.dtype, shape: self.shape.clone(), data }
    }

    /// Sum of all elements.
    pub fn sum(&self) -> f64 {
        self.data.iter().sum()
    }

    /// Max of all elements.
    pub fn max(&self) -> f64 {
        self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }

    /// Slice along the first axis: tensor[start..end].
    pub fn slice(&self, start: usize, end: usize) -> Result<TensorValue, RuntimeError> {
        if self.shape.is_empty() {
            return Err(RuntimeError::IndexOutOfBounds { index: start, len: 0 });
        }
        let first_dim = self.shape[0];
        if start > first_dim || end > first_dim || start > end {
            return Err(RuntimeError::IndexOutOfBounds {
                index: end,
                len: first_dim,
            });
        }
        let stride: usize = if self.shape.len() > 1 {
            self.shape[1..].iter().product()
        } else {
            1
        };
        let data_start = start * stride;
        let data_end = end * stride;
        let data = self.data[data_start..data_end].to_vec();
        let mut new_shape = self.shape.clone();
        new_shape[0] = end - start;
        Ok(TensorValue { dtype: self.dtype, shape: new_shape, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_correct_shape_and_values() {
        let t = TensorValue::zeros(PrimTy::F32, vec![2, 3]);
        assert_eq!(t.shape, vec![2, 3]);
        assert_eq!(t.data.len(), 6);
        assert!(t.data.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn ones_correct_shape_and_values() {
        let t = TensorValue::ones(PrimTy::F64, vec![3, 2]);
        assert_eq!(t.shape, vec![3, 2]);
        assert_eq!(t.data.len(), 6);
        assert!(t.data.iter().all(|&x| x == 1.0));
    }

    #[test]
    fn rand_correct_shape_and_range() {
        let t = TensorValue::rand(PrimTy::F32, vec![10, 10], 42);
        assert_eq!(t.shape, vec![10, 10]);
        assert_eq!(t.data.len(), 100);
        assert!(t.data.iter().all(|&x| (0.0..1.0).contains(&x)));
    }

    #[test]
    fn rand_different_seeds_different_values() {
        let t1 = TensorValue::rand(PrimTy::F32, vec![5], 1);
        let t2 = TensorValue::rand(PrimTy::F32, vec![5], 2);
        assert_ne!(t1.data, t2.data);
    }

    #[test]
    fn identity_creates_correct_matrix() {
        let t = TensorValue::identity(PrimTy::F64, 3);
        assert_eq!(t.shape, vec![3, 3]);
        assert_eq!(t.data, vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn add_matching_shapes() {
        let a = TensorValue::ones(PrimTy::F32, vec![2, 2]);
        let b = TensorValue::ones(PrimTy::F32, vec![2, 2]);
        let c = a.add(&b).unwrap();
        assert_eq!(c.shape, vec![2, 2]);
        assert!(c.data.iter().all(|&x| x == 2.0));
    }

    #[test]
    fn add_mismatched_shapes_fails() {
        let a = TensorValue::ones(PrimTy::F32, vec![2, 3]);
        let b = TensorValue::ones(PrimTy::F32, vec![3, 2]);
        assert!(a.add(&b).is_err());
    }

    #[test]
    fn mul_elementwise() {
        let a = TensorValue { dtype: PrimTy::F64, shape: vec![3], data: vec![1.0, 2.0, 3.0] };
        let b = TensorValue { dtype: PrimTy::F64, shape: vec![3], data: vec![4.0, 5.0, 6.0] };
        let c = a.mul(&b).unwrap();
        assert_eq!(c.data, vec![4.0, 10.0, 18.0]);
    }

    #[test]
    fn matmul_2x3_times_3x2() {
        // [[1,2,3],[4,5,6]] * [[7,8],[9,10],[11,12]] = [[58,64],[139,154]]
        let a = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![2, 3],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        };
        let b = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![3, 2],
            data: vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
        };
        let c = a.matmul(&b).unwrap();
        assert_eq!(c.shape, vec![2, 2]);
        assert_eq!(c.data, vec![58.0, 64.0, 139.0, 154.0]);
    }

    #[test]
    fn matmul_dimension_mismatch_fails() {
        let a = TensorValue::ones(PrimTy::F64, vec![2, 3]);
        let b = TensorValue::ones(PrimTy::F64, vec![2, 3]);
        assert!(a.matmul(&b).is_err());
    }

    #[test]
    fn matmul_non_2d_fails() {
        let a = TensorValue::ones(PrimTy::F64, vec![2, 3, 4]);
        let b = TensorValue::ones(PrimTy::F64, vec![4, 2]);
        assert!(a.matmul(&b).is_err());
    }

    #[test]
    fn reshape_preserves_data() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![2, 3],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        };
        let r = t.reshape(vec![3, 2]).unwrap();
        assert_eq!(r.shape, vec![3, 2]);
        assert_eq!(r.data, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn reshape_invalid_count_fails() {
        let t = TensorValue::ones(PrimTy::F64, vec![2, 3]);
        assert!(t.reshape(vec![2, 2]).is_err());
    }

    #[test]
    fn transpose_2d() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![2, 3],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        };
        let r = t.transpose().unwrap();
        assert_eq!(r.shape, vec![3, 2]);
        assert_eq!(r.data, vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    }

    #[test]
    fn transpose_non_2d_fails() {
        let t = TensorValue::ones(PrimTy::F64, vec![2, 3, 4]);
        assert!(t.transpose().is_err());
    }

    #[test]
    fn softmax_sums_to_one() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![4],
            data: vec![1.0, 2.0, 3.0, 4.0],
        };
        let s = t.softmax();
        let total: f64 = s.data.iter().sum();
        assert!((total - 1.0).abs() < 1e-10, "softmax sum = {}", total);
    }

    #[test]
    fn softmax_preserves_ordering() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![3],
            data: vec![1.0, 3.0, 2.0],
        };
        let s = t.softmax();
        assert!(s.data[1] > s.data[2]);
        assert!(s.data[2] > s.data[0]);
    }

    #[test]
    fn softmax_2d_row_wise() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![2, 3],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        };
        let s = t.softmax();
        // Each row should sum to ~1.0
        let row1_sum: f64 = s.data[0..3].iter().sum();
        let row2_sum: f64 = s.data[3..6].iter().sum();
        assert!((row1_sum - 1.0).abs() < 1e-10);
        assert!((row2_sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn sum_and_max() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![4],
            data: vec![1.0, 5.0, 3.0, 2.0],
        };
        assert_eq!(t.sum(), 11.0);
        assert_eq!(t.max(), 5.0);
    }

    #[test]
    fn slice_first_axis() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![4, 2],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        };
        let s = t.slice(1, 3).unwrap();
        assert_eq!(s.shape, vec![2, 2]);
        assert_eq!(s.data, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn slice_out_of_bounds_fails() {
        let t = TensorValue::ones(PrimTy::F64, vec![3]);
        assert!(t.slice(0, 5).is_err());
    }

    #[test]
    fn slice_1d() {
        let t = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![5],
            data: vec![10.0, 20.0, 30.0, 40.0, 50.0],
        };
        let s = t.slice(1, 4).unwrap();
        assert_eq!(s.shape, vec![3]);
        assert_eq!(s.data, vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn identity_matmul_is_identity() {
        let eye = TensorValue::identity(PrimTy::F64, 3);
        let a = TensorValue {
            dtype: PrimTy::F64,
            shape: vec![3, 3],
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
        };
        let result = a.matmul(&eye).unwrap();
        assert_eq!(result.data, a.data);
    }
}
