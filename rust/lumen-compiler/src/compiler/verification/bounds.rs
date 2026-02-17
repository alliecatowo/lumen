//! Compile-time array bounds propagation and index safety analysis.
//!
//! This module provides flow-sensitive analysis to prove or warn about
//! list/tuple index accesses at compile time. By tracking known lengths
//! and variable bounds through branch conditions, the analyzer can
//! determine whether an index access is provably safe, provably unsafe,
//! or unknown.
//!
//! ## Example
//!
//! ```text
//! cell get_first(items: List[Int]) -> Int
//!   if len(items) > 0
//!     items[0]    # Safe — proven in bounds
//!   else
//!     -1
//!   end
//! end
//! ```
//!
//! The analysis infers from `len(items) > 0` that `items` has at least
//! 1 element, making `items[0]` provably safe.

use std::collections::HashMap;

// ── Bounds knowledge ────────────────────────────────────────────────

/// What we know about the bounds of a variable or collection length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundsInfo {
    /// The variable this information pertains to.
    pub variable: String,
    /// Known lower bound (inclusive), if any.
    pub lower: Option<i64>,
    /// Known upper bound (inclusive), if any.
    pub upper: Option<i64>,
    /// Known exact length, if the variable is a collection with a
    /// statically determinable size.
    pub exact_length: Option<usize>,
}

impl BoundsInfo {
    /// Create a new `BoundsInfo` with no knowledge.
    pub fn new(variable: &str) -> Self {
        Self {
            variable: variable.to_string(),
            lower: None,
            upper: None,
            exact_length: None,
        }
    }

    /// Create a `BoundsInfo` with a known exact length.
    pub fn with_exact_length(variable: &str, length: usize) -> Self {
        Self {
            variable: variable.to_string(),
            lower: Some(length as i64),
            upper: Some(length as i64),
            exact_length: Some(length),
        }
    }

    /// Create a `BoundsInfo` with known lower and upper bounds.
    pub fn with_bounds(variable: &str, lower: Option<i64>, upper: Option<i64>) -> Self {
        Self {
            variable: variable.to_string(),
            lower,
            upper,
            exact_length: None,
        }
    }
}

// ── Comparison operator ─────────────────────────────────────────────

/// Comparison operators used in bounds conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundsOp {
    /// Greater than (`>`).
    Gt,
    /// Less than (`<`).
    Lt,
    /// Greater than or equal (`>=`).
    Ge,
    /// Less than or equal (`<=`).
    Le,
    /// Equal (`==`).
    Eq,
    /// Not equal (`!=`).
    Ne,
}

// ── Active condition ────────────────────────────────────────────────

/// A condition currently active in the analysis scope, e.g. from an
/// `if` branch guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCondition {
    /// The variable being constrained.
    pub variable: String,
    /// The comparison operator.
    pub op: BoundsOp,
    /// The constant value on the right-hand side.
    pub value: i64,
    /// Whether the condition is negated (e.g. we are in the else branch).
    pub negated: bool,
}

// ── Bounds result ───────────────────────────────────────────────────

/// The result of checking an index access against known bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundsResult {
    /// The access is provably within bounds.
    Safe,
    /// The access is provably out of bounds.
    Unsafe {
        /// Human-readable explanation of why the access is unsafe.
        reason: String,
    },
    /// There is insufficient information to determine safety.
    Unknown,
    /// The access is safe only under a specific condition.
    ConditionalSafe {
        /// Description of the condition under which the access is safe.
        condition: String,
    },
}

// ── Index check record ──────────────────────────────────────────────

/// Records the details and result of a single index access check.
#[derive(Debug, Clone)]
pub struct IndexCheck {
    /// The collection being indexed.
    pub collection: String,
    /// String representation of the index expression.
    pub index_expr: String,
    /// Statically known index value, if available.
    pub index_value: Option<i64>,
    /// Bounds information about the collection's length, if known.
    pub collection_length: Option<BoundsInfo>,
    /// The outcome of the bounds check.
    pub result: BoundsResult,
}

// ── Bounds context (flow-sensitive) ─────────────────────────────────

