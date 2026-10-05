//! Lumen Language Server Protocol implementation
//!
//! Provides IDE features: diagnostics, completion, hover, go-to-definition,
//! semantic tokens, inlay hints, rename, code actions, and more.

mod cache;
mod code_actions;
mod completion;
pub mod dap;
mod diagnostics;
mod document_symbols;
mod folding_ranges;
mod formatting;
mod goto_definition;
mod hover;
mod implementations;
mod inlay_hints;
mod position;
mod rename;
mod semantic_tokens;
mod signature_help;

use cache::CompilationCache;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::*;
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::markdown::extract::extract_blocks;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

fn main() {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into(), ":".into()]),
            ..Default::default()
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: vec![
                        SemanticTokenType::KEYWORD,     // 0
                        SemanticTokenType::TYPE,        // 1
                        SemanticTokenType::FUNCTION,    // 2
                        SemanticTokenType::VARIABLE,    // 3
                        SemanticTokenType::PARAMETER,   // 4
                        SemanticTokenType::OPERATOR,    // 5
                        SemanticTokenType::STRING,      // 6
                        SemanticTokenType::NUMBER,      // 7
                        SemanticTokenType::COMMENT,     // 8
                        SemanticTokenType::ENUM_MEMBER, // 9
                        SemanticTokenType::STRUCT,      // 10
                        SemanticTokenType::ENUM,        // 11
                        SemanticTokenType::DECORATOR,   // 12
                    ],
                    token_modifiers: vec![
                        SemanticTokenModifier::DECLARATION,
                        SemanticTokenModifier::DEFINITION,
                        SemanticTokenModifier::READONLY,
                        SemanticTokenModifier::STATIC,
                    ],
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(false),
                ..Default::default()
            },
        )),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".into(), ",".into()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        inlay_hint_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        references_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };

    let caps_json = match serde_json::to_value(capabilities) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("lumen-lsp: cannot serialize server capabilities: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = connection.initialize(caps_json) {
        eprintln!("lumen-lsp: initialize failed: {e}");
        std::process::exit(1);
    }

    let mut cache = CompilationCache::new();
    let mut diagnostics_latency = DiagnosticsLatency::default();

    for msg in &connection.receiver {
        match msg {
            Message::Notification(not) => {
                // A bad notification must not take the whole server down.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_notification(&not, &connection, &mut cache, &mut diagnostics_latency);
                }));
                if outcome.is_err() {
                    eprintln!(
                        "lumen-lsp: panic while handling notification {}",
                        not.method
                    );
                }
            }
            Message::Request(req) => {
                match connection.handle_shutdown(&req) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("lumen-lsp: shutdown handshake failed: {e}");
                        break;
                    }
                }
                dispatch_request(&req, &connection, &cache);
            }
            _ => {}
        }
    }

    if let Err(e) = io_threads.join() {
        eprintln!("lumen-lsp: io threads ended with an error: {e}");
    }
}

thread_local! {
    /// Set by `send_response`; lets the dispatcher know whether a handler answered.
    static REPLIED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn send_response(connection: &Connection, response: Response) {
    REPLIED.with(|r| r.set(true));
    let _ = connection.sender.send(Message::Response(response));
}

/// Run a request handler so that every request gets exactly one reply and a
/// panic in a handler is reported as an error instead of killing the server.
fn dispatch_request(req: &Request, connection: &Connection, cache: &CompilationCache) {
    REPLIED.with(|r| r.set(false));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle_request(req, connection, cache)
    }));
    if REPLIED.with(|r| r.get()) {
        return;
    }
    let (code, message) = match outcome {
        Err(_) => {
            eprintln!("lumen-lsp: panic while handling {}", req.method);
            (-32603, format!("internal error handling {}", req.method))
        }
        Ok(true) => (-32602, format!("invalid params for {}", req.method)),
        Ok(false) => (-32601, format!("method not found: {}", req.method)),
    };
    let _ = connection.sender.send(Message::Response(Response::new_err(
        req.id.clone(),
        code,
        message,
    )));
}

