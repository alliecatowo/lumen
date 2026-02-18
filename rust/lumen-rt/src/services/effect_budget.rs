//! Per-effect budget enforcement for the Lumen runtime.
//!
//! While [`crate::services::reduction::ReductionCounter`] provides generic cooperative
//! preemption, this module tracks **per-effect** call counts so that
//! individual effects (e.g. `http`, `fs`, `llm`) can be independently
//! rate-limited at runtime.
//!
//! # Example
//!
//! ```rust
//! use lumen_rt::services::effect_budget::EffectBudgetTracker;
//!
//! let mut tracker = EffectBudgetTracker::new();
//! tracker.set_budget("http", 5);
//!
//! for _ in 0..5 {
//!     assert!(tracker.record_call("http").is_ok());
//! }
//! // Sixth call exceeds the budget.
//! assert!(tracker.record_call("http").is_err());
//! ```

use std::collections::HashMap;

use crate::services::tools::ToolError;

/// Tracks per-effect invocation counts and enforces configurable budgets.
///
/// Effects without an explicit budget are **unconstrained** — calls to
/// [`record_call`] for those effects always succeed.
#[derive(Debug, Clone)]
pub struct EffectBudgetTracker {
    /// Maximum allowed calls per effect name.
    budgets: HashMap<String, u64>,
    /// Number of calls recorded so far per effect name.
    counts: HashMap<String, u64>,
}

impl EffectBudgetTracker {
    /// Create a new tracker with no budgets configured.
    pub fn new() -> Self {
        Self {
            budgets: HashMap::new(),
            counts: HashMap::new(),
        }
    }

    /// Set the maximum number of calls allowed for `effect_name`.
    ///
    /// This replaces any previously configured budget for the same effect.
    /// It does **not** reset the current count — call [`reset`] or
    /// [`reset_effect`] if the counter should restart.
    pub fn set_budget(&mut self, effect_name: &str, max_calls: u64) {
        self.budgets.insert(effect_name.to_string(), max_calls);
    }

    /// Remove the budget for `effect_name`, making it unconstrained.
    ///
    /// Returns `true` if a budget was previously set.
    pub fn remove_budget(&mut self, effect_name: &str) -> bool {
        self.budgets.remove(effect_name).is_some()
    }

    /// Record a call to `effect_name`.
    ///
    /// Returns `Ok(())` if the call is within budget (or no budget is set).
    /// Returns `Err(ToolError::BudgetExhausted { .. })` if the budget is
    /// exceeded.
    pub fn record_call(&mut self, effect_name: &str) -> Result<(), ToolError> {
        let count = self.counts.entry(effect_name.to_string()).or_insert(0);

        if let Some(&limit) = self.budgets.get(effect_name) {
            if *count >= limit {
                return Err(ToolError::BudgetExhausted {
                    effect: effect_name.to_string(),
                    limit: limit as u32,
                    message: format!(
                        "effect '{}' has been called {} time(s), budget is {}",
                        effect_name, *count, limit
                    ),
                });
            }
        }

        *count += 1;
        Ok(())
    }

    /// Return the remaining budget for `effect_name`, or `None` if no budget
    /// is configured.
    pub fn remaining(&self, effect_name: &str) -> Option<u64> {
        let limit = self.budgets.get(effect_name)?;
        let used = self.counts.get(effect_name).copied().unwrap_or(0);
        Some(limit.saturating_sub(used))
    }

    /// Return the current call count for `effect_name`.
    pub fn call_count(&self, effect_name: &str) -> u64 {
        self.counts.get(effect_name).copied().unwrap_or(0)
    }

    /// Return the configured budget for `effect_name`, or `None` if
    /// unconstrained.
    pub fn budget(&self, effect_name: &str) -> Option<u64> {
        self.budgets.get(effect_name).copied()
    }

    /// Reset all counters (but keep budgets).
    pub fn reset(&mut self) {
        self.counts.clear();
    }

    /// Reset the counter for a single effect (budget is kept).
    pub fn reset_effect(&mut self, effect_name: &str) {
        self.counts.remove(effect_name);
    }

    /// Return all effect names that have a budget configured.
    pub fn budgeted_effects(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.budgets.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Return `true` if the budget for `effect_name` is exhausted.
    ///
    /// Returns `false` when no budget is configured (unconstrained).
    pub fn is_exhausted(&self, effect_name: &str) -> bool {
        match self.budgets.get(effect_name) {
            Some(&limit) => {
                let used = self.counts.get(effect_name).copied().unwrap_or(0);
                used >= limit
            }
            None => false,
        }
    }
}

impl Default for EffectBudgetTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tracker_has_no_budgets() {
        let tracker = EffectBudgetTracker::new();
        assert!(tracker.budgeted_effects().is_empty());
        assert_eq!(tracker.remaining("http"), None);
        assert_eq!(tracker.call_count("http"), 0);
    }

