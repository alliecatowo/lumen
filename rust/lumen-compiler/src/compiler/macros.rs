//! Hygienic macro system for Lumen.
//!
//! This module implements a syntactic macro system that operates on AST
//! fragments. Hygiene is enforced by mangling names introduced within a
//! macro body so that they cannot accidentally shadow user-defined names
//! at the expansion site.
//!
//! # Overview
//!
//! 1. **Registration** — Macro definitions are parsed and stored in a
//!    [`MacroRegistry`].  Each definition records its parameter list and
//!    a template body composed of [`MacroBodyItem`] fragments.
//!
//! 2. **Expansion** — When a macro is invoked, a fresh [`MacroScope`] is
//!    allocated.  Parameter references are replaced with the caller's
//!    arguments and any names introduced by the macro (marked as
//!    [`MacroBodyItem::ScopeIntro`]) are mangled to include the scope ID,
//!    preventing capture.
//!
//! 3. **Validation** — [`validate_macro_def`] checks that a macro body
//!    only references declared parameters and does not contain direct
//!    recursive expansion.

use std::collections::HashMap;
use std::fmt;

use super::tokens::Span;

// ── Hygiene context ──────────────────────────────────────────────────

/// Tracks the hygiene scope for a single macro expansion.
///
/// Each expansion receives a unique `id` so that names it introduces
/// will not collide with names from other expansions or from user code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroScope {
    /// Unique identifier for this scope.
    pub id: u64,
    /// Optional parent scope (for nested macro expansions).
    pub parent: Option<u64>,
}

/// A name together with the hygiene scope in which it was introduced.
///
/// The [`mangled`](HygienicName::mangled) field holds the rewritten
/// identifier that is actually emitted into the expanded code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygienicName {
    /// The name as written in the macro definition.
    pub original: String,
    /// The scope in which this name was introduced.
    pub scope_id: u64,
    /// The mangled form: `__hyg_{scope_id}_{original}`.
    pub mangled: String,
}

// ── Macro definition ─────────────────────────────────────────────────

/// A stored macro definition.
#[derive(Debug, Clone)]
pub struct MacroDef {
    /// The macro's name (without the trailing `!`).
    pub name: String,
    /// Declared parameter names.
    pub params: Vec<String>,
    /// Template body — a sequence of literal text, parameter references,
    /// and names introduced by the macro.
    pub body_template: Vec<MacroBodyItem>,
    /// Source location of the `macro` keyword.
    pub span: Span,
}

/// One fragment of a macro body template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroBodyItem {
    /// Raw text that is emitted verbatim.
    Literal(String),
    /// A reference to a declared parameter — will be replaced by the
    /// corresponding argument at expansion time.
    ParamRef(String),
    /// A new name introduced by the macro that must be mangled for
    /// hygiene.
    ScopeIntro(String),
}

// ── Expansion output ─────────────────────────────────────────────────

/// The result of expanding a single macro invocation.
#[derive(Debug, Clone)]
pub struct MacroExpansion {
    /// The expanded fragments in order.
    pub fragments: Vec<ExpandedFragment>,
    /// All hygienic names that were introduced during this expansion.
    pub introduced_names: Vec<HygienicName>,
    /// The scope that was allocated for this expansion.
    pub scope: MacroScope,
}

/// One fragment of an expanded macro invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandedFragment {
    /// Verbatim text.
    Text(String),
    /// A parameter that was substituted with the caller's argument.
    Substituted {
        /// The parameter name.
        param: String,
        /// The argument value that replaced it.
        arg: String,
    },
    /// A name binding that was mangled for hygiene.
    HygienicBinding(HygienicName),
}

// ── Errors ───────────────────────────────────────────────────────────

/// Errors that can occur during macro registration, expansion, or
/// validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroError {
    /// A macro with this name is already registered.
    AlreadyDefined(String),
    /// No macro with this name exists.
    Undefined(String),
    /// The number of arguments does not match the parameter count.
    ArgCountMismatch {
        /// Expected number of arguments.
        expected: usize,
        /// Actually provided.
        actual: usize,
    },
    /// A parameter reference in the body names a parameter that was not
    /// declared.
    InvalidParam(String),
    /// The macro body appears to expand itself recursively.
    RecursiveExpansion(String),
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MacroError::AlreadyDefined(name) => {
                write!(f, "macro '{}' is already defined", name)
            }
            MacroError::Undefined(name) => {
                write!(f, "undefined macro '{}'", name)
            }
            MacroError::ArgCountMismatch { expected, actual } => {
                write!(
                    f,
                    "macro expects {} argument(s) but {} were provided",
                    expected, actual
                )
            }
            MacroError::InvalidParam(name) => {
                write!(f, "invalid parameter reference '{}'", name)
            }
            MacroError::RecursiveExpansion(name) => {
                write!(f, "recursive expansion detected in macro '{}'", name)
            }
        }
    }
}

impl std::error::Error for MacroError {}

// ── Registry ─────────────────────────────────────────────────────────

/// Central registry for macro definitions.
///
/// Also acts as the scope-ID allocator so that every expansion across
/// the entire compilation gets a unique scope.
#[derive(Debug)]
pub struct MacroRegistry {
    /// Registered macros keyed by name.
    pub macros: HashMap<String, MacroDef>,
    /// Monotonically increasing scope counter.
    pub next_scope_id: u64,
}