const DIAGNOSTIC_LATENCY_WINDOW: usize = 200;
const DIAGNOSTIC_LATENCY_REPORT_EVERY: u64 = 20;

#[derive(Debug, Clone, Copy)]
enum DiagnosticAction {
    Recompiled,
    ReusedCache,
}

impl DiagnosticAction {
    fn label(self) -> &'static str {
        match self {
            Self::Recompiled => "recompiled",
            Self::ReusedCache => "reused_cache",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DocumentEvent {
    Open,
    Change,
    Save,
}

impl DocumentEvent {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Change => "change",
            Self::Save => "save",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EditLineSpan {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, Default)]
struct ChangeContext {
    ranged_line_spans: Vec<EditLineSpan>,
    saw_full_content_replace: bool,
    inserted_key_path_markers: bool,
}

#[derive(Default)]
struct DiagnosticsLatency {
    compile_samples_ms: VecDeque<f64>,
    cache_samples_ms: VecDeque<f64>,
    events_seen: u64,
}

impl DiagnosticsLatency {
    fn record(&mut self, action: DiagnosticAction, event: DocumentEvent, elapsed: Duration) {
        let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
        match action {
            DiagnosticAction::Recompiled => {
                Self::push_sample(&mut self.compile_samples_ms, elapsed_ms);
            }
            DiagnosticAction::ReusedCache => {
                Self::push_sample(&mut self.cache_samples_ms, elapsed_ms);
            }
        }

        self.events_seen += 1;
        if !self
            .events_seen
            .is_multiple_of(DIAGNOSTIC_LATENCY_REPORT_EVERY)
        {
            return;
        }

        let compile_p50 = Self::percentile(&self.compile_samples_ms, 0.50);
        let compile_p95 = Self::percentile(&self.compile_samples_ms, 0.95);
        let cache_p50 = Self::percentile(&self.cache_samples_ms, 0.50);
        let cache_p95 = Self::percentile(&self.cache_samples_ms, 0.95);

        eprintln!(
            "[lumen-lsp][diag-latency] event={} action={} latency_ms={:.2} compile_n={} compile_p50_ms={} compile_p95_ms={} cache_n={} cache_p50_ms={} cache_p95_ms={}",
            event.label(),
            action.label(),
            elapsed_ms,
            self.compile_samples_ms.len(),
            format_metric(compile_p50),
            format_metric(compile_p95),
            self.cache_samples_ms.len(),
            format_metric(cache_p50),
            format_metric(cache_p95),
        );
    }

    fn push_sample(samples: &mut VecDeque<f64>, value: f64) {
        samples.push_back(value);
        if samples.len() > DIAGNOSTIC_LATENCY_WINDOW {
            let _ = samples.pop_front();
        }
    }

    fn percentile(samples: &VecDeque<f64>, percentile: f64) -> Option<f64> {
        if samples.is_empty() {
            return None;
        }

        let mut sorted: Vec<f64> = samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

        let rank = ((sorted.len().saturating_sub(1)) as f64 * percentile).round() as usize;
        sorted.get(rank).copied()
    }
}

fn format_metric(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn handle_notification(
    not: &Notification,
    connection: &Connection,
    cache: &mut CompilationCache,
    diagnostics_latency: &mut DiagnosticsLatency,
) {
    if not.method == notification::DidOpenTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();
            let text = params.text_document.text.clone();

            process_document(
                &uri,
                &text,
                cache,
                connection,
                DocumentEvent::Open,
                None,
                diagnostics_latency,
            );
        }
    } else if not.method == notification::DidChangeTextDocument::METHOD {
        if let Ok(params) =
            serde_json::from_value::<DidChangeTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();
            let previous_text = cache.get_text(&uri).cloned().unwrap_or_default();
            let content_changes = params.content_changes;

            match apply_text_document_changes(&previous_text, &content_changes) {
                Some((text, change_context)) => {
                    process_document(
                        &uri,
                        &text,
                        cache,
                        connection,
                        DocumentEvent::Change,
                        Some(change_context),
                        diagnostics_latency,
                    );
                }
                None => {
                    eprintln!(
                        "[lumen-lsp][diagnostics] failed to apply incremental changes; falling back to full replacement when available"
                    );

                    if let Some(full_change) = content_changes
                        .iter()
                        .rev()
                        .find(|change| change.range.is_none())
                    {
                        let fallback_context = ChangeContext {
                            ranged_line_spans: Vec::new(),
                            saw_full_content_replace: true,
                            inserted_key_path_markers: text_may_affect_markdown_key_paths(
                                &full_change.text,
                            ),
                        };

                        process_document(
                            &uri,
                            &full_change.text,
                            cache,
                            connection,
                            DocumentEvent::Change,
                            Some(fallback_context),
                            diagnostics_latency,
                        );
                    }
                }
            }
        }
    } else if not.method == notification::DidSaveTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();

