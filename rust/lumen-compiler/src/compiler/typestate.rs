//! Typestate checker — ensures operations are only valid in certain states.
//!
//! Typestate analysis tracks a finite state machine on typed variables and
//! verifies that method calls are valid transitions from the current state.
//!
//! ## Example
//!
//! ```text
//! typestate File { Open, Closed }
//! transition File: Open -> Closed via close()
//! transition File: Open -> Open via read()
//! transition File: Open -> Open via write()
//!
//! cell process_file(path: String) -> String
//!   let f = File::open(path)      # f is in state Open
//!   let data = f.read()           # valid: Open -> Open
//!   f.close()                     # valid: Open -> Closed
//!   # f.read()                    # ERROR: f is in state Closed
//!   data
//! end
//! ```
//!
//! ## Integration
//!
//! This pass is **opt-in** — it is not wired into the main `compile()` pipeline.
//! Call [`TypestateChecker::check_cell`] to run typestate analysis on a cell body
//! given a set of typestate declarations.

use crate::compiler::ast::*;
use crate::compiler::tokens::Span;

use std::collections::HashMap;
use std::fmt;

// ── Declarations ────────────────────────────────────────────────────

/// A typestate declaration: a finite state machine on a type.
///
/// Defines which states a type can be in and which method calls
/// transition between states.
#[derive(Debug, Clone)]
pub struct TypestateDecl {
    /// The type this typestate governs (e.g., "File").
    pub type_name: String,
    /// All valid states (e.g., ["Open", "Closed"]).
    pub states: Vec<String>,
    /// The state assigned when a variable of this type is first created.
    pub initial_state: String,
    /// Valid transitions: method calls that move between states.
    pub transitions: Vec<Transition>,
}

/// A single state transition: calling `via_method` on a value in `from_state`
/// moves it to `to_state`.
#[derive(Debug, Clone)]
pub struct Transition {
    pub from_state: String,
    pub to_state: String,
    pub via_method: String,
}

// ── Errors ──────────────────────────────────────────────────────────

/// Errors produced during typestate checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypestateError {
    /// A method was called that is not a valid transition from the current state.
    InvalidTransition {
        var: String,
        current_state: String,
        attempted_method: String,
        span: Span,
    },
    /// A variable was used in an operation that requires a different state.
    UseInWrongState {
        var: String,
        expected_state: String,
        actual_state: String,
        operation: String,
        span: Span,
    },
    /// A typestate-tracked variable was used before being initialized.
    UninitializedTypestate {
        var: String,
        type_name: String,
        span: Span,
    },
    /// A typestate was referenced that has not been declared.
    UndeclaredTypestate { type_name: String, span: Span },
    /// If/else branches end in different states — cannot merge.
    BranchStateMismatch {
        var: String,
        then_state: String,
        else_state: String,
        span: Span,
    },
}

impl fmt::Display for TypestateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypestateError::InvalidTransition {
                var,
                current_state,
                attempted_method,
                span,
            } => write!(
                f,
                "invalid transition: '{}' is in state '{}', method '{}' is not valid (line {})",
                var, current_state, attempted_method, span.line
            ),
            TypestateError::UseInWrongState {
                var,
                expected_state,
                actual_state,
                operation,
                span,
            } => write!(
                f,
                "typestate error: '{}' is in state '{}', but '{}' requires state '{}' (line {})",
                var, actual_state, operation, expected_state, span.line
            ),
            TypestateError::UninitializedTypestate {
                var,
                type_name,
                span,
            } => write!(
                f,
                "typestate '{}' variable '{}' used before initialization (line {})",
                type_name, var, span.line
            ),
            TypestateError::UndeclaredTypestate { type_name, span } => write!(
                f,
                "undeclared typestate '{}' (line {})",
                type_name, span.line
            ),
            TypestateError::BranchStateMismatch {
                var,
                then_state,
                else_state,
                span,
            } => write!(
                f,
                "typestate mismatch at branch join: '{}' is '{}' in then-branch but '{}' in else-branch (line {})",
                var, then_state, else_state, span.line
            ),
        }
    }
}

impl std::error::Error for TypestateError {}

// ── Checker ─────────────────────────────────────────────────────────

/// Tracks the current typestate of each variable during analysis.
pub struct TypestateChecker {
    /// Declared typestates, keyed by type name.
    declarations: HashMap<String, TypestateDecl>,
    /// Current state per variable: var_name -> current_state.
    var_states: HashMap<String, String>,
    /// Maps variable name to its typestate type name.
    var_types: HashMap<String, String>,
    /// Errors accumulated during checking.
    errors: Vec<TypestateError>,
}

