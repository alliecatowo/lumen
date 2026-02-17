//! Counter-example generation for constraint violations.
//!
//! When the verification solver determines that a constraint is violated
//! (UNSAT), this module generates concrete counter-examples showing input
//! values that cause the violation. It uses boundary value analysis to
//! find violating assignments for numeric, boolean, and string constraints.

use std::fmt;

// ── Concrete values ─────────────────────────────────────────────────

/// A concrete runtime value used in counter-examples.
#[derive(Debug, Clone, PartialEq)]
pub enum ConcreteValue {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// String value.
    Str(String),
    /// Null value.
    Null,
    /// List of values.
    List(Vec<ConcreteValue>),
    /// Tuple of values.
    Tuple(Vec<ConcreteValue>),
    /// Record with a type name and named fields.
    Record(String, Vec<(String, ConcreteValue)>),
}

impl fmt::Display for ConcreteValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConcreteValue::Int(v) => write!(f, "{}", v),
            ConcreteValue::Float(v) => write!(f, "{}", v),
            ConcreteValue::Bool(v) => write!(f, "{}", v),
            ConcreteValue::Str(v) => write!(f, "\"{}\"", v),
            ConcreteValue::Null => write!(f, "null"),
            ConcreteValue::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            ConcreteValue::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            ConcreteValue::Record(name, fields) => {
                write!(f, "{}(", name)?;
                for (i, (fname, fval)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", fname, fval)?;
                }
                write!(f, ")")
            }
        }
    }
}

// ── Constraint role ─────────────────────────────────────────────────

/// The role a variable plays within a constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintRole {
    /// An input parameter to a cell or function.
    Input,
    /// A return value or output.
    Output,
    /// An intermediate computed value.
    Intermediate,
    /// A bound/limit value from a constraint literal.
    Bound,
}

impl fmt::Display for ConstraintRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstraintRole::Input => write!(f, "input"),
            ConstraintRole::Output => write!(f, "output"),
            ConstraintRole::Intermediate => write!(f, "intermediate"),
            ConstraintRole::Bound => write!(f, "bound"),
        }
    }
}

// ── Variable binding ────────────────────────────────────────────────

/// A variable assignment in a counter-example.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableBinding {
    /// Variable name.
    pub name: String,
    /// The concrete value assigned.
    pub value: ConcreteValue,
    /// The role this variable plays in the constraint.
    pub constraint_role: ConstraintRole,
}

// ── Evaluation step ─────────────────────────────────────────────────

/// A single step in the evaluation trace of a counter-example.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalStep {
    /// The expression being evaluated (human-readable).
    pub expression: String,
    /// The result of evaluating that expression.
    pub result: ConcreteValue,
    /// An optional explanatory note.
    pub note: Option<String>,
}

// ── Counter-example ─────────────────────────────────────────────────

/// A concrete counter-example demonstrating a constraint violation.
///
/// Contains variable assignments that cause the constraint to be violated,
/// along with a step-by-step evaluation trace.
#[derive(Debug, Clone, PartialEq)]
pub struct CounterExample {
    /// Variable assignments that violate the constraint.
    pub variables: Vec<VariableBinding>,
    /// The constraint that was violated (human-readable).
    pub violated_constraint: String,
    /// A human-readable explanation of why the constraint is violated.
    pub explanation: String,
    /// Step-by-step evaluation trace.
    pub trace: Vec<EvalStep>,
}

// ── Comparison operator ─────────────────────────────────────────────

/// Comparison operators recognized by the simplified constraint parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompOp {
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

impl fmt::Display for CompOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompOp::Gt => write!(f, ">"),
            CompOp::Lt => write!(f, "<"),
            CompOp::Ge => write!(f, ">="),
            CompOp::Le => write!(f, "<="),
            CompOp::Eq => write!(f, "=="),
            CompOp::Ne => write!(f, "!="),
        }
    }
}

// ── Parsed constraint ───────────────────────────────────────────────

