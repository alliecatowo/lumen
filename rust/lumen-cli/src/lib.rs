//! Lumen CLI library.
//!
//! This crate provides shared functionality for the Lumen CLI tools.

pub mod audit;
pub mod auth;
pub mod binary_cache;
pub mod bindgen;
pub mod build_script;
pub mod cache;
pub mod ci;
pub mod ci_output;
pub mod colors;
pub mod config;
pub mod doc;
pub mod error_chain;
pub mod fmt;
pub mod git;
pub mod lang_ref;
pub mod lint;
pub mod lockfile;
pub mod module_resolver;
pub mod oidc;
pub mod registry;
pub mod registry_cmd;
pub mod repl;
pub mod semver;
pub mod service_template;
pub mod test_cmd;
pub mod transparency;
pub mod tuf;
pub mod wares;
pub mod workspace;

// Package signing and TUF metadata verification have no safe fallback: without
// ed25519 they would silently use non-cryptographic placeholder signatures.
#[cfg(not(feature = "ed25519"))]
compile_error!("lumen-cli requires the `ed25519` feature (enabled by default)");