impl TypestateChecker {
    /// Create a new empty checker.
    pub fn new() -> Self {
        Self {
            declarations: HashMap::new(),
            var_states: HashMap::new(),
            var_types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Register a typestate declaration.
    pub fn declare_typestate(&mut self, decl: TypestateDecl) {
        self.declarations.insert(decl.type_name.clone(), decl);
    }

    /// Initialize a variable to the initial state of its typestate.
    ///
    /// Returns `Err` if the type has no typestate declaration.
    pub fn init_var(&mut self, var_name: &str, type_name: &str) -> Result<(), Box<TypestateError>> {
        if let Some(decl) = self.declarations.get(type_name) {
            let initial = decl.initial_state.clone();
            self.var_states.insert(var_name.to_string(), initial);
            self.var_types
                .insert(var_name.to_string(), type_name.to_string());
            Ok(())
        } else {
            Err(Box::new(TypestateError::UndeclaredTypestate {
                type_name: type_name.to_string(),
                span: Span::dummy(),
            }))
        }
    }

    /// Verify that `method` is a valid transition from the current state of `var_name`.
    ///
    /// On success, updates the variable's state and returns the new state name.
    /// On failure, returns a `TypestateError`.
    pub fn check_method_call(
        &mut self,
        var_name: &str,
        method: &str,
        span: Span,
    ) -> Result<String, Box<TypestateError>> {
        // Look up the variable's current state.
        let current_state = match self.var_states.get(var_name) {
            Some(s) => s.clone(),
            None => {
                // Variable exists but isn't typestate-tracked — not an error for this checker.
                // The caller should only call this for typestate-tracked variables.
                if let Some(type_name) = self.var_types.get(var_name) {
                    return Err(Box::new(TypestateError::UninitializedTypestate {
                        var: var_name.to_string(),
                        type_name: type_name.clone(),
                        span,
                    }));
                }
                // Not a typestate-tracked variable at all — no-op.
                return Ok(String::new());
            }
        };

        // Look up the typestate declaration.
        let type_name = match self.var_types.get(var_name) {
            Some(t) => t.clone(),
            None => return Ok(String::new()),
        };

        let decl = match self.declarations.get(&type_name) {
            Some(d) => d,
            None => {
                return Err(Box::new(TypestateError::UndeclaredTypestate {
                    type_name,
                    span,
                }));
            }
        };

        // Find a matching transition.
        for tr in &decl.transitions {
            if tr.from_state == current_state && tr.via_method == method {
                let new_state = tr.to_state.clone();
                self.var_states
                    .insert(var_name.to_string(), new_state.clone());
                return Ok(new_state);
            }
        }

        Err(Box::new(TypestateError::InvalidTransition {
            var: var_name.to_string(),
            current_state,
            attempted_method: method.to_string(),
            span,
        }))
    }

    /// Get the current state of a variable, if it is typestate-tracked.
    pub fn current_state(&self, var_name: &str) -> Option<&str> {
        self.var_states.get(var_name).map(|s| s.as_str())
    }

    /// Check whether a variable is tracked by a typestate.
    pub fn is_tracked(&self, var_name: &str) -> bool {
        self.var_types.contains_key(var_name)
    }

    /// Merge states at an if/else join point.
    ///
    /// Both branches must end with the same state for each typestate-tracked
    /// variable. If they diverge, a `BranchStateMismatch` error is produced.
    pub fn merge_states(
        &self,
        var_name: &str,
        then_state: &str,
        else_state: &str,
        span: Span,
    ) -> Result<String, Box<TypestateError>> {
        if then_state == else_state {
            Ok(then_state.to_string())
        } else {
            Err(Box::new(TypestateError::BranchStateMismatch {
                var: var_name.to_string(),
                then_state: then_state.to_string(),
                else_state: else_state.to_string(),
                span,
            }))
        }
    }

    /// Take a snapshot of all current variable states (for branching).
    fn snapshot(&self) -> (HashMap<String, String>, HashMap<String, String>) {
        (self.var_states.clone(), self.var_types.clone())
    }

    /// Restore variable states from a snapshot.
    fn restore(&mut self, snapshot: &(HashMap<String, String>, HashMap<String, String>)) {
        self.var_states = snapshot.0.clone();
        self.var_types = snapshot.1.clone();
    }

    /// Consume accumulated errors.
    pub fn take_errors(&mut self) -> Vec<TypestateError> {
        std::mem::take(&mut self.errors)
    }

    /// Return a reference to accumulated errors.
    pub fn errors(&self) -> &[TypestateError] {
        &self.errors
    }

    // ── AST walking ─────────────────────────────────────────────

    /// Walk a cell body and check all method calls on typestate-tracked variables.
    ///
    /// `type_env` maps type names to their typestate declarations. This is
    /// the external set of declarations that should be registered before checking.
    pub fn check_cell(
        &mut self,
        cell: &CellDef,
        type_env: &HashMap<String, TypestateDecl>,
    ) -> Vec<TypestateError> {
        // Register all typestate declarations from the environment.
        for (name, decl) in type_env {
            self.declarations.insert(name.clone(), decl.clone());
        }

        // Walk the body.
        for stmt in &cell.body {
            self.check_stmt(stmt);
        }

        std::mem::take(&mut self.errors)
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ls) => {
                // Check the RHS expression first.
                self.check_expr(&ls.value);

                // If the RHS is a constructor call for a typestate type,
                // initialize the variable.
                if let Some(type_name) = self.extract_constructor_type(&ls.value) {
                    if self.declarations.contains_key(&type_name) {
                        if let Err(e) = self.init_var(&ls.name, &type_name) {
                            self.errors.push(*e);
                        }
                    }
                }
            }
            Stmt::Assign(a) => {
                self.check_expr(&a.value);

                // Reassignment may re-initialize typestate.
                if let Some(type_name) = self.extract_constructor_type(&a.value) {
                    if self.declarations.contains_key(&type_name) {
                        if let Err(e) = self.init_var(&a.target, &type_name) {
                            self.errors.push(*e);
                        }
                    }
                }
            }
            Stmt::Expr(es) => {
                self.check_expr(&es.expr);
            }
            Stmt::If(ifs) => {
                self.check_expr(&ifs.condition);

                let snapshot = self.snapshot();

                // Check then-branch.
                for s in &ifs.then_body {
                    self.check_stmt(s);
                }
                let then_snapshot = self.snapshot();

                if let Some(ref else_body) = ifs.else_body {
                    // Restore pre-branch state, check else-branch.
                    self.restore(&snapshot);
                    for s in else_body {
                        self.check_stmt(s);
                    }
                    let else_snapshot = self.snapshot();

                    // Merge: all typestate-tracked variables must agree.
                    for (var_name, then_state) in &then_snapshot.0 {
                        if let Some(else_state) = else_snapshot.0.get(var_name) {
                            if then_state != else_state {
                                self.errors.push(TypestateError::BranchStateMismatch {
                                    var: var_name.clone(),
                                    then_state: then_state.clone(),
                                    else_state: else_state.clone(),
                                    span: ifs.span,
                                });
                            }
                        }
                    }

                    // Use then-branch state (they should match; if not, error was logged).
                    self.restore(&then_snapshot);
                } else {
                    // No else-branch: both paths (then executed vs not executed) must agree.
                    // Compare snapshot (no-op path) with then-branch.
                    for (var_name, then_state) in &then_snapshot.0 {
                        if let Some(orig_state) = snapshot.0.get(var_name) {
                            if then_state != orig_state {
                                self.errors.push(TypestateError::BranchStateMismatch {
                                    var: var_name.clone(),
                                    then_state: then_state.clone(),
                                    else_state: orig_state.clone(),
                                    span: ifs.span,
                                });
                            }
                        }
                    }
                    // Conservatively keep the original state (the if might not execute).
                    self.restore(&snapshot);
                }
            }
            Stmt::Return(r) => {
                self.check_expr(&r.value);
            }
            Stmt::Halt(h) => {
                self.check_expr(&h.message);
            }
            Stmt::For(fs) => {
                self.check_expr(&fs.iter);
                if let Some(ref filter) = fs.filter {
                    self.check_expr(filter);
                }
                for s in &fs.body {
                    self.check_stmt(s);
                }
            }
            Stmt::While(ws) => {
                self.check_expr(&ws.condition);
                for s in &ws.body {
                    self.check_stmt(s);
                }
            }
            Stmt::Loop(ls) => {
                for s in &ls.body {
                    self.check_stmt(s);
                }
            }
            Stmt::Match(ms) => {
                self.check_expr(&ms.subject);

                let snapshot = self.snapshot();
                let mut arm_states: Vec<HashMap<String, String>> = Vec::new();

                for arm in &ms.arms {
                    self.restore(&snapshot);
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                    arm_states.push(self.var_states.clone());
                }

                // All arms must agree on states for typestate-tracked variables.
                if let Some(first) = arm_states.first() {
                    for (var_name, first_state) in first {
                        for (i, arm_state) in arm_states.iter().enumerate().skip(1) {
                            if let Some(other_state) = arm_state.get(var_name) {
                                if first_state != other_state {
                                    self.errors.push(TypestateError::BranchStateMismatch {
                                        var: var_name.clone(),
                                        then_state: first_state.clone(),
                                        else_state: other_state.clone(),
                                        span: ms.arms.get(i).map_or(ms.span, |a| a.span),
                                    });
                                }
                            }
                        }
                    }
                    // Use first arm's state (they should all agree).
                    self.var_states = first.clone();
                }
            }
            Stmt::CompoundAssign(ca) => {
                self.check_expr(&ca.value);
            }
            Stmt::Emit(e) => {
                self.check_expr(&e.value);
            }
            Stmt::Defer(d) => {
                for s in &d.body {
                    self.check_stmt(s);
                }
            }
            Stmt::Yield(y) => {
                self.check_expr(&y.value);
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            // Method call on a variable: var.method(args)
            // This is represented as Call(DotAccess(Ident(var), method), args)
            Expr::Call(callee, args, span) => {
                // Check args first.
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => self.check_expr(e),
                        CallArg::Named(_, e, _) => self.check_expr(e),
                        CallArg::Role(_, e, _) => self.check_expr(e),
                    }
                }

                // Check if this is a method call on a typestate-tracked variable.
                if let Expr::DotAccess(base, method, _) = callee.as_ref() {
                    if let Expr::Ident(var_name, _) = base.as_ref() {
                        if self.is_tracked(var_name) {
                            match self.check_method_call(var_name, method, *span) {
                                Ok(_new_state) => {}
                                Err(e) => self.errors.push(*e),
                            }
                            return;
                        }
                    }
                }

                // Otherwise, just recurse into the callee.
                self.check_expr(callee);
            }
            // Dot access without call — reading a field. We don't track field access
            // as transitions, but we still need to recurse.
            Expr::DotAccess(base, _field, _span) => {
                self.check_expr(base);
            }
            Expr::BinOp(lhs, _, rhs, _) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            Expr::UnaryOp(_, operand, _) => {
                self.check_expr(operand);
            }
            Expr::ToolCall(callee, args, _) => {
                self.check_expr(callee);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => self.check_expr(e),
                        CallArg::Named(_, e, _) => self.check_expr(e),
                        CallArg::Role(_, e, _) => self.check_expr(e),
                    }
                }
            }
            Expr::ListLit(elems, _) | Expr::TupleLit(elems, _) | Expr::SetLit(elems, _) => {
                for e in elems {
                    self.check_expr(e);
                }
            }
            Expr::MapLit(entries, _) => {
                for (k, v) in entries {
                    self.check_expr(k);
                    self.check_expr(v);
                }
            }
            Expr::RecordLit(_, fields, _) => {
                for (_, v) in fields {
                    self.check_expr(v);
                }
            }
            Expr::IndexAccess(base, index, _) => {
                self.check_expr(base);
                self.check_expr(index);
            }
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                self.check_expr(cond);
                self.check_expr(then_val);
                self.check_expr(else_val);
            }
            Expr::Lambda { body, .. } => match body {
                LambdaBody::Expr(e) => self.check_expr(e),
                LambdaBody::Block(stmts) => {
                    for s in stmts {
                        self.check_stmt(s);
                    }
                }
            },
            Expr::BlockExpr(stmts, _) => {
                for s in stmts {
                    self.check_stmt(s);
                }
            }
            Expr::Pipe { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            Expr::StringInterp(segments, _) => {
                for seg in segments {
                    if let StringSegment::Interpolation(e) = seg {
                        self.check_expr(e);
                    }
                }
            }
            Expr::AwaitExpr(inner, _)
            | Expr::TryExpr(inner, _)
            | Expr::NullAssert(inner, _)
            | Expr::SpreadExpr(inner, _)
            | Expr::ComptimeExpr(inner, _)
            | Expr::ResumeExpr(inner, _) => {
                self.check_expr(inner);
            }
            Expr::NullCoalesce(lhs, rhs, _) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            Expr::NullSafeAccess(base, _, _) => {
                self.check_expr(base);
            }
            Expr::NullSafeIndex(base, index, _) => {
                self.check_expr(base);
                self.check_expr(index);
            }
            Expr::RoleBlock(_, body, _) | Expr::ExpectSchema(body, _, _) => {
                self.check_expr(body);
            }
            Expr::Perform { args, .. } => {
                for arg in args {
                    self.check_expr(arg);
                }
            }
            Expr::HandleExpr { body, handlers, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
                for handler in handlers {
                    for s in &handler.body {
                        self.check_stmt(s);
                    }
                }
            }
            Expr::MatchExpr { subject, arms, .. } => {
                self.check_expr(subject);
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }
            Expr::WhenExpr {
                arms, else_body, ..
            } => {
                for arm in arms {
                    self.check_expr(&arm.condition);
                    self.check_expr(&arm.body);
                }
                if let Some(e) = else_body {
                    self.check_expr(e);
                }
            }
            Expr::Comprehension {
                body,
                iter,
                condition,
                ..
            } => {
                self.check_expr(iter);
                if let Some(cond) = condition {
                    self.check_expr(cond);
                }
                self.check_expr(body);
            }
            Expr::RangeExpr {
                start, end, step, ..
            } => {
                if let Some(s) = start {
                    self.check_expr(s);
                }
                if let Some(e) = end {
                    self.check_expr(e);
                }
                if let Some(st) = step {
                    self.check_expr(st);
                }
            }
            // Literals and identifiers — no typestate implications.
            Expr::IntLit(_, _)
            | Expr::BigIntLit(_, _)
            | Expr::FloatLit(_, _)
            | Expr::StringLit(_, _)
            | Expr::BoolLit(_, _)
            | Expr::NullLit(_)
            | Expr::RawStringLit(_, _)
            | Expr::BytesLit(_, _)
            | Expr::Ident(_, _)
            | Expr::IsType { .. }
            | Expr::TypeCast { .. } => {}
        }
    }

    /// Attempt to extract a constructor type name from an expression.
    ///
    /// Recognizes patterns like:
    /// - `TypeName::new(...)` → "TypeName"
    /// - `TypeName::open(...)` → "TypeName"
    /// - `TypeName(...)` (record literal) → "TypeName"
    fn extract_constructor_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            // Call to TypeName::method(...) — the callee is DotAccess(Ident("TypeName"), "method")
            // Actually in Lumen AST this might be Ident("TypeName") with DotAccess for static methods.
            Expr::Call(callee, _, _) => {
                match callee.as_ref() {
                    // TypeName.new() / TypeName.open() — static method call
                    Expr::DotAccess(base, _method, _) => {
                        if let Expr::Ident(name, _) = base.as_ref() {
                            // Check if this is a typestate type.
                            if self.declarations.contains_key(name) {
                                return Some(name.clone());
                            }
                        }
                        None
                    }
                    // Direct call: TypeName(...) — record constructor
                    Expr::Ident(name, _) => {
                        if self.declarations.contains_key(name) {
                            return Some(name.clone());
                        }
                        None
                    }
                    _ => None,
                }
            }
            // Record literal: TypeName(field: val, ...)
            Expr::RecordLit(name, _, _) => {
                if self.declarations.contains_key(name) {
                    return Some(name.clone());
                }
                None
            }
            _ => None,
        }
    }
}