impl MacroRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            next_scope_id: 1,
        }
    }

    /// Register a macro definition.
    ///
    /// Returns `Err(MacroError::AlreadyDefined)` if a macro with the
    /// same name has already been registered.
    pub fn register(&mut self, name: String, def: MacroDef) -> Result<(), MacroError> {
        if self.macros.contains_key(&name) {
            return Err(MacroError::AlreadyDefined(name));
        }
        self.macros.insert(name, def);
        Ok(())
    }

    /// Look up a macro by name.
    pub fn lookup(&self, name: &str) -> Option<&MacroDef> {
        self.macros.get(name)
    }

    /// Allocate a fresh [`MacroScope`].
    ///
    /// Each call increments the internal counter, guaranteeing uniqueness.
    pub fn new_scope(&mut self) -> MacroScope {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        MacroScope { id, parent: None }
    }
}

impl Default for MacroRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Name mangling ────────────────────────────────────────────────────

/// Produce a [`HygienicName`] by mangling `name` with the given scope
/// ID.
///
/// The mangled form is `__hyg_{scope_id}_{name}`.
pub fn mangle_name(name: &str, scope_id: u64) -> HygienicName {
    HygienicName {
        original: name.to_string(),
        scope_id,
        mangled: format!("__hyg_{}_{}", scope_id, name),
    }
}

// ── Expansion ────────────────────────────────────────────────────────

/// Expand a macro invocation.
///
/// Looks up `macro_name` in `registry`, verifies the argument count,
/// allocates a new scope, and processes the body template — substituting
/// parameters with `args` and mangling introduced names.
pub fn expand_macro(
    registry: &mut MacroRegistry,
    macro_name: &str,
    args: &[String],
) -> Result<MacroExpansion, MacroError> {
    let def = registry
        .lookup(macro_name)
        .ok_or_else(|| MacroError::Undefined(macro_name.to_string()))?
        .clone();

    if args.len() != def.params.len() {
        return Err(MacroError::ArgCountMismatch {
            expected: def.params.len(),
            actual: args.len(),
        });
    }

    let scope = registry.new_scope();

    // Build a param→arg map for fast lookup.
    let param_map: HashMap<&str, &str> = def
        .params
        .iter()
        .zip(args.iter())
        .map(|(p, a)| (p.as_str(), a.as_str()))
        .collect();

    let mut fragments = Vec::new();
    let mut introduced_names = Vec::new();

    for item in &def.body_template {
        match item {
            MacroBodyItem::Literal(text) => {
                fragments.push(ExpandedFragment::Text(text.clone()));
            }
            MacroBodyItem::ParamRef(param) => {
                if let Some(&arg) = param_map.get(param.as_str()) {
                    fragments.push(ExpandedFragment::Substituted {
                        param: param.clone(),
                        arg: arg.to_string(),
                    });
                } else {
                    // Should not happen if validation passed, but be safe.
                    return Err(MacroError::InvalidParam(param.clone()));
                }
            }
            MacroBodyItem::ScopeIntro(name) => {
                let hyg = mangle_name(name, scope.id);
                fragments.push(ExpandedFragment::HygienicBinding(hyg.clone()));
                introduced_names.push(hyg);
            }
        }
    }

    Ok(MacroExpansion {
        fragments,
        introduced_names,
        scope,
    })
}

// ── Validation ───────────────────────────────────────────────────────

/// Validate a macro definition before registration.
///
/// Checks:
/// - All parameter names are non-empty and consist of valid identifier
///   characters.
/// - Every `ParamRef` in the body references a declared parameter.
/// - The body does not contain a `Literal` fragment that looks like a
///   recursive invocation of the macro itself (simple textual check).
pub fn validate_macro_def(def: &MacroDef) -> Result<(), Vec<MacroError>> {
    let mut errors = Vec::new();

    // 1. Validate parameter names.
    for param in &def.params {
        if param.is_empty() || !param.chars().all(|c| c.is_alphanumeric() || c == '_') {
            errors.push(MacroError::InvalidParam(param.clone()));
        }
    }

    // 2. Check body references.
    let param_set: std::collections::HashSet<&str> =
        def.params.iter().map(|s| s.as_str()).collect();

    for item in &def.body_template {
        match item {
            MacroBodyItem::ParamRef(name) => {
                if !param_set.contains(name.as_str()) {
                    errors.push(MacroError::InvalidParam(name.clone()));
                }
            }
            MacroBodyItem::Literal(text) => {
                // Simple textual check for direct recursive invocation.
                let pattern = format!("{}!", def.name);
                if text.contains(&pattern) {
                    errors.push(MacroError::RecursiveExpansion(def.name.clone()));
                }
            }
            MacroBodyItem::ScopeIntro(_) => { /* always valid */ }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn test_mangle_name_format() {
        let h = mangle_name("result", 7);
        assert_eq!(h.original, "result");
        assert_eq!(h.scope_id, 7);
        assert_eq!(h.mangled, "__hyg_7_result");
    }

    #[test]
    fn test_registry_new_empty() {
        let reg = MacroRegistry::new();
        assert!(reg.macros.is_empty());
        assert_eq!(reg.next_scope_id, 1);
    }

    #[test]
    fn test_register_and_lookup() {
        let mut reg = MacroRegistry::new();
        let def = MacroDef {
            name: "assert_eq".into(),
            params: vec!["expected".into(), "actual".into()],
            body_template: vec![MacroBodyItem::Literal("ok".into())],
            span: dummy_span(),
        };
        reg.register("assert_eq".into(), def).unwrap();
        assert!(reg.lookup("assert_eq").is_some());
    }

    #[test]
    fn test_scope_ids_increment() {
        let mut reg = MacroRegistry::new();
        let s1 = reg.new_scope();
        let s2 = reg.new_scope();
        let s3 = reg.new_scope();
        assert_eq!(s1.id, 1);
        assert_eq!(s2.id, 2);
        assert_eq!(s3.id, 3);
        assert!(s1.parent.is_none());
    }
}
