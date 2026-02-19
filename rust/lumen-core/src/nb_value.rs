//! NaN-boxed 64-bit value representation — re-export module.
//!
//! The canonical [`NbValue`] implementation lives in [`crate::values`].
//! This module re-exports it so that `lumen_core::nb_value::NbValue` continues
//! to resolve for any downstream crates that imported from this path.

pub use crate::values::NbValue;
