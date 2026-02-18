//! Retry-After header support and intelligent retry logic (T184).
//!
//! Provides configurable retry policies with multiple backoff strategies,
//! jitter support, Retry-After header parsing, and detailed attempt history.

use std::fmt;

// ---------------------------------------------------------------------------
// Backoff strategy
// ---------------------------------------------------------------------------

/// Strategy for computing delay between retry attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Same delay every time.
    Constant,
    /// Delay increases linearly: `base * attempt`.
    Linear,
    /// Delay doubles each attempt: `base * 2^attempt`.
    Exponential,
    /// Delay follows the Fibonacci sequence scaled by `base`.
    Fibonacci,
}

// ---------------------------------------------------------------------------
// Retry condition
// ---------------------------------------------------------------------------

/// Condition under which a retry should be attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryCondition {
    /// Retry on a specific HTTP status code.
    StatusCode(u16),
    /// Retry on 429 Too Many Requests.
    RateLimit,
    /// Retry on any 5xx server error.
    ServerError,
    /// Retry on request timeout.
    Timeout,
    /// Retry on network / connection error.
    ConnectionError,
    /// Custom named condition.
    Custom(String),
}

impl fmt::Display for RetryCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetryCondition::StatusCode(code) => write!(f, "StatusCode({code})"),
            RetryCondition::RateLimit => write!(f, "RateLimit"),
            RetryCondition::ServerError => write!(f, "ServerError"),
            RetryCondition::Timeout => write!(f, "Timeout"),
            RetryCondition::ConnectionError => write!(f, "ConnectionError"),
            RetryCondition::Custom(name) => write!(f, "Custom({name})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------------

/// Configurable policy governing retry behaviour.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Base delay in milliseconds before the first retry.
    pub base_delay_ms: u64,
    /// Upper bound on computed delay (before jitter).
    pub max_delay_ms: u64,
    /// Backoff strategy applied to successive retries.
    pub backoff: BackoffStrategy,
    /// Whether to add random jitter to the computed delay.
    pub jitter: bool,
    /// Conditions under which retries are allowed.
    pub retry_on: Vec<RetryCondition>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 10_000,
            backoff: BackoffStrategy::Exponential,
            jitter: true,
            retry_on: vec![
                RetryCondition::RateLimit,
                RetryCondition::ServerError,
                RetryCondition::Timeout,
                RetryCondition::ConnectionError,
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Retry attempt record
// ---------------------------------------------------------------------------

/// Record of a single retry attempt.
#[derive(Debug, Clone)]
pub struct RetryAttempt {
    /// 1-based attempt number.
    pub attempt_number: u32,
    /// Delay in ms that was waited before this attempt.
    pub delay_ms: u64,
    /// Error message that triggered the retry.
    pub error: String,
    /// Timestamp (epoch ms) when the attempt was made.
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// Retry state
// ---------------------------------------------------------------------------

/// Mutable state tracked across retry attempts.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// Current attempt number (0 = initial attempt, 1 = first retry, …).
    pub attempt: u32,
    /// Cumulative delay spent waiting across all retries.
    pub total_delay_ms: u64,
    /// Most recent error message, if any.
    pub last_error: Option<String>,
    /// Delay parsed from a Retry-After header, if present.
    pub retry_after_ms: Option<u64>,
    /// History of all retry attempts.
    pub history: Vec<RetryAttempt>,
}

impl RetryState {
    /// Create a fresh initial state (no retries yet).
    pub fn new() -> Self {
        Self {
            attempt: 0,
            total_delay_ms: 0,
            last_error: None,
            retry_after_ms: None,
            history: Vec::new(),
        }
    }

    /// Record an attempt.
    pub fn record_attempt(&mut self, delay_ms: u64, error: &str, timestamp_ms: u64) {
        self.attempt = self.attempt.saturating_add(1);
        self.total_delay_ms = self.total_delay_ms.saturating_add(delay_ms);
        self.last_error = Some(error.to_string());
        self.history.push(RetryAttempt {
            attempt_number: self.attempt,
            delay_ms,
            error: error.to_string(),
            timestamp_ms,
        });
    }
}

impl Default for RetryState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Retry executor
// ---------------------------------------------------------------------------

/// Stateless executor that computes delays and evaluates retry decisions
/// according to a [`RetryPolicy`].
pub struct RetryExecutor {
    policy: RetryPolicy,
}

impl RetryExecutor {
    /// Create an executor with the given policy.
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// Create an executor with [`RetryPolicy::default()`].
    pub fn with_defaults() -> Self {
        Self::new(RetryPolicy::default())
    }

    /// Return a reference to the underlying policy.
    pub fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    // -- delay computation -------------------------------------------------

    /// Compute the raw backoff delay for a given 0-based attempt number
    /// (before jitter and max-delay clamping).
    pub fn compute_backoff(&self, attempt: u32) -> u64 {
        let base = self.policy.base_delay_ms;
        match self.policy.backoff {
            BackoffStrategy::Constant => base,
            BackoffStrategy::Linear => base.saturating_mul(attempt.saturating_add(1) as u64),
            BackoffStrategy::Exponential => {
                let shift = attempt.min(63) as u64;
                base.saturating_mul(1u64.checked_shl(shift as u32).unwrap_or(u64::MAX))
            }
            BackoffStrategy::Fibonacci => {
                let fib = fibonacci(attempt);
                base.saturating_mul(fib)
            }
        }
    }

    /// Apply jitter to a delay value. Uses a deterministic seed-based
    /// approach: jitter is ±25 % of the delay value.
    pub fn apply_jitter(&self, delay_ms: u64, seed: u64) -> u64 {
        if delay_ms == 0 {
            return 0;
        }
        // Pseudo-random value derived from seed (simple xorshift-like).
        let hash = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Map hash to [-25%, +25%] range of delay.
        let quarter = delay_ms / 4;
        if quarter == 0 {
            return delay_ms;
        }
        let jitter_val = hash % quarter.saturating_mul(2).saturating_add(1);
        // Subtract quarter to centre around 0, then add to delay.
        delay_ms.saturating_add(jitter_val).saturating_sub(quarter)
    }

    /// Calculate the next delay to wait given the current [`RetryState`].
    /// Returns `None` if retries are exhausted.
    pub fn next_delay(&self, state: &RetryState) -> Option<u64> {
        if state.attempt >= self.policy.max_retries {
            return None;
        }

        // If a Retry-After header value is available, honour it (clamped to
        // max_delay_ms).
        if let Some(retry_after) = state.retry_after_ms {
            return Some(retry_after.min(self.policy.max_delay_ms));
        }

        let raw = self.compute_backoff(state.attempt);
        let clamped = raw.min(self.policy.max_delay_ms);

        let delay = if self.policy.jitter {
            self.apply_jitter(clamped, state.attempt as u64)
        } else {
            clamped
        };

        Some(delay)
    }

    /// Decide whether a retry should be attempted for the given condition.
    pub fn should_retry(&self, state: &RetryState, condition: &RetryCondition) -> bool {
        if state.attempt >= self.policy.max_retries {
            return false;
        }

        self.policy.retry_on.iter().any(|c| match (c, condition) {
            (RetryCondition::StatusCode(a), RetryCondition::StatusCode(b)) => a == b,
            (RetryCondition::RateLimit, RetryCondition::RateLimit) => true,
            (RetryCondition::RateLimit, RetryCondition::StatusCode(429)) => true,
            (RetryCondition::ServerError, RetryCondition::ServerError) => true,
            (RetryCondition::ServerError, RetryCondition::StatusCode(code)) => {
                *code >= 500 && *code < 600
            }
            (RetryCondition::Timeout, RetryCondition::Timeout) => true,
            (RetryCondition::ConnectionError, RetryCondition::ConnectionError) => true,
            (RetryCondition::Custom(a), RetryCondition::Custom(b)) => a == b,
            _ => false,
        })
    }

    // -- header parsing ----------------------------------------------------

    /// Parse an HTTP `Retry-After` header value.
    ///
    /// Supports integer seconds (e.g. `"120"` → `Some(120_000)`).
    /// HTTP-date format is not supported without a date library and returns
    /// `None`.
    pub fn parse_retry_after(header_value: &str) -> Option<u64> {
        let trimmed = header_value.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try integer seconds first.
        if let Ok(seconds) = trimmed.parse::<u64>() {
            return Some(seconds.saturating_mul(1000));
        }

        // HTTP-date format — too complex without chrono; return None.
        None
    }

    // -- pre-built policies ------------------------------------------------

    /// Policy tuned for rate-limit (429) responses.
    pub fn rate_limit_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 5,
            base_delay_ms: 1_000,
            max_delay_ms: 60_000,
            backoff: BackoffStrategy::Exponential,
            jitter: true,
            retry_on: vec![RetryCondition::RateLimit],
        }
    }

    /// Policy tuned for transient server errors (5xx).
    pub fn server_error_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            backoff: BackoffStrategy::Exponential,
            jitter: true,
            retry_on: vec![RetryCondition::ServerError],
        }
    }

    /// Aggressive policy: many retries with short delays.
    pub fn aggressive_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 10,
            base_delay_ms: 50,
            max_delay_ms: 5_000,
            backoff: BackoffStrategy::Linear,
            jitter: false,
            retry_on: vec![
                RetryCondition::RateLimit,
                RetryCondition::ServerError,
                RetryCondition::Timeout,
                RetryCondition::ConnectionError,
            ],
        }
    }

    /// Conservative policy: few retries with long delays.
    pub fn conservative_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 2,
            base_delay_ms: 2_000,
            max_delay_ms: 30_000,
            backoff: BackoffStrategy::Exponential,
            jitter: true,
            retry_on: vec![RetryCondition::ServerError, RetryCondition::Timeout],
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the `n`-th Fibonacci number (0-indexed: fib(0)=1, fib(1)=1, …).
fn fibonacci(n: u32) -> u64 {
    let mut a: u64 = 1;
    let mut b: u64 = 1;
    for _ in 0..n {
        let next = a.saturating_add(b);
        a = b;
        b = next;
    }
    a
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- BackoffStrategy ---------------------------------------------------

    #[test]
    fn constant_backoff_is_constant() {
        let exec = RetryExecutor::new(RetryPolicy {
            backoff: BackoffStrategy::Constant,
            base_delay_ms: 200,
            ..RetryPolicy::default()
        });
        assert_eq!(exec.compute_backoff(0), 200);
        assert_eq!(exec.compute_backoff(1), 200);
        assert_eq!(exec.compute_backoff(5), 200);
    }

    #[test]
    fn linear_backoff_scales_linearly() {
        let exec = RetryExecutor::new(RetryPolicy {
            backoff: BackoffStrategy::Linear,
            base_delay_ms: 100,
            ..RetryPolicy::default()
        });
        assert_eq!(exec.compute_backoff(0), 100); // 100*1
        assert_eq!(exec.compute_backoff(1), 200); // 100*2
        assert_eq!(exec.compute_backoff(4), 500); // 100*5
    }

    #[test]
    fn exponential_backoff_doubles() {
        let exec = RetryExecutor::new(RetryPolicy {
            backoff: BackoffStrategy::Exponential,
            base_delay_ms: 100,
            ..RetryPolicy::default()
        });
        assert_eq!(exec.compute_backoff(0), 100); // 100*1
        assert_eq!(exec.compute_backoff(1), 200); // 100*2
        assert_eq!(exec.compute_backoff(2), 400); // 100*4
        assert_eq!(exec.compute_backoff(3), 800); // 100*8
    }

    #[test]
    fn fibonacci_backoff() {
        let exec = RetryExecutor::new(RetryPolicy {
            backoff: BackoffStrategy::Fibonacci,
            base_delay_ms: 100,
            ..RetryPolicy::default()
        });
        // fib sequence: 1, 1, 2, 3, 5, 8, …
        assert_eq!(exec.compute_backoff(0), 100);
        assert_eq!(exec.compute_backoff(1), 100);
        assert_eq!(exec.compute_backoff(2), 200);
        assert_eq!(exec.compute_backoff(3), 300);
        assert_eq!(exec.compute_backoff(4), 500);
        assert_eq!(exec.compute_backoff(5), 800);
    }

    #[test]
    fn exponential_saturates_on_large_attempt() {
        let exec = RetryExecutor::new(RetryPolicy {
            backoff: BackoffStrategy::Exponential,
            base_delay_ms: 1_000,
            ..RetryPolicy::default()
        });
        // Attempt 64+ should saturate, not panic.
        let delay = exec.compute_backoff(64);
        assert_eq!(delay, u64::MAX);
    }

    // -- Jitter ------------------------------------------------------------

    #[test]
    fn jitter_zero_delay_returns_zero() {
        let exec = RetryExecutor::with_defaults();
        assert_eq!(exec.apply_jitter(0, 42), 0);
    }

    #[test]
    fn jitter_small_delay_unchanged() {
        let exec = RetryExecutor::with_defaults();
        // quarter = 3/4 = 0, so returns delay unchanged.
        assert_eq!(exec.apply_jitter(3, 42), 3);
    }

    #[test]
    fn jitter_stays_within_bounds() {
        let exec = RetryExecutor::with_defaults();
        for seed in 0..100 {
            let result = exec.apply_jitter(1000, seed);
            // ±25% → [750, 1250]
            assert!(result >= 750, "seed {seed}: {result} < 750");
            assert!(result <= 1250, "seed {seed}: {result} > 1250");
        }
    }

    // -- parse_retry_after -------------------------------------------------

    #[test]
    fn parse_retry_after_integer_seconds() {
        assert_eq!(RetryExecutor::parse_retry_after("120"), Some(120_000));
        assert_eq!(RetryExecutor::parse_retry_after("0"), Some(0));
        assert_eq!(RetryExecutor::parse_retry_after("1"), Some(1_000));
    }

    #[test]
    fn parse_retry_after_with_whitespace() {
        assert_eq!(RetryExecutor::parse_retry_after("  60  "), Some(60_000));
    }

    #[test]
    fn parse_retry_after_http_date_returns_none() {
        assert_eq!(
            RetryExecutor::parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            None
        );
    }

    #[test]
    fn parse_retry_after_empty_returns_none() {
        assert_eq!(RetryExecutor::parse_retry_after(""), None);
        assert_eq!(RetryExecutor::parse_retry_after("   "), None);
    }

    #[test]
    fn parse_retry_after_garbage_returns_none() {
        assert_eq!(RetryExecutor::parse_retry_after("abc"), None);
        assert_eq!(RetryExecutor::parse_retry_after("-5"), None);
    }

    // -- should_retry ------------------------------------------------------

    #[test]
    fn should_retry_rate_limit() {
        let exec = RetryExecutor::new(RetryExecutor::rate_limit_policy());
        let state = RetryState::new();
        assert!(exec.should_retry(&state, &RetryCondition::RateLimit));
        assert!(!exec.should_retry(&state, &RetryCondition::Timeout));
    }

    #[test]
    fn should_retry_server_error() {
        let exec = RetryExecutor::new(RetryExecutor::server_error_policy());
        let state = RetryState::new();
        assert!(exec.should_retry(&state, &RetryCondition::ServerError));
        assert!(exec.should_retry(&state, &RetryCondition::StatusCode(503)));
        assert!(!exec.should_retry(&state, &RetryCondition::StatusCode(404)));
    }

    #[test]
    fn should_retry_exhausted() {
        let exec = RetryExecutor::new(RetryPolicy {
            max_retries: 2,
            ..RetryPolicy::default()
        });
        let mut state = RetryState::new();
        state.attempt = 2;
        assert!(!exec.should_retry(&state, &RetryCondition::RateLimit));
    }

    #[test]
    fn should_retry_status_code_429_matches_rate_limit() {
        let exec = RetryExecutor::with_defaults();
        let state = RetryState::new();
        assert!(exec.should_retry(&state, &RetryCondition::StatusCode(429)));
    }

    #[test]
    fn should_retry_custom_condition() {
        let exec = RetryExecutor::new(RetryPolicy {
            retry_on: vec![RetryCondition::Custom("my_err".to_string())],
            ..RetryPolicy::default()
        });
        let state = RetryState::new();
        assert!(exec.should_retry(&state, &RetryCondition::Custom("my_err".to_string())));
        assert!(!exec.should_retry(&state, &RetryCondition::Custom("other".to_string())));
    }

    // -- next_delay --------------------------------------------------------

    #[test]
    fn next_delay_respects_max_retries() {
        let exec = RetryExecutor::new(RetryPolicy {
            max_retries: 2,
            jitter: false,
            ..RetryPolicy::default()
        });
        let mut state = RetryState::new();
        assert!(exec.next_delay(&state).is_some());
        state.attempt = 2;
        assert!(exec.next_delay(&state).is_none());
    }

    #[test]
    fn next_delay_honours_retry_after_header() {
        let exec = RetryExecutor::new(RetryPolicy {
            max_delay_ms: 5_000,
            jitter: false,
            ..RetryPolicy::default()
        });
        let mut state = RetryState::new();
        state.retry_after_ms = Some(3_000);
        assert_eq!(exec.next_delay(&state), Some(3_000));
    }

    #[test]
    fn next_delay_clamps_retry_after_to_max() {
        let exec = RetryExecutor::new(RetryPolicy {
            max_delay_ms: 2_000,
            jitter: false,
            ..RetryPolicy::default()
        });
        let mut state = RetryState::new();
        state.retry_after_ms = Some(10_000);
        assert_eq!(exec.next_delay(&state), Some(2_000));
    }

    #[test]
    fn next_delay_clamps_backoff_to_max() {
        let exec = RetryExecutor::new(RetryPolicy {
            base_delay_ms: 5_000,
            max_delay_ms: 8_000,
            backoff: BackoffStrategy::Exponential,
            jitter: false,
            ..RetryPolicy::default()
        });
        let mut state = RetryState::new();
        state.attempt = 2; // raw = 5000 * 4 = 20000 → clamped to 8000
        assert_eq!(exec.next_delay(&state), Some(8_000));
    }

    // -- RetryState --------------------------------------------------------

    #[test]
    fn retry_state_new_defaults() {
        let state = RetryState::new();
        assert_eq!(state.attempt, 0);
        assert_eq!(state.total_delay_ms, 0);
        assert!(state.last_error.is_none());
        assert!(state.retry_after_ms.is_none());
        assert!(state.history.is_empty());
    }

    #[test]
    fn retry_state_record_attempt() {
        let mut state = RetryState::new();
        state.record_attempt(100, "timeout", 1000);
        assert_eq!(state.attempt, 1);
        assert_eq!(state.total_delay_ms, 100);
        assert_eq!(state.last_error.as_deref(), Some("timeout"));
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].attempt_number, 1);
        assert_eq!(state.history[0].delay_ms, 100);
        assert_eq!(state.history[0].error, "timeout");
        assert_eq!(state.history[0].timestamp_ms, 1000);
    }

    #[test]
    fn retry_state_cumulative_delay() {
        let mut state = RetryState::new();
        state.record_attempt(100, "err1", 1000);
        state.record_attempt(200, "err2", 2000);
        state.record_attempt(400, "err3", 3000);
        assert_eq!(state.attempt, 3);
        assert_eq!(state.total_delay_ms, 700);
        assert_eq!(state.history.len(), 3);
    }

    // -- Policy builders ---------------------------------------------------

    #[test]
    fn rate_limit_policy_values() {
        let p = RetryExecutor::rate_limit_policy();
        assert_eq!(p.max_retries, 5);
        assert_eq!(p.base_delay_ms, 1_000);
        assert_eq!(p.max_delay_ms, 60_000);
        assert!(p.jitter);
        assert_eq!(p.retry_on, vec![RetryCondition::RateLimit]);
    }

    #[test]
    fn server_error_policy_values() {
        let p = RetryExecutor::server_error_policy();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.base_delay_ms, 500);
        assert!(p.jitter);
        assert_eq!(p.retry_on, vec![RetryCondition::ServerError]);
    }

    #[test]
    fn aggressive_policy_values() {
        let p = RetryExecutor::aggressive_policy();
        assert_eq!(p.max_retries, 10);
        assert_eq!(p.base_delay_ms, 50);
        assert!(!p.jitter);
        assert_eq!(p.backoff, BackoffStrategy::Linear);
    }

    #[test]
    fn conservative_policy_values() {
        let p = RetryExecutor::conservative_policy();
        assert_eq!(p.max_retries, 2);
        assert_eq!(p.base_delay_ms, 2_000);
        assert_eq!(p.max_delay_ms, 30_000);
        assert!(p.jitter);
    }

    // -- Default policy ----------------------------------------------------

    #[test]
    fn default_policy_values() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.base_delay_ms, 100);
        assert_eq!(p.max_delay_ms, 10_000);
        assert_eq!(p.backoff, BackoffStrategy::Exponential);
        assert!(p.jitter);
        assert_eq!(p.retry_on.len(), 4);
    }

    // -- RetryCondition Display --------------------------------------------

    #[test]
    fn retry_condition_display() {
        assert_eq!(format!("{}", RetryCondition::RateLimit), "RateLimit");
        assert_eq!(
            format!("{}", RetryCondition::StatusCode(503)),
            "StatusCode(503)"
        );
        assert_eq!(format!("{}", RetryCondition::Timeout), "Timeout");
        assert_eq!(
            format!("{}", RetryCondition::Custom("x".into())),
            "Custom(x)"
        );
    }

    // -- fibonacci helper --------------------------------------------------

    #[test]
    fn fibonacci_sequence() {
        assert_eq!(fibonacci(0), 1);
        assert_eq!(fibonacci(1), 1);
        assert_eq!(fibonacci(2), 2);
        assert_eq!(fibonacci(3), 3);
        assert_eq!(fibonacci(4), 5);
        assert_eq!(fibonacci(5), 8);
        assert_eq!(fibonacci(6), 13);
    }
}
