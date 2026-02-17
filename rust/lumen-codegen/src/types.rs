//! Mapping from Lumen types to Cranelift IR types.

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type as ClifType;
use lumen_compiler::compiler::typecheck::Type as LumenType;

/// Convert a Lumen type to the corresponding Cranelift IR type.
///
/// Primitive scalars map directly:
///   Int   -> I64
///   Float -> F64
///   Bool  -> I8 (Cranelift has no i1)
///
/// All heap-allocated / complex types (String, List, Map, Record, etc.)
/// are represented as opaque pointers (I64 on 64-bit targets) for now.
/// A future GC/runtime integration pass will refine these.
pub fn lumen_type_to_cl_type(ty: &LumenType, pointer_type: ClifType) -> ClifType {
    match ty {
        LumenType::Int => types::I64,
        LumenType::Float => types::F64,
        LumenType::Bool => types::I8,
        // Null is a sentinel integer value (0).
        LumenType::Null => types::I64,
        // Everything else is a pointer to a heap object.
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
/// LIR cells carry return types as plain strings like "Int", "Float", "Bool", etc.
/// This provides a quick mapping for the common cases used during lowering.
pub fn lir_type_str_to_cl_type(ty_str: &str, pointer_type: ClifType) -> ClifType {
    match ty_str {
        "Int" => types::I64,
        "Float" => types::F64,
        "Bool" => types::I8,
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
        assert_eq!(lumen_type_to_cl_type(&LumenType::Int, PTR), types::I64);
        assert_eq!(lumen_type_to_cl_type(&LumenType::Float, PTR), types::F64);
        assert_eq!(lumen_type_to_cl_type(&LumenType::Bool, PTR), types::I8);
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
        assert_eq!(lir_type_str_to_cl_type("Int", PTR), types::I64);
        assert_eq!(lir_type_str_to_cl_type("Float", PTR), types::F64);
        assert_eq!(lir_type_str_to_cl_type("Bool", PTR), types::I8);
        assert_eq!(lir_type_str_to_cl_type("String", PTR), PTR);
        assert_eq!(lir_type_str_to_cl_type("SomeRecord", PTR), PTR);
    }
}
