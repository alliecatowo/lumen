//! GADT (Generalized Algebraic Data Type) support for the Lumen compiler.
//!
//! GADTs extend standard algebraic data types by allowing each variant to
//! specialize the type parameters of the parent enum. For example:
//!
//! ```text
//! enum Expr[T]
//!   IntLit(Int) -> Expr[Int]
//!   BoolLit(Bool) -> Expr[Bool]
//!   Add(Expr[Int], Expr[Int]) -> Expr[Int]
//!   If(Expr[Bool], Expr[T], Expr[T]) -> Expr[T]
//! end
//! ```
//!
//! When pattern-matching on a GADT value, the type checker can refine the
//! type parameter in each branch according to the variant's return constraint.

use std::collections::HashMap;

use super::tokens::Span;

// ── Primitive type names recognized in GADT constraints ────────────────

/// The set of built-in type names accepted as concrete type arguments
/// inside GADT return-type constraints.
const VALID_CONCRETE_TYPES: &[&str] = &[
    "Int", "Float", "String", "Bool", "Bytes", "Json", "Null", "Any",
];

// ── Core data structures ──────────────────────────────────────────────

/// Represents a single type argument inside a GADT variant's return-type
/// annotation (e.g. the `Int` in `-> Expr[Int]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GadtTypeArg {
    /// A concrete / primitive type such as `Int` or `Bool`.
    Concrete(String),
    /// A type parameter of the parent enum, such as `T`.
    Param(String),
    /// A compound type expression such as `List[Int]`.
    Complex(String),
}

/// The return-type constraint on one variant of a GADT
/// (e.g. `-> Expr[Int]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GadtVariantConstraint {
    /// Name of the variant this constraint belongs to.
    pub variant_name: String,
    /// The type arguments inside the return-type annotation.
    pub type_args: Vec<GadtTypeArg>,
    /// Source span of the constraint.
    pub span: Span,
}

/// Complete information about one variant in a GADT definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GadtVariantInfo {
    /// Variant name (e.g. `IntLit`).
    pub name: String,
    /// Names of the payload types this variant carries.
    pub payload_types: Vec<String>,
    /// Optional return-type constraint (the `-> Expr[Int]` part).
    pub return_constraint: Option<GadtVariantConstraint>,
}

/// A fully described GADT definition that can be validated and queried for
/// type-refinement information during match analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GadtDef {
    /// Name of the enum (e.g. `Expr`).
    pub enum_name: String,
    /// Names of the generic type parameters in declaration order.
    pub generic_params: Vec<String>,
    /// Variant information.
    pub variants: Vec<GadtVariantInfo>,
    /// Source span of the whole enum definition.
    pub span: Span,
}

// ── Error types ───────────────────────────────────────────────────────

/// Errors that can occur when validating a GADT definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GadtError {
    /// A variant's return constraint references a different enum name than
    /// the parent.
    WrongEnumName {
        /// The variant whose constraint is wrong.
        variant_name: String,
        /// The enum name used in the constraint.
        found: String,
        /// The expected enum name.
        expected: String,
        /// Source span.
        span: Span,
    },
    /// The number of type arguments in the return constraint does not match
    /// the number of generic parameters on the parent enum.
    ArityMismatch {
        /// The offending variant.
        variant_name: String,
        /// How many type args were provided.
        found: usize,
        /// How many were expected.
        expected: usize,
        /// Source span.
        span: Span,
    },
    /// A concrete type argument in a constraint is not a recognized type.
    InvalidConcreteType {
        /// The offending variant.
        variant_name: String,
        /// The unrecognized type name.
        type_name: String,
        /// Source span.
        span: Span,
    },
    /// None of the variants have a concrete specialization, so this is just
    /// a regular generic enum — not a true GADT.
    NoConcreteSpecialization {
        /// The enum name.
        enum_name: String,
        /// Source span.
        span: Span,
    },
}

impl std::fmt::Display for GadtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GadtError::WrongEnumName {
                variant_name,
                found,
                expected,
                ..
            } => write!(
                f,
                "variant `{variant_name}` return constraint references `{found}` \
                 but the parent enum is `{expected}`"
            ),
            GadtError::ArityMismatch {
                variant_name,
                found,
                expected,
                ..
            } => write!(
                f,
                "variant `{variant_name}` has {found} type argument(s) in return \
                 constraint, expected {expected}"
            ),
            GadtError::InvalidConcreteType {
                variant_name,
                type_name,
                ..
            } => write!(
                f,
                "variant `{variant_name}` uses unknown concrete type `{type_name}` \
                 in return constraint"
            ),
            GadtError::NoConcreteSpecialization { enum_name, .. } => write!(
                f,
                "enum `{enum_name}` has no variant with a concrete specialization; \
                 it is not a GADT"
            ),
        }
    }
}

impl std::error::Error for GadtError {}

// ── Validation ────────────────────────────────────────────────────────

