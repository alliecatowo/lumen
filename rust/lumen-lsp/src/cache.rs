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
        // While the user is mid-edit the text often fails to parse. Keep the last
        // good AST/symbols so hover, completion and go-to-definition keep working.
        let (program, symbols) = match self.entries.remove(&uri) {
            Some(prev) => (program.or(prev.program), symbols.or(prev.symbols)),
            None => (program, symbols),
        };
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

    /// Forget a document (on `textDocument/didClose`).
    pub fn remove(&mut self, uri: &Uri) {
        self.entries.remove(uri);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str) -> Option<Program> {
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
        parser.parse_program(vec![]).ok()
    }

    #[test]
    fn a_failed_parse_keeps_the_last_good_program() {
        let uri: Uri = "file:///t.lm".parse().unwrap();
        let mut cache = CompilationCache::new();
        let good = program("cell main() -> Int\n  return 1\nend");
        assert!(good.is_some());
        cache.update(
            uri.clone(),
            "good".into(),
            good,
            None,
            vec![],
            DiagnosticContext::default(),
        );
        cache.update(
            uri.clone(),
            "cell main(".into(),
            None,
            None,
            vec![],
            DiagnosticContext::default(),
        );

        assert_eq!(cache.get_text(&uri).map(String::as_str), Some("cell main("));
        assert!(
            cache.get_program(&uri).is_some(),
            "last good AST was dropped"
        );
    }

    #[test]
    fn remove_forgets_the_document() {
        let uri: Uri = "file:///t.lm".parse().unwrap();
        let mut cache = CompilationCache::new();
        cache.update(
            uri.clone(),
            "x".into(),
            None,
            None,
            vec![],
            DiagnosticContext::default(),
        );
        cache.remove(&uri);
        assert!(cache.get_text(&uri).is_none());
    }
}
