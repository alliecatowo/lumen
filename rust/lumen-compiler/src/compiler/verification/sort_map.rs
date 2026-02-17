//! Lumen type to SMT sort mapping.
//!
//! Maps Lumen's `Type` enum to SMT-LIB sorts so that the constraint solver
//! can reason about typed symbolic values.  The mapping is intentionally
//! conservative: types that have no clean SMT analogue produce a
//! `MappingError` so callers can fall back to `Unverifiable`.

use crate::compiler::resolve::{SymbolTable, TypeInfoKind};
use crate::compiler::typecheck::Type;
use thiserror::Error;

// ── SMT sort representation ─────────────────────────────────────────

/// A variant in an algebraic datatype sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTypeVariant {
    pub name: String,
    pub fields: Vec<(String, SmtSort)>,
}

/// SMT-LIB sort that a Lumen type maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtSort {
    /// Mathematical integers (arbitrary precision).
    IntSort,
    /// Real numbers (used for Lumen `Float`).
    RealSort,
    /// Boolean.
    BoolSort,
    /// SMT-LIB `String` sort.
    StringSort,
    /// Fixed-width bitvector.
    BitVecSort(u32),
    /// SMT-LIB `Array` sort (index → element).
    ArraySort {
        index: Box<SmtSort>,
        element: Box<SmtSort>,
    },
    /// Algebraic datatype (records, enums).
    DataTypeSort {
        name: String,
        variants: Vec<DataTypeVariant>,
    },
    /// Product of sorts (used for tuples).
    TupleSort(Vec<SmtSort>),
}

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MappingError {
    #[error("type `{0}` has no SMT sort mapping")]
    Unsupported(String),
    #[error("record `{0}` not found in symbol table")]
    RecordNotFound(String),
    #[error("enum `{0}` not found in symbol table")]
    EnumNotFound(String),
}

// ── Public API ──────────────────────────────────────────────────────

