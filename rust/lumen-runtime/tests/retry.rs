//! Integration tests for `lumen_runtime::retry` (T184: Retry-After header support).
//!
//! Covers RetryPolicy, BackoffStrategy, RetryCondition, RetryState,
//! RetryExecutor, Retry-After header parsing, and pre-built policies.

use lumen_runtime::retry::*;

// ===========================================================================
// BackoffStrategy — compute_backoff
// ===========================================================================

#[test]
fn constant_backoff_all_attempts_equal() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Constant,
        base_delay_ms: 500,
        jitter: false,
        ..RetryPolicy::default()
    });
    for attempt in 0..10 {
        assert_eq!(exec.compute_backoff(attempt), 500);
    }
}

#[test]
fn linear_backoff_first_five_attempts() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Linear,
        base_delay_ms: 100,
        jitter: false,
        ..RetryPolicy::default()
    });
    assert_eq!(exec.compute_backoff(0), 100);
    assert_eq!(exec.compute_backoff(1), 200);
    assert_eq!(exec.compute_backoff(2), 300);
    assert_eq!(exec.compute_backoff(3), 400);
    assert_eq!(exec.compute_backoff(4), 500);
}

#[test]
fn exponential_backoff_first_five_attempts() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Exponential,
        base_delay_ms: 100,
        jitter: false,
        ..RetryPolicy::default()
    });
    assert_eq!(exec.compute_backoff(0), 100);
    assert_eq!(exec.compute_backoff(1), 200);
    assert_eq!(exec.compute_backoff(2), 400);
    assert_eq!(exec.compute_backoff(3), 800);
    assert_eq!(exec.compute_backoff(4), 1600);
}

#[test]
fn fibonacci_backoff_first_seven_attempts() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Fibonacci,
        base_delay_ms: 50,
        jitter: false,
        ..RetryPolicy::default()
    });
    // fib: 1, 1, 2, 3, 5, 8, 13
    assert_eq!(exec.compute_backoff(0), 50);
    assert_eq!(exec.compute_backoff(1), 50);
    assert_eq!(exec.compute_backoff(2), 100);
    assert_eq!(exec.compute_backoff(3), 150);
    assert_eq!(exec.compute_backoff(4), 250);
    assert_eq!(exec.compute_backoff(5), 400);
    assert_eq!(exec.compute_backoff(6), 650);
}

#[test]
fn exponential_backoff_saturates_large_attempt() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Exponential,
        base_delay_ms: 1_000,
        jitter: false,
        ..RetryPolicy::default()
    });
    let delay = exec.compute_backoff(64);
    assert_eq!(delay, u64::MAX);
}

#[test]
fn linear_backoff_saturates() {
    let exec = RetryExecutor::new(RetryPolicy {
        backoff: BackoffStrategy::Linear,
        base_delay_ms: u64::MAX,
        jitter: false,
        ..RetryPolicy::default()
    });
    let delay = exec.compute_backoff(5);
    assert_eq!(delay, u64::MAX);
}

// ===========================================================================
// Jitter
// ===========================================================================

#[test]
fn jitter_zero_delay() {
    let exec = RetryExecutor::with_defaults();
    assert_eq!(exec.apply_jitter(0, 123), 0);
}

#[test]
fn jitter_within_bounds_many_seeds() {
    let exec = RetryExecutor::with_defaults();
    for seed in 0..200 {
        let result = exec.apply_jitter(2000, seed);
        assert!(result >= 1500, "seed {seed}: got {result}");
        assert!(result <= 2500, "seed {seed}: got {result}");
    }
}

#[test]
fn jitter_very_small_delay_unchanged() {
    let exec = RetryExecutor::with_defaults();
    // quarter = 2/4 = 0, returned as-is
    assert_eq!(exec.apply_jitter(2, 99), 2);
}

// ===========================================================================
// parse_retry_after
// ===========================================================================

#[test]
fn parse_retry_after_integer() {
    assert_eq!(RetryExecutor::parse_retry_after("120"), Some(120_000));
}