    #[test]
    fn set_and_query_budget() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 10);
        assert_eq!(tracker.budget("http"), Some(10));
        assert_eq!(tracker.remaining("http"), Some(10));
        assert_eq!(tracker.budgeted_effects(), vec!["http"]);
    }

    #[test]
    fn record_calls_within_budget() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("fs", 3);

        assert!(tracker.record_call("fs").is_ok());
        assert_eq!(tracker.call_count("fs"), 1);
        assert_eq!(tracker.remaining("fs"), Some(2));

        assert!(tracker.record_call("fs").is_ok());
        assert!(tracker.record_call("fs").is_ok());
        assert_eq!(tracker.call_count("fs"), 3);
        assert_eq!(tracker.remaining("fs"), Some(0));
    }

    #[test]
    fn record_call_exceeds_budget() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("llm", 2);

        assert!(tracker.record_call("llm").is_ok());
        assert!(tracker.record_call("llm").is_ok());

        let err = tracker.record_call("llm").unwrap_err();
        match err {
            ToolError::BudgetExhausted {
                effect,
                limit,
                message,
            } => {
                assert_eq!(effect, "llm");
                assert_eq!(limit, 2);
                assert!(message.contains("llm"));
                assert!(message.contains("2"));
            }
            other => panic!("expected BudgetExhausted, got: {other}"),
        }
    }

    #[test]
    fn unconstrained_effect_always_succeeds() {
        let mut tracker = EffectBudgetTracker::new();
        // No budget set for "trace"
        for _ in 0..100 {
            assert!(tracker.record_call("trace").is_ok());
        }
        assert_eq!(tracker.call_count("trace"), 100);
        assert_eq!(tracker.remaining("trace"), None);
    }

    #[test]
    fn reset_clears_all_counters() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 5);
        tracker.set_budget("fs", 3);

        tracker.record_call("http").unwrap();
        tracker.record_call("http").unwrap();
        tracker.record_call("fs").unwrap();

        tracker.reset();

        assert_eq!(tracker.call_count("http"), 0);
        assert_eq!(tracker.call_count("fs"), 0);
        assert_eq!(tracker.remaining("http"), Some(5));
        assert_eq!(tracker.remaining("fs"), Some(3));
        // Budgets are preserved
        assert_eq!(tracker.budget("http"), Some(5));
    }

    #[test]
    fn reset_single_effect() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 5);
        tracker.set_budget("fs", 3);

        tracker.record_call("http").unwrap();
        tracker.record_call("http").unwrap();
        tracker.record_call("fs").unwrap();

        tracker.reset_effect("http");

        assert_eq!(tracker.call_count("http"), 0);
        assert_eq!(tracker.call_count("fs"), 1); // unchanged
    }

    #[test]
    fn is_exhausted_checks() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 2);

        assert!(!tracker.is_exhausted("http"));
        assert!(!tracker.is_exhausted("unknown")); // unconstrained

        tracker.record_call("http").unwrap();
        assert!(!tracker.is_exhausted("http"));

        tracker.record_call("http").unwrap();
        assert!(tracker.is_exhausted("http"));
    }

    #[test]
    fn remove_budget_makes_unconstrained() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 1);
        tracker.record_call("http").unwrap();

        // Budget exhausted
        assert!(tracker.record_call("http").is_err());

        // Remove budget → unconstrained
        assert!(tracker.remove_budget("http"));
        assert!(tracker.record_call("http").is_ok());
        assert!(!tracker.remove_budget("http")); // second removal returns false
    }

    #[test]
    fn zero_budget_immediately_exhausted() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("deny", 0);

        assert!(tracker.is_exhausted("deny"));
        assert_eq!(tracker.remaining("deny"), Some(0));

        let err = tracker.record_call("deny").unwrap_err();
        match err {
            ToolError::BudgetExhausted { effect, limit, .. } => {
                assert_eq!(effect, "deny");
                assert_eq!(limit, 0);
            }
            other => panic!("expected BudgetExhausted, got: {other}"),
        }
    }

    #[test]
    fn multiple_effects_independent() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 2);
        tracker.set_budget("fs", 3);

        tracker.record_call("http").unwrap();
        tracker.record_call("http").unwrap();
        // http exhausted, but fs still has budget
        assert!(tracker.record_call("http").is_err());
        assert!(tracker.record_call("fs").is_ok());
        assert!(tracker.record_call("fs").is_ok());
        assert!(tracker.record_call("fs").is_ok());
        assert!(tracker.record_call("fs").is_err());
    }

    #[test]
    fn default_tracker_is_empty() {
        let tracker = EffectBudgetTracker::default();
        assert!(tracker.budgeted_effects().is_empty());
    }

    #[test]
    fn set_budget_replaces_previous() {
        let mut tracker = EffectBudgetTracker::new();
        tracker.set_budget("http", 5);
        tracker.record_call("http").unwrap();
        tracker.record_call("http").unwrap();

        // Raise budget
        tracker.set_budget("http", 10);
        assert_eq!(tracker.budget("http"), Some(10));
        assert_eq!(tracker.remaining("http"), Some(8)); // 10 - 2
        assert!(tracker.record_call("http").is_ok());
    }
}