/// A simplified parsed representation of a constraint expression.
///
/// This is used by the counter-example generator to understand
/// the structure of constraint strings without requiring the full
/// compiler AST.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedConstraint {
    /// A simple comparison: `left op right`.
    Comparison {
        /// Left-hand operand (variable name or literal).
        left: String,
        /// Comparison operator.
        op: CompOp,
        /// Right-hand operand (variable name or literal).
        right: String,
    },
    /// Conjunction of two constraints.
    And(Box<ParsedConstraint>, Box<ParsedConstraint>),
    /// Disjunction of two constraints.
    Or(Box<ParsedConstraint>, Box<ParsedConstraint>),
    /// Negation of a constraint.
    Not(Box<ParsedConstraint>),
    /// A function call comparison: `name(arg) op value`.
    FuncCall {
        /// Function name (e.g. "len").
        name: String,
        /// Function argument (e.g. variable name "s").
        arg: String,
        /// Comparison operator.
        op: CompOp,
        /// Value to compare against.
        value: String,
    },
}

// ── Constraint parsing ──────────────────────────────────────────────

/// Parse a simplified constraint expression string.
///
/// Handles patterns like:
/// - `x > 0`
/// - `x >= 1 and x <= 10`
/// - `len(s) > 0`
/// - `not(x > 0)`
/// - `x == 5`
///
/// Returns `None` for expressions too complex to parse with this
/// simplified parser (e.g. `a + b == c`).
pub fn parse_simple_constraint(expr: &str) -> Option<ParsedConstraint> {
    let expr = expr.trim();

    // Try "and" split (lowest precedence after "or")
    if let Some(result) = try_parse_binary_connective(expr, " and ") {
        return Some(result);
    }

    // Try "or" split
    if let Some(result) = try_parse_binary_connective_or(expr, " or ") {
        return Some(result);
    }

    // Try "not(...)" prefix
    if let Some(inner) = expr.strip_prefix("not(").and_then(|s| s.strip_suffix(')')) {
        let parsed_inner = parse_simple_constraint(inner)?;
        return Some(ParsedConstraint::Not(Box::new(parsed_inner)));
    }

    // Try function call pattern: name(arg) op value
    if let Some(result) = try_parse_func_call(expr) {
        return Some(result);
    }

    // Try simple comparison: left op right
    try_parse_comparison(expr)
}

/// Try to split on " and " and parse both sides.
fn try_parse_binary_connective(expr: &str, keyword: &str) -> Option<ParsedConstraint> {
    // Find the keyword, but only at the top level (not inside parentheses).
    let pos = find_top_level_keyword(expr, keyword)?;
    let left = &expr[..pos];
    let right = &expr[pos + keyword.len()..];
    let left_parsed = parse_simple_constraint(left)?;
    let right_parsed = parse_simple_constraint(right)?;
    Some(ParsedConstraint::And(
        Box::new(left_parsed),
        Box::new(right_parsed),
    ))
}

/// Try to split on " or " and parse both sides.
fn try_parse_binary_connective_or(expr: &str, keyword: &str) -> Option<ParsedConstraint> {
    let pos = find_top_level_keyword(expr, keyword)?;
    let left = &expr[..pos];
    let right = &expr[pos + keyword.len()..];
    let left_parsed = parse_simple_constraint(left)?;
    let right_parsed = parse_simple_constraint(right)?;
    Some(ParsedConstraint::Or(
        Box::new(left_parsed),
        Box::new(right_parsed),
    ))
}

/// Find a keyword at the top level (not nested inside parentheses).
fn find_top_level_keyword(expr: &str, keyword: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = expr.as_bytes();
    let kw_bytes = keyword.as_bytes();
    if bytes.len() < kw_bytes.len() {
        return None;
    }
    for i in 0..=(bytes.len() - kw_bytes.len()) {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + kw_bytes.len()] == kw_bytes {
            return Some(i);
        }
    }
    None
}

/// Try to parse a function call comparison like `len(s) > 0`.
fn try_parse_func_call(expr: &str) -> Option<ParsedConstraint> {
    // Pattern: ident(ident) op value
    let paren_open = expr.find('(')?;
    let paren_close = expr.find(')')?;
    if paren_close <= paren_open {
        return None;
    }
    let func_name = expr[..paren_open].trim();
    let func_arg = expr[paren_open + 1..paren_close].trim();

    // Validate that func_name is an identifier
    if func_name.is_empty() || !func_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    if func_arg.is_empty() || !func_arg.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    let rest = expr[paren_close + 1..].trim();
    let (op, value_str) = parse_op_and_rest(rest)?;

    Some(ParsedConstraint::FuncCall {
        name: func_name.to_string(),
        arg: func_arg.to_string(),
        op,
        value: value_str.to_string(),
    })
}