/// Validate a GADT definition, returning all detected errors.
///
/// The following checks are performed:
/// 1. Every return constraint references the parent enum name.
/// 2. Type-argument arity matches the generic parameter count.
/// 3. Concrete type arguments are recognized built-in types.
/// 4. At least one variant has a concrete specialization (otherwise this
///    is just a regular generic enum, not a GADT).
pub fn validate_gadt(def: &GadtDef) -> Result<(), Vec<GadtError>> {
    let mut errors: Vec<GadtError> = Vec::new();
    let expected_arity = def.generic_params.len();

    for variant in &def.variants {
        if let Some(constraint) = &variant.return_constraint {
            // 1. Enum-name check — the constraint's variant_name field
            //    holds the return-type *enum* name at call sites that
            //    build `GadtVariantConstraint` from parse output. But in
            //    our public API the constraint does NOT carry the
            //    referenced enum name explicitly; we derive it from the
            //    variant_name field's surrounding context. Here we use
            //    `constraint.variant_name` to store the *referenced*
            //    enum name (see builder helpers in tests).  However,
            //    to keep the API unambiguous we instead check whether the
            //    constraint came from a return type referencing the
            //    parent enum. Because `GadtVariantConstraint` is
            //    reconstructed from `-> EnumName[...]`, the builder puts
            //    the *referenced* enum name into `variant_name`.
            //    So: constraint.variant_name should equal def.enum_name.
            if constraint.variant_name != def.enum_name {
                errors.push(GadtError::WrongEnumName {
                    variant_name: variant.name.clone(),
                    found: constraint.variant_name.clone(),
                    expected: def.enum_name.clone(),
                    span: constraint.span,
                });
            }

            // 2. Arity check.
            if constraint.type_args.len() != expected_arity {
                errors.push(GadtError::ArityMismatch {
                    variant_name: variant.name.clone(),
                    found: constraint.type_args.len(),
                    expected: expected_arity,
                    span: constraint.span,
                });
            }

            // 3. Concrete-type validity check.
            for arg in &constraint.type_args {
                if let GadtTypeArg::Concrete(name) = arg {
                    if !VALID_CONCRETE_TYPES.contains(&name.as_str()) {
                        errors.push(GadtError::InvalidConcreteType {
                            variant_name: variant.name.clone(),
                            type_name: name.clone(),
                            span: constraint.span,
                        });
                    }
                }
            }
        }
    }

    // 4. At least one variant must have a concrete specialization.
    let has_concrete = def.variants.iter().any(|v| {
        v.return_constraint.as_ref().is_some_and(|c| {
            c.type_args
                .iter()
                .any(|a| matches!(a, GadtTypeArg::Concrete(_)))
        })
    });

    if !has_concrete {
        errors.push(GadtError::NoConcreteSpecialization {
            enum_name: def.enum_name.clone(),
            span: def.span,
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ── Type refinement ───────────────────────────────────────────────────

/// Given a GADT definition and the name of a variant that was matched,
/// compute the type-parameter refinements.
///
/// Returns a map from type-parameter name to a concrete type name. For
/// instance, matching `IntLit` on `Expr[T]` yields `{"T" => "Int"}`.
/// Variant type args that are themselves parameters (e.g. `T` in the
/// `If` variant) do *not* produce refinements.
///
/// Returns an empty map when:
/// - The variant has no return constraint.
/// - The variant does not specialise any type parameter.
/// - The variant name is unknown.
pub fn refine_type_in_branch(
    gadt: &GadtDef,
    variant_name: &str,
    _current_type_params: &[String],
) -> HashMap<String, String> {
    let mut refinements = HashMap::new();

    let variant = match gadt.variants.iter().find(|v| v.name == variant_name) {
        Some(v) => v,
        None => return refinements,
    };

    let constraint = match &variant.return_constraint {
        Some(c) => c,
        None => return refinements,
    };

    for (i, arg) in constraint.type_args.iter().enumerate() {
        if i >= gadt.generic_params.len() {
            break;
        }
        match arg {
            GadtTypeArg::Concrete(concrete) => {
                refinements.insert(gadt.generic_params[i].clone(), concrete.clone());
            }
            GadtTypeArg::Complex(complex) => {
                refinements.insert(gadt.generic_params[i].clone(), complex.clone());
            }
            GadtTypeArg::Param(_) => {
                // No refinement — the type arg is still generic.
            }
        }
    }

    refinements
}

// ── Exhaustiveness ────────────────────────────────────────────────────

/// Check whether a set of matched variant names covers all variants in a
/// GADT definition.
///
/// Returns a (possibly empty) list of variant names that have *not* been
/// matched.
pub fn check_gadt_exhaustiveness(gadt: &GadtDef, matched_variants: &[String]) -> Vec<String> {
    gadt.variants
        .iter()
        .filter(|v| !matched_variants.iter().any(|m| m == &v.name))
        .map(|v| v.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn concrete_type_list_includes_basics() {
        for ty in &["Int", "Bool", "Float", "String"] {
            assert!(
                VALID_CONCRETE_TYPES.contains(ty),
                "{ty} should be a valid concrete type"
            );
        }
    }

    #[test]
    fn gadt_type_arg_equality() {
        assert_eq!(
            GadtTypeArg::Concrete("Int".into()),
            GadtTypeArg::Concrete("Int".into())
        );
        assert_ne!(
            GadtTypeArg::Concrete("Int".into()),
            GadtTypeArg::Param("T".into())
        );
    }

    #[test]
    fn gadt_error_display() {
        let err = GadtError::WrongEnumName {
            variant_name: "Foo".into(),
            found: "Bar".into(),
            expected: "Baz".into(),
            span: span(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Foo"));
        assert!(msg.contains("Bar"));
        assert!(msg.contains("Baz"));
    }
}