/// Map a Lumen `Type` to the corresponding `SmtSort`.
///
/// `symbols` is needed to resolve record field types and enum variants.
/// Pass `None` when symbol-table lookup is unnecessary (e.g. testing
/// primitive-only mappings).
pub fn type_to_smt_sort(ty: &Type, symbols: Option<&SymbolTable>) -> Result<SmtSort, MappingError> {
    match ty {
        // Primitives
        Type::Int => Ok(SmtSort::IntSort),
        Type::Float => Ok(SmtSort::RealSort),
        Type::Bool => Ok(SmtSort::BoolSort),
        Type::String => Ok(SmtSort::StringSort),
        Type::Bytes => Ok(SmtSort::BitVecSort(8)),

        // Null maps to Bool (used as a sentinel; most solvers lack a
        // dedicated unit sort and Bool with a fixed constant works).
        Type::Null => Ok(SmtSort::BoolSort),

        // Collections
        Type::List(elem) => {
            let elem_sort = type_to_smt_sort(elem, symbols)?;
            Ok(SmtSort::ArraySort {
                index: Box::new(SmtSort::IntSort),
                element: Box::new(elem_sort),
            })
        }
        Type::Set(elem) => {
            let elem_sort = type_to_smt_sort(elem, symbols)?;
            Ok(SmtSort::ArraySort {
                index: Box::new(elem_sort),
                element: Box::new(SmtSort::BoolSort),
            })
        }
        Type::Map(key, val) => {
            let key_sort = type_to_smt_sort(key, symbols)?;
            let val_sort = type_to_smt_sort(val, symbols)?;
            Ok(SmtSort::ArraySort {
                index: Box::new(key_sort),
                element: Box::new(val_sort),
            })
        }

        // Tuples
        Type::Tuple(elems) => {
            let sorts: Result<Vec<_>, _> =
                elems.iter().map(|t| type_to_smt_sort(t, symbols)).collect();
            Ok(SmtSort::TupleSort(sorts?))
        }

        // Named record — look up fields in the symbol table
        Type::Record(name) => {
            let symbols = symbols.ok_or_else(|| MappingError::RecordNotFound(name.clone()))?;
            let info = symbols
                .types
                .get(name)
                .ok_or_else(|| MappingError::RecordNotFound(name.clone()))?;
            match &info.kind {
                TypeInfoKind::Record(def) => {
                    let mut fields = Vec::with_capacity(def.fields.len());
                    for f in &def.fields {
                        let field_type =
                            crate::compiler::typecheck::resolve_type_expr(&f.ty, symbols);
                        let sort = type_to_smt_sort(&field_type, Some(symbols))?;
                        fields.push((f.name.clone(), sort));
                    }
                    Ok(SmtSort::DataTypeSort {
                        name: name.clone(),
                        variants: vec![DataTypeVariant {
                            name: name.clone(),
                            fields,
                        }],
                    })
                }
                _ => Err(MappingError::RecordNotFound(name.clone())),
            }
        }

        // Named enum
        Type::Enum(name) => {
            let symbols = symbols.ok_or_else(|| MappingError::EnumNotFound(name.clone()))?;
            let info = symbols
                .types
                .get(name)
                .ok_or_else(|| MappingError::EnumNotFound(name.clone()))?;
            match &info.kind {
                TypeInfoKind::Enum(def) => {
                    let mut variants = Vec::with_capacity(def.variants.len());
                    for v in &def.variants {
                        let fields = match &v.payload {
                            Some(te) => {
                                let payload_type =
                                    crate::compiler::typecheck::resolve_type_expr(te, symbols);
                                let sort = type_to_smt_sort(&payload_type, Some(symbols))?;
                                vec![("value".to_string(), sort)]
                            }
                            None => vec![],
                        };
                        variants.push(DataTypeVariant {
                            name: v.name.clone(),
                            fields,
                        });
                    }
                    Ok(SmtSort::DataTypeSort {
                        name: name.clone(),
                        variants,
                    })
                }
                _ => Err(MappingError::EnumNotFound(name.clone())),
            }
        }

        // Result is structurally an enum with Ok/Err variants
        Type::Result(ok, err) => {
            let ok_sort = type_to_smt_sort(ok, symbols)?;
            let err_sort = type_to_smt_sort(err, symbols)?;
            Ok(SmtSort::DataTypeSort {
                name: "Result".to_string(),
                variants: vec![
                    DataTypeVariant {
                        name: "Ok".to_string(),
                        fields: vec![("value".to_string(), ok_sort)],
                    },
                    DataTypeVariant {
                        name: "Err".to_string(),
                        fields: vec![("value".to_string(), err_sort)],
                    },
                ],
            })
        }

        // Union — no single SMT sort captures a tagged union directly.
        Type::Union(_) => Err(MappingError::Unsupported("Union".to_string())),

        // Function types cannot be represented as first-class SMT sorts.
        Type::Fn(_, _) => Err(MappingError::Unsupported("Fn".to_string())),

        // Json is dynamically typed.
        Type::Json => Err(MappingError::Unsupported("Json".to_string())),

        // Generic / unresolved
        Type::Generic(name) => Err(MappingError::Unsupported(format!("Generic({})", name))),
        Type::TypeRef(name, _) => Err(MappingError::Unsupported(format!("TypeRef({})", name))),

        // Any is inherently unverifiable.
        Type::Any => Err(MappingError::Unsupported("Any".to_string())),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_int() {
        assert_eq!(
            type_to_smt_sort(&Type::Int, None).unwrap(),
            SmtSort::IntSort
        );
    }

    #[test]
    fn primitive_float() {
        assert_eq!(
            type_to_smt_sort(&Type::Float, None).unwrap(),
            SmtSort::RealSort,
        );
    }

    #[test]
    fn primitive_bool() {
        assert_eq!(
            type_to_smt_sort(&Type::Bool, None).unwrap(),
            SmtSort::BoolSort,
        );
    }

    #[test]
    fn primitive_string() {
        assert_eq!(
            type_to_smt_sort(&Type::String, None).unwrap(),
            SmtSort::StringSort,
        );
    }

    #[test]
    fn list_maps_to_array() {
        let sort = type_to_smt_sort(&Type::List(Box::new(Type::Int)), None).unwrap();
        assert_eq!(
            sort,
            SmtSort::ArraySort {
                index: Box::new(SmtSort::IntSort),
                element: Box::new(SmtSort::IntSort),
            },
        );
    }

    #[test]
    fn set_maps_to_array_of_bool() {
        let sort = type_to_smt_sort(&Type::Set(Box::new(Type::String)), None).unwrap();
        assert_eq!(
            sort,
            SmtSort::ArraySort {
                index: Box::new(SmtSort::StringSort),
                element: Box::new(SmtSort::BoolSort),
            },
        );
    }

    #[test]
    fn map_maps_to_array() {
        let sort = type_to_smt_sort(
            &Type::Map(Box::new(Type::String), Box::new(Type::Int)),
            None,
        )
        .unwrap();
        assert_eq!(
            sort,
            SmtSort::ArraySort {
                index: Box::new(SmtSort::StringSort),
                element: Box::new(SmtSort::IntSort),
            },
        );
    }

    #[test]
    fn tuple_sort() {
        let sort = type_to_smt_sort(&Type::Tuple(vec![Type::Int, Type::Bool]), None).unwrap();
        assert_eq!(
            sort,
            SmtSort::TupleSort(vec![SmtSort::IntSort, SmtSort::BoolSort]),
        );
    }

    #[test]
    fn result_maps_to_datatype() {
        let sort = type_to_smt_sort(
            &Type::Result(Box::new(Type::Int), Box::new(Type::String)),
            None,
        )
        .unwrap();
        match sort {
            SmtSort::DataTypeSort { name, variants } => {
                assert_eq!(name, "Result");
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].name, "Ok");
                assert_eq!(variants[1].name, "Err");
            }
            other => panic!("expected DataTypeSort, got {:?}", other),
        }
    }

    #[test]
    fn unsupported_any() {
        assert!(type_to_smt_sort(&Type::Any, None).is_err());
    }

    #[test]
    fn unsupported_fn() {
        let ty = Type::Fn(vec![Type::Int], Box::new(Type::Bool));
        assert!(type_to_smt_sort(&ty, None).is_err());
    }

    #[test]
    fn unsupported_json() {
        assert!(type_to_smt_sort(&Type::Json, None).is_err());
    }

    #[test]
    fn bytes_maps_to_bitvec() {
        assert_eq!(
            type_to_smt_sort(&Type::Bytes, None).unwrap(),
            SmtSort::BitVecSort(8),
        );
    }
}