/// Try to parse a simple comparison expression like `x > 0`.
fn try_parse_comparison(expr: &str) -> Option<ParsedConstraint> {
    // Try each operator (longer ones first to avoid prefix matches)
    for (op_str, op) in &[
        (">=", CompOp::Ge),
        ("<=", CompOp::Le),
        ("!=", CompOp::Ne),
        ("==", CompOp::Eq),
        (">", CompOp::Gt),
        ("<", CompOp::Lt),
    ] {
        if let Some(pos) = expr.find(op_str) {
            let left = expr[..pos].trim();
            let right = expr[pos + op_str.len()..].trim();
            if left.is_empty() || right.is_empty() {
                continue;
            }
            // Reject complex expressions (containing arithmetic operators)
            if left.contains('+') || left.contains('-') || left.contains('*') || left.contains('/')
            {
                return None;
            }
            return Some(ParsedConstraint::Comparison {
                left: left.to_string(),
                op: *op,
                right: right.to_string(),
            });
        }
    }
    None
}

/// Parse an operator and the rest of a string: ` > 0` -> (Gt, "0").
fn parse_op_and_rest(s: &str) -> Option<(CompOp, &str)> {
    let s = s.trim();
    for (op_str, op) in &[
        (">=", CompOp::Ge),
        ("<=", CompOp::Le),
        ("!=", CompOp::Ne),
        ("==", CompOp::Eq),
        (">", CompOp::Gt),
        ("<", CompOp::Lt),
    ] {
        if let Some(rest) = s.strip_prefix(op_str) {
            let val = rest.trim();
            if !val.is_empty() {
                return Some((*op, val));
            }
        }
    }
    None
}

// ── Counter-example generation ──────────────────────────────────────

/// Generate a counter-example for a violated constraint.
///
/// Given a constraint expression string, a list of variable names with
/// their types, and whether the constraint was violated, this function
/// attempts to find concrete values that cause the violation using
/// boundary value analysis.
///
/// # Arguments
///
/// * `constraint` — The constraint expression as a string (e.g. `"x > 0"`).
/// * `variables` — Pairs of (variable name, type name) for the variables
///   appearing in the constraint.
/// * `violated` — Whether the constraint is known to be violated. If `false`,
///   returns `None` (no counter-example needed).
///
/// # Returns
///
/// `Some(CounterExample)` if a counter-example could be generated, or
/// `None` if the constraint is not violated or cannot be analyzed.
pub fn generate_counterexample(
    constraint: &str,
    variables: &[(String, String)],
    violated: bool,
) -> Option<CounterExample> {
    if !violated {
        return None;
    }

    let parsed = parse_simple_constraint(constraint)?;
    generate_from_parsed(constraint, &parsed, variables)
}

/// Generate a counter-example from a parsed constraint.
fn generate_from_parsed(
    constraint_str: &str,
    parsed: &ParsedConstraint,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    match parsed {
        ParsedConstraint::Comparison { left, op, right } => {
            generate_comparison_counterexample(constraint_str, left, *op, right, variables)
        }
        ParsedConstraint::And(lhs, rhs) => {
            generate_and_counterexample(constraint_str, lhs, rhs, variables)
        }
        ParsedConstraint::Or(lhs, rhs) => {
            generate_or_counterexample(constraint_str, lhs, rhs, variables)
        }
        ParsedConstraint::Not(inner) => {
            generate_not_counterexample(constraint_str, inner, variables)
        }
        ParsedConstraint::FuncCall {
            name,
            arg,
            op,
            value,
        } => generate_func_call_counterexample(constraint_str, name, arg, *op, value, variables),
    }
}

