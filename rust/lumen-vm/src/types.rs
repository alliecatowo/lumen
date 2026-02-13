//! Runtime type registry for schema validation.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RuntimeType {
    pub name: String,
    pub kind: RuntimeTypeKind,
}

#[derive(Debug, Clone)]
pub enum RuntimeTypeKind {
    Record(Vec<RuntimeField>),
    Enum(Vec<RuntimeVariant>),
}

#[derive(Debug, Clone)]
pub struct RuntimeField {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeVariant {
    pub name: String,
    pub payload: Option<String>,
}

#[derive(Debug, Default)]
pub struct TypeTable {
    types: HashMap<String, RuntimeType>,
}

impl TypeTable {
    pub fn new() -> Self { Self::default() }

    pub fn register(&mut self, ty: RuntimeType) {
        self.types.insert(ty.name.clone(), ty);
    }

    pub fn get(&self, name: &str) -> Option<&RuntimeType> {
        self.types.get(name)
    }
}