            // Re-run full compilation on save to ensure fresh diagnostics
            if let Some(text) = cache.get_text(&uri) {
                let text_owned = text.clone();
                process_document(
                    &uri,
                    &text_owned,
                    cache,
                    connection,
                    DocumentEvent::Save,
                    None,
                    diagnostics_latency,
                );
            }
        }
    } else if not.method == notification::DidCloseTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(not.params.clone())
        {
            // Drop the cached document and clear its diagnostics so closed files
            // neither leak memory nor leave stale squiggles in the Problems panel.
            let uri = params.text_document.uri;
            cache.remove(&uri);
            publish_diagnostics(connection, uri, Vec::new());
        }
    }
}

/// Process a document: compile it and publish diagnostics
fn process_document(
    uri: &Uri,
    text: &str,
    cache: &mut CompilationCache,
    connection: &Connection,
    event: DocumentEvent,
    change_context: Option<ChangeContext>,
    diagnostics_latency: &mut DiagnosticsLatency,
) {
    let is_markdown = uri.path().as_str().ends_with(".md");

    let can_reuse_cached = matches!(event, DocumentEvent::Change)
        && is_markdown
        && change_context
            .as_ref()
            .map(|change| can_reuse_markdown_diagnostics(change, cache.get_diagnostic_context(uri)))
            .unwrap_or(false)
        && cache.get_diagnostics(uri).is_some();

    let started = Instant::now();
    if can_reuse_cached {
        let diagnostics = cache.get_diagnostics(uri).cloned().unwrap_or_default();
        publish_diagnostics(connection, uri.clone(), diagnostics);
        diagnostics_latency.record(DiagnosticAction::ReusedCache, event, started.elapsed());
        cache.update_text_only(uri, text.to_string());
        return;
    }

    // Run full compilation
    let compile_result = if is_markdown {
        lumen_compiler::compile(text)
    } else {
        lumen_compiler::compile_raw(text)
    };

    let diagnostics = match &compile_result {
        Ok(_) => vec![],
        Err(err) => diagnostics::compile_error_to_diagnostics(err, text),
    };
    let diagnostics_for_cache = diagnostics.clone();

    // Publish diagnostics
    publish_diagnostics(connection, uri.clone(), diagnostics);
    diagnostics_latency.record(DiagnosticAction::Recompiled, event, started.elapsed());

    // Try to parse for completion/hover even if full compilation failed
    let (program, symbols) = parse_for_features(text, is_markdown);

    // Update cache
    cache.update(
        uri.clone(),
        text.to_string(),
        program,
        symbols,
        diagnostics_for_cache,
        build_diagnostic_context(text, is_markdown),
    );
}

