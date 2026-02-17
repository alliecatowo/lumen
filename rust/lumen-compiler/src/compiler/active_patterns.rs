//! Active Patterns (F#-style) — T116
//!
//! Active patterns allow matching on the result of a function call, providing
//! a way to define custom decomposition logic that can be used in `match` arms.
//!
//! ## Example
//!
//! ```text
//! active pattern ValidEmail(input: String) -> (String, String)?
//!   let parts = split(input, "@")
//!   if len(parts) == 2
//!     (parts[0], parts[1])
//!   else
//!     null
//!   end
//! end
//!
//! match email
//!   ValidEmail(user, domain) -> "Valid: {user}@{domain}"
//!   _ -> "Invalid email"
//! end
//! ```
//!
//! ## Integration
//!
//! This pass is **opt-in** — it is not wired into the main `compile()` pipeline yet.
//! The coordinator will integrate active pattern parsing, typechecking, and lowering
//! by modifying `parser.rs`, `typecheck.rs`, and `lower.rs` respectively.

use super::ast::Stmt;
use super::tokens::Span;

use std::collections::HashMap;

// ── Definitions ────────────────────────────────────────────────────

/// The return-type shape of an active pattern, determining how the pattern
/// decomposes its input value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivePatternReturn {
    /// An optional tuple of bindings. The pattern succeeds if the function
    /// returns a non-null value, binding the tuple elements to the given names.
    ///
    /// Example: `active pattern Foo(x: String) -> (Int, String)?`
    Option(Vec<String>),

    /// A choice between named alternatives. Exactly one alternative is selected
    /// at runtime based on the function's return value.
    ///
    /// Example: `active pattern Classify(x: Int) -> |Positive|Zero|Negative|`
    Choice(Vec<String>),

    /// A partial pattern that may fail. Similar to `Option` but explicitly
    /// signals that the pattern is partial and the match must include a
    /// fallback arm.
    ///
    /// Example: `active pattern ParseInt(s: String) -> Int partial`
    Partial(Vec<String>),
}

/// Definition of an active pattern: a named decomposition function that can
/// be referenced in `match` arms.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ActivePatternDef {
    /// The name of the active pattern (must be PascalCase).
    pub name: String,
    /// The name of the input parameter.
    pub param_name: String,
    /// The type of the input parameter (as a string for now; will be replaced
    /// with `TypeExpr` once parser integration is complete).
    pub param_type: String,
    /// The return-type shape describing how the pattern decomposes its input.
    pub return_type: ActivePatternReturn,
    /// The body of the active pattern (a sequence of statements).
    pub body: Vec<Stmt>,
    /// Source span for diagnostics.
    pub span: Span,
}

/// An active pattern reference used inside a `match` arm, representing a call
/// to a previously defined active pattern with binding names for the
/// decomposed values.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePatternMatch {
    /// The name of the active pattern being invoked.
    pub pattern_name: String,
    /// The binding names for values produced by the active pattern.
    pub bindings: Vec<String>,
    /// Source span for diagnostics.
    pub span: Span,
}

/// Information needed by the lowerer to emit the function call and conditional
/// jump for an active pattern match arm.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePatternCallInfo {
    /// The generated function name to call at runtime (derived from the
    /// active pattern name).
    pub fn_name: String,
    /// The number of bindings the caller should expect.
    pub binding_count: usize,
    /// Whether the pattern is partial (may fail), requiring a conditional jump
    /// past the arm body.
    pub is_partial: bool,
}

// ── Validation ─────────────────────────────────────────────────────