impl Default for TypestateChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a File typestate with Open and Closed states.
    fn file_typestate() -> TypestateDecl {
        TypestateDecl {
            type_name: "File".to_string(),
            states: vec!["Open".to_string(), "Closed".to_string()],
            initial_state: "Open".to_string(),
            transitions: vec![
                Transition {
                    from_state: "Open".to_string(),
                    to_state: "Open".to_string(),
                    via_method: "read".to_string(),
                },
                Transition {
                    from_state: "Open".to_string(),
                    to_state: "Open".to_string(),
                    via_method: "write".to_string(),
                },
                Transition {
                    from_state: "Open".to_string(),
                    to_state: "Closed".to_string(),
                    via_method: "close".to_string(),
                },
            ],
        }
    }

    /// Helper: a Connection typestate with Connected, Authenticated, Disconnected.
    fn connection_typestate() -> TypestateDecl {
        TypestateDecl {
            type_name: "Connection".to_string(),
            states: vec![
                "Connected".to_string(),
                "Authenticated".to_string(),
                "Disconnected".to_string(),
            ],
            initial_state: "Connected".to_string(),
            transitions: vec![
                Transition {
                    from_state: "Connected".to_string(),
                    to_state: "Authenticated".to_string(),
                    via_method: "authenticate".to_string(),
                },
                Transition {
                    from_state: "Authenticated".to_string(),
                    to_state: "Authenticated".to_string(),
                    via_method: "query".to_string(),
                },
                Transition {
                    from_state: "Authenticated".to_string(),
                    to_state: "Disconnected".to_string(),
                    via_method: "disconnect".to_string(),
                },
                Transition {
                    from_state: "Connected".to_string(),
                    to_state: "Disconnected".to_string(),
                    via_method: "disconnect".to_string(),
                },
            ],
        }
    }

    fn span(line: usize) -> Span {
        Span::new(0, 0, line, 1)
    }

    // ── Unit tests for the checker API ──────────────────────────

    #[test]
    fn test_init_var_sets_initial_state() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();
        assert_eq!(checker.current_state("f"), Some("Open"));
    }

    #[test]
    fn test_init_var_undeclared_type() {
        let mut checker = TypestateChecker::new();
        let result = checker.init_var("f", "NonExistent");
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::UndeclaredTypestate { type_name, .. } => {
                assert_eq!(type_name, "NonExistent");
            }
            other => panic!("expected UndeclaredTypestate, got {:?}", other),
        }
    }

    #[test]
    fn test_valid_transition_open_to_open_via_read() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        let result = checker.check_method_call("f", "read", span(1));
        assert_eq!(result, Ok("Open".to_string()));
        assert_eq!(checker.current_state("f"), Some("Open"));
    }

    #[test]
    fn test_valid_transition_open_to_closed_via_close() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        let result = checker.check_method_call("f", "close", span(1));
        assert_eq!(result, Ok("Closed".to_string()));
        assert_eq!(checker.current_state("f"), Some("Closed"));
    }

    #[test]
    fn test_invalid_transition_closed_read() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();
        checker.check_method_call("f", "close", span(1)).unwrap();

        let result = checker.check_method_call("f", "read", span(2));
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::InvalidTransition {
                var,
                current_state,
                attempted_method,
                ..
            } => {
                assert_eq!(var, "f");
                assert_eq!(current_state, "Closed");
                assert_eq!(attempted_method, "read");
            }
            other => panic!("expected InvalidTransition, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_transition_closed_write() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();
        checker.check_method_call("f", "close", span(1)).unwrap();

        let result = checker.check_method_call("f", "write", span(2));
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::InvalidTransition {
                current_state,
                attempted_method,
                ..
            } => {
                assert_eq!(current_state, "Closed");
                assert_eq!(attempted_method, "write");
            }
            other => panic!("expected InvalidTransition, got {:?}", other),
        }
    }

    #[test]
    fn test_transition_chain_read_read_close() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        assert_eq!(
            checker.check_method_call("f", "read", span(1)),
            Ok("Open".to_string())
        );
        assert_eq!(
            checker.check_method_call("f", "read", span(2)),
            Ok("Open".to_string())
        );
        assert_eq!(
            checker.check_method_call("f", "close", span(3)),
            Ok("Closed".to_string())
        );
        assert_eq!(checker.current_state("f"), Some("Closed"));
    }

    #[test]
    fn test_write_then_read_then_close() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        assert_eq!(
            checker.check_method_call("f", "write", span(1)),
            Ok("Open".to_string())
        );
        assert_eq!(
            checker.check_method_call("f", "read", span(2)),
            Ok("Open".to_string())
        );
        assert_eq!(
            checker.check_method_call("f", "close", span(3)),
            Ok("Closed".to_string())
        );
    }

    #[test]
    fn test_double_close_fails() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();
        checker.check_method_call("f", "close", span(1)).unwrap();

        let result = checker.check_method_call("f", "close", span(2));
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_states_same() {
        let checker = TypestateChecker::new();
        let result = checker.merge_states("f", "Open", "Open", span(1));
        assert_eq!(result, Ok("Open".to_string()));
    }

    #[test]
    fn test_merge_states_different() {
        let checker = TypestateChecker::new();
        let result = checker.merge_states("f", "Open", "Closed", span(1));
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::BranchStateMismatch {
                var,
                then_state,
                else_state,
                ..
            } => {
                assert_eq!(var, "f");
                assert_eq!(then_state, "Open");
                assert_eq!(else_state, "Closed");
            }
            other => panic!("expected BranchStateMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_vars_tracked_independently() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f1", "File").unwrap();
        checker.init_var("f2", "File").unwrap();

        // Close f1 but keep f2 open.
        checker.check_method_call("f1", "close", span(1)).unwrap();
        assert_eq!(checker.current_state("f1"), Some("Closed"));
        assert_eq!(checker.current_state("f2"), Some("Open"));

        // f2 can still be read.
        assert_eq!(
            checker.check_method_call("f2", "read", span(2)),
            Ok("Open".to_string())
        );

        // f1 cannot be read.
        assert!(checker.check_method_call("f1", "read", span(3)).is_err());
    }

    #[test]
    fn test_connection_full_protocol() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(connection_typestate());
        checker.init_var("conn", "Connection").unwrap();

        assert_eq!(checker.current_state("conn"), Some("Connected"));

        assert_eq!(
            checker.check_method_call("conn", "authenticate", span(1)),
            Ok("Authenticated".to_string())
        );
        assert_eq!(
            checker.check_method_call("conn", "query", span(2)),
            Ok("Authenticated".to_string())
        );
        assert_eq!(
            checker.check_method_call("conn", "query", span(3)),
            Ok("Authenticated".to_string())
        );
        assert_eq!(
            checker.check_method_call("conn", "disconnect", span(4)),
            Ok("Disconnected".to_string())
        );
    }

    #[test]
    fn test_connection_query_before_auth_fails() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(connection_typestate());
        checker.init_var("conn", "Connection").unwrap();

        // query requires Authenticated, but we're in Connected.
        let result = checker.check_method_call("conn", "query", span(1));
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::InvalidTransition {
                current_state,
                attempted_method,
                ..
            } => {
                assert_eq!(current_state, "Connected");
                assert_eq!(attempted_method, "query");
            }
            other => panic!("expected InvalidTransition, got {:?}", other),
        }
    }

    #[test]
    fn test_connection_disconnect_from_connected() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(connection_typestate());
        checker.init_var("conn", "Connection").unwrap();

        // Disconnect directly from Connected (allowed).
        assert_eq!(
            checker.check_method_call("conn", "disconnect", span(1)),
            Ok("Disconnected".to_string())
        );
    }

    #[test]
    fn test_untracked_variable_is_noop() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());

        // "x" is not tracked — check_method_call returns Ok("").
        let result = checker.check_method_call("x", "anything", span(1));
        assert_eq!(result, Ok(String::new()));
    }

    #[test]
    fn test_is_tracked() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        assert!(!checker.is_tracked("f"));

        checker.init_var("f", "File").unwrap();
        assert!(checker.is_tracked("f"));
        assert!(!checker.is_tracked("other"));
    }

    // ── AST integration tests ───────────────────────────────────

    /// Helper to build a simple cell with statements.
    fn make_cell(name: &str, body: Vec<Stmt>) -> CellDef {
        CellDef {
            name: name.to_string(),
            generic_params: vec![],
            params: vec![],
            return_type: None,
            effects: vec![],
            body,
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![],
            span: span(1),
            doc: None,
        }
    }

    /// Helper: let var = TypeName.open()
    fn make_let_constructor(var: &str, type_name: &str, method: &str, line: usize) -> Stmt {
        Stmt::Let(LetStmt {
            name: var.to_string(),
            mutable: false,
            pattern: None,
            ty: None,
            value: Expr::Call(
                Box::new(Expr::DotAccess(
                    Box::new(Expr::Ident(type_name.to_string(), span(line))),
                    method.to_string(),
                    span(line),
                )),
                vec![],
                span(line),
            ),
            span: span(line),
        })
    }

    /// Helper: var.method()
    fn make_method_call(var: &str, method: &str, line: usize) -> Stmt {
        Stmt::Expr(ExprStmt {
            expr: Expr::Call(
                Box::new(Expr::DotAccess(
                    Box::new(Expr::Ident(var.to_string(), span(line))),
                    method.to_string(),
                    span(line),
                )),
                vec![],
                span(line),
            ),
            span: span(line),
        })
    }

    #[test]
    fn test_cell_valid_file_usage() {
        let cell = make_cell(
            "valid_file",
            vec![
                make_let_constructor("f", "File", "open", 1),
                make_method_call("f", "read", 2),
                make_method_call("f", "close", 3),
            ],
        );

        let mut type_env = HashMap::new();
        type_env.insert("File".to_string(), file_typestate());

        let mut checker = TypestateChecker::new();
        let errors = checker.check_cell(&cell, &type_env);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_cell_read_after_close_error() {
        let cell = make_cell(
            "bad_file",
            vec![
                make_let_constructor("f", "File", "open", 1),
                make_method_call("f", "close", 2),
                make_method_call("f", "read", 3), // ERROR
            ],
        );

        let mut type_env = HashMap::new();
        type_env.insert("File".to_string(), file_typestate());

        let mut checker = TypestateChecker::new();
        let errors = checker.check_cell(&cell, &type_env);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypestateError::InvalidTransition {
                var,
                current_state,
                attempted_method,
                ..
            } => {
                assert_eq!(var, "f");
                assert_eq!(current_state, "Closed");
                assert_eq!(attempted_method, "read");
            }
            other => panic!("expected InvalidTransition, got {:?}", other),
        }
    }

    #[test]
    fn test_cell_if_else_same_state_ok() {
        // if cond then f.read() else f.write() end — both leave f in Open.
        let cell = make_cell(
            "branch_ok",
            vec![
                make_let_constructor("f", "File", "open", 1),
                Stmt::If(IfStmt {
                    condition: Expr::BoolLit(true, span(2)),
                    then_body: vec![make_method_call("f", "read", 3)],
                    else_body: Some(vec![make_method_call("f", "write", 4)]),
                    span: span(2),
                }),
                // After if/else, f is still Open.
                make_method_call("f", "close", 5),
            ],
        );

        let mut type_env = HashMap::new();
        type_env.insert("File".to_string(), file_typestate());

        let mut checker = TypestateChecker::new();
        let errors = checker.check_cell(&cell, &type_env);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_cell_if_else_state_mismatch_error() {
        // if cond then f.close() else f.read() end — then=Closed, else=Open.
        let cell = make_cell(
            "branch_mismatch",
            vec![
                make_let_constructor("f", "File", "open", 1),
                Stmt::If(IfStmt {
                    condition: Expr::BoolLit(true, span(2)),
                    then_body: vec![make_method_call("f", "close", 3)],
                    else_body: Some(vec![make_method_call("f", "read", 4)]),
                    span: span(2),
                }),
            ],
        );

        let mut type_env = HashMap::new();
        type_env.insert("File".to_string(), file_typestate());

        let mut checker = TypestateChecker::new();
        let errors = checker.check_cell(&cell, &type_env);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypestateError::BranchStateMismatch {
                var,
                then_state,
                else_state,
                ..
            } => {
                assert_eq!(var, "f");
                assert_eq!(then_state, "Closed");
                assert_eq!(else_state, "Open");
            }
            other => panic!("expected BranchStateMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_cell_if_no_else_state_change_error() {
        // if cond then f.close() end — then=Closed, fallthrough=Open → mismatch
        let cell = make_cell(
            "branch_no_else",
            vec![
                make_let_constructor("f", "File", "open", 1),
                Stmt::If(IfStmt {
                    condition: Expr::BoolLit(true, span(2)),
                    then_body: vec![make_method_call("f", "close", 3)],
                    else_body: None,
                    span: span(2),
                }),
            ],
        );

        let mut type_env = HashMap::new();
        type_env.insert("File".to_string(), file_typestate());

        let mut checker = TypestateChecker::new();
        let errors = checker.check_cell(&cell, &type_env);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypestateError::BranchStateMismatch { .. } => {}
            other => panic!("expected BranchStateMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_error_display() {
        let err = TypestateError::InvalidTransition {
            var: "f".to_string(),
            current_state: "Closed".to_string(),
            attempted_method: "read".to_string(),
            span: span(5),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Closed"));
        assert!(msg.contains("read"));
        assert!(msg.contains("line 5"));
    }

    #[test]
    fn test_display_uninitialized() {
        let err = TypestateError::UninitializedTypestate {
            var: "x".to_string(),
            type_name: "File".to_string(),
            span: span(3),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("File"));
        assert!(msg.contains("x"));
    }

    #[test]
    fn test_display_undeclared() {
        let err = TypestateError::UndeclaredTypestate {
            type_name: "Widget".to_string(),
            span: span(7),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Widget"));
    }

    #[test]
    fn test_display_branch_mismatch() {
        let err = TypestateError::BranchStateMismatch {
            var: "f".to_string(),
            then_state: "Closed".to_string(),
            else_state: "Open".to_string(),
            span: span(10),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Closed"));
        assert!(msg.contains("Open"));
    }

    #[test]
    fn test_display_use_in_wrong_state() {
        let err = TypestateError::UseInWrongState {
            var: "conn".to_string(),
            expected_state: "Authenticated".to_string(),
            actual_state: "Connected".to_string(),
            operation: "query".to_string(),
            span: span(4),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Authenticated"));
        assert!(msg.contains("Connected"));
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        let snap = checker.snapshot();
        assert_eq!(checker.current_state("f"), Some("Open"));

        checker.check_method_call("f", "close", span(1)).unwrap();
        assert_eq!(checker.current_state("f"), Some("Closed"));

        checker.restore(&snap);
        assert_eq!(checker.current_state("f"), Some("Open"));
    }

    #[test]
    fn test_unknown_method_on_typestate() {
        let mut checker = TypestateChecker::new();
        checker.declare_typestate(file_typestate());
        checker.init_var("f", "File").unwrap();

        let result = checker.check_method_call("f", "delete", span(1));
        assert!(result.is_err());
        match *result.unwrap_err() {
            TypestateError::InvalidTransition {
                attempted_method, ..
            } => {
                assert_eq!(attempted_method, "delete");
            }
            other => panic!("expected InvalidTransition, got {:?}", other),
        }
    }
}
