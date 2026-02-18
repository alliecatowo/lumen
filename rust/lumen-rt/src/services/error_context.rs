//! Error context chaining for the Lumen runtime.
//!
//! Provides [`ErrorContext`] and [`ErrorChain`] utilities that allow wrapping
//! errors with descriptive context messages, producing human-readable chains
//! like:
//!
//! ```text
//! tool 'HttpGet' failed → network unreachable → TLS handshake failed
//! ```
//!
//! # Example
//!
//! ```rust
//! use lumen_rt::services::error_context::{ErrorContext, ErrorChain};
//!
//! let root = ErrorContext::new("TLS handshake failed");
//! let chain = ErrorChain::new(root)
//!     .context("network unreachable")
//!     .context("tool 'HttpGet' failed");
//! assert_eq!(
//!     chain.display_chain(),
//!     "tool 'HttpGet' failed → network unreachable → TLS handshake failed"
//! );
//! ```

use std::fmt;

// ---------------------------------------------------------------------------
// ErrorContext
// ---------------------------------------------------------------------------

/// A single error node with a message and an optional source cause.
#[derive(Debug)]
pub struct ErrorContext {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ErrorContext {
    /// Create a new error context with the given message and no source.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Create an error context wrapping a source error.
    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Return the context message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Return the source error, if any.
    pub fn source_error(&self) -> Option<&(dyn std::error::Error + Send + Sync)> {
        self.source.as_deref()
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref src) = self.source {
            write!(f, ": {src}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ErrorContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

// ---------------------------------------------------------------------------
// ErrorChain
// ---------------------------------------------------------------------------

/// A builder for chaining error context layers.
///
/// Layers are stored outermost-first: the last `.context()` call becomes the
/// top-level message, and the initial [`ErrorContext`] is the root cause.
#[derive(Debug)]
pub struct ErrorChain {
    /// Layers from outermost (index 0) to root cause (last index).
    layers: Vec<String>,
}

impl ErrorChain {
    /// Start a new chain from a root [`ErrorContext`].
    pub fn new(root: ErrorContext) -> Self {
        let mut layers = Vec::new();
        // Walk the source chain of the root ErrorContext and collect messages.
        layers.push(root.message.clone());
        let mut current: Option<&dyn std::error::Error> =
            root.source.as_deref().map(|e| e as &dyn std::error::Error);
        while let Some(err) = current {
            layers.push(err.to_string());
            current = err.source();
        }
        Self { layers }
    }

    /// Start a chain from a plain message (no source).
    pub fn from_message(message: impl Into<String>) -> Self {
        Self {
            layers: vec![message.into()],
        }
    }

    /// Wrap the current chain with an additional context layer.
    ///
    /// The new layer becomes the outermost message in [`display_chain`].
    pub fn context(mut self, message: impl Into<String>) -> Self {
        self.layers.insert(0, message.into());
        self
    }

    /// Format the full chain as a `" → "`-separated string.
    ///
    /// Outermost context is first, root cause is last.
    pub fn display_chain(&self) -> String {
        self.layers.join(" → ")
    }

    /// Return the number of layers in the chain.
    pub fn depth(&self) -> usize {
        self.layers.len()
    }

    /// Return the outermost (top-level) message.
    pub fn top(&self) -> &str {
        self.layers.first().map(|s| s.as_str()).unwrap_or("<empty>")
    }

    /// Return the root cause message.
    pub fn root_cause(&self) -> &str {
        self.layers.last().map(|s| s.as_str()).unwrap_or("<empty>")
    }

    /// Return all layers as a slice (outermost first).
    pub fn layers(&self) -> &[String] {
        &self.layers
    }
}

impl fmt::Display for ErrorChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_chain())
    }
}

// ---------------------------------------------------------------------------
// ToolError integration
// ---------------------------------------------------------------------------

use crate::services::tools::ToolError;

impl ToolError {
    /// Wrap this error with additional context, returning an [`ErrorChain`].
    pub fn with_context(self, message: impl Into<String>) -> ErrorChain {
        let root = ErrorContext::new(self.to_string());
        ErrorChain::new(root).context(message)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_context_message_only() {
        let ctx = ErrorContext::new("something broke");
        assert_eq!(ctx.message(), "something broke");
        assert!(ctx.source_error().is_none());
        assert_eq!(ctx.to_string(), "something broke");
    }

    #[test]
    fn error_context_with_source() {
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let ctx = ErrorContext::with_source("could not read config", source);
        assert_eq!(ctx.message(), "could not read config");
        assert!(ctx.source_error().is_some());
        assert_eq!(ctx.to_string(), "could not read config: file missing");
    }

    #[test]
    fn error_chain_single_layer() {
        let chain = ErrorChain::from_message("root cause");
        assert_eq!(chain.display_chain(), "root cause");
        assert_eq!(chain.depth(), 1);
        assert_eq!(chain.top(), "root cause");
        assert_eq!(chain.root_cause(), "root cause");
    }

    #[test]
    fn error_chain_multiple_contexts() {
        let root = ErrorContext::new("TLS handshake failed");
        let chain = ErrorChain::new(root)
            .context("network unreachable")
            .context("tool 'HttpGet' failed");

        assert_eq!(
            chain.display_chain(),
            "tool 'HttpGet' failed → network unreachable → TLS handshake failed"
        );
        assert_eq!(chain.depth(), 3);
        assert_eq!(chain.top(), "tool 'HttpGet' failed");
        assert_eq!(chain.root_cause(), "TLS handshake failed");
    }

    #[test]
    fn error_chain_display_trait() {
        let chain = ErrorChain::from_message("a").context("b").context("c");
        assert_eq!(format!("{chain}"), "c → b → a");
    }

    #[test]
    fn error_chain_from_error_context_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out");
        let root = ErrorContext::with_source("request failed", io_err);
        let chain = ErrorChain::new(root).context("tool dispatch");

        assert_eq!(chain.depth(), 3);
        assert_eq!(
            chain.display_chain(),
            "tool dispatch → request failed → connection timed out"
        );
    }

    #[test]
    fn tool_error_with_context() {
        let err = ToolError::ExecutionFailed("timeout".to_string());
        let chain = err.with_context("calling weather API");

        assert_eq!(chain.depth(), 2);
        assert_eq!(chain.top(), "calling weather API");
        assert!(chain.root_cause().contains("timeout"));
    }

    #[test]
    fn tool_error_with_nested_context() {
        let err = ToolError::NotFound("weather_tool".to_string());
        let chain = err
            .with_context("dispatching tool")
            .context("running cell 'main'");

        assert_eq!(chain.depth(), 3);
        assert_eq!(chain.top(), "running cell 'main'");
        assert!(chain.root_cause().contains("weather_tool"));
    }

    #[test]
    fn error_chain_layers_accessor() {
        let chain = ErrorChain::from_message("root")
            .context("middle")
            .context("top");
        let layers = chain.layers();
        assert_eq!(layers, &["top", "middle", "root"]);
    }

    #[test]
    fn error_context_implements_std_error() {
        let ctx = ErrorContext::new("test error");
        let err: &dyn std::error::Error = &ctx;
        assert_eq!(err.to_string(), "test error");
        assert!(err.source().is_none());
    }
}
