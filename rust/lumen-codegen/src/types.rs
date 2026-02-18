//! Mapping from Lumen types to Cranelift IR types.
//!
//! With NaN-boxing, ALL values are stored as I64 in registers:
//!   Int   -> I64  (NaN-boxed: `(val << 1) | 1`)
//!   Float -> I64  (NaN-boxed: raw f64 bits bitcast to i64)
//!   Bool  -> I64  (NaN-boxed: quiet NaN payloads)
//!   Null  -> I64  (NaN-boxed: canonical quiet NaN sentinel)
//!
//! Heap-allocated types (String, List, etc.) are also I64 (raw pointers).

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type as ClifType;
use lumen_compiler::compiler::typecheck::Type as LumenType;

/// Convert a Lumen type to the corresponding Cranelift IR type.
///
/// With NaN-boxing, every value is uniformly represented as I64.
/// The `pointer_type` parameter is kept for API compatibility but
/// all types map to I64 on 64-bit targets.
pub fn lumen_type_to_cl_type(ty: &LumenType, pointer_type: ClifType) -> ClifType {
    match ty {
        LumenType::Int => types::I64,
        LumenType::Float => types::I64, // NaN-boxed: f64 bits as i64
        LumenType::Bool => types::I64,  // NaN-boxed: quiet NaN payloads
        LumenType::Null => types::I64,
        // Everything else is a pointer to a heap object (also I64).
        LumenType::String
        | LumenType::Bytes
        | LumenType::Json
        | LumenType::List(_)
        | LumenType::Map(_, _)
        | LumenType::Record(_)
        | LumenType::Enum(_)
        | LumenType::Result(_, _)
        | LumenType::Union(_)
        | LumenType::Tuple(_)
        | LumenType::Set(_)
        | LumenType::Fn(_, _)
        | LumenType::Generic(_)
        | LumenType::TypeRef(_, _)
        | LumenType::Any => pointer_type,
    }
}

/// Parse a Lumen type string (as stored in LIR metadata) into a Cranelift type.
///
/// With NaN-boxing, Int/Float/Bool/Null all become I64.
pub fn lir_type_str_to_cl_type(ty_str: &str, pointer_type: ClifType) -> ClifType {
    match ty_str {
        "Int" => types::I64,
        "Float" => types::I64, // NaN-boxed
        "Bool" => types::I64,  // NaN-boxed
        "Null" => types::I64,
        _ => pointer_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PTR: ClifType = types::I64;

    #[test]
    fn primitive_type_mapping() {
        // With NaN-boxing, all primitives are I64
        assert_eq!(lumen_type_to_cl_type(&LumenType::Int, PTR), types::I64);
        assert_eq!(lumen_type_to_cl_type(&LumenType::Float, PTR), types::I64);
        assert_eq!(lumen_type_to_cl_type(&LumenType::Bool, PTR), types::I64);
        assert_eq!(lumen_type_to_cl_type(&LumenType::Null, PTR), types::I64);
    }

    #[test]
    fn complex_types_become_pointers() {
        assert_eq!(lumen_type_to_cl_type(&LumenType::String, PTR), PTR);
        assert_eq!(
            lumen_type_to_cl_type(&LumenType::List(Box::new(LumenType::Int)), PTR),
            PTR
        );
        assert_eq!(
            lumen_type_to_cl_type(
                &LumenType::Map(Box::new(LumenType::String), Box::new(LumenType::Int)),
                PTR
            ),
            PTR
        );
        assert_eq!(
            lumen_type_to_cl_type(&LumenType::Record("Foo".to_string()), PTR),
            PTR
        );
        assert_eq!(lumen_type_to_cl_type(&LumenType::Any, PTR), PTR);
    }

    #[test]
    fn lir_string_type_mapping() {
        // With NaN-boxing, Float and Bool also map to I64
        assert_eq!(lir_type_str_to_cl_type("Int", PTR), types::I64);
        assert_eq!(lir_type_str_to_cl_type("Float", PTR), types::I64);
        assert_eq!(lir_type_str_to_cl_type("Bool", PTR), types::I64);
        assert_eq!(lir_type_str_to_cl_type("String", PTR), PTR);
        assert_eq!(lir_type_str_to_cl_type("SomeRecord", PTR), PTR);
    }
}