#[test]
fn parse_retry_after_zero() {
    assert_eq!(RetryExecutor::parse_retry_after("0"), Some(0));
}

#[test]
fn parse_retry_after_large_value() {
    assert_eq!(RetryExecutor::parse_retry_after("3600"), Some(3_600_000));
}

#[test]
fn parse_retry_after_whitespace() {
    assert_eq!(RetryExecutor::parse_retry_after("  30  "), Some(30_000));
}

#[test]
fn parse_retry_after_http_date() {
    assert_eq!(
        RetryExecutor::parse_retry_after("Thu, 01 Dec 2025 16:00:00 GMT"),
        None
    );
}

#[test]
fn parse_retry_after_empty() {
    assert_eq!(RetryExecutor::parse_retry_after(""), None);
}

#[test]
fn parse_retry_after_garbage() {
    assert_eq!(RetryExecutor::parse_retry_after("not-a-number"), None);
}

#[test]
fn parse_retry_after_negative() {
    assert_eq!(RetryExecutor::parse_retry_after("-10"), None);
}

// ===========================================================================
// should_retry
// ===========================================================================

#[test]
fn should_retry_rate_limit_condition() {
    let exec = RetryExecutor::new(RetryExecutor::rate_limit_policy());
    let state = RetryState::new();
    assert!(exec.should_retry(&state, &RetryCondition::RateLimit));
}

#[test]
fn should_retry_server_error_condition() {
    let exec = RetryExecutor::new(RetryExecutor::server_error_policy());
    let state = RetryState::new();
    assert!(exec.should_retry(&state, &RetryCondition::ServerError));
    assert!(exec.should_retry(&state, &RetryCondition::StatusCode(500)));
    assert!(exec.should_retry(&state, &RetryCondition::StatusCode(503)));
    assert!(exec.should_retry(&state, &RetryCondition::StatusCode(599)));
    assert!(!exec.should_retry(&state, &RetryCondition::StatusCode(499)));
}

#[test]
fn should_retry_429_matches_rate_limit() {
    let exec = RetryExecutor::with_defaults();
    let state = RetryState::new();
    assert!(exec.should_retry(&state, &RetryCondition::StatusCode(429)));
}

#[test]
fn should_retry_respects_max_retries() {
    let exec = RetryExecutor::new(RetryPolicy {
        max_retries: 2,
        ..RetryPolicy::default()
    });
    let mut state = RetryState::new();
    assert!(exec.should_retry(&state, &RetryCondition::RateLimit));
    state.attempt = 1;
    assert!(exec.should_retry(&state, &RetryCondition::RateLimit));
    state.attempt = 2;
    assert!(!exec.should_retry(&state, &RetryCondition::RateLimit));
}

#[test]
fn should_retry_custom_condition_match() {
    let exec = RetryExecutor::new(RetryPolicy {
        retry_on: vec![RetryCondition::Custom("db_lock".to_string())],
        ..RetryPolicy::default()
    });
    let state = RetryState::new();
    assert!(exec.should_retry(&state, &RetryCondition::Custom("db_lock".into())));
    assert!(!exec.should_retry(&state, &RetryCondition::Custom("other".into())));
}

#[test]
fn should_retry_no_matching_condition() {
    let exec = RetryExecutor::new(RetryPolicy {
        retry_on: vec![RetryCondition::Timeout],
        ..RetryPolicy::default()
    });
    let state = RetryState::new();
    assert!(!exec.should_retry(&state, &RetryCondition::ConnectionError));
}

// ===========================================================================
// next_delay
// ===========================================================================

#[test]
fn next_delay_exhausted() {
    let exec = RetryExecutor::new(RetryPolicy {
        max_retries: 0,
        jitter: false,
        ..RetryPolicy::default()
    });
    let state = RetryState::new();
    assert_eq!(exec.next_delay(&state), None);
}

#[test]
fn next_delay_first_attempt_returns_base() {
    let exec = RetryExecutor::new(RetryPolicy {
        base_delay_ms: 200,
        max_retries: 5,
        backoff: BackoffStrategy::Constant,
        jitter: false,
        ..RetryPolicy::default()
    });
    let state = RetryState::new();
    assert_eq!(exec.next_delay(&state), Some(200));
}

