use crate::shape::{Shape, ShapeError};
use crate::simd;
use crate::tensor::Tensor;

/// Error type for tensor operations.
#[derive(Debug, Clone)]
pub enum OpError {
    Shape(ShapeError),
    InvalidOperation(String),
}

impl From<ShapeError> for OpError {
    fn from(e: ShapeError) -> Self {
        OpError::Shape(e)
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Shape(e) => write!(f, "{}", e),
            OpError::InvalidOperation(msg) => write!(f, "invalid operation: {}", msg),
        }
    }
}

impl std::error::Error for OpError {}

// ── Broadcasting helper ─────────────────────────────────────────────────

/// Apply a binary element-wise operation with NumPy-style broadcasting.
fn broadcast_binary_op(
    a: &Tensor,
    b: &Tensor,
    op: impl Fn(f64, f64) -> f64,
) -> Result<Tensor, OpError> {
    let out_shape = a.shape().broadcast_with(b.shape())?;
    let n = out_shape.numel();
    let out_dims = out_shape.dims();
    let a_dims = a.shape().dims();
    let b_dims = b.shape().dims();
    let ndim = out_dims.len();

    let mut data = Vec::with_capacity(n);

    for flat in 0..n {
        // Convert flat index to multi-dim index in output shape
        let mut remaining = flat;
        let mut out_idx = vec![0usize; ndim];
        for d in (0..ndim).rev() {
            if out_dims[d] > 0 {
                out_idx[d] = remaining % out_dims[d];
                remaining /= out_dims[d];
            }
        }

        // Map output index to a's index (broadcast: if a_dim==1, use 0)
        let a_offset_ndim = ndim.saturating_sub(a_dims.len());
        let mut a_flat = 0usize;
        let a_strides = a.strides();
        for d in 0..a_dims.len() {
            let out_d = d + a_offset_ndim;
            let idx = if a_dims[d] == 1 { 0 } else { out_idx[out_d] };
            if d < a_strides.len() {
                a_flat += idx * a_strides[d];
            }
        }

        // Map output index to b's index
        let b_offset_ndim = ndim.saturating_sub(b_dims.len());
        let mut b_flat = 0usize;
        let b_strides = b.strides();
        for d in 0..b_dims.len() {
            let out_d = d + b_offset_ndim;
            let idx = if b_dims[d] == 1 { 0 } else { out_idx[out_d] };
            if d < b_strides.len() {
                b_flat += idx * b_strides[d];
            }
        }

        let va = if a.shape().is_scalar() {
            a.data()[0]
        } else {
            a.data()[a_flat]
        };
        let vb = if b.shape().is_scalar() {
            b.data()[0]
        } else {
            b.data()[b_flat]
        };

        data.push(op(va, vb));
    }

    Ok(Tensor::from_vec(data, out_shape)?)
}

// ── Element-wise binary ops ─────────────────────────────────────────────

/// Element-wise addition with broadcasting.
pub fn add(a: &Tensor, b: &Tensor) -> Result<Tensor, OpError> {
    broadcast_binary_op(a, b, |x, y| x + y)
}

/// Element-wise subtraction with broadcasting.
pub fn sub(a: &Tensor, b: &Tensor) -> Result<Tensor, OpError> {
    broadcast_binary_op(a, b, |x, y| x - y)
}

/// Element-wise multiplication with broadcasting.
pub fn mul(a: &Tensor, b: &Tensor) -> Result<Tensor, OpError> {
    broadcast_binary_op(a, b, |x, y| x * y)
}

/// Element-wise division with broadcasting.
pub fn div(a: &Tensor, b: &Tensor) -> Result<Tensor, OpError> {
    broadcast_binary_op(a, b, |x, y| x / y)
}

// ── Unary ops ───────────────────────────────────────────────────────────

fn unary_op(a: &Tensor, op: impl Fn(f64) -> f64) -> Tensor {
    let data: Vec<f64> = a.data().iter().map(|&x| op(x)).collect();
    Tensor::from_vec(data, a.shape().clone()).unwrap()
}

/// Element-wise negation.
pub fn neg(a: &Tensor) -> Tensor {
    unary_op(a, |x| -x)
}

/// Element-wise exponential.
pub fn exp(a: &Tensor) -> Tensor {
    unary_op(a, f64::exp)
}

/// Element-wise natural logarithm.
pub fn log(a: &Tensor) -> Tensor {
    unary_op(a, f64::ln)
}

/// Element-wise ReLU: max(0, x).
pub fn relu(a: &Tensor) -> Tensor {
    unary_op(a, |x| x.max(0.0))
}

/// Element-wise sigmoid: 1 / (1 + exp(-x)).
pub fn sigmoid(a: &Tensor) -> Tensor {
    unary_op(a, |x| 1.0 / (1.0 + (-x).exp()))
}

/// Element-wise tanh.
pub fn tanh(a: &Tensor) -> Tensor {
    unary_op(a, f64::tanh)
}

// ── Reduction ops ───────────────────────────────────────────────────────