/// Flow-sensitive context that accumulates bounds knowledge as analysis
/// proceeds through branches and scopes.
#[derive(Debug, Clone)]
pub struct BoundsContext {
    /// Known bounds for each variable, keyed by variable name.
    pub known_bounds: HashMap<String, BoundsInfo>,
    /// Stack of conditions active in the current scope (from `if` guards).
    pub conditions: Vec<ActiveCondition>,
    /// Current nesting depth (for tracking scope entry/exit).
    pub scope_depth: usize,
}

impl BoundsContext {
    /// Create a new, empty bounds context.
    pub fn new() -> Self {
        Self {
            known_bounds: HashMap::new(),
            conditions: Vec::new(),
            scope_depth: 0,
        }
    }

    /// Record a condition from an `if` branch guard.
    pub fn push_condition(&mut self, cond: ActiveCondition) {
        // Also refine bounds for the variable mentioned in the condition.
        if !cond.negated {
            self.infer_from_condition(&cond.variable, &cond.op, cond.value);
        } else {
            // Negated condition: flip the operator.
            let flipped = negate_op(&cond.op);
            self.infer_from_condition(&cond.variable, &flipped, cond.value);
        }
        self.conditions.push(cond);
    }

    /// Remove the most recently pushed condition (when leaving a branch).
    pub fn pop_condition(&mut self) {
        self.conditions.pop();
    }

    /// Record a known exact length for a collection variable.
    pub fn set_length(&mut self, var: &str, length: usize) {
        let info = self
            .known_bounds
            .entry(var.to_string())
            .or_insert_with(|| BoundsInfo::new(var));
        info.exact_length = Some(length);
        info.lower = Some(length as i64);
        info.upper = Some(length as i64);
    }

    /// Set arbitrary lower/upper bounds for a variable.
    pub fn set_bounds(&mut self, var: &str, lower: Option<i64>, upper: Option<i64>) {
        let info = self
            .known_bounds
            .entry(var.to_string())
            .or_insert_with(|| BoundsInfo::new(var));
        if let Some(l) = lower {
            info.lower = Some(match info.lower {
                Some(existing) => existing.max(l),
                None => l,
            });
        }
        if let Some(u) = upper {
            info.upper = Some(match info.upper {
                Some(existing) => existing.min(u),
                None => u,
            });
        }
    }

    /// Refine known bounds for `var` based on a comparison condition.
    ///
    /// For example, if we learn `x > 3`, the lower bound of `x` becomes
    /// at least 4. If we learn `x <= 10`, the upper bound becomes at
    /// most 10.
    pub fn infer_from_condition(&mut self, var: &str, op: &BoundsOp, value: i64) {
        let info = self
            .known_bounds
            .entry(var.to_string())
            .or_insert_with(|| BoundsInfo::new(var));
        match op {
            BoundsOp::Gt => {
                // var > value  =>  var >= value + 1
                let new_lower = value + 1;
                info.lower = Some(match info.lower {
                    Some(existing) => existing.max(new_lower),
                    None => new_lower,
                });
            }
            BoundsOp::Ge => {
                // var >= value
                info.lower = Some(match info.lower {
                    Some(existing) => existing.max(value),
                    None => value,
                });
            }
            BoundsOp::Lt => {
                // var < value  =>  var <= value - 1
                let new_upper = value - 1;
                info.upper = Some(match info.upper {
                    Some(existing) => existing.min(new_upper),
                    None => new_upper,
                });
            }
            BoundsOp::Le => {
                // var <= value
                info.upper = Some(match info.upper {
                    Some(existing) => existing.min(value),
                    None => value,
                });
            }
            BoundsOp::Eq => {
                // var == value  =>  lower = upper = value
                info.lower = Some(value);
                info.upper = Some(value);
            }
            BoundsOp::Ne => {
                // var != value — doesn't tighten a range in general,
                // so we leave bounds unchanged.
            }
        }
    }
}

impl Default for BoundsContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── Index checking ──────────────────────────────────────────────────

