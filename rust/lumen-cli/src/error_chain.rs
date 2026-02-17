//! Error context chaining for CLI diagnostics.
//!
//! Provides an `ErrorChain` wrapper that collects nested errors into a
//! cause chain and formats them with indented "caused by:" lines for
//! human-readable CLI output.

use std::fmt;

// =============================================================================
// ErrorChain
// =============================================================================

/// A wrapper that collects a primary error message along with a chain
/// of underlying causes for rich CLI error display.
///
/// # Example
///
/// ```
/// use lumen_cli::error_chain::ErrorChain;
///
/// let chain = ErrorChain::new("compilation failed")
///     .caused_by("type error in cell 'main'")
///     .caused_by("expected Int, found String at line 42");
///
/// let formatted = chain.format_for_display();
/// assert!(formatted.contains("compilation failed"));
/// assert!(formatted.contains("caused by:"));
/// ```
#[derive(Debug, Clone)]
pub struct ErrorChain {
    /// The primary (top-level) error message.
    pub primary: String,
    /// Ordered chain of causes, from outermost to innermost.
    pub causes: Vec<String>,
}

impl ErrorChain {
    /// Create a new error chain with a primary message.
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            primary: primary.into(),
            causes: Vec::new(),
        }
    }

    /// Append a cause to the chain. Returns `self` for builder-style chaining.
    pub fn caused_by(mut self, cause: impl Into<String>) -> Self {
        self.causes.push(cause.into());
        self
    }

    /// Append a cause to the chain (mutable reference variant).
    pub fn add_cause(&mut self, cause: impl Into<String>) {
        self.causes.push(cause.into());
    }

    /// Return the total number of messages (primary + causes).
    pub fn len(&self) -> usize {
        1 + self.causes.len()
    }

    /// Return true if the chain has no causes (only the primary message).
    pub fn is_empty(&self) -> bool {
        self.causes.is_empty()
    }

    /// Format the error chain for CLI display.
    ///
    /// Output format:
    /// ```text
    /// error: <primary message>
    ///   caused by: <cause 1>
    ///   caused by: <cause 2>
    /// ```
    pub fn format_for_display(&self) -> String {
        let mut out = format!("error: {}", self.primary);
        for cause in &self.causes {
            out.push_str(&format!("\n  caused by: {}", cause));
        }
        out
    }

    /// Format the chain with a custom prefix instead of "error:".
    pub fn format_with_prefix(&self, prefix: &str) -> String {
        let mut out = format!("{} {}", prefix, self.primary);
        for cause in &self.causes {
            out.push_str(&format!("\n  caused by: {}", cause));
        }
        out
    }
}

impl fmt::Display for ErrorChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.primary)?;
        for cause in &self.causes {
            write!(f, "\n  caused by: {}", cause)?;
        }
        Ok(())
    }
}

impl std::error::Error for ErrorChain {}

// =============================================================================
// From implementations for common error types
// =============================================================================

impl From<std::io::Error> for ErrorChain {
    fn from(err: std::io::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl From<String> for ErrorChain {
    fn from(msg: String) -> Self {
        Self::new(msg)
    }
}

impl From<&str> for ErrorChain {
    fn from(msg: &str) -> Self {
        Self::new(msg)
    }
}

// =============================================================================
// Conversion helpers
// =============================================================================

/// Walk a `std::error::Error` source chain and collect all messages into
/// an `ErrorChain`.
pub fn chain_from_error(err: &dyn std::error::Error) -> ErrorChain {
    let mut chain = ErrorChain::new(err.to_string());
    let mut source = err.source();
    while let Some(cause) = source {
        chain.add_cause(cause.to_string());
        source = cause.source();
    }
    chain
}

/// Convenience: format any `std::error::Error` with its full cause chain.
pub fn format_error_chain(err: &dyn std::error::Error) -> String {
    chain_from_error(err).format_for_display()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_error() {
        let chain = ErrorChain::new("something broke");
        assert_eq!(chain.len(), 1);
        assert!(chain.is_empty()); // no causes
        assert_eq!(chain.format_for_display(), "error: something broke");
    }

    #[test]
    fn test_one_cause() {
        let chain = ErrorChain::new("compilation failed").caused_by("type error at line 5");
        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());
        let output = chain.format_for_display();
        assert_eq!(
            output,
            "error: compilation failed\n  caused by: type error at line 5"
        );
    }

    #[test]
    fn test_multiple_causes() {
        let chain = ErrorChain::new("run failed")
            .caused_by("compilation error")
            .caused_by("unresolved symbol 'foo'")
            .caused_by("module 'bar' not found");
        assert_eq!(chain.len(), 4);
        let output = chain.format_for_display();
        assert!(output.starts_with("error: run failed"));
        assert!(output.contains("  caused by: compilation error"));
        assert!(output.contains("  caused by: unresolved symbol 'foo'"));
        assert!(output.contains("  caused by: module 'bar' not found"));
    }

    #[test]
    fn test_display_trait() {
        let chain = ErrorChain::new("primary").caused_by("secondary");
        let display = format!("{}", chain);
        assert_eq!(display, "primary\n  caused by: secondary");
    }

    #[test]
    fn test_format_with_prefix() {
        let chain = ErrorChain::new("bad input").caused_by("expected number");
        let output = chain.format_with_prefix("warning:");
        assert_eq!(output, "warning: bad input\n  caused by: expected number");
    }

    #[test]
    fn test_add_cause_mutable() {
        let mut chain = ErrorChain::new("root");
        chain.add_cause("cause 1");
        chain.add_cause("cause 2");
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn test_from_string() {
        let chain: ErrorChain = "something failed".into();
        assert_eq!(chain.primary, "something failed");
        assert!(chain.causes.is_empty());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let chain: ErrorChain = io_err.into();
        assert_eq!(chain.primary, "file not found");
    }

    #[test]
    fn test_chain_from_std_error() {
        // Build a nested error with source chain using thiserror-like pattern
        let inner = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let chain = chain_from_error(&inner);
        assert_eq!(chain.primary, "access denied");
        assert!(chain.causes.is_empty()); // io::Error has no source
    }

    #[test]
    fn test_format_error_chain_helper() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
        let output = format_error_chain(&err);
        assert_eq!(output, "error: missing file");
    }

    #[test]
    fn test_error_chain_is_error() {
        // Verify ErrorChain implements std::error::Error
        let chain = ErrorChain::new("test error").caused_by("inner cause");
        let err: &dyn std::error::Error = &chain;
        assert_eq!(err.to_string(), "test error\n  caused by: inner cause");
    }

    #[test]
    fn test_chain_from_nested_error() {
        // Create a simple wrapper error with a source
        #[derive(Debug)]
        struct WrapperError {
            msg: String,
            source: Option<Box<dyn std::error::Error + 'static>>,
        }
        impl fmt::Display for WrapperError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.msg)
            }
        }
        impl std::error::Error for WrapperError {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.source.as_deref()
            }
        }

        let inner = Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config.toml missing",
        ));
        let outer = WrapperError {
            msg: "failed to load config".to_string(),
            source: Some(inner),
        };

        let chain = chain_from_error(&outer);
        assert_eq!(chain.primary, "failed to load config");
        assert_eq!(chain.causes.len(), 1);
        assert_eq!(chain.causes[0], "config.toml missing");

        let formatted = chain.format_for_display();
        assert_eq!(
            formatted,
            "error: failed to load config\n  caused by: config.toml missing"
        );
    }
}