/// Sum all elements, returning a scalar tensor.
pub fn sum(a: &Tensor) -> Tensor {
    let s: f64 = a.data().iter().sum();
    Tensor::scalar(s)
}

/// Mean of all elements, returning a scalar tensor.
pub fn mean(a: &Tensor) -> Tensor {
    let n = a.numel() as f64;
    let s: f64 = a.data().iter().sum();
    Tensor::scalar(s / n)
}

// ── Matrix ops ──────────────────────────────────────────────────────────

/// Matrix multiplication with shape validation.
///
/// Supports:
/// - (m, k) @ (k, n) -> (m, n)
/// - (m, k) @ (k,)   -> (m,)
/// - (k,) @ (k, n)   -> (n,)
/// - (k,) @ (k,)     -> scalar (dot product)
pub fn matmul(a: &Tensor, b: &Tensor) -> Result<Tensor, OpError> {
    let out_shape = Shape::matmul_shape(a.shape(), b.shape())?;

    match (a.ndim(), b.ndim()) {
        (1, 1) => {
            // Dot product — SIMD-accelerated
            let dot = simd::simd_dot_product(a.data(), b.data());
            Ok(Tensor::scalar(dot))
        }
        (2, 1) => {
            // Matrix-vector: each output element is a dot product of a row of A
            // with the vector B — SIMD-accelerated.
            let m = a.shape().dims()[0];
            let k = a.shape().dims()[1];
            let mut data = vec![0.0; m];
            let a_data = a.data();
            let b_data = b.data();
            for (i, data_i) in data.iter_mut().enumerate() {
                let row = &a_data[i * k..(i + 1) * k];
                *data_i = simd::simd_dot_product(row, b_data);
            }
            Ok(Tensor::from_vec(data, out_shape)?)
        }
        (1, 2) => {
            let k = b.shape().dims()[0];
            let n = b.shape().dims()[1];
            let mut data = vec![0.0; n];
            for (j, data_j) in data.iter_mut().enumerate() {
                let mut s = 0.0;
                for i in 0..k {
                    s += a.data()[i] * b.data()[i * n + j];
                }
                *data_j = s;
            }
            Ok(Tensor::from_vec(data, out_shape)?)
        }
        (2, 2) => {
            // General matmul: use SIMD for each (row_A · col_B) dot product.
            // We transpose B so columns become contiguous rows, then dot-product.
            let m = a.shape().dims()[0];
            let k = a.shape().dims()[1];
            let n = b.shape().dims()[1];
            let a_data = a.data();

            // Transpose B into column-major layout for contiguous column access.
            let b_data = b.data();
            let mut bt = vec![0.0f64; k * n];
            for r in 0..k {
                for c in 0..n {
                    bt[c * k + r] = b_data[r * n + c];
                }
            }

            let mut data = vec![0.0; m * n];
            for i in 0..m {
                let row_a = &a_data[i * k..(i + 1) * k];
                for j in 0..n {
                    let col_b = &bt[j * k..(j + 1) * k];
                    data[i * n + j] = simd::simd_dot_product(row_a, col_b);
                }
            }
            Ok(Tensor::from_vec(data, out_shape)?)
        }
        _ => Err(OpError::InvalidOperation(format!(
            "matmul not supported for {}D x {}D tensors",
            a.ndim(),
            b.ndim()
        ))),
    }
}

/// 2-D matrix transpose.
pub fn transpose(a: &Tensor) -> Result<Tensor, OpError> {
    if a.ndim() != 2 {
        return Err(OpError::InvalidOperation(format!(
            "transpose requires 2D tensor, got {}D",
            a.ndim()
        )));
    }
    let rows = a.shape().dims()[0];
    let cols = a.shape().dims()[1];
    let mut data = vec![0.0; rows * cols];
    for i in 0..rows {
        for j in 0..cols {
            data[j * rows + i] = a.data()[i * cols + j];
        }
    }
    Ok(Tensor::from_vec(data, Shape::new(vec![cols, rows]))?)
}

// ── std::ops implementations for &Tensor ────────────────────────────────

impl std::ops::Add for &Tensor {
    type Output = Tensor;
    fn add(self, rhs: &Tensor) -> Tensor {
        crate::ops::add(self, rhs).expect("add: shapes not broadcast-compatible")
    }
}

impl std::ops::Sub for &Tensor {
    type Output = Tensor;
    fn sub(self, rhs: &Tensor) -> Tensor {
        crate::ops::sub(self, rhs).expect("sub: shapes not broadcast-compatible")
    }
}

impl std::ops::Mul for &Tensor {
    type Output = Tensor;
    fn mul(self, rhs: &Tensor) -> Tensor {
        crate::ops::mul(self, rhs).expect("mul: shapes not broadcast-compatible")
    }
}

impl std::ops::Div for &Tensor {
    type Output = Tensor;
    fn div(self, rhs: &Tensor) -> Tensor {
        crate::ops::div(self, rhs).expect("div: shapes not broadcast-compatible")
    }
}

impl std::ops::Neg for &Tensor {
    type Output = Tensor;
    fn neg(self) -> Tensor {
        crate::ops::neg(self)
    }
}
