use crate::dtype::DType;
use crate::shape::{Shape, ShapeError};

/// A multi-dimensional array of f64 values with optional gradient tracking.
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Flat storage in row-major (C-contiguous) order.
    data: Vec<f64>,
    /// Shape of the tensor.
    shape: Shape,
    /// Strides for indexing into flat storage.
    strides: Vec<usize>,
    /// Element data type (always F64 for now).
    dtype: DType,
    /// Whether this tensor tracks gradients.
    requires_grad: bool,
    /// Accumulated gradient (set after backward pass).
    grad: Option<Box<Tensor>>,
}

impl Tensor {
    // ── Constructors ────────────────────────────────────────────────────

    /// Create a tensor of zeros with the given shape.
    pub fn zeros(shape: Shape) -> Self {
        let n = shape.numel();
        let strides = shape.strides();
        Tensor {
            data: vec![0.0; n],
            shape,
            strides,
            dtype: DType::F64,
            requires_grad: false,
            grad: None,
        }
    }

    /// Create a tensor of ones with the given shape.
    pub fn ones(shape: Shape) -> Self {
        let n = shape.numel();
        let strides = shape.strides();
        Tensor {
            data: vec![1.0; n],
            shape,
            strides,
            dtype: DType::F64,
            requires_grad: false,
            grad: None,
        }
    }

    /// Create a tensor from a flat data vector and a shape.
    ///
    /// Returns `Err` if the data length doesn't match the shape's element count.
    pub fn from_vec(data: Vec<f64>, shape: Shape) -> Result<Self, ShapeError> {
        if data.len() != shape.numel() {
            return Err(ShapeError::ReshapeIncompatible {
                from_numel: data.len(),
                to_numel: shape.numel(),
            });
        }
        let strides = shape.strides();
        Ok(Tensor {
            data,
            shape,
            strides,
            dtype: DType::F64,
            requires_grad: false,
            grad: None,
        })
    }

    /// Create a scalar tensor (0-dimensional) containing a single value.
    pub fn scalar(value: f64) -> Self {
        Tensor {
            data: vec![value],
            shape: Shape::scalar(),
            strides: vec![],
            dtype: DType::F64,
            requires_grad: false,
            grad: None,
        }
    }

    /// Create a tensor of random values drawn from N(0, 1) using Box-Muller.
    ///
    /// Uses a simple LCG PRNG seeded from the shape for reproducibility in
    /// tests. Not cryptographically secure.
    pub fn randn(shape: Shape) -> Self {
        let n = shape.numel();
        let strides = shape.strides();

        // Simple LCG PRNG (not for crypto, just for generating test data)
        let mut seed: u64 = 42;
        let mut next_u64 = move || -> u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };
        let mut next_f64 = || -> f64 {
            let bits = next_u64();
            // Map to (0, 1) exclusive
            (bits >> 11) as f64 / (1u64 << 53) as f64
        };

        let mut data = Vec::with_capacity(n);
        // Box-Muller transform: generate pairs of normal values
        let mut i = 0;
        while i < n {
            let u1 = next_f64().max(1e-15); // avoid log(0)
            let u2 = next_f64();
            let r = (-2.0 * u1.ln()).sqrt();
            let theta = 2.0 * std::f64::consts::PI * u2;
            data.push(r * theta.cos());
            if i + 1 < n {
                data.push(r * theta.sin());
            }
            i += 2;
        }
        data.truncate(n);

        Tensor {
            data,
            shape,
            strides,
            dtype: DType::F64,
            requires_grad: false,
            grad: None,
        }
    }

    // ── Accessors ───────────────────────────────────────────────────────

    /// Returns the shape of this tensor.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Returns the number of dimensions.
    pub fn ndim(&self) -> usize {
        self.shape.ndim()
    }

    /// Returns the total number of elements.
    pub fn numel(&self) -> usize {
        self.shape.numel()
    }

    /// Returns the data type.
    pub fn dtype(&self) -> DType {
        self.dtype
    }

    /// Returns a reference to the flat data.
    pub fn data(&self) -> &[f64] {
        &self.data
    }

    /// Returns a mutable reference to the flat data.
    pub fn data_mut(&mut self) -> &mut [f64] {
        &mut self.data
    }

    /// Returns the strides.
    pub fn strides(&self) -> &[usize] {
        &self.strides
    }

    /// Whether this tensor requires gradient computation.
    pub fn requires_grad(&self) -> bool {
        self.requires_grad
    }

    /// Set whether this tensor requires gradient computation.
    pub fn set_requires_grad(&mut self, val: bool) {
        self.requires_grad = val;
    }

    /// Returns the gradient tensor, if set.
    pub fn grad(&self) -> Option<&Tensor> {
        self.grad.as_deref()
    }

    /// Set the gradient for this tensor.
    pub fn set_grad(&mut self, grad: Tensor) {
        self.grad = Some(Box::new(grad));
    }

    // ── Indexing ────────────────────────────────────────────────────────

    /// Convert multi-dimensional indices to a flat offset.
    fn flat_index(&self, indices: &[usize]) -> Result<usize, ShapeError> {
        let dims = self.shape.dims();
        if indices.len() != dims.len() {
            return Err(ShapeError::DimensionMismatch {
                expected: dims.len(),
                got: indices.len(),
            });
        }
        for (i, (&idx, &dim)) in indices.iter().zip(dims.iter()).enumerate() {
            if idx >= dim {
                return Err(ShapeError::IndexOutOfBounds {
                    index: indices.to_vec(),
                    shape: dims.to_vec(),
                });
            }
            let _ = i;
        }
        let offset: usize = indices
            .iter()
            .zip(self.strides.iter())
            .map(|(&i, &s)| i * s)
            .sum();
        Ok(offset)
    }

    /// Get the value at the given multi-dimensional index.
    pub fn get(&self, indices: &[usize]) -> Result<f64, ShapeError> {
        if self.shape.is_scalar() && indices.is_empty() {
            return Ok(self.data[0]);
        }
        let offset = self.flat_index(indices)?;
        Ok(self.data[offset])
    }

    /// Set the value at the given multi-dimensional index.
    pub fn set(&mut self, indices: &[usize], val: f64) -> Result<(), ShapeError> {
        if self.shape.is_scalar() && indices.is_empty() {
            self.data[0] = val;
            return Ok(());
        }
        let offset = self.flat_index(indices)?;
        self.data[offset] = val;
        Ok(())
    }

    // ── Reshape ─────────────────────────────────────────────────────────

    /// Reshape the tensor to a new shape. The total element count must match.
    pub fn reshape(&self, new_shape: Shape) -> Result<Tensor, ShapeError> {
        if self.numel() != new_shape.numel() {
            return Err(ShapeError::ReshapeIncompatible {
                from_numel: self.numel(),
                to_numel: new_shape.numel(),
            });
        }
        let strides = new_shape.strides();
        Ok(Tensor {
            data: self.data.clone(),
            shape: new_shape,
            strides,
            dtype: self.dtype,
            requires_grad: self.requires_grad,
            grad: None,
        })
    }

    /// Return the scalar value if this is a 0-d or 1-element tensor.
    pub fn to_scalar(&self) -> Option<f64> {
        if self.data.len() == 1 {
            Some(self.data[0])
        } else {
            None
        }
    }
}

impl PartialEq for Tensor {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape && self.data == other.data
    }
}
