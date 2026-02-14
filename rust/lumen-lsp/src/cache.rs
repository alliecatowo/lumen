//! Compilation cache to avoid recompiling on every request

use lsp_types::Diagnostic;
use lsp_types::Uri;
use lumen_compiler::compiler::ast::Program;
use lumen_compiler::compiler::resolve::SymbolTable;
use std::collections::HashMap;

pub struct CompilationCache {
    entries: HashMap<Uri, CacheEntry>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiagnosticContext {
    pub markdown_relevant_max_line: Option<u32>,
}

struct CacheEntry {
    text: String,
    program: Option<Program>,
    symbols: Option<SymbolTable>,
    diagnostics: Vec<Diagnostic>,
    diagnostic_context: DiagnosticContext,
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
        diagnostics: Vec<Diagnostic>,
        diagnostic_context: DiagnosticContext,
    ) {
        self.entries.insert(
            uri,
            CacheEntry {
                text,
                program,
                symbols,
                diagnostics,
                diagnostic_context,
            },
        );
    }

    pub fn update_text_only(&mut self, uri: &Uri, text: String) {
        if let Some(entry) = self.entries.get_mut(uri) {
            entry.text = text;
            return;
        }

        self.entries.insert(
            uri.clone(),
            CacheEntry {
                text,
                program: None,
                symbols: None,
                diagnostics: Vec::new(),
                diagnostic_context: DiagnosticContext::default(),
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

    pub fn get_diagnostics(&self, uri: &Uri) -> Option<&Vec<Diagnostic>> {
        self.entries.get(uri).map(|e| &e.diagnostics)
    }

    pub fn get_diagnostic_context(&self, uri: &Uri) -> Option<DiagnosticContext> {
        self.entries.get(uri).map(|e| e.diagnostic_context)
    }
}
