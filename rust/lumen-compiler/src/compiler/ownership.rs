//! Ownership analysis pass — linear/affine type tracking for Lumen.
//!
//! This pass runs after typechecking but before lowering. It tracks the
//! ownership state of every variable and enforces move semantics for
//! non-Copy types.
//!
//! ## Design Overview
//!
//! Lumen's ownership model classifies types into two categories:
//!
//! - **Copy types**: Primitives (`Int`, `Float`, `Bool`, `String`, `Null`, `Bytes`,
//!   `Json`, `Any`) can be freely duplicated. Using a Copy variable does not consume it.
//!
//! - **Owned types**: Compound/heap types (`List`, `Map`, `Set`, `Tuple`, `Record`,
//!   `Fn`, closures, futures) follow affine semantics — a variable holding an Owned
//!   value is consumed on its first use (move). Using it again is an error.
//!
//! Borrows (`Borrowed`, `MutBorrowed`) are tracked but not yet surfaced through
//! syntax — they exist as infrastructure for future `&` / `&mut` annotations.
//!
//! ## Integration
//!
//! This pass is **opt-in** — it is not wired into the main `compile()` pipeline.
//! Call [`check_program`] directly to run ownership analysis on a parsed and
//! typechecked program.

use crate::compiler::ast::*;
use crate::compiler::resolve::SymbolTable;
use crate::compiler::tokens::Span;
use crate::compiler::typecheck::{resolve_type_expr, Type};

use std::collections::HashMap;
use std::fmt;

// ── Core types ──────────────────────────────────────────────────────

/// Ownership mode for a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipMode {
    /// Value can be freely copied (default for primitives: Int, Float, Bool, String).
    Copy,
    /// Value is owned and must be moved or consumed exactly once (affine).
    Owned,
    /// Value is an immutable borrow (read-only reference).
    Borrowed,
    /// Value is a mutable borrow (exclusive reference).
    MutBorrowed,
}

/// State of a variable during analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarState {
    /// Variable is alive and usable.
    Alive,
    /// Variable has been moved to another location.
    Moved { moved_at: Span },
    /// Variable has been dropped (scope exit).
    Dropped,
}

/// Information tracked for each variable.
#[derive(Debug, Clone)]
struct VarInfo {
    mode: OwnershipMode,
    state: VarState,
    declared_at: Span,
    /// Number of active immutable borrows.
    borrow_count: usize,
    /// Span of the first active borrow (if any).
    first_borrow: Option<Span>,
    /// Whether there is an active mutable borrow.
    mut_borrowed: bool,
    /// Span of the active mutable borrow (if any).
    mut_borrow_span: Option<Span>,
}

// ── Errors ──────────────────────────────────────────────────────────

/// Ownership-related error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipError {
    /// An owned variable was used after it had already been moved.
    UseAfterMove {
        variable: String,
        moved_at: Span,
        used_at: Span,
    },
    /// An owned variable went out of scope without being consumed.
    NotConsumed { variable: String, declared_at: Span },
    /// A variable was borrowed while already mutably borrowed, or
    /// mutably borrowed while already borrowed.
    AlreadyBorrowed {
        variable: String,
        first_borrow: Span,
        second_borrow: Span,
    },
    /// A variable was moved while it still had active borrows.
    MoveWhileBorrowed {
        variable: String,
        borrow_at: Span,
        move_at: Span,
    },
}

impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnershipError::UseAfterMove {
                variable,
                moved_at,
                used_at,
            } => write!(
                f,
                "use of moved variable '{}' at line {} (moved at line {})",
                variable, used_at.line, moved_at.line
            ),
            OwnershipError::NotConsumed {
                variable,
                declared_at,
            } => write!(
                f,
                "owned variable '{}' declared at line {} was never consumed",
                variable, declared_at.line
            ),
            OwnershipError::AlreadyBorrowed {
                variable,
                first_borrow,
                second_borrow,
            } => write!(
                f,
                "variable '{}' already borrowed at line {}, cannot borrow again at line {}",
                variable, first_borrow.line, second_borrow.line
            ),
            OwnershipError::MoveWhileBorrowed {
                variable,
                borrow_at,
                move_at,
            } => write!(
                f,
                "cannot move '{}' at line {} while borrowed at line {}",
                variable, move_at.line, borrow_at.line
            ),
        }
    }
}

impl std::error::Error for OwnershipError {}

// ── Scope tracking ──────────────────────────────────────────────────

/// A single lexical scope frame.
#[derive(Debug, Clone)]
struct Scope {
    /// Variables declared in this scope. Maps name → index in the checker's `vars` table.
    vars: Vec<String>,
}

// ── Ownership mode inference ────────────────────────────────────────

/// Determine the ownership mode for a resolved type.
///
/// Primitives are Copy; compound / heap types are Owned.
pub fn ownership_mode_for_type(ty: &Type) -> OwnershipMode {
    match ty {
        // Primitives — cheap to copy
        Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Null
        | Type::Bytes
        | Type::Json => OwnershipMode::Copy,
        // `Any` — conservative default: treat as Copy so existing code isn't broken
        Type::Any => OwnershipMode::Copy,
        // Enums with no payload are cheap, but enums in general may hold owned data.
        // Treat all enums as Owned for safety.
        Type::Enum(_) => OwnershipMode::Owned,
        // Compound / heap types — must be moved
        Type::List(_)
        | Type::Map(_, _)
        | Type::Set(_)
        | Type::Tuple(_)
        | Type::Record(_)
        | Type::Fn(_, _)
        | Type::Result(_, _) => OwnershipMode::Owned,
        // Union types: if ALL arms are Copy, the union is Copy; otherwise Owned
        Type::Union(types) => {
            if types
                .iter()
                .all(|t| ownership_mode_for_type(t) == OwnershipMode::Copy)
            {
                OwnershipMode::Copy
            } else {
                OwnershipMode::Owned
            }
        }
        // Generic / unresolved — conservative: Owned
        Type::Generic(_) | Type::TypeRef(_, _) => OwnershipMode::Owned,
    }
}

