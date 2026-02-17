//! Lumen CLI library.
//!
//! This crate provides shared functionality for the Lumen CLI tools.

pub mod auth;
pub mod build_script;
pub mod cache;
pub mod ci_output;
pub mod colors;
pub mod config;
pub mod doc;
pub mod fmt;
pub mod git;
pub mod lang_ref;
pub mod lint;
pub mod lockfile;
pub mod module_resolver;
pub mod registry;
pub mod registry_cmd;
pub mod repl;
pub mod semver;
pub mod test_cmd;
pub mod wares;
pub mod workspace;
