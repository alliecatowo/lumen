use std::fmt;

/// Error type for shape-related operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeError {
    /// Shapes are incompatible for broadcasting.
    BroadcastIncompatible {
        shape_a: Vec<usize>,
        shape_b: Vec<usize>,
    },
    /// Shapes are incompatible for matrix multiplication.
    MatMulIncompatible {
        shape_a: Vec<usize>,
        shape_b: Vec<usize>,
    },
    /// Reshape would change total element count.
    ReshapeIncompatible { from_numel: usize, to_numel: usize },
    /// Index out of bounds.
    IndexOutOfBounds {
        index: Vec<usize>,
        shape: Vec<usize>,
    },
    /// Wrong number of dimensions for indexing.
    DimensionMismatch { expected: usize, got: usize },
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::BroadcastIncompatible { shape_a, shape_b } => {
                write!(
                    f,
                    "shapes {:?} and {:?} are not broadcast-compatible",
                    shape_a, shape_b
                )
            }
            ShapeError::MatMulIncompatible { shape_a, shape_b } => {
                write!(
                    f,
                    "shapes {:?} and {:?} are not compatible for matrix multiplication",
                    shape_a, shape_b
                )
            }
            ShapeError::ReshapeIncompatible {
                from_numel,
                to_numel,
            } => {
                write!(
                    f,
                    "cannot reshape: source has {} elements but target has {}",
                    from_numel, to_numel
                )
            }
            ShapeError::IndexOutOfBounds { index, shape } => {
                write!(
                    f,
                    "index {:?} is out of bounds for shape {:?}",
                    index, shape
                )
            }
            ShapeError::DimensionMismatch { expected, got } => {
                write!(f, "expected {} dimensions but got {}", expected, got)
            }
        }
    }
}

impl std::error::Error for ShapeError {}

/// Describes the dimensionality of a tensor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    /// Create a new shape from dimension sizes.
    pub fn new(dims: Vec<usize>) -> Self {
        Shape { dims }
    }

    /// Create a scalar shape (0 dimensions).
    pub fn scalar() -> Self {
        Shape { dims: vec![] }
    }

    /// Number of dimensions (rank).
    pub fn ndim(&self) -> usize {
        self.dims.len()
    }

    /// Total number of elements.
    pub fn numel(&self) -> usize {
        if self.dims.is_empty() {
            1 // scalar
        } else {
            self.dims.iter().product()
        }
    }

    /// Returns the dimension sizes as a slice.
    pub fn dims(&self) -> &[usize] {
        &self.dims
    }

    /// True if this is a scalar (0-dimensional).
    pub fn is_scalar(&self) -> bool {
        self.dims.is_empty()
    }

    /// True if this is a 1-D tensor.
    pub fn is_vector(&self) -> bool {
        self.dims.len() == 1
    }

    /// True if this is a 2-D tensor.
    pub fn is_matrix(&self) -> bool {
        self.dims.len() == 2
    }

    /// Compute C-contiguous (row-major) strides.
    pub fn strides(&self) -> Vec<usize> {
        if self.dims.is_empty() {
            return vec![];
        }
        let mut strides = vec![1usize; self.dims.len()];
        for i in (0..self.dims.len() - 1).rev() {
            strides[i] = strides[i + 1] * self.dims[i + 1];
        }
        strides
    }

    /// Compute the broadcasted shape of `self` and `other` using NumPy-style
    /// broadcasting rules.
    ///
    /// Rules:
    /// 1. Shapes are right-aligned
    /// 2. Dimensions are compatible if they are equal, or one of them is 1
    /// 3. The output dimension is the maximum of the two
    pub fn broadcast_with(&self, other: &Shape) -> Result<Shape, ShapeError> {
        let a = &self.dims;
        let b = &other.dims;
        let max_ndim = a.len().max(b.len());
        let mut result = Vec::with_capacity(max_ndim);

        for i in 0..max_ndim {
            let da = if i < a.len() { a[a.len() - 1 - i] } else { 1 };
            let db = if i < b.len() { b[b.len() - 1 - i] } else { 1 };

            if da == db {
                result.push(da);
            } else if da == 1 {
                result.push(db);
            } else if db == 1 {
                result.push(da);
            } else {
                return Err(ShapeError::BroadcastIncompatible {
                    shape_a: a.clone(),
                    shape_b: b.clone(),
                });
            }
        }

        result.reverse();
        Ok(Shape::new(result))
    }

    /// Validate and compute the output shape for matrix multiplication A @ B.
    ///
    /// For 2-D tensors: (m, k) @ (k, n) -> (m, n)
    /// For 1-D: treated as row/column vectors as appropriate.
    pub fn matmul_shape(a: &Shape, b: &Shape) -> Result<Shape, ShapeError> {
        match (a.ndim(), b.ndim()) {
            (1, 1) => {
                // dot product: (k,) @ (k,) -> scalar
                if a.dims[0] != b.dims[0] {
                    return Err(ShapeError::MatMulIncompatible {
                        shape_a: a.dims.clone(),
                        shape_b: b.dims.clone(),
                    });
                }
                Ok(Shape::scalar())
            }
            (2, 1) => {
                // (m, k) @ (k,) -> (m,)
                if a.dims[1] != b.dims[0] {
                    return Err(ShapeError::MatMulIncompatible {
                        shape_a: a.dims.clone(),
                        shape_b: b.dims.clone(),
                    });
                }
                Ok(Shape::new(vec![a.dims[0]]))
            }
            (1, 2) => {
                // (k,) @ (k, n) -> (n,)
                if a.dims[0] != b.dims[0] {
                    return Err(ShapeError::MatMulIncompatible {
                        shape_a: a.dims.clone(),
                        shape_b: b.dims.clone(),
                    });
                }
                Ok(Shape::new(vec![b.dims[1]]))
            }
            (2, 2) => {
                // (m, k) @ (k, n) -> (m, n)
                if a.dims[1] != b.dims[0] {
                    return Err(ShapeError::MatMulIncompatible {
                        shape_a: a.dims.clone(),
                        shape_b: b.dims.clone(),
                    });
                }
                Ok(Shape::new(vec![a.dims[0], b.dims[1]]))
            }
            _ => Err(ShapeError::MatMulIncompatible {
                shape_a: a.dims.clone(),
                shape_b: b.dims.clone(),
            }),
        }
    }
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, d) in self.dims.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", d)?;
        }
        write!(f, ")")
    }
}