/// Check whether a constant index into a collection is within bounds.
///
/// Returns `Safe` if the access is provably valid, `Unsafe` if it is
/// provably invalid, or `Unknown` if there is insufficient information.
pub fn check_index_access(ctx: &BoundsContext, collection: &str, index: i64) -> BoundsResult {
    if let Some(info) = ctx.known_bounds.get(collection) {
        // If we know the exact length, we can give a definitive answer.
        if let Some(len) = info.exact_length {
            let len_i64 = len as i64;
            // Support Python-style negative indices.
            if index >= 0 && index < len_i64 {
                return BoundsResult::Safe;
            }
            if index < 0 && index >= -len_i64 {
                return BoundsResult::Safe;
            }
            return BoundsResult::Unsafe {
                reason: format!(
                    "index {} is out of bounds for collection '{}' with length {}",
                    index, collection, len
                ),
            };
        }

        // If we have a lower bound on length, we can partially reason.
        if let Some(min_len) = info.lower {
            if index >= 0 && index < min_len {
                return BoundsResult::Safe;
            }
            // If the index is negative and we don't know the exact length,
            // we can't be sure.
        }

        // If we have an upper bound on length and the index exceeds it,
        // the access is definitely unsafe.
        if let Some(max_len) = info.upper {
            if index >= 0 && index >= max_len {
                return BoundsResult::Unsafe {
                    reason: format!(
                        "index {} is out of bounds: collection '{}' has at most {} elements",
                        index, collection, max_len
                    ),
                };
            }
        }
    }

    BoundsResult::Unknown
}

/// Check whether a dynamic (variable) index into a collection is within bounds.
///
/// Uses known bounds on both the index variable and the collection length
/// to determine safety.
pub fn check_dynamic_index(ctx: &BoundsContext, collection: &str, index_var: &str) -> BoundsResult {
    let col_info = ctx.known_bounds.get(collection);
    let idx_info = ctx.known_bounds.get(index_var);

    match (col_info, idx_info) {
        (Some(col), Some(idx)) => {
            // We need both bounds on the index and a known collection size.
            let col_len = col.exact_length.map(|l| l as i64).or(col.lower);

            if let (Some(idx_low), Some(idx_high), Some(len)) = (idx.lower, idx.upper, col_len) {
                if idx_low >= 0 && idx_high < len {
                    return BoundsResult::Safe;
                }
                if idx_high >= len || idx_low < 0 {
                    return BoundsResult::Unsafe {
                        reason: format!(
                            "index '{}' (range [{}, {}]) may be out of bounds for '{}' (length {})",
                            index_var, idx_low, idx_high, collection, len
                        ),
                    };
                }
            }

            BoundsResult::Unknown
        }
        _ => BoundsResult::Unknown,
    }
}

// ── Length inference from conditions ─────────────────────────────────

/// Infer collection length bounds from a condition like `len(x) > 0`.
///
/// `condition_var` is the variable name (e.g. `"x"` from `len(x)`),
/// `op` is the comparison operator, and `value` is the constant
/// being compared against.
///
/// Returns `Some(BoundsInfo)` with the inferred length bounds, or
/// `None` if no useful information can be extracted.
pub fn infer_length_from_condition(
    condition_var: &str,
    op: &BoundsOp,
    value: i64,
) -> Option<BoundsInfo> {
    let mut info = BoundsInfo::new(condition_var);

    match op {
        BoundsOp::Gt => {
            // len(x) > value  =>  min length is value + 1
            if value >= 0 {
                info.lower = Some(value + 1);
                Some(info)
            } else {
                None
            }
        }
        BoundsOp::Ge => {
            // len(x) >= value  =>  min length is value
            if value >= 0 {
                info.lower = Some(value);
                Some(info)
            } else {
                None
            }
        }
        BoundsOp::Lt => {
            // len(x) < value  =>  max length is value - 1
            if value > 0 {
                info.upper = Some(value - 1);
                Some(info)
            } else {
                None
            }
        }
        BoundsOp::Le => {
            // len(x) <= value  =>  max length is value
            if value >= 0 {
                info.upper = Some(value);
                Some(info)
            } else {
                None
            }
        }
        BoundsOp::Eq => {
            // len(x) == value  =>  exact length
            if value >= 0 {
                info.exact_length = Some(value as usize);
                info.lower = Some(value);
                info.upper = Some(value);
                Some(info)
            } else {
                None
            }
        }
        BoundsOp::Ne => {
            // len(x) != value — not enough to tighten bounds
            None
        }
    }
}