fn build_diagnostic_context(text: &str, is_markdown: bool) -> cache::DiagnosticContext {
    if !is_markdown {
        return cache::DiagnosticContext::default();
    }

    cache::DiagnosticContext {
        markdown_relevant_max_line: markdown_relevant_max_line(text),
    }
}

fn markdown_relevant_max_line(text: &str) -> Option<u32> {
    let extracted = extract_blocks(text);
    let mut max_line: Option<u32> = None;

    for directive in extracted.directives {
        let line = to_u32_saturating(directive.span.line.saturating_sub(1));
        max_line = Some(max_line.map_or(line, |current| current.max(line)));
    }

    for block in extracted.code_blocks {
        let code_line_count = to_u32_saturating(block.code.lines().count());
        let block_end_line = to_u32_saturating(block.code_start_line.saturating_sub(1))
            .saturating_add(code_line_count);
        max_line = Some(max_line.map_or(block_end_line, |current| current.max(block_end_line)));
    }

    max_line
}

fn to_u32_saturating(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn can_reuse_markdown_diagnostics(
    change: &ChangeContext,
    previous_context: Option<cache::DiagnosticContext>,
) -> bool {
    if change.saw_full_content_replace
        || change.inserted_key_path_markers
        || change.ranged_line_spans.is_empty()
    {
        return false;
    }

    let Some(max_relevant_line) = previous_context.and_then(|ctx| ctx.markdown_relevant_max_line)
    else {
        return true;
    };

    change
        .ranged_line_spans
        .iter()
        .all(|span| span.start > max_relevant_line && span.end >= span.start)
}

fn text_may_affect_markdown_key_paths(inserted_text: &str) -> bool {
    inserted_text.contains("```") || inserted_text.contains('@')
}

fn apply_text_document_changes(
    previous_text: &str,
    content_changes: &[TextDocumentContentChangeEvent],
) -> Option<(String, ChangeContext)> {
    let mut updated = previous_text.to_string();
    let mut context = ChangeContext::default();

    for change in content_changes {
        context.inserted_key_path_markers |= text_may_affect_markdown_key_paths(&change.text);

        if let Some(range) = change.range {
            let start = lsp_position_to_byte_offset(&updated, range.start)?;
            let end = lsp_position_to_byte_offset(&updated, range.end)?;
            if start > end || end > updated.len() {
                return None;
            }

            updated.replace_range(start..end, &change.text);
            context.ranged_line_spans.push(EditLineSpan {
                start: range.start.line,
                end: range.end.line.max(range.start.line),
            });
        } else {
            updated = change.text.clone();
            context.saw_full_content_replace = true;
            context.ranged_line_spans.clear();
        }
    }

    Some((updated, context))
}

/// Byte offset of an LSP position, clamped like the spec says: a column past
/// the end of the line means the end of the line, and a line past the end of the
/// document means the end of the document.
fn lsp_position_to_byte_offset(text: &str, position: Position) -> Option<usize> {
    let Some(line_start) = line_start_offset(text, position.line) else {
        return Some(text.len());
    };
    let line_end = text[line_start..]
        .find('\n')
        .map(|idx| line_start + idx)
        .unwrap_or(text.len());
    // Keep a CRLF terminator out of the line so a clamped column stays before it.
    let line_slice = text[line_start..line_end].trim_end_matches('\r');
    Some(line_start + position::utf16_col_to_byte(line_slice, position.character))
}

fn line_start_offset(text: &str, line: u32) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }

    let mut current_line = 0;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            current_line += 1;
            if current_line == line {
                return Some(idx + 1);
            }
        }
    }

    None
}