/// Generate a counter-example for a simple comparison constraint.
fn generate_comparison_counterexample(
    constraint_str: &str,
    left: &str,
    op: CompOp,
    right: &str,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    // Determine which side is a variable and which is a literal
    let var_name: &str;
    let bound_value: i64;
    let effective_op: CompOp;

    if let Ok(rv) = right.parse::<i64>() {
        var_name = left;
        bound_value = rv;
        effective_op = op;
    } else if let Ok(lv) = left.parse::<i64>() {
        var_name = right;
        bound_value = lv;
        // Flip the operator: "5 > x" means x < 5
        effective_op = flip_op(op);
    } else {
        // Both sides are variables or non-numeric — try boolean
        return generate_boolean_counterexample(constraint_str, left, op, right, variables);
    }

    let violating_value = find_violating_int(effective_op, bound_value);
    let role = find_role(var_name, variables);

    let trace = vec![EvalStep {
        expression: constraint_str.to_string(),
        result: ConcreteValue::Bool(false),
        note: Some(format!(
            "{} = {} violates {} {} {}",
            var_name, violating_value, var_name, op, bound_value
        )),
    }];

    Some(CounterExample {
        variables: vec![VariableBinding {
            name: var_name.to_string(),
            value: ConcreteValue::Int(violating_value),
            constraint_role: role,
        }],
        violated_constraint: constraint_str.to_string(),
        explanation: format!(
            "When {} = {}, the constraint {} is false",
            var_name, violating_value, constraint_str
        ),
        trace,
    })
}

/// Generate a counter-example for a boolean comparison (e.g. `x == true`).
fn generate_boolean_counterexample(
    constraint_str: &str,
    left: &str,
    op: CompOp,
    right: &str,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    // Handle "x == true", "x == false", "x != true", "x != false"
    let (var_name, expected_bool) = if right == "true" || right == "false" {
        (left, right == "true")
    } else if left == "true" || left == "false" {
        (right, left == "true")
    } else {
        // Two non-boolean, non-numeric variables — cannot generate
        return None;
    };

    let violating = match op {
        CompOp::Eq => !expected_bool,
        CompOp::Ne => expected_bool,
        _ => return None,
    };

    let role = find_role(var_name, variables);

    let trace = vec![EvalStep {
        expression: constraint_str.to_string(),
        result: ConcreteValue::Bool(false),
        note: None,
    }];

    Some(CounterExample {
        variables: vec![VariableBinding {
            name: var_name.to_string(),
            value: ConcreteValue::Bool(violating),
            constraint_role: role,
        }],
        violated_constraint: constraint_str.to_string(),
        explanation: format!(
            "When {} = {}, the constraint {} is false",
            var_name, violating, constraint_str
        ),
        trace,
    })
}

/// Generate a counter-example for an "and" constraint by violating one side.
fn generate_and_counterexample(
    constraint_str: &str,
    lhs: &ParsedConstraint,
    rhs: &ParsedConstraint,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    // For "x >= a and x <= b", try boundary values that violate either side.
    // Strategy: extract bounds from both sides and pick a value outside the range.

    if let (
        ParsedConstraint::Comparison {
            left: l_left,
            op: l_op,
            right: l_right,
        },
        ParsedConstraint::Comparison {
            left: r_left,
            op: r_op,
            right: r_right,
        },
    ) = (lhs, rhs)
    {
        // Check if both constraints are on the same variable
        let l_var = if l_right.parse::<i64>().is_ok() {
            l_left.as_str()
        } else if l_left.parse::<i64>().is_ok() {
            l_right.as_str()
        } else {
            ""
        };
        let r_var = if r_right.parse::<i64>().is_ok() {
            r_left.as_str()
        } else if r_left.parse::<i64>().is_ok() {
            r_right.as_str()
        } else {
            ""
        };

        if !l_var.is_empty() && l_var == r_var {
            // Both sides constrain the same variable — find a violating value
            // by going outside the range defined by the conjunction.
            let l_bound = l_right
                .parse::<i64>()
                .ok()
                .or_else(|| l_left.parse::<i64>().ok())?;
            let r_bound = r_right
                .parse::<i64>()
                .ok()
                .or_else(|| r_left.parse::<i64>().ok())?;

            let l_eff_op = if l_right.parse::<i64>().is_ok() {
                *l_op
            } else {
                flip_op(*l_op)
            };
            let r_eff_op = if r_right.parse::<i64>().is_ok() {
                *r_op
            } else {
                flip_op(*r_op)
            };

            // Try to violate the lower bound
            let violating =
                find_violating_int_for_conjunction(l_eff_op, l_bound, r_eff_op, r_bound);
            let role = find_role(l_var, variables);

            let trace = vec![
                EvalStep {
                    expression: lhs_to_string(lhs),
                    result: ConcreteValue::Bool(!eval_comparison_bool(
                        violating, l_eff_op, l_bound,
                    )),
                    note: None,
                },
                EvalStep {
                    expression: lhs_to_string(rhs),
                    result: ConcreteValue::Bool(!eval_comparison_bool(
                        violating, r_eff_op, r_bound,
                    )),
                    note: None,
                },
                EvalStep {
                    expression: constraint_str.to_string(),
                    result: ConcreteValue::Bool(false),
                    note: None,
                },
            ];

            return Some(CounterExample {
                variables: vec![VariableBinding {
                    name: l_var.to_string(),
                    value: ConcreteValue::Int(violating),
                    constraint_role: role,
                }],
                violated_constraint: constraint_str.to_string(),
                explanation: format!(
                    "When {} = {}, the constraint {} is false",
                    l_var, violating, constraint_str
                ),
                trace,
            });
        }
    }

    // Fall back: try to generate from the left side
    generate_from_parsed(constraint_str, lhs, variables)
}

