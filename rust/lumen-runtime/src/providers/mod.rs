//! Built-in tool providers, each gated behind a `provider-*` cargo feature.
//!
//! These were previously the separate `lumen-provider-*` crates.

#[cfg(feature = "provider-crypto")]
pub mod crypto;
#[cfg(feature = "provider-env")]
pub mod env;
#[cfg(feature = "provider-fs")]
pub mod fs;
#[cfg(feature = "provider-gemini")]
pub mod gemini;
#[cfg(feature = "provider-http")]
pub mod http;
#[cfg(feature = "provider-json")]
pub mod json;
#[cfg(feature = "provider-mcp")]
pub mod mcp;