/// Parse source to extract AST and symbols for IDE features
fn parse_for_features(
    text: &str,
    is_markdown: bool,
) -> (
    Option<lumen_compiler::compiler::ast::Program>,
    Option<lumen_compiler::compiler::resolve::SymbolTable>,
) {
    let (code, first_line, first_offset) = if is_markdown {
        let extracted = extract_blocks(text);
        let mut full_code = String::new();
        let mut first_block_line = 1;
        let mut first_block_offset = 0;

        for (i, block) in extracted.code_blocks.iter().enumerate() {
            if i == 0 {
                first_block_line = block.code_start_line;
                first_block_offset = block.code_offset;
            }
            if !full_code.is_empty() {
                full_code.push('\n');
            }
            full_code.push_str(&block.code);
        }

        if full_code.is_empty() {
            return (None, None);
        }

        // Extract directives
        let _directives: Vec<lumen_compiler::compiler::ast::Directive> = extracted
            .directives
            .iter()
            .map(|d| lumen_compiler::compiler::ast::Directive {
                name: d.name.clone(),
                value: d.value.clone(),
                span: d.span,
            })
            .collect();

        // Return code with directives
        (full_code, first_block_line, first_block_offset)
    } else {
        (text.to_string(), 1, 0)
    };

    // Lex
    let mut lexer = Lexer::new(&code, first_line, first_offset);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return (None, None),
    };

    // Parse (use empty directives for raw, we already extracted them for markdown)
    let mut parser = Parser::new(tokens);
    let directives = if is_markdown {
        // Extract directives again for the parser
        let extracted = extract_blocks(text);
        extracted
            .directives
            .iter()
            .map(|d| lumen_compiler::compiler::ast::Directive {
                name: d.name.clone(),
                value: d.value.clone(),
                span: d.span,
            })
            .collect()
    } else {
        vec![]
    };

    let program = match parser.parse_program(directives) {
        Ok(p) => p,
        Err(_) => return (None, None),
    };

    // Resolve symbols (best effort)
    let symbols = lumen_compiler::compiler::resolve::resolve(&program).ok();

    (Some(program), symbols)
}

fn publish_diagnostics(connection: &Connection, uri: Uri, diagnostics: Vec<Diagnostic>) {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let not = Notification::new(notification::PublishDiagnostics::METHOD.to_string(), params);
    let _ = connection.sender.send(Message::Notification(not));
}