/// Generate a counter-example for an "or" constraint by violating both sides.
fn generate_or_counterexample(
    constraint_str: &str,
    lhs: &ParsedConstraint,
    _rhs: &ParsedConstraint,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    // For "or", we need to violate *both* sides simultaneously.
    // This is more complex; fall back to generating from one side.
    generate_from_parsed(constraint_str, lhs, variables)
}

/// Generate a counter-example for a "not" constraint.
fn generate_not_counterexample(
    constraint_str: &str,
    inner: &ParsedConstraint,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    // "not(C)" is violated when C is true.
    // So we need to find values that make C true (i.e. satisfy C).
    match inner {
        ParsedConstraint::Comparison { left, op, right } => {
            if let Ok(rv) = right.parse::<i64>() {
                let satisfying = find_satisfying_int(*op, rv);
                let role = find_role(left, variables);
                let trace = vec![EvalStep {
                    expression: constraint_str.to_string(),
                    result: ConcreteValue::Bool(false),
                    note: Some(format!(
                        "{} = {} makes inner constraint true",
                        left, satisfying
                    )),
                }];
                Some(CounterExample {
                    variables: vec![VariableBinding {
                        name: left.to_string(),
                        value: ConcreteValue::Int(satisfying),
                        constraint_role: role,
                    }],
                    violated_constraint: constraint_str.to_string(),
                    explanation: format!(
                        "When {} = {}, the constraint {} is false",
                        left, satisfying, constraint_str
                    ),
                    trace,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Generate a counter-example for a function call constraint like `len(s) > 0`.
fn generate_func_call_counterexample(
    constraint_str: &str,
    func_name: &str,
    arg: &str,
    op: CompOp,
    value: &str,
    variables: &[(String, String)],
) -> Option<CounterExample> {
    let bound: i64 = value.parse().ok()?;
    let role = find_role(arg, variables);

    match func_name {
        "len" => {
            // For len(s) op value, generate a string with violating length.
            let violating_len = find_violating_int(op, bound);
            // Clamp to 0 since string length cannot be negative
            let actual_len = violating_len.max(0) as usize;
            let violating_str = "x".repeat(actual_len);
            // If violating_len was negative, use empty string
            let final_str = if violating_len < 0 {
                String::new()
            } else {
                violating_str
            };

            let trace = vec![
                EvalStep {
                    expression: format!("len({})", arg),
                    result: ConcreteValue::Int(final_str.len() as i64),
                    note: None,
                },
                EvalStep {
                    expression: constraint_str.to_string(),
                    result: ConcreteValue::Bool(false),
                    note: None,
                },
            ];

            Some(CounterExample {
                variables: vec![VariableBinding {
                    name: arg.to_string(),
                    value: ConcreteValue::Str(final_str.clone()),
                    constraint_role: role,
                }],
                violated_constraint: constraint_str.to_string(),
                explanation: format!(
                    "When {} = \"{}\", len({}) = {} which violates {}",
                    arg,
                    final_str,
                    arg,
                    final_str.len(),
                    constraint_str
                ),
                trace,
            })
        }
        _ => None,
    }
}

// ── Boundary value helpers ──────────────────────────────────────────

/// Find an integer value that violates `x op bound`.
fn find_violating_int(op: CompOp, bound: i64) -> i64 {
    match op {
        CompOp::Gt => bound,     // x > bound violated by x = bound
        CompOp::Ge => bound - 1, // x >= bound violated by x = bound - 1
        CompOp::Lt => bound,     // x < bound violated by x = bound
        CompOp::Le => bound + 1, // x <= bound violated by x = bound + 1
        CompOp::Eq => bound + 1, // x == bound violated by x = bound + 1
        CompOp::Ne => bound,     // x != bound violated by x = bound
    }
}

/// Find an integer value that satisfies `x op bound`.
fn find_satisfying_int(op: CompOp, bound: i64) -> i64 {
    match op {
        CompOp::Gt => bound + 1,
        CompOp::Ge => bound,
        CompOp::Lt => bound - 1,
        CompOp::Le => bound,
        CompOp::Eq => bound,
        CompOp::Ne => bound + 1,
    }
}

/// Find a violating value for a conjunction of two constraints on the same variable.
fn find_violating_int_for_conjunction(op1: CompOp, bound1: i64, op2: CompOp, bound2: i64) -> i64 {
    // Determine the effective range [lo, hi] and pick a value outside it.
    let lo = effective_lower(op1, bound1, op2, bound2);
    let hi = effective_upper(op1, bound1, op2, bound2);

    match (lo, hi) {
        (Some(lo_val), Some(_)) => lo_val - 1, // go below the range
        (Some(lo_val), None) => lo_val - 1,
        (None, Some(hi_val)) => hi_val + 1,
        (None, None) => 0, // fallback
    }
}

/// Compute the effective inclusive lower bound from two constraints.
fn effective_lower(op1: CompOp, b1: i64, op2: CompOp, b2: i64) -> Option<i64> {
    let l1 = match op1 {
        CompOp::Gt => Some(b1 + 1),
        CompOp::Ge => Some(b1),
        _ => None,
    };
    let l2 = match op2 {
        CompOp::Gt => Some(b2 + 1),
        CompOp::Ge => Some(b2),
        _ => None,
    };
    match (l1, l2) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// Compute the effective inclusive upper bound from two constraints.
fn effective_upper(op1: CompOp, b1: i64, op2: CompOp, b2: i64) -> Option<i64> {
    let u1 = match op1 {
        CompOp::Lt => Some(b1 - 1),
        CompOp::Le => Some(b1),
        _ => None,
    };
    let u2 = match op2 {
        CompOp::Lt => Some(b2 - 1),
        CompOp::Le => Some(b2),
        _ => None,
    };
    match (u1, u2) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Check whether `val op bound` is true.
fn eval_comparison_bool(val: i64, op: CompOp, bound: i64) -> bool {
    match op {
        CompOp::Gt => val > bound,
        CompOp::Lt => val < bound,
        CompOp::Ge => val >= bound,
        CompOp::Le => val <= bound,
        CompOp::Eq => val == bound,
        CompOp::Ne => val != bound,
    }
}

/// Flip a comparison operator (swap sides).
fn flip_op(op: CompOp) -> CompOp {
    match op {
        CompOp::Gt => CompOp::Lt,
        CompOp::Lt => CompOp::Gt,
        CompOp::Ge => CompOp::Le,
        CompOp::Le => CompOp::Ge,
        CompOp::Eq => CompOp::Eq,
        CompOp::Ne => CompOp::Ne,
    }
}

/// Find the role for a variable from the variables list, defaulting to `Input`.
fn find_role(var_name: &str, variables: &[(String, String)]) -> ConstraintRole {
    for (name, ty) in variables {
        if name == var_name {
            return match ty.as_str() {
                "output" => ConstraintRole::Output,
                "intermediate" => ConstraintRole::Intermediate,
                "bound" => ConstraintRole::Bound,
                _ => ConstraintRole::Input,
            };
        }
    }
    ConstraintRole::Input
}

/// Convert a parsed constraint back to a display string.
fn lhs_to_string(parsed: &ParsedConstraint) -> String {
    match parsed {
        ParsedConstraint::Comparison { left, op, right } => {
            format!("{} {} {}", left, op, right)
        }
        ParsedConstraint::And(l, r) => {
            format!("{} and {}", lhs_to_string(l), lhs_to_string(r))
        }
        ParsedConstraint::Or(l, r) => {
            format!("{} or {}", lhs_to_string(l), lhs_to_string(r))
        }
        ParsedConstraint::Not(inner) => {
            format!("not({})", lhs_to_string(inner))
        }
        ParsedConstraint::FuncCall {
            name,
            arg,
            op,
            value,
        } => {
            format!("{}({}) {} {}", name, arg, op, value)
        }
    }
}

// ── Formatting ──────────────────────────────────────────────────────

/// Format a counter-example as a human-readable multi-line string.
///
/// # Example output
///
/// ```text
/// Counter-example for violated constraint: x > 0
///   x = -1 (input)
/// Trace:
///   x > 0 => false
/// ```
pub fn format_counterexample(ce: &CounterExample) -> String {
    let mut out = format!(
        "Counter-example for violated constraint: {}\n",
        ce.violated_constraint
    );

    for binding in &ce.variables {
        out.push_str(&format!(
            "  {} = {} ({})\n",
            binding.name, binding.value, binding.constraint_role
        ));
    }

    if !ce.trace.is_empty() {
        out.push_str("Trace:\n");
        for step in &ce.trace {
            out.push_str(&format!("  {} => {}", step.expression, step.result));
            if let Some(ref note) = step.note {
                out.push_str(&format!(" -- {}", note));
            }
            out.push('\n');
        }
    }

    out
}

/// Format a counter-example as a single-line summary.
///
/// # Example output
///
/// ```text
/// x = -1 violates x > 0
/// ```
pub fn format_counterexample_short(ce: &CounterExample) -> String {
    let bindings: Vec<String> = ce
        .variables
        .iter()
        .map(|b| format!("{} = {}", b.name, b.value))
        .collect();
    format!(
        "{} violates {}",
        bindings.join(", "),
        ce.violated_constraint
    )
}

// ── Unit tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_value_display_int() {
        assert_eq!(format!("{}", ConcreteValue::Int(42)), "42");
    }

    #[test]
    fn concrete_value_display_str() {
        assert_eq!(
            format!("{}", ConcreteValue::Str("hello".to_string())),
            "\"hello\""
        );
    }

    #[test]
    fn concrete_value_display_list() {
        let list = ConcreteValue::List(vec![ConcreteValue::Int(1), ConcreteValue::Int(2)]);
        assert_eq!(format!("{}", list), "[1, 2]");
    }

    #[test]
    fn concrete_value_display_record() {
        let rec = ConcreteValue::Record(
            "Point".to_string(),
            vec![
                ("x".to_string(), ConcreteValue::Int(1)),
                ("y".to_string(), ConcreteValue::Int(2)),
            ],
        );
        assert_eq!(format!("{}", rec), "Point(x: 1, y: 2)");
    }

    #[test]
    fn constraint_role_display() {
        assert_eq!(format!("{}", ConstraintRole::Input), "input");
        assert_eq!(format!("{}", ConstraintRole::Output), "output");
    }

    #[test]
    fn parse_simple_gt() {
        let parsed = parse_simple_constraint("x > 0").unwrap();
        assert_eq!(
            parsed,
            ParsedConstraint::Comparison {
                left: "x".to_string(),
                op: CompOp::Gt,
                right: "0".to_string(),
            }
        );
    }

    #[test]
    fn parse_and_constraint() {
        let parsed = parse_simple_constraint("x >= 1 and x <= 10").unwrap();
        match parsed {
            ParsedConstraint::And(l, r) => {
                assert!(matches!(*l, ParsedConstraint::Comparison { .. }));
                assert!(matches!(*r, ParsedConstraint::Comparison { .. }));
            }
            other => panic!("expected And, got {:?}", other),
        }
    }

    #[test]
    fn parse_func_call() {
        let parsed = parse_simple_constraint("len(s) > 0").unwrap();
        assert_eq!(
            parsed,
            ParsedConstraint::FuncCall {
                name: "len".to_string(),
                arg: "s".to_string(),
                op: CompOp::Gt,
                value: "0".to_string(),
            }
        );
    }

    #[test]
    fn parse_complex_returns_none() {
        // "a + b == c" contains arithmetic — too complex for simplified parser
        assert!(parse_simple_constraint("a + b == c").is_none());
    }
}
