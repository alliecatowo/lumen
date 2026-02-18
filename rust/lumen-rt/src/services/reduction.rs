//! Reduction counting for cooperative preemption.
//!
//! Each task (lightweight process) is given a *budget* of reductions
//! (conceptually: bytecode instructions executed). After the budget is
//! exhausted the task should voluntarily yield back to the scheduler so that
//! other tasks get a fair share of CPU time.
//!
//! The [`ReductionCounter`] is a lightweight, per-task value type with no heap
//! allocation and no atomics — it lives on the task's stack or inside its
//! process control block.
//!
//! # Typical usage
//!
//! ```rust
//! use lumen_rt::services::reduction::ReductionCounter;
//!
//! let mut counter = ReductionCounter::new(2000);
//! // In the VM dispatch loop:
//! loop {
//!     // … execute one instruction …
//!     if counter.tick() {
//!         // Budget exhausted — yield to the scheduler.
//!         break;
//!     }
//! }
//! // When the task is rescheduled:
//! counter.reset();
//! ```

/// Default reduction budget per scheduling quantum.
///
/// 2 000 reductions is a reasonable starting point for a cooperative
/// scheduler — roughly on par with Erlang's default. The value can be
/// tuned at runtime via [`ReductionCounter::set_budget`].
pub const DEFAULT_BUDGET: u32 = 2_000;

/// A per-task reduction counter for cooperative preemption.
///
/// The counter starts at `budget` and decrements by one on each [`tick()`]
/// call. When it reaches zero, `tick()` returns `true` to signal that the
/// task should yield.
///
/// This is intentionally a plain value type: no `Rc`, no `Arc`, no `Mutex`.
/// It is owned by a single task and accessed from a single thread at a time.
#[derive(Debug, Clone)]
pub struct ReductionCounter {
    /// Reductions remaining in the current quantum.
    remaining: u32,
    /// The full budget that [`reset()`] restores.
    budget: u32,
}

impl ReductionCounter {
    /// Create a new counter with the given budget.
    ///
    /// `remaining` is initialised to `budget`.
    pub fn new(budget: u32) -> Self {
        Self {
            remaining: budget,
            budget,
        }
    }

    /// Consume one reduction.
    ///
    /// Returns `true` when the budget is exhausted (i.e. the task should
    /// yield). Returns `false` while reductions remain.
    ///
    /// When the counter is already at zero, repeated calls continue to
    /// return `true` without underflow.
    #[inline]
    pub fn tick(&mut self) -> bool {
        if self.remaining == 0 {
            return true;
        }
        self.remaining -= 1;
        self.remaining == 0
    }

    /// Reset the counter to the full budget.
    ///
    /// Called when a task is rescheduled after yielding.
    #[inline]
    pub fn reset(&mut self) {
        self.remaining = self.budget;
    }

    /// Return the number of reductions remaining.
    #[inline]
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Return the configured budget.
    #[inline]
    pub fn budget(&self) -> u32 {
        self.budget
    }

    /// Change the budget.
    ///
    /// This updates the budget for future [`reset()`] calls. It does **not**
    /// alter the current `remaining` count — call [`reset()`] afterwards if
    /// you want the new budget to take effect immediately.
    #[inline]
    pub fn set_budget(&mut self, budget: u32) {
        self.budget = budget;
    }

    /// Return the number of reductions consumed since the last reset.
    #[inline]
    pub fn consumed(&self) -> u32 {
        self.budget.saturating_sub(self.remaining)
    }

    /// Return `true` if the budget is exhausted.
    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.remaining == 0
    }
}

impl Default for ReductionCounter {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget() {
        let counter = ReductionCounter::default();
        assert_eq!(counter.budget(), DEFAULT_BUDGET);
        assert_eq!(counter.remaining(), DEFAULT_BUDGET);
        assert!(!counter.is_exhausted());
    }

    #[test]
    fn custom_budget() {
        let counter = ReductionCounter::new(500);
        assert_eq!(counter.budget(), 500);
        assert_eq!(counter.remaining(), 500);
    }

    #[test]
    fn tick_decrements_and_signals_exhaustion() {
        let mut counter = ReductionCounter::new(3);
        assert!(!counter.tick()); // remaining: 2
        assert_eq!(counter.remaining(), 2);
        assert!(!counter.tick()); // remaining: 1
        assert_eq!(counter.remaining(), 1);
        assert!(counter.tick()); // remaining: 0 → exhausted
        assert_eq!(counter.remaining(), 0);
        assert!(counter.is_exhausted());
    }

    #[test]
    fn tick_at_zero_stays_zero() {
        let mut counter = ReductionCounter::new(1);
        assert!(counter.tick()); // 1 → 0
                                 // Repeated ticks at zero should still return true, no underflow.
        assert!(counter.tick());
        assert!(counter.tick());
        assert_eq!(counter.remaining(), 0);
    }

    #[test]
    fn reset_restores_budget() {
        let mut counter = ReductionCounter::new(10);
        for _ in 0..10 {
            counter.tick();
        }
        assert!(counter.is_exhausted());
        counter.reset();
        assert_eq!(counter.remaining(), 10);
        assert!(!counter.is_exhausted());
    }

    #[test]
    fn set_budget_affects_next_reset() {
        let mut counter = ReductionCounter::new(100);
        // Consume some reductions.
        for _ in 0..50 {
            counter.tick();
        }
        assert_eq!(counter.remaining(), 50);

        // Change budget — remaining is NOT altered.
        counter.set_budget(200);
        assert_eq!(counter.remaining(), 50);
        assert_eq!(counter.budget(), 200);

        // After reset, remaining reflects the new budget.
        counter.reset();
        assert_eq!(counter.remaining(), 200);
    }

    #[test]
    fn consumed_tracks_work_done() {
        let mut counter = ReductionCounter::new(100);
        assert_eq!(counter.consumed(), 0);

        for _ in 0..37 {
            counter.tick();
        }
        assert_eq!(counter.consumed(), 37);
        assert_eq!(counter.remaining(), 63);
    }

    #[test]
    fn budget_exhaustion_at_2000() {
        let mut counter = ReductionCounter::default();
        for i in 0..1999 {
            assert!(!counter.tick(), "tick {} should not exhaust", i);
        }
        assert!(
            counter.tick(),
            "tick 1999 should exhaust (remaining hits 0)"
        );
        assert!(counter.is_exhausted());
        assert_eq!(counter.remaining(), 0);
        assert_eq!(counter.consumed(), 2000);
    }

    #[test]
    fn zero_budget() {
        let mut counter = ReductionCounter::new(0);
        assert!(counter.is_exhausted());
        // tick on zero-budget immediately returns true.
        assert!(counter.tick());
        assert_eq!(counter.remaining(), 0);
    }

    #[test]
    fn clone_is_independent() {
        let mut a = ReductionCounter::new(10);
        for _ in 0..5 {
            a.tick();
        }
        let mut b = a.clone();
        assert_eq!(b.remaining(), 5);

        // Mutating b doesn't affect a.
        b.tick();
        assert_eq!(a.remaining(), 5);
        assert_eq!(b.remaining(), 4);
    }

    #[test]
    fn debug_format() {
        let counter = ReductionCounter::new(42);
        let dbg = format!("{:?}", counter);
        assert!(dbg.contains("ReductionCounter"));
        assert!(dbg.contains("42"));
    }
}