/// Validates an active pattern definition, returning a list of errors if
/// the definition is invalid.
///
/// Checks performed:
/// - Name must be PascalCase (starts with an uppercase letter, contains no
///   underscores).
/// - Return type bindings must be non-empty.
/// - No recursive active pattern references in the body.
#[allow(dead_code)]
pub fn validate_active_pattern(def: &ActivePatternDef) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Check PascalCase: first char uppercase, no leading underscores
    if !is_pascal_case(&def.name) {
        errors.push(format!(
            "Active pattern name '{}' must be PascalCase (start with uppercase letter, no underscores)",
            def.name
        ));
    }

    // Check return type has at least one binding/choice
    match &def.return_type {
        ActivePatternReturn::Option(bindings) => {
            if bindings.is_empty() {
                errors.push("Option active pattern must have at least one binding".to_string());
            }
        }
        ActivePatternReturn::Choice(choices) => {
            if choices.is_empty() {
                errors.push("Choice active pattern must have at least one alternative".to_string());
            }
        }
        ActivePatternReturn::Partial(bindings) => {
            if bindings.is_empty() {
                errors.push("Partial active pattern must have at least one binding".to_string());
            }
        }
    }

    // Check for recursive references in the body
    if body_references_pattern(&def.body, &def.name) {
        errors.push(format!(
            "Active pattern '{}' must not recursively reference itself",
            def.name
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Checks whether a name is PascalCase: starts with an uppercase ASCII letter
/// and contains no underscores.
fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let first = name.chars().next().unwrap();
    first.is_ascii_uppercase() && !name.contains('_')
}

/// Scans a statement body for calls to the given pattern name, indicating
/// a recursive reference.
fn body_references_pattern(stmts: &[Stmt], pattern_name: &str) -> bool {
    use super::ast::ExprStmt;
    for stmt in stmts {
        match stmt {
            Stmt::Expr(ExprStmt { expr, .. }) => {
                if expr_references_pattern(expr, pattern_name) {
                    return true;
                }
            }
            Stmt::Let(let_stmt) => {
                if expr_references_pattern(&let_stmt.value, pattern_name) {
                    return true;
                }
            }
            Stmt::Return(ret) => {
                if expr_references_pattern(&ret.value, pattern_name) {
                    return true;
                }
            }
            Stmt::If(if_stmt) => {
                if expr_references_pattern(&if_stmt.condition, pattern_name) {
                    return true;
                }
                if body_references_pattern(&if_stmt.then_body, pattern_name) {
                    return true;
                }
                if let Some(else_body) = &if_stmt.else_body {
                    if body_references_pattern(else_body, pattern_name) {
                        return true;
                    }
                }
            }
            Stmt::For(for_stmt) => {
                if expr_references_pattern(&for_stmt.iter, pattern_name) {
                    return true;
                }
                if body_references_pattern(&for_stmt.body, pattern_name) {
                    return true;
                }
            }
            Stmt::While(while_stmt) => {
                if expr_references_pattern(&while_stmt.condition, pattern_name) {
                    return true;
                }
                if body_references_pattern(&while_stmt.body, pattern_name) {
                    return true;
                }
            }
            Stmt::Match(match_stmt) => {
                if expr_references_pattern(&match_stmt.subject, pattern_name) {
                    return true;
                }
                for arm in &match_stmt.arms {
                    if body_references_pattern(&arm.body, pattern_name) {
                        return true;
                    }
                }
            }
            Stmt::Assign(assign) => {
                if expr_references_pattern(&assign.value, pattern_name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Checks whether an expression contains a call (or identifier reference) to
/// the given pattern name.
fn expr_references_pattern(expr: &super::ast::Expr, pattern_name: &str) -> bool {
    use super::ast::Expr;
    match expr {
        Expr::Ident(name, _) => name == pattern_name,
        Expr::Call(callee, args, _) => {
            if expr_references_pattern(callee, pattern_name) {
                return true;
            }
            for arg in args {
                let arg_expr = match arg {
                    super::ast::CallArg::Positional(e) => e,
                    super::ast::CallArg::Named(_, e, _) => e,
                    super::ast::CallArg::Role(_, e, _) => e,
                };
                if expr_references_pattern(arg_expr, pattern_name) {
                    return true;
                }
            }
            false
        }
        Expr::BinOp(lhs, _, rhs, _) => {
            expr_references_pattern(lhs, pattern_name) || expr_references_pattern(rhs, pattern_name)
        }
        Expr::UnaryOp(_, operand, _) => expr_references_pattern(operand, pattern_name),
        Expr::DotAccess(inner, _, _) => expr_references_pattern(inner, pattern_name),
        Expr::IndexAccess(inner, idx, _) => {
            expr_references_pattern(inner, pattern_name)
                || expr_references_pattern(idx, pattern_name)
        }
        Expr::TupleLit(elems, _) | Expr::ListLit(elems, _) | Expr::SetLit(elems, _) => elems
            .iter()
            .any(|e| expr_references_pattern(e, pattern_name)),
        Expr::TryExpr(inner, _) | Expr::NullAssert(inner, _) => {
            expr_references_pattern(inner, pattern_name)
        }
        Expr::NullCoalesce(lhs, rhs, _) => {
            expr_references_pattern(lhs, pattern_name) || expr_references_pattern(rhs, pattern_name)
        }
        Expr::NullSafeAccess(inner, _, _) => expr_references_pattern(inner, pattern_name),
        Expr::NullSafeIndex(inner, idx, _) => {
            expr_references_pattern(inner, pattern_name)
                || expr_references_pattern(idx, pattern_name)
        }
        _ => false,
    }
}

// ── Resolution ─────────────────────────────────────────────────────

/// Resolves an active pattern match by looking up the pattern name in the
/// registry and returning its return-type shape.
///
/// Returns an error if the pattern name is not found in the registry.
#[allow(dead_code)]
pub fn resolve_active_pattern_match(
    name: &str,
    registry: &HashMap<String, ActivePatternDef>,
) -> Result<ActivePatternReturn, String> {
    registry
        .get(name)
        .map(|def| def.return_type.clone())
        .ok_or_else(|| format!("Unknown active pattern '{}'", name))
}

// ── Lowering helpers ───────────────────────────────────────────────

/// Produces the call information that the lowerer needs to emit a call to
/// the active pattern's backing function plus a conditional jump.
#[allow(dead_code)]
pub fn lower_active_pattern_call(def: &ActivePatternDef) -> ActivePatternCallInfo {
    let binding_count = match &def.return_type {
        ActivePatternReturn::Option(bindings) => bindings.len(),
        ActivePatternReturn::Choice(choices) => choices.len(),
        ActivePatternReturn::Partial(bindings) => bindings.len(),
    };

    let is_partial = matches!(
        &def.return_type,
        ActivePatternReturn::Option(_) | ActivePatternReturn::Partial(_)
    );

    ActivePatternCallInfo {
        fn_name: format!("__active_pattern_{}", def.name),
        binding_count,
        is_partial,
    }
}
