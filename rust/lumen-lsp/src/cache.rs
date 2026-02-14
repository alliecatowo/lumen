//! Compilation cache to avoid recompiling on every request

use lsp_types::Uri;
use lumen_compiler::compiler::ast::Program;
use lumen_compiler::compiler::resolve::SymbolTable;
use std::collections::HashMap;

pub struct CompilationCache {
    entries: HashMap<Uri, CacheEntry>,
}

struct CacheEntry {
    text: String,
    program: Option<Program>,
    symbols: Option<SymbolTable>,
}

impl CompilationCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn update(
        &mut self,
        uri: Uri,
        text: String,
        program: Option<Program>,
        symbols: Option<SymbolTable>,
    ) {
        self.entries.insert(
            uri,
            CacheEntry {
                text,
                program,
                symbols,
            },
        );
    }

    pub fn get_text(&self, uri: &Uri) -> Option<&String> {
        self.entries.get(uri).map(|e| &e.text)
    }

    pub fn get_program(&self, uri: &Uri) -> Option<&Program> {
        self.entries.get(uri).and_then(|e| e.program.as_ref())
    }

    pub fn get_symbols(&self, uri: &Uri) -> Option<&SymbolTable> {
        self.entries.get(uri).and_then(|e| e.symbols.as_ref())
    }
}
