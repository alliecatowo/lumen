use serde::{Deserialize, Serialize};

/// Data types for tensor elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DType {
    F32,
    F64,
    I32,
    I64,
    Bool,
}

impl DType {
    /// Returns the size in bytes of a single element of this dtype.
    pub fn size_bytes(&self) -> usize {
        match self {
            DType::F32 => 4,
            DType::F64 => 8,
            DType::I32 => 4,
            DType::I64 => 8,
            DType::Bool => 1,
        }
    }

    /// Returns true if this dtype is a floating-point type.
    pub fn is_float(&self) -> bool {
        matches!(self, DType::F32 | DType::F64)
    }

    /// Returns true if this dtype is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(self, DType::I32 | DType::I64)
    }
}

impl std::fmt::Display for DType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DType::F32 => write!(f, "f32"),
            DType::F64 => write!(f, "f64"),
            DType::I32 => write!(f, "i32"),
            DType::I64 => write!(f, "i64"),
            DType::Bool => write!(f, "bool"),
        }
    }
}
