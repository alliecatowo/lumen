//! Storage backend for wares (R2/S3-compatible).
//!
//! This module re-exports the fully-implemented R2 client from `crate::registry`.
//! All R2 operations (upload, download, signing) are handled by the real SigV4
//! implementation there. Do NOT add a second R2 client here.

pub use crate::registry::{R2Client, R2Config, R2Error, R2Result};