#[test]
fn next_delay_honours_retry_after() {
    let exec = RetryExecutor::new(RetryPolicy {
        max_delay_ms: 10_000,
        jitter: false,
        ..RetryPolicy::default()
    });
    let mut state = RetryState::new();
    state.retry_after_ms = Some(5_000);
    assert_eq!(exec.next_delay(&state), Some(5_000));
}

#[test]
fn next_delay_clamps_retry_after_to_max() {
    let exec = RetryExecutor::new(RetryPolicy {
        max_delay_ms: 3_000,
        jitter: false,
        ..RetryPolicy::default()
    });
    let mut state = RetryState::new();
    state.retry_after_ms = Some(20_000);
    assert_eq!(exec.next_delay(&state), Some(3_000));
}

#[test]
fn next_delay_clamps_backoff_to_max() {
    let exec = RetryExecutor::new(RetryPolicy {
        base_delay_ms: 1_000,
        max_delay_ms: 5_000,
        max_retries: 10,
        backoff: BackoffStrategy::Exponential,
        jitter: false,
        ..RetryPolicy::default()
    });
    let mut state = RetryState::new();
    state.attempt = 4; // raw = 1000 * 16 = 16000 → clamped to 5000
    assert_eq!(exec.next_delay(&state), Some(5_000));
}

// ===========================================================================
// RetryState
// ===========================================================================

#[test]
fn retry_state_initial() {
    let state = RetryState::new();
    assert_eq!(state.attempt, 0);
    assert_eq!(state.total_delay_ms, 0);
    assert!(state.last_error.is_none());
    assert!(state.retry_after_ms.is_none());
    assert!(state.history.is_empty());
}

#[test]
fn retry_state_record_multiple_attempts() {
    let mut state = RetryState::new();
    state.record_attempt(100, "err1", 1000);
    state.record_attempt(200, "err2", 1200);
    state.record_attempt(400, "err3", 1500);

    assert_eq!(state.attempt, 3);
    assert_eq!(state.total_delay_ms, 700);
    assert_eq!(state.last_error.as_deref(), Some("err3"));
    assert_eq!(state.history.len(), 3);
    assert_eq!(state.history[0].attempt_number, 1);
    assert_eq!(state.history[1].attempt_number, 2);
    assert_eq!(state.history[2].attempt_number, 3);
    assert_eq!(state.history[2].delay_ms, 400);
    assert_eq!(state.history[2].timestamp_ms, 1500);
}

#[test]
fn retry_state_default_is_new() {
    let state = RetryState::default();
    assert_eq!(state.attempt, 0);
    assert!(state.history.is_empty());
}

// ===========================================================================
// Policy builders
// ===========================================================================

#[test]
fn rate_limit_policy_fields() {
    let p = RetryExecutor::rate_limit_policy();
    assert_eq!(p.max_retries, 5);
    assert_eq!(p.base_delay_ms, 1_000);
    assert_eq!(p.max_delay_ms, 60_000);
    assert!(p.jitter);
    assert_eq!(p.backoff, BackoffStrategy::Exponential);
    assert!(p.retry_on.contains(&RetryCondition::RateLimit));
}

#[test]
fn server_error_policy_fields() {
    let p = RetryExecutor::server_error_policy();
    assert_eq!(p.max_retries, 3);
    assert_eq!(p.base_delay_ms, 500);
    assert!(p.jitter);
    assert!(p.retry_on.contains(&RetryCondition::ServerError));
}

#[test]
fn aggressive_policy_fields() {
    let p = RetryExecutor::aggressive_policy();
    assert_eq!(p.max_retries, 10);
    assert_eq!(p.base_delay_ms, 50);
    assert_eq!(p.max_delay_ms, 5_000);
    assert!(!p.jitter);
    assert_eq!(p.backoff, BackoffStrategy::Linear);
    assert_eq!(p.retry_on.len(), 4);
}

