//! Interpreter execution engines for Lumen bytecode.
//!
//! This module contains alternative execution strategies for LIR bytecode:
//! - **Copy-and-Patch**: Pre-compiled stencils with runtime patching (fast startup, near-native performance)
//!
//! Future additions may include:
//! - Threaded code interpreter
//! - Switch-based interpreter (reference implementation)
//! - Super-instruction generation

pub mod stencil;
