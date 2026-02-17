//! Panic-vs-result boundary enforcement for the Lumen runtime.
//!
//! This module provides utilities to catch panics at well-defined boundaries
//! and convert them into `Result` values, ensuring that panics in tool
//! implementations or user code never propagate into the VM dispatch loop.
//!
//! # Policy
//!
//! The Lumen runtime follows these rules:
//!
//! - **Panics** are reserved for *unrecoverable programmer errors* (violated
//!   invariants, unreachable code, corrupted internal state).
//! - **Results** are used for *expected operational failures* (tool errors,
//!   network timeouts, invalid user input, budget exhaustion).
//!
//! At every boundary where user-supplied or plugin-supplied code is invoked
//! (tool dispatch, effect handlers, FFI), a panic boundary should be
//! established via [`catch_panic`] or [`with_panic_boundary`].
//!
//! # Example
//!
//! ```rust
//! use lumen_runtime::panic_boundary::{catch_panic, PanicPolicy, with_panic_boundary};
//!
//! // Catch a panic and convert to Result
//! let result = catch_panic(|| {
//!     panic!("oops");
//! });
//! assert!(result.is_err());
//!
//! // Use a policy
//! let result = with_panic_boundary(PanicPolicy::CatchAndReturn, || 42);
//! assert_eq!(result.unwrap(), 42);
//! ```

use std::any::Any;
use std::fmt;

// ---------------------------------------------------------------------------
// PanicError
// ---------------------------------------------------------------------------

/// An error type representing a caught panic.
///
/// The original panic payload is inspected and, where possible, its message
/// is extracted as a `String`.
#[derive(Debug, Clone)]
pub struct PanicError {
    message: String,
}

impl PanicError {
    /// Create a `PanicError` from a raw panic payload (`Box<dyn Any>`).
    pub fn from_payload(payload: Box<dyn Any + Send>) -> Self {
        let message = extract_panic_message(&payload);
        Self { message }
    }

    /// Create a `PanicError` with a specific message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Return the panic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PanicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "panic: {}", self.message)
    }
}

impl std::error::Error for PanicError {}

/// Extract a human-readable message from a panic payload.
///
/// Handles `&str` and `String` payloads; falls back to a generic message.
fn extract_panic_message(payload: &Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

// ---------------------------------------------------------------------------
// PanicPolicy
// ---------------------------------------------------------------------------

/// Policy for handling panics at a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicPolicy {
    /// Catch the panic and return it as `Err(PanicError)`.
    CatchAndReturn,

    /// Catch the panic, log the message to stderr, and return a default
    /// error.
    LogAndContinue,

    /// Do not catch panics — let them propagate (abort the process if
    /// uncaught). Useful for debugging or in tests where you want the
    /// full backtrace.
    Abort,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Catch a panic from `f` and convert it to `Result<T, PanicError>`.
///
/// This is the low-level primitive. For policy-based handling, use
/// [`with_panic_boundary`].
pub fn catch_panic<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Result<T, PanicError> {
    match std::panic::catch_unwind(f) {
        Ok(value) => Ok(value),
        Err(payload) => Err(PanicError::from_payload(payload)),
    }
}

/// Execute `f` under the given [`PanicPolicy`].
///
/// - [`PanicPolicy::CatchAndReturn`]: equivalent to [`catch_panic`].
/// - [`PanicPolicy::LogAndContinue`]: catches the panic, prints the error
///   to stderr, and returns `Err`.
/// - [`PanicPolicy::Abort`]: does **not** catch the panic; if `f` panics
///   the panic propagates normally.
pub fn with_panic_boundary<T>(
    policy: PanicPolicy,
    f: impl FnOnce() -> T + std::panic::UnwindSafe,
) -> Result<T, PanicError> {
    match policy {
        PanicPolicy::CatchAndReturn => catch_panic(f),
        PanicPolicy::LogAndContinue => match catch_panic(f) {
            Ok(v) => Ok(v),
            Err(e) => {
                eprintln!("[lumen-runtime] caught panic: {}", e.message());
                Err(e)
            }
        },
        PanicPolicy::Abort => {
            // No catch — if f panics, the panic propagates.
            Ok(f())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_panic_on_success() {
        let result = catch_panic(|| 42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn catch_panic_on_str_panic() {
        let result = catch_panic(|| -> i32 { panic!("boom") });
        let err = result.unwrap_err();
        assert_eq!(err.message(), "boom");
        assert_eq!(err.to_string(), "panic: boom");
    }

    #[test]
    fn catch_panic_on_string_panic() {
        let result = catch_panic(|| -> i32 {
            let msg = String::from("string panic");
            panic!("{}", msg);
        });
        let err = result.unwrap_err();
        assert!(err.message().contains("string panic"));
    }

    #[test]
    fn catch_panic_unknown_payload() {
        // Panic with a non-string payload
        let result = catch_panic(|| -> i32 {
            std::panic::panic_any(42_i32);
        });
        let err = result.unwrap_err();
        assert_eq!(err.message(), "unknown panic payload");
    }

    #[test]
    fn policy_catch_and_return_success() {
        let result = with_panic_boundary(PanicPolicy::CatchAndReturn, || "hello");
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn policy_catch_and_return_panic() {
        let result = with_panic_boundary(PanicPolicy::CatchAndReturn, || -> i32 { panic!("test") });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message(), "test");
    }

    #[test]
    fn policy_log_and_continue_success() {
        let result = with_panic_boundary(PanicPolicy::LogAndContinue, || 99);
        assert_eq!(result.unwrap(), 99);
    }

    #[test]
    fn policy_log_and_continue_panic() {
        let result =
            with_panic_boundary(PanicPolicy::LogAndContinue, || -> i32 { panic!("logged") });
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("logged"));
    }

    #[test]
    fn policy_abort_success() {
        let result = with_panic_boundary(PanicPolicy::Abort, || 7);
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn panic_error_new() {
        let err = PanicError::new("custom message");
        assert_eq!(err.message(), "custom message");
        assert_eq!(err.to_string(), "panic: custom message");
    }

    #[test]
    fn panic_error_implements_std_error() {
        let err = PanicError::new("test");
        let dyn_err: &dyn std::error::Error = &err;
        assert_eq!(dyn_err.to_string(), "panic: test");
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn panic_policy_equality() {
        assert_eq!(PanicPolicy::CatchAndReturn, PanicPolicy::CatchAndReturn);
        assert_ne!(PanicPolicy::CatchAndReturn, PanicPolicy::Abort);
        assert_ne!(PanicPolicy::LogAndContinue, PanicPolicy::Abort);
    }
}