// ── Diagnostic generation ───────────────────────────────────────────

/// A diagnostic message produced by the bounds checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundsDiagnostic {
    /// The collection being accessed.
    pub collection: String,
    /// String representation of the index expression.
    pub index: String,
    /// The bounds check result that triggered this diagnostic.
    pub result: BoundsResult,
    /// Source line number where the access occurs.
    pub line: usize,
    /// Optional suggestion for the programmer.
    pub suggestion: Option<String>,
}

/// Generate a diagnostic for an index check, if warranted.
///
/// - `Safe` accesses produce no diagnostic (`None`).
/// - `Unsafe` accesses produce an error-level diagnostic.
/// - `Unknown` accesses produce a warning with a suggestion to add a
///   bounds check.
/// - `ConditionalSafe` accesses produce an informational diagnostic.
pub fn generate_diagnostic(check: &IndexCheck, line: usize) -> Option<BoundsDiagnostic> {
    match &check.result {
        BoundsResult::Safe => None,
        BoundsResult::Unsafe { reason } => Some(BoundsDiagnostic {
            collection: check.collection.clone(),
            index: check.index_expr.clone(),
            result: check.result.clone(),
            line,
            suggestion: Some(format!(
                "index is out of bounds: {}. Add a bounds check before accessing.",
                reason
            )),
        }),
        BoundsResult::Unknown => Some(BoundsDiagnostic {
            collection: check.collection.clone(),
            index: check.index_expr.clone(),
            result: check.result.clone(),
            line,
            suggestion: Some(format!(
                "cannot prove index '{}' is in bounds for '{}'. \
                 Consider adding a length check: `if len({}) > {}`",
                check.index_expr, check.collection, check.collection, check.index_expr
            )),
        }),
        BoundsResult::ConditionalSafe { condition } => Some(BoundsDiagnostic {
            collection: check.collection.clone(),
            index: check.index_expr.clone(),
            result: check.result.clone(),
            line,
            suggestion: Some(format!("access is safe only when: {}", condition)),
        }),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Negate a comparison operator (for else-branch analysis).
fn negate_op(op: &BoundsOp) -> BoundsOp {
    match op {
        BoundsOp::Gt => BoundsOp::Le,
        BoundsOp::Lt => BoundsOp::Ge,
        BoundsOp::Ge => BoundsOp::Lt,
        BoundsOp::Le => BoundsOp::Gt,
        BoundsOp::Eq => BoundsOp::Ne,
        BoundsOp::Ne => BoundsOp::Eq,
    }
}

// ── Unit tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_info_new_is_empty() {
        let info = BoundsInfo::new("x");
        assert_eq!(info.variable, "x");
        assert_eq!(info.lower, None);
        assert_eq!(info.upper, None);
        assert_eq!(info.exact_length, None);
    }

    #[test]
    fn bounds_info_with_exact_length() {
        let info = BoundsInfo::with_exact_length("items", 5);
        assert_eq!(info.exact_length, Some(5));
        assert_eq!(info.lower, Some(5));
        assert_eq!(info.upper, Some(5));
    }

    #[test]
    fn bounds_context_starts_empty() {
        let ctx = BoundsContext::new();
        assert!(ctx.known_bounds.is_empty());
        assert!(ctx.conditions.is_empty());
        assert_eq!(ctx.scope_depth, 0);
    }

    #[test]
    fn negate_op_round_trips() {
        assert_eq!(negate_op(&BoundsOp::Gt), BoundsOp::Le);
        assert_eq!(negate_op(&BoundsOp::Le), BoundsOp::Gt);
        assert_eq!(negate_op(&BoundsOp::Eq), BoundsOp::Ne);
        assert_eq!(negate_op(&BoundsOp::Ne), BoundsOp::Eq);
    }
}