/// Returns false when the method is not one this server implements.
fn handle_request(req: &Request, connection: &Connection, cache: &CompilationCache) -> bool {
    match req.method.as_str() {
        request::GotoDefinition::METHOD => {
            if let Ok(params) = serde_json::from_value::<GotoDefinitionParams>(req.params.clone()) {
                let uri = params
                    .text_document_position_params
                    .text_document
                    .uri
                    .clone();
                let text = cache.get_text(&uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(&uri);

                let result = goto_definition::build_goto_definition(params, text, program, &uri);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::GotoImplementation::METHOD => {
            if let Ok(params) = serde_json::from_value::<GotoDefinitionParams>(req.params.clone()) {
                let uri = params
                    .text_document_position_params
                    .text_document
                    .uri
                    .clone();
                let position = params.text_document_position_params.position;
                let text = cache.get_text(&uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(&uri);

                let result = implementations::build_implementations(position, text, program, &uri);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::HoverRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<HoverParams>(req.params.clone()) {
                let uri = &params.text_document_position_params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(uri);

                let result = hover::build_hover(params, text, program);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::Completion::METHOD => {
            if let Ok(params) = serde_json::from_value::<CompletionParams>(req.params.clone()) {
                let uri = &params.text_document_position.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(uri);

                let result = completion::build_completion(params, text, program);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(serde_json::to_value(result).unwrap()),
                };
                send_response(connection, response);
            }
        }
        request::SemanticTokensFullRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<SemanticTokensParams>(req.params.clone()) {
                let uri = &params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let is_markdown = uri.path().as_str().ends_with(".md");

                let result = semantic_tokens::build_semantic_tokens(text, is_markdown);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::InlayHintRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<InlayHintParams>(req.params.clone()) {
                let uri = &params.text_document.uri;
                let program = cache.get_program(uri);
                let symbols = cache.get_symbols(uri);

                let result = inlay_hints::build_inlay_hints(params, program, symbols);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(serde_json::to_value(result).unwrap()),
                };
                send_response(connection, response);
            }
        }
        request::DocumentSymbolRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<DocumentSymbolParams>(req.params.clone()) {
                let uri = &params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(uri);

                let result = document_symbols::build_document_symbols(params, text, program);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::SignatureHelpRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<SignatureHelpParams>(req.params.clone()) {
                let uri = &params.text_document_position_params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(uri);

                let result = signature_help::build_signature_help(params, text, program);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(
                        serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                    ),
                };
                send_response(connection, response);
            }
        }
        request::CodeActionRequest::METHOD => {
            let result = if let Ok(params) =
                serde_json::from_value::<CodeActionParams>(req.params.clone())
            {
                let uri = params.text_document.uri;
                let text = cache.get_text(&uri).cloned().unwrap_or_default();
                let actions =
                    code_actions::build_code_actions(&uri, &text, &params.context.diagnostics);
                Some(serde_json::to_value(actions).unwrap())
            } else {
                Some(serde_json::to_value(Vec::<CodeAction>::new()).unwrap())
            };
            let response = Response {
                id: req.id.clone(),
                response_result: Ok(result.unwrap_or(serde_json::Value::Null)),
            };
            send_response(connection, response);
        }
        request::FoldingRangeRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<FoldingRangeParams>(req.params.clone()) {
                let uri = &params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(uri);

                let result = folding_ranges::build_folding_ranges(params, text, program);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(serde_json::to_value(result).unwrap()),
                };
                send_response(connection, response);
            }
        }
        request::Formatting::METHOD => {
            if let Ok(params) =
                serde_json::from_value::<DocumentFormattingParams>(req.params.clone())
            {
                let uri = params.text_document.uri.clone();
                let text = cache.get_text(&uri).map(|s| s.as_str()).unwrap_or("");
                let uri_path = uri.path().as_str().to_string();

                let result = formatting::build_formatting(params, text, &uri_path);

                let response = Response {
                    id: req.id.clone(),
                    response_result: Ok(serde_json::to_value(result).unwrap()),
                };
                send_response(connection, response);
            }
        }
        request::References::METHOD => {
            let response = Response {
                id: req.id.clone(),
                response_result: Ok(serde_json::to_value(Vec::<Location>::new()).unwrap()),
            };
            send_response(connection, response);
        }
        request::WorkspaceSymbolRequest::METHOD => {
            let response = Response {
                id: req.id.clone(),
                response_result: Ok(serde_json::to_value(Vec::<SymbolInformation>::new()).unwrap()),
            };
            send_response(connection, response);
        }
        request::Rename::METHOD => {
            let result =
                if let Ok(params) = serde_json::from_value::<RenameParams>(req.params.clone()) {
                    let uri = params.text_document_position.text_document.uri.clone();
                    let text = cache.get_text(&uri).cloned().unwrap_or_default();
                    let program = cache.get_program(&uri);
                    let position = params.text_document_position.position;

                    rename::rename_symbol(&uri, &text, position, &params.new_name, program)
                } else {
                    None
                };
            let response = Response {
                id: req.id.clone(),
                response_result: Ok(serde_json::to_value(result).unwrap()),
            };
            send_response(connection, response);
        }
        request::PrepareRenameRequest::METHOD => {
            let result = if let Ok(params) =
                serde_json::from_value::<TextDocumentPositionParams>(req.params.clone())
            {
                let uri = params.text_document.uri.clone();
                let text = cache.get_text(&uri).cloned().unwrap_or_default();
                let program = cache.get_program(&uri);

                rename::prepare_rename(&text, params.position, program)
            } else {
                None
            };
            let response = Response {
                id: req.id.clone(),
                response_result: Ok(serde_json::to_value(result).unwrap()),
            };
            send_response(connection, response);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranged_change(
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: start_line,
                    character: start_char,
                },
                end: Position {
                    line: end_line,
                    character: end_char,
                },
            }),
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn applies_ranged_changes_incrementally() {
        let previous = "alpha\nbeta\ngamma";
        let changes = vec![ranged_change(1, 0, 1, 4, "BETA")];

        let (updated, context) = apply_text_document_changes(previous, &changes).unwrap();
        assert_eq!(updated, "alpha\nBETA\ngamma");
        assert!(!context.saw_full_content_replace);
        assert_eq!(
            context.ranged_line_spans,
            vec![EditLineSpan { start: 1, end: 1 }]
        );
    }

    #[test]
    fn utf16_positions_resolve_to_byte_offsets() {
        let text = "a🙂b\n";

        assert_eq!(
            lsp_position_to_byte_offset(
                text,
                Position {
                    line: 0,
                    character: 1
                }
            ),
            Some(1)
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                text,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            Some(5)
        );
        assert_eq!(
            lsp_position_to_byte_offset(
                text,
                Position {
                    line: 0,
                    character: 2
                }
            ),
            // Between the surrogates of the emoji: snap back to its start.
            Some(1)
        );
    }

    #[test]
    fn out_of_range_positions_clamp_instead_of_dropping_the_edit() {
        let text = "ab\ncd\n";
        // Column past the end of line 0 clamps to the end of that line (before the newline).
        assert_eq!(
            lsp_position_to_byte_offset(
                text,
                Position {
                    line: 0,
                    character: 99
                }
            ),
            Some(2)
        );
        // Line past the end of the document clamps to the end of the document.
        assert_eq!(
            lsp_position_to_byte_offset(
                text,
                Position {
                    line: 99,
                    character: 0
                }
            ),
            Some(text.len())
        );
        // CRLF documents keep the clamped column before the \r.
        assert_eq!(
            lsp_position_to_byte_offset(
                "ab\r\ncd",
                Position {
                    line: 0,
                    character: 99
                }
            ),
            Some(2)
        );
    }

    #[test]
    fn edits_with_out_of_range_positions_are_still_applied() {
        let changes = [TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 50,
                },
            }),
            range_length: None,
            text: "X".to_string(),
        }];
        let (updated, _) = apply_text_document_changes("abc\ndef", &changes).expect("applies");
        assert_eq!(updated, "aX\ndef");
    }

    #[test]
    fn reuse_decision_requires_trailing_non_keypath_edits() {
        let change = ChangeContext {
            ranged_line_spans: vec![EditLineSpan { start: 50, end: 50 }],
            saw_full_content_replace: false,
            inserted_key_path_markers: false,
        };
        let context = Some(cache::DiagnosticContext {
            markdown_relevant_max_line: Some(20),
        });
        assert!(can_reuse_markdown_diagnostics(&change, context));

        let keypath_change = ChangeContext {
            inserted_key_path_markers: true,
            ..change
        };
        assert!(!can_reuse_markdown_diagnostics(&keypath_change, context));
    }

    #[test]
    fn markdown_relevant_max_line_tracks_last_directive_or_code_fence() {
        let markdown = r#"@package "demo"

Text.
```lumen
cell main() -> Int
  return 1
end
```
"#;

        assert_eq!(markdown_relevant_max_line(markdown), Some(7));
        assert_eq!(markdown_relevant_max_line("Just prose\n\nNo code"), None);
    }

    #[test]
    fn percentile_uses_sorted_sample_rank() {
        let mut samples = VecDeque::new();
        samples.extend([1.0, 2.0, 3.0, 4.0, 5.0]);

        assert_eq!(DiagnosticsLatency::percentile(&samples, 0.50), Some(3.0));
        assert_eq!(DiagnosticsLatency::percentile(&samples, 0.95), Some(5.0));
    }
}