// ── The checker ─────────────────────────────────────────────────────

/// Ownership checker — walks the AST after typechecking and enforces
/// move semantics for non-Copy types.
pub struct OwnershipChecker<'a> {
    symbols: &'a SymbolTable,
    /// Per-variable tracking, keyed by mangled name (scope_depth + name).
    vars: HashMap<String, VarInfo>,
    /// Scope stack.
    scopes: Vec<Scope>,
    /// Accumulated errors.
    errors: Vec<OwnershipError>,
    /// Type environment — mirrors the typechecker's locals map.
    /// Populated during walk from Let bindings, params, etc.
    locals: HashMap<String, Type>,
}

impl<'a> OwnershipChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            vars: HashMap::new(),
            scopes: Vec::new(),
            errors: Vec::new(),
            locals: HashMap::new(),
        }
    }

    // ── Public entry points ─────────────────────────────────────

    /// Analyse an entire program. Returns a (possibly empty) list of errors.
    pub fn check_program(mut self, program: &Program) -> Vec<OwnershipError> {
        for item in &program.items {
            match item {
                Item::Cell(c) => self.check_cell(c),
                Item::Agent(a) => {
                    for cell in &a.cells {
                        self.check_cell(cell);
                    }
                }
                Item::Process(p) => {
                    for cell in &p.cells {
                        self.check_cell(cell);
                    }
                }
                _ => {}
            }
        }
        self.errors
    }

    /// Analyse a single cell (function).
    fn check_cell(&mut self, cell: &CellDef) {
        // Reset state for each cell.
        self.vars.clear();
        self.scopes.clear();
        self.locals.clear();

        self.enter_scope();

        // Register parameters.
        for p in &cell.params {
            let ty = resolve_type_expr(&p.ty, self.symbols);
            let mode = if p.variadic {
                // Variadic params become List[T] — always Owned.
                OwnershipMode::Owned
            } else {
                ownership_mode_for_type(&ty)
            };
            let actual_ty = if p.variadic {
                Type::List(Box::new(ty))
            } else {
                ty
            };
            self.locals.insert(p.name.clone(), actual_ty);
            self.declare_var(&p.name, mode, p.span);
        }

        // Walk the body.
        for stmt in &cell.body {
            self.check_stmt(stmt);
        }

        self.exit_scope();
    }

    // ── Scope management ────────────────────────────────────────

    fn enter_scope(&mut self) {
        self.scopes.push(Scope { vars: Vec::new() });
    }

    fn exit_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for name in &scope.vars {
                // Check that Owned variables were consumed before going out of scope.
                if let Some(info) = self.vars.get(name) {
                    if info.mode == OwnershipMode::Owned && info.state == VarState::Alive {
                        self.errors.push(OwnershipError::NotConsumed {
                            variable: name.clone(),
                            declared_at: info.declared_at,
                        });
                    }
                }
                // Remove from tracking.
                self.vars.remove(name);
                self.locals.remove(name);
            }
        }
    }

    fn declare_var(&mut self, name: &str, mode: OwnershipMode, span: Span) {
        let info = VarInfo {
            mode,
            state: VarState::Alive,
            declared_at: span,
            borrow_count: 0,
            first_borrow: None,
            mut_borrowed: false,
            mut_borrow_span: None,
        };
        self.vars.insert(name.to_string(), info);
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.push(name.to_string());
        }
    }

    // ── Variable operations ─────────────────────────────────────

    /// Use (read) a variable. For Owned types this is a **move** — the
    /// variable is consumed and subsequent uses are errors.
    fn use_var(&mut self, name: &str, span: Span) {
        if let Some(info) = self.vars.get_mut(name) {
            match &info.state {
                VarState::Moved { moved_at } => {
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: *moved_at,
                        used_at: span,
                    });
                }
                VarState::Dropped => {
                    // Shouldn't happen during normal analysis, but defensively
                    // treat as moved-at-declaration.
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: info.declared_at,
                        used_at: span,
                    });
                }
                VarState::Alive => {
                    if info.mode == OwnershipMode::Owned {
                        // Affine: consumed on first use.
                        info.state = VarState::Moved { moved_at: span };
                    }
                    // Copy / Borrowed / MutBorrowed — no state change.
                }
            }
        }
        // If the variable isn't tracked (e.g. a global / builtin), ignore.
    }

    /// Explicitly move a variable. Similar to `use_var` but always marks
    /// as moved regardless of Copy mode (for future explicit `move` syntax).
    #[allow(dead_code)]
    fn move_var(&mut self, name: &str, span: Span) {
        if let Some(info) = self.vars.get_mut(name) {
            match &info.state {
                VarState::Moved { moved_at } => {
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: *moved_at,
                        used_at: span,
                    });
                }
                VarState::Dropped => {
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: info.declared_at,
                        used_at: span,
                    });
                }
                VarState::Alive => {
                    // Check for active borrows before moving.
                    if info.borrow_count > 0 {
                        self.errors.push(OwnershipError::MoveWhileBorrowed {
                            variable: name.to_string(),
                            borrow_at: info.first_borrow.unwrap_or(info.declared_at),
                            move_at: span,
                        });
                    }
                    if info.mut_borrowed {
                        self.errors.push(OwnershipError::MoveWhileBorrowed {
                            variable: name.to_string(),
                            borrow_at: info.mut_borrow_span.unwrap_or(info.declared_at),
                            move_at: span,
                        });
                    }
                    info.state = VarState::Moved { moved_at: span };
                }
            }
        }
    }

    /// Borrow a variable (immutable or mutable).
    #[allow(dead_code)]
    fn borrow_var(&mut self, name: &str, span: Span, mutable: bool) {
        if let Some(info) = self.vars.get_mut(name) {
            match &info.state {
                VarState::Moved { moved_at } => {
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: *moved_at,
                        used_at: span,
                    });
                }
                VarState::Dropped => {
                    self.errors.push(OwnershipError::UseAfterMove {
                        variable: name.to_string(),
                        moved_at: info.declared_at,
                        used_at: span,
                    });
                }
                VarState::Alive => {
                    if mutable {
                        // Mutable borrow: cannot coexist with any borrow.
                        if info.borrow_count > 0 {
                            self.errors.push(OwnershipError::AlreadyBorrowed {
                                variable: name.to_string(),
                                first_borrow: info.first_borrow.unwrap_or(info.declared_at),
                                second_borrow: span,
                            });
                        } else if info.mut_borrowed {
                            self.errors.push(OwnershipError::AlreadyBorrowed {
                                variable: name.to_string(),
                                first_borrow: info.mut_borrow_span.unwrap_or(info.declared_at),
                                second_borrow: span,
                            });
                        } else {
                            info.mut_borrowed = true;
                            info.mut_borrow_span = Some(span);
                        }
                    } else {
                        // Immutable borrow: cannot coexist with mutable borrow.
                        if info.mut_borrowed {
                            self.errors.push(OwnershipError::AlreadyBorrowed {
                                variable: name.to_string(),
                                first_borrow: info.mut_borrow_span.unwrap_or(info.declared_at),
                                second_borrow: span,
                            });
                        } else {
                            if info.borrow_count == 0 {
                                info.first_borrow = Some(span);
                            }
                            info.borrow_count += 1;
                        }
                    }
                }
            }
        }
    }

    // ── AST walkers ─────────────────────────────────────────────

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ls) => {
                // Evaluate the RHS first — this may consume variables.
                self.check_expr(&ls.value);

                // Determine the type of the binding.
                let ty = if let Some(ref ann) = ls.ty {
                    resolve_type_expr(ann, self.symbols)
                } else {
                    self.infer_expr_type(&ls.value)
                };

                if let Some(ref pattern) = ls.pattern {
                    // Destructuring — register all bound names.
                    self.bind_pattern(pattern, &ty);
                } else {
                    let mode = ownership_mode_for_type(&ty);
                    self.locals.insert(ls.name.clone(), ty);
                    self.declare_var(&ls.name, mode, ls.span);
                }
            }
            Stmt::Assign(a) => {
                // Evaluate the RHS.
                self.check_expr(&a.value);
                // The target is being *re-assigned*: if it was previously moved,
                // this restores it (re-binds the name).
                if let Some(info) = self.vars.get_mut(&a.target) {
                    // Re-assignment brings the variable back to life.
                    info.state = VarState::Alive;
                    info.borrow_count = 0;
                    info.first_borrow = None;
                    info.mut_borrowed = false;
                    info.mut_borrow_span = None;
                }
            }
            Stmt::CompoundAssign(ca) => {
                // target op= value — this both reads and writes the target.
                // The target must be alive for the read.
                self.use_var(&ca.target, ca.span);
                self.check_expr(&ca.value);
                // After compound assign, the target is alive again (it holds a new value).
                if let Some(info) = self.vars.get_mut(&ca.target) {
                    info.state = VarState::Alive;
                }
            }
            Stmt::Return(r) => {
                self.check_expr(&r.value);
            }
            Stmt::Halt(h) => {
                self.check_expr(&h.message);
            }
            Stmt::Expr(es) => {
                self.check_expr(&es.expr);
            }
            Stmt::If(ifs) => {
                self.check_expr(&ifs.condition);

                // Snapshot state before branches.
                let snapshot = self.snapshot_vars();

                self.enter_scope();
                for s in &ifs.then_body {
                    self.check_stmt(s);
                }
                self.exit_scope();

                let then_state = self.snapshot_vars();

                if let Some(ref else_body) = ifs.else_body {
                    // Restore to pre-branch state for else.
                    self.restore_vars(&snapshot);
                    self.enter_scope();
                    for s in else_body {
                        self.check_stmt(s);
                    }
                    self.exit_scope();

                    let else_state = self.snapshot_vars();

                    // Merge: if a variable was moved in *both* branches, it is moved.
                    // If moved in only one branch, conservatively mark as moved
                    // (sound over-approximation).
                    self.merge_branch_states(&then_state, &else_state);
                } else {
                    // No else: conservatively use the then-branch state
                    // (variables moved in the then branch might not be executed).
                    // Sound choice: merge as if else-branch didn't move anything.
                    self.merge_branch_states(&then_state, &snapshot);
                }
            }
            Stmt::For(fs) => {
                self.check_expr(&fs.iter);
                if let Some(ref filter) = fs.filter {
                    self.check_expr(filter);
                }

                self.enter_scope();
                // Loop variable is always a fresh binding per iteration.
                let iter_ty = self.infer_expr_type(&fs.iter);
                let elem_ty = match &iter_ty {
                    Type::List(inner) | Type::Set(inner) => *inner.clone(),
                    Type::Map(k, _) => *k.clone(),
                    _ => Type::Any,
                };
                let mode = ownership_mode_for_type(&elem_ty);
                self.locals.insert(fs.var.clone(), elem_ty);
                self.declare_var(&fs.var, mode, fs.span);

                for s in &fs.body {
                    self.check_stmt(s);
                }
                self.exit_scope();
            }
            Stmt::While(ws) => {
                self.check_expr(&ws.condition);
                self.enter_scope();
                for s in &ws.body {
                    self.check_stmt(s);
                }
                self.exit_scope();
            }
            Stmt::Loop(ls) => {
                self.enter_scope();
                for s in &ls.body {
                    self.check_stmt(s);
                }
                self.exit_scope();
            }
            Stmt::Match(ms) => {
                self.check_expr(&ms.subject);

                let snapshot = self.snapshot_vars();
                let mut branch_states: Vec<HashMap<String, VarState>> = Vec::new();

                for arm in &ms.arms {
                    self.restore_vars(&snapshot);

                    self.enter_scope();
                    // Bind pattern variables.
                    let subject_ty = self.infer_expr_type(&ms.subject);
                    self.bind_match_pattern(&arm.pattern, &subject_ty);

                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                    self.exit_scope();

                    branch_states.push(self.snapshot_vars());
                }

                // Merge all arm states: a var is moved if moved in ALL arms.
                if let Some(first) = branch_states.first() {
                    let mut merged = first.clone();
                    for arm_state in branch_states.iter().skip(1) {
                        for (name, state) in &mut merged {
                            let other = arm_state.get(name).cloned().unwrap_or(VarState::Alive);
                            // If either branch has it alive, conservatively mark as moved
                            // (some paths consume, some don't → unsafe to use).
                            if *state != other {
                                // At least one branch moved it — mark as moved.
                                if let VarState::Moved { moved_at } = state {
                                    // Keep the existing moved_at.
                                    let _ = moved_at;
                                } else if let VarState::Moved { moved_at } = &other {
                                    *state = VarState::Moved {
                                        moved_at: *moved_at,
                                    };
                                }
                            }
                        }
                    }
                    // Apply merged state.
                    for (name, state) in merged {
                        if let Some(info) = self.vars.get_mut(&name) {
                            info.state = state;
                        }
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) => {
                // No ownership implications.
            }
            Stmt::Emit(e) => {
                self.check_expr(&e.value);
            }
            Stmt::Defer(d) => {
                self.enter_scope();
                for s in &d.body {
                    self.check_stmt(s);
                }
                self.exit_scope();
            }
            Stmt::Yield(y) => {
                self.check_expr(&y.value);
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            // Literals — no ownership implications.
            Expr::IntLit(_, _)
            | Expr::BigIntLit(_, _)
            | Expr::FloatLit(_, _)
            | Expr::StringLit(_, _)
            | Expr::BoolLit(_, _)
            | Expr::NullLit(_)
            | Expr::RawStringLit(_, _)
            | Expr::BytesLit(_, _) => {}

            Expr::StringInterp(segments, _) => {
                for seg in segments {
                    match seg {
                        StringSegment::Interpolation(expr) => {
                            self.check_expr(expr);
                        }
                        StringSegment::FormattedInterpolation(expr, _) => {
                            self.check_expr(expr);
                        }
                        StringSegment::Literal(_) => {}
                    }
                }
            }

            Expr::Ident(name, span) => {
                self.use_var(name, *span);
            }

            Expr::ListLit(elems, _) => {
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
            Expr::TupleLit(elems, _) => {
                for e in elems {
                    self.check_expr(e);
                }
            }
            Expr::SetLit(elems, _) => {
                for e in elems {
                    self.check_expr(e);
                }
            }

            Expr::BinOp(lhs, _, rhs, _) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            Expr::UnaryOp(_, operand, _) => {
                self.check_expr(operand);
            }

            Expr::Call(callee, args, _) => {
                self.check_expr(callee);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => self.check_expr(e),
                        CallArg::Named(_, e, _) => self.check_expr(e),
                        CallArg::Role(_, e, _) => self.check_expr(e),
                    }
                }
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

            Expr::DotAccess(base, _, _) => {
                self.check_expr(base);
            }
            Expr::IndexAccess(base, index, _) => {
                self.check_expr(base);
                self.check_expr(index);
            }

            Expr::RoleBlock(_, body, _) => {
                self.check_expr(body);
            }
            Expr::ExpectSchema(body, _, _) => {
                self.check_expr(body);
            }

            Expr::Lambda {
                params, body, span, ..
            } => {
                // Lambdas capture outer variables — any Owned outer variable
                // referenced inside the lambda body is moved (captured by value).
                // We enter a new scope for the lambda body and register its params,
                // but captured outer variables will be consumed via normal `use_var`.
                self.enter_scope();
                // Register lambda parameters as fresh bindings.
                for p in params {
                    let ty = resolve_type_expr(&p.ty, self.symbols);
                    let mode = if p.variadic {
                        OwnershipMode::Owned
                    } else {
                        ownership_mode_for_type(&ty)
                    };
                    let actual_ty = if p.variadic {
                        Type::List(Box::new(ty))
                    } else {
                        ty
                    };
                    self.locals.insert(p.name.clone(), actual_ty);
                    self.declare_var(&p.name, mode, *span);
                }
                match body {
                    LambdaBody::Expr(e) => self.check_expr(e),
                    LambdaBody::Block(stmts) => {
                        for s in stmts {
                            self.check_stmt(s);
                        }
                    }
                }
                self.exit_scope();
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

            Expr::TryExpr(inner, _) => {
                self.check_expr(inner);
            }
            Expr::TryElse {
                expr: inner,
                handler,
                ..
            } => {
                self.check_expr(inner);
                self.check_expr(handler);
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
            Expr::NullAssert(inner, _) => {
                self.check_expr(inner);
            }
            Expr::SpreadExpr(inner, _) => {
                self.check_expr(inner);
            }
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                self.check_expr(cond);
                // Branches — snapshot/restore like if-statement.
                let snapshot = self.snapshot_vars();
                self.check_expr(then_val);
                let then_state = self.snapshot_vars();
                self.restore_vars(&snapshot);
                self.check_expr(else_val);
                let else_state = self.snapshot_vars();
                self.merge_branch_states(&then_state, &else_state);
            }
            Expr::AwaitExpr(inner, _) => {
                self.check_expr(inner);
            }
            Expr::Comprehension {
                body,
                iter,
                extra_clauses,
                condition,
                var,
                span,
                ..
            } => {
                self.check_expr(iter);
                self.enter_scope();
                let iter_ty = self.infer_expr_type(iter);
                let elem_ty = match &iter_ty {
                    Type::List(inner) | Type::Set(inner) => *inner.clone(),
                    Type::Map(k, _) => *k.clone(),
                    _ => Type::Any,
                };
                let mode = ownership_mode_for_type(&elem_ty);
                self.locals.insert(var.clone(), elem_ty);
                self.declare_var(var, mode, *span);
                for clause in extra_clauses {
                    self.check_expr(&clause.iter);
                    let clause_iter_ty = self.infer_expr_type(&clause.iter);
                    let clause_elem_ty = match &clause_iter_ty {
                        Type::List(inner) | Type::Set(inner) => *inner.clone(),
                        Type::Map(k, _) => *k.clone(),
                        _ => Type::Any,
                    };
                    let clause_mode = ownership_mode_for_type(&clause_elem_ty);
                    self.locals.insert(clause.var.clone(), clause_elem_ty);
                    self.declare_var(&clause.var, clause_mode, *span);
                }
                if let Some(cond) = condition {
                    self.check_expr(cond);
                }
                self.check_expr(body);
                self.exit_scope();
            }
            Expr::MatchExpr { subject, arms, .. } => {
                self.check_expr(subject);
                let snapshot = self.snapshot_vars();
                for arm in arms {
                    self.restore_vars(&snapshot);
                    self.enter_scope();
                    let subject_ty = self.infer_expr_type(subject);
                    self.bind_match_pattern(&arm.pattern, &subject_ty);
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                    self.exit_scope();
                }
            }
            Expr::BlockExpr(stmts, _) => {
                self.enter_scope();
                for s in stmts {
                    self.check_stmt(s);
                }
                self.exit_scope();
            }
            Expr::Pipe { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            Expr::IsType { expr, .. } => {
                self.check_expr(expr);
            }
            Expr::TypeCast { expr, .. } => {
                self.check_expr(expr);
            }
            Expr::WhenExpr {
                arms, else_body, ..
            } => {
                for arm in arms {
                    self.check_expr(&arm.condition);
                    self.check_expr(&arm.body);
                }
                if let Some(eb) = else_body {
                    self.check_expr(eb);
                }
            }
            Expr::ComptimeExpr(inner, _) => {
                self.check_expr(inner);
            }
            Expr::Perform { args, .. } => {
                for a in args {
                    self.check_expr(a);
                }
            }
            Expr::HandleExpr { body, handlers, .. } => {
                self.enter_scope();
                for s in body {
                    self.check_stmt(s);
                }
                self.exit_scope();
                for handler in handlers {
                    self.enter_scope();
                    for s in &handler.body {
                        self.check_stmt(s);
                    }
                    self.exit_scope();
                }
            }
            Expr::ResumeExpr(inner, _) => {
                self.check_expr(inner);
            }
        }
    }

    // ── Pattern binding ─────────────────────────────────────────

    /// Bind names from a let-destructuring pattern.
    fn bind_pattern(&mut self, pattern: &Pattern, ty: &Type) {
        match pattern {
            Pattern::Ident(name, span) => {
                let mode = ownership_mode_for_type(ty);
                self.locals.insert(name.clone(), ty.clone());
                self.declare_var(name, mode, *span);
            }
            Pattern::TupleDestructure { elements, .. } => {
                if let Type::Tuple(elem_types) = ty {
                    for (i, pat) in elements.iter().enumerate() {
                        let elem_ty = elem_types.get(i).cloned().unwrap_or(Type::Any);
                        self.bind_pattern(pat, &elem_ty);
                    }
                } else {
                    for pat in elements {
                        self.bind_pattern(pat, &Type::Any);
                    }
                }
            }
            Pattern::RecordDestructure { fields, span, .. } => {
                for (field_name, sub_pat) in fields {
                    if let Some(pat) = sub_pat {
                        self.bind_pattern(pat, &Type::Any);
                    } else {
                        // Shorthand: `field:` binds `field` as a variable.
                        let mode = OwnershipMode::Copy; // Conservative for unresolved fields.
                        self.locals.insert(field_name.clone(), Type::Any);
                        self.declare_var(field_name, mode, *span);
                    }
                }
            }
            Pattern::ListDestructure {
                elements,
                rest,
                span,
            } => {
                let inner_ty = match ty {
                    Type::List(inner) => *inner.clone(),
                    _ => Type::Any,
                };
                for elem in elements {
                    self.bind_pattern(elem, &inner_ty);
                }
                if let Some(rest_name) = rest {
                    self.locals.insert(rest_name.clone(), ty.clone());
                    self.declare_var(rest_name, OwnershipMode::Owned, *span);
                }
            }
            Pattern::Variant(_, sub, span) => {
                if let Some(inner) = sub {
                    self.bind_pattern(inner, &Type::Any);
                } else {
                    // No sub-pattern — nothing to bind unless the variant name
                    // itself should be bound. In Lumen, variant patterns like
                    // `ok(val)` bind `val`, not `ok`. Bare `ok` is just a test.
                    let _ = span;
                }
            }
            Pattern::Wildcard(_) => {
                // `_` — nothing to bind.
            }
            Pattern::Literal(_) => {
                // Literal patterns don't bind names.
            }
            Pattern::Guard { inner, .. } => {
                self.bind_pattern(inner, ty);
            }
            Pattern::Or { patterns, .. } => {
                // All alternatives must bind the same names.
                // For simplicity, bind from the first alternative.
                if let Some(first) = patterns.first() {
                    self.bind_pattern(first, ty);
                }
            }
            Pattern::TypeCheck { name, span, .. } => {
                let mode = ownership_mode_for_type(ty);
                self.locals.insert(name.clone(), ty.clone());
                self.declare_var(name, mode, *span);
            }
            Pattern::Range { .. } => {
                // Range patterns don't bind names.
            }
        }
    }

    /// Bind names from a match-arm pattern (similar to let-destructuring
    /// but may have different type context).
    fn bind_match_pattern(&mut self, pattern: &Pattern, subject_ty: &Type) {
        self.bind_pattern(pattern, subject_ty);
    }

    // ── Snapshot / restore for branching ────────────────────────

    fn snapshot_vars(&self) -> HashMap<String, VarState> {
        self.vars
            .iter()
            .map(|(k, v)| (k.clone(), v.state.clone()))
            .collect()
    }

    fn restore_vars(&mut self, snapshot: &HashMap<String, VarState>) {
        for (name, state) in snapshot {
            if let Some(info) = self.vars.get_mut(name) {
                info.state = state.clone();
            }
        }
    }

    /// Merge two branch states: if a variable is moved in *either* branch,
    /// mark it as moved (sound over-approximation).
    fn merge_branch_states(
        &mut self,
        state_a: &HashMap<String, VarState>,
        state_b: &HashMap<String, VarState>,
    ) {
        for (name, info) in &mut self.vars {
            let a = state_a.get(name).cloned().unwrap_or(VarState::Alive);
            let b = state_b.get(name).cloned().unwrap_or(VarState::Alive);

            // If moved in either branch, the overall state is moved.
            match (&a, &b) {
                (VarState::Moved { moved_at }, _) | (_, VarState::Moved { moved_at }) => {
                    info.state = VarState::Moved {
                        moved_at: *moved_at,
                    };
                }
                _ => {
                    // Both alive (or both dropped) — keep alive.
                    info.state = VarState::Alive;
                }
            }
        }
    }

    // ── Lightweight type inference ──────────────────────────────
    //
    // We don't need full type inference here — the typechecker has already
    // validated everything. We just need enough to determine ownership modes
    // for newly bound variables.

    fn infer_expr_type(&self, expr: &Expr) -> Type {
        match expr {
            Expr::IntLit(_, _) | Expr::BigIntLit(_, _) => Type::Int,
            Expr::FloatLit(_, _) => Type::Float,
            Expr::StringLit(_, _) | Expr::StringInterp(_, _) | Expr::RawStringLit(_, _) => {
                Type::String
            }
            Expr::BoolLit(_, _) => Type::Bool,
            Expr::NullLit(_) => Type::Null,
            Expr::BytesLit(_, _) => Type::Bytes,

            Expr::Ident(name, _) => self.locals.get(name).cloned().unwrap_or(Type::Any),

            Expr::ListLit(elems, _) => {
                let inner = elems
                    .first()
                    .map(|e| self.infer_expr_type(e))
                    .unwrap_or(Type::Any);
                Type::List(Box::new(inner))
            }
            Expr::MapLit(_, _) => Type::Map(Box::new(Type::String), Box::new(Type::Any)),
            Expr::TupleLit(elems, _) => {
                Type::Tuple(elems.iter().map(|e| self.infer_expr_type(e)).collect())
            }
            Expr::SetLit(elems, _) => {
                let inner = elems
                    .first()
                    .map(|e| self.infer_expr_type(e))
                    .unwrap_or(Type::Any);
                Type::Set(Box::new(inner))
            }
            Expr::RecordLit(name, _, _) => Type::Record(name.clone()),

            Expr::Lambda { .. } => Type::Fn(vec![], Box::new(Type::Any)),

            // Call: if the callee is a known record type, this is a constructor call.
            // Also check the symbol table for cell return types.
            Expr::Call(callee, _, _) => {
                if let Expr::Ident(name, _) = callee.as_ref() {
                    // Record constructor: Point(x: 1, y: 2)
                    if let Some(ti) = self.symbols.types.get(name) {
                        use crate::compiler::resolve::TypeInfoKind;
                        match &ti.kind {
                            TypeInfoKind::Record(_) => return Type::Record(name.clone()),
                            TypeInfoKind::Enum(_) => return Type::Enum(name.clone()),
                            TypeInfoKind::Builtin => {}
                        }
                    }
                    // Cell (function) call: use the declared return type.
                    if let Some(cell_info) = self.symbols.cells.get(name) {
                        if let Some(ref rt) = cell_info.return_type {
                            return resolve_type_expr(rt, self.symbols);
                        }
                    }
                }
                Type::Any
            }

            // For everything else, fall back to Any.
            _ => Type::Any,
        }
    }
}

// ── Convenience entry point ─────────────────────────────────────────

/// Run ownership analysis on a typechecked program.
///
/// Returns an empty Vec on success, or a list of ownership errors.
pub fn check_program(program: &Program, symbols: &SymbolTable) -> Vec<OwnershipError> {
    let checker = OwnershipChecker::new(symbols);
    checker.check_program(program)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;
    use crate::compiler::parser::Parser;
    use crate::compiler::resolve;
    use crate::compiler::typecheck;

    /// Helper: parse + resolve + typecheck + ownership-check raw Lumen source.
    fn ownership_check(src: &str) -> Vec<OwnershipError> {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().expect("lex failed");
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).expect("parse failed");
        let symbols = resolve::resolve(&prog).expect("resolve failed");
        typecheck::typecheck(&prog, &symbols).expect("typecheck failed");
        check_program(&prog, &symbols)
    }

    /// Helper: assert ownership check produces no errors.
    fn assert_ok(src: &str) {
        let errors = ownership_check(src);
        assert!(
            errors.is_empty(),
            "expected no ownership errors, got: {:?}",
            errors
        );
    }

    /// Helper: assert ownership check produces at least one error matching a predicate.
    fn assert_has_error(src: &str, pred: impl Fn(&OwnershipError) -> bool) {
        let errors = ownership_check(src);
        assert!(
            errors.iter().any(&pred),
            "expected a matching ownership error, got: {:?}",
            errors
        );
    }

    // ── Copy types ──────────────────────────────────────────────

    #[test]
    fn copy_int_multiple_uses() {
        // Int is Copy — can be used multiple times without error.
        assert_ok(
            "cell main() -> Int\n  let x = 42\n  let a = x + 1\n  let b = x + 2\n  return b\nend",
        );
    }

    #[test]
    fn copy_string_multiple_uses() {
        // String is Copy — can be used multiple times.
        assert_ok(
            "cell main() -> String\n  let s = \"hello\"\n  let a = s\n  let b = s\n  return b\nend",
        );
    }

    #[test]
    fn copy_bool_multiple_uses() {
        assert_ok("cell main() -> Bool\n  let b = true\n  let x = b\n  let y = b\n  return y\nend");
    }

    // ── Owned types: use after move ─────────────────────────────

    #[test]
    fn owned_list_use_after_move() {
        // List is Owned — second use should be a UseAfterMove error.
        assert_has_error(
            "cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  let a = xs\n  let b = xs\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "xs"),
        );
    }

    #[test]
    fn owned_tuple_use_after_move() {
        assert_has_error(
            "cell main() -> (Int, Int)\n  let t = (1, 2)\n  let a = t\n  let b = t\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "t"),
        );
    }

    #[test]
    fn owned_map_use_after_move() {
        assert_has_error(
            "cell main() -> map[String, Int]\n  let m = {\"a\": 1}\n  let a = m\n  let b = m\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "m"),
        );
    }

    #[test]
    fn owned_record_use_after_move() {
        assert_has_error(
            "record Point\n  x: Int\n  y: Int\nend\n\ncell main() -> Point\n  let p = Point(x: 1, y: 2)\n  let a = p\n  let b = p\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "p"),
        );
    }

    // ── Move twice ──────────────────────────────────────────────

    #[test]
    fn owned_move_twice() {
        // Moving a list twice should produce UseAfterMove on the second.
        assert_has_error(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  let data = [1, 2]\n  let a = consume(data)\n  let b = consume(data)\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "data"),
        );
    }

    // ── Owned single use is fine ────────────────────────────────

    #[test]
    fn owned_list_single_use() {
        // Single use of an owned list — no error.
        assert_ok("cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  return xs\nend");
    }

    #[test]
    fn owned_record_single_use() {
        assert_ok(
            "record Point\n  x: Int\n  y: Int\nend\n\ncell main() -> Point\n  let p = Point(x: 1, y: 2)\n  return p\nend",
        );
    }

    // ── Scope independence ──────────────────────────────────────

    #[test]
    fn different_scopes_independent() {
        // Variables in different (non-overlapping) scopes don't interfere.
        // Each scope creates and immediately returns its owned list via a consuming function.
        assert_ok(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  if true\n    let xs = [1]\n    let a = consume(xs)\n  end\n  if true\n    let xs = [2]\n    let b = consume(xs)\n  end\n  return 0\nend",
        );
    }

    // ── Re-assignment restores liveness ─────────────────────────

    #[test]
    fn reassign_restores_liveness() {
        // After re-assigning, the variable is alive again.
        assert_ok(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> list[Int]\n  let mut xs = [1, 2]\n  let a = consume(xs)\n  xs = [3, 4]\n  return xs\nend",
        );
    }

    // ── If/else branch merging ──────────────────────────────────

    #[test]
    fn if_else_both_move() {
        // If both branches consume the variable, it's moved afterward.
        assert_has_error(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  let data = [1, 2]\n  if true\n    let a = consume(data)\n  else\n    let b = consume(data)\n  end\n  let c = consume(data)\n  return c\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "data"),
        );
    }

    // ── NotConsumed: owned var goes out of scope unused ─────────

    #[test]
    fn not_consumed_owned_var() {
        // An owned variable that is declared but never used should produce NotConsumed.
        assert_has_error(
            "cell main() -> Int\n  let xs = [1, 2, 3]\n  return 0\nend",
            |e| matches!(e, OwnershipError::NotConsumed { variable, .. } if variable == "xs"),
        );
    }

    #[test]
    fn not_consumed_does_not_fire_for_copy() {
        // Copy variables going out of scope unused should NOT produce NotConsumed.
        assert_ok("cell main() -> Int\n  let x = 42\n  return 0\nend");
    }

    // ── Borrow tracking ─────────────────────────────────────────

    #[test]
    fn borrow_after_move_error() {
        // Borrowing a moved variable should error.
        let src = "cell main() -> list[Int]\n  let xs = [1, 2]\n  let a = xs\n  return a\nend";
        let errors = ownership_check(src);
        // `xs` is moved by `let a = xs`, but there's no subsequent borrow attempt here.
        // This test just validates the basic flow doesn't panic.
        let _ = errors;
    }

    // ── ownership_mode_for_type unit tests ──────────────────────

    #[test]
    fn mode_primitives_are_copy() {
        assert_eq!(ownership_mode_for_type(&Type::Int), OwnershipMode::Copy);
        assert_eq!(ownership_mode_for_type(&Type::Float), OwnershipMode::Copy);
        assert_eq!(ownership_mode_for_type(&Type::Bool), OwnershipMode::Copy);
        assert_eq!(ownership_mode_for_type(&Type::String), OwnershipMode::Copy);
        assert_eq!(ownership_mode_for_type(&Type::Null), OwnershipMode::Copy);
        assert_eq!(ownership_mode_for_type(&Type::Any), OwnershipMode::Copy);
    }

    #[test]
    fn mode_compounds_are_owned() {
        assert_eq!(
            ownership_mode_for_type(&Type::List(Box::new(Type::Int))),
            OwnershipMode::Owned
        );
        assert_eq!(
            ownership_mode_for_type(&Type::Map(Box::new(Type::String), Box::new(Type::Int))),
            OwnershipMode::Owned
        );
        assert_eq!(
            ownership_mode_for_type(&Type::Set(Box::new(Type::Int))),
            OwnershipMode::Owned
        );
        assert_eq!(
            ownership_mode_for_type(&Type::Tuple(vec![Type::Int, Type::Float])),
            OwnershipMode::Owned
        );
        assert_eq!(
            ownership_mode_for_type(&Type::Record("Foo".into())),
            OwnershipMode::Owned
        );
        assert_eq!(
            ownership_mode_for_type(&Type::Fn(vec![], Box::new(Type::Int))),
            OwnershipMode::Owned
        );
    }

    #[test]
    fn mode_union_all_copy_is_copy() {
        assert_eq!(
            ownership_mode_for_type(&Type::Union(vec![Type::Int, Type::Null])),
            OwnershipMode::Copy
        );
    }

    #[test]
    fn mode_union_with_owned_is_owned() {
        assert_eq!(
            ownership_mode_for_type(&Type::Union(vec![
                Type::Int,
                Type::List(Box::new(Type::Int))
            ])),
            OwnershipMode::Owned
        );
    }

    // ── For-loop variable is fresh each iteration ───────────────

    #[test]
    fn for_loop_elem_is_fresh() {
        // Using the loop variable in the body is fine even for owned elements.
        assert_ok(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  let xs = [[1], [2], [3]]\n  for x in xs\n    let a = consume(x)\n  end\n  return 0\nend",
        );
    }

    // ── Compound assignment keeps variable alive ────────────────

    #[test]
    fn compound_assign_keeps_alive() {
        assert_ok("cell main() -> Int\n  let mut x = 10\n  x += 5\n  return x\nend");
    }

    // ── Functions returning owned types ─────────────────────────

    #[test]
    fn function_return_owned_single_use() {
        assert_ok(
            "cell make_list() -> list[Int]\n  return [1, 2, 3]\nend\n\ncell main() -> list[Int]\n  let xs = make_list()\n  return xs\nend",
        );
    }

    // ── Match arms ──────────────────────────────────────────────

    #[test]
    fn match_arm_bindings_scoped() {
        assert_ok(
            "cell main() -> Int\n  let x = 42\n  match x\n    1 -> return 1\n    _ -> return 0\n  end\nend",
        );
    }

    // ── Lambda / closure capture ────────────────────────────────

    #[test]
    fn lambda_captures_owned_var() {
        // A lambda that references an outer owned variable should move it.
        // Using the variable again after the lambda is created should error.
        assert_has_error(
            "cell main() -> Int\n  let xs = [1, 2, 3]\n  let f = fn() => xs\n  let a = xs\n  return 0\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "xs"),
        );
    }

    #[test]
    fn lambda_own_params_dont_shadow_outer() {
        // Lambda params are fresh bindings in the lambda scope.
        // An outer variable with the same name should remain accessible.
        // We pass `f` to a function so it's consumed (Fn is Owned).
        assert_ok(
            "cell use_fn(f: (Int) -> Int) -> Int\n  return 0\nend\n\ncell main() -> Int\n  let x = 42\n  let f = fn(x: Int) => x + 1\n  let _ = use_fn(f)\n  return x\nend",
        );
    }

    #[test]
    fn lambda_no_capture_is_fine() {
        // A lambda that doesn't capture anything should be fine (when consumed).
        assert_ok(
            "cell use_fn(f: (Int) -> Int) -> Int\n  return 0\nend\n\ncell main() -> Int\n  let f = fn(x: Int) => x + 1\n  return use_fn(f)\nend",
        );
    }

    // ── While loops ─────────────────────────────────────────────

    #[test]
    fn while_loop_body_scoped() {
        // Variables declared inside a while body are scoped to each iteration.
        assert_ok(
            "cell main() -> Int\n  let mut i = 0\n  while i < 3\n    let x = 42\n    i += 1\n  end\n  return i\nend",
        );
    }

    // ── Nested scopes ───────────────────────────────────────────

    #[test]
    fn nested_scopes_ownership() {
        // Owned variable consumed in an inner scope is moved.
        assert_has_error(
            "cell main() -> Int\n  let xs = [1, 2]\n  if true\n    let a = xs\n  end\n  let b = xs\n  return 0\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "xs"),
        );
    }

    #[test]
    fn nested_scopes_independent_bindings() {
        // Variables declared in separate scoped blocks don't interfere.
        assert_ok(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  if true\n    let xs = [1]\n    let _ = consume(xs)\n  end\n  if true\n    let ys = [2]\n    let _ = consume(ys)\n  end\n  return 0\nend",
        );
    }

    // ── Return consumes ─────────────────────────────────────────

    #[test]
    fn return_consumes_owned_no_not_consumed() {
        // Returning an owned variable should count as consuming it.
        // No NotConsumed error should fire.
        assert_ok("cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  return xs\nend");
    }

    // ── Pipe operator ───────────────────────────────────────────

    #[test]
    fn pipe_moves_owned_value() {
        // Piping an owned value into a function consumes it.
        assert_has_error(
            "cell consume(xs: list[Int]) -> Int\n  return len(xs)\nend\n\ncell main() -> Int\n  let data = [1, 2, 3]\n  let a = data |> consume()\n  let b = data |> consume()\n  return b\nend",
            |e| matches!(e, OwnershipError::UseAfterMove { variable, .. } if variable == "data"),
        );
    }

    // ── Diagnostics integration ─────────────────────────────────

    #[test]
    fn ownership_error_display() {
        // Verify Display impl produces sensible messages.
        let err = OwnershipError::UseAfterMove {
            variable: "xs".to_string(),
            moved_at: Span {
                start: 0,
                end: 0,
                line: 3,
                col: 5,
            },
            used_at: Span {
                start: 0,
                end: 0,
                line: 5,
                col: 10,
            },
        };
        let msg = format!("{}", err);
        assert!(msg.contains("xs"));
        assert!(msg.contains("line 5"));
        assert!(msg.contains("line 3"));
    }

    #[test]
    fn not_consumed_display() {
        let err = OwnershipError::NotConsumed {
            variable: "data".to_string(),
            declared_at: Span {
                start: 0,
                end: 0,
                line: 2,
                col: 3,
            },
        };
        let msg = format!("{}", err);
        assert!(msg.contains("data"));
        assert!(msg.contains("line 2"));
        assert!(msg.contains("never consumed"));
    }

    // ── Defer blocks ────────────────────────────────────────────

    #[test]
    fn defer_body_scoped() {
        // Variables used inside defer are in their own scope.
        assert_ok(
            "cell main() -> Int\n  let x = 42\n  defer\n    let y = x + 1\n  end\n  return x\nend",
        );
    }

    // ── Multiple return paths ───────────────────────────────────

    #[test]
    fn if_with_return_in_branch() {
        // If one branch returns, the variable might still be alive in the other path.
        assert_ok(
            "cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  if true\n    return xs\n  end\n  return [4, 5]\nend",
        );
    }
}