#[test]
fn conservative_policy_fields() {
    let p = RetryExecutor::conservative_policy();
    assert_eq!(p.max_retries, 2);
    assert_eq!(p.base_delay_ms, 2_000);
    assert_eq!(p.max_delay_ms, 30_000);
    assert!(p.jitter);
}

// ===========================================================================
// Default policy
// ===========================================================================

#[test]
fn default_policy_contains_all_conditions() {
    let p = RetryPolicy::default();
    assert_eq!(p.retry_on.len(), 4);
    assert!(p.retry_on.contains(&RetryCondition::RateLimit));
    assert!(p.retry_on.contains(&RetryCondition::ServerError));
    assert!(p.retry_on.contains(&RetryCondition::Timeout));
    assert!(p.retry_on.contains(&RetryCondition::ConnectionError));
}

// ===========================================================================
// RetryCondition Display
// ===========================================================================

#[test]
fn retry_condition_display_all_variants() {
    assert_eq!(
        format!("{}", RetryCondition::StatusCode(503)),
        "StatusCode(503)"
    );
    assert_eq!(format!("{}", RetryCondition::RateLimit), "RateLimit");
    assert_eq!(format!("{}", RetryCondition::ServerError), "ServerError");
    assert_eq!(format!("{}", RetryCondition::Timeout), "Timeout");
    assert_eq!(
        format!("{}", RetryCondition::ConnectionError),
        "ConnectionError"
    );
    assert_eq!(
        format!("{}", RetryCondition::Custom("my_cond".into())),
        "Custom(my_cond)"
    );
}

// ===========================================================================
// with_defaults convenience
// ===========================================================================

#[test]
fn with_defaults_uses_default_policy() {
    let exec = RetryExecutor::with_defaults();
    let p = exec.policy();
    assert_eq!(p.max_retries, 3);
    assert_eq!(p.base_delay_ms, 100);
    assert!(p.jitter);
}

// ===========================================================================
// End-to-end retry simulation
// ===========================================================================

#[test]
fn full_retry_simulation_exponential() {
    let policy = RetryPolicy {
        max_retries: 4,
        base_delay_ms: 100,
        max_delay_ms: 5_000,
        backoff: BackoffStrategy::Exponential,
        jitter: false,
        retry_on: vec![RetryCondition::ServerError],
    };
    let exec = RetryExecutor::new(policy);
    let mut state = RetryState::new();

    // Simulate 4 retries
    let mut delays = Vec::new();
    while let Some(delay) = exec.next_delay(&state) {
        if !exec.should_retry(&state, &RetryCondition::ServerError) {
            break;
        }
        delays.push(delay);
        state.record_attempt(
            delay,
            "500 Internal Server Error",
            state.attempt as u64 * 1000,
        );
    }

    assert_eq!(delays, vec![100, 200, 400, 800]);
    assert_eq!(state.attempt, 4);
    assert_eq!(state.total_delay_ms, 1500);
    assert_eq!(state.history.len(), 4);
}

#[test]
fn full_retry_simulation_with_retry_after_header() {
    let policy = RetryPolicy {
        max_retries: 3,
        base_delay_ms: 100,
        max_delay_ms: 10_000,
        backoff: BackoffStrategy::Exponential,
        jitter: false,
        retry_on: vec![RetryCondition::RateLimit],
    };
    let exec = RetryExecutor::new(policy);
    let mut state = RetryState::new();

    // First attempt: get 429 with Retry-After: 5
    let retry_after = RetryExecutor::parse_retry_after("5");
    state.retry_after_ms = retry_after;

    let delay = exec.next_delay(&state).unwrap();
    assert_eq!(delay, 5_000); // honour the server's Retry-After

    state.record_attempt(delay, "429 Too Many Requests", 0);
    state.retry_after_ms = None; // clear for next attempt

    // Second attempt: no Retry-After, use backoff
    let delay2 = exec.next_delay(&state).unwrap();
    assert_eq!(delay2, 200); // attempt=1 → 100 * 2^1 = 200
}
