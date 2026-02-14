//! Lumen Language Server Protocol implementation
//!
//! Provides IDE features: diagnostics, completion, hover, go-to-definition,
//! semantic tokens, inlay hints, and more.

mod cache;
mod completion;
mod diagnostics;
mod goto_definition;
mod hover;
mod inlay_hints;
mod semantic_tokens;

use cache::CompilationCache;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::*;
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::markdown::extract::extract_blocks;

fn main() {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
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
                        SemanticTokenType::KEYWORD,      // 0
                        SemanticTokenType::TYPE,         // 1
                        SemanticTokenType::FUNCTION,     // 2
                        SemanticTokenType::VARIABLE,     // 3
                        SemanticTokenType::PARAMETER,    // 4
                        SemanticTokenType::OPERATOR,     // 5
                        SemanticTokenType::STRING,       // 6
                        SemanticTokenType::NUMBER,       // 7
                        SemanticTokenType::COMMENT,      // 8
                        SemanticTokenType::ENUM_MEMBER,  // 9
                        SemanticTokenType::STRUCT,       // 10
                        SemanticTokenType::ENUM,         // 11
                        SemanticTokenType::DECORATOR,    // 12
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
        document_formatting_provider: Some(OneOf::Left(false)), // Formatter not implemented via LSP
        references_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };

    let caps_json = serde_json::to_value(capabilities).unwrap();
    let _init_params = connection.initialize(caps_json).unwrap();

    let mut cache = CompilationCache::new();

    for msg in &connection.receiver {
        match msg {
            Message::Notification(not) => {
                handle_notification(&not, &connection, &mut cache);
            }
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap() {
                    break;
                }
                handle_request(&req, &connection, &cache);
            }
            _ => {}
        }
    }

    io_threads.join().unwrap();
}

fn handle_notification(not: &Notification, connection: &Connection, cache: &mut CompilationCache) {
    if not.method == notification::DidOpenTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();
            let text = params.text_document.text.clone();

            process_document(&uri, &text, cache, connection);
        }
    } else if not.method == notification::DidChangeTextDocument::METHOD {
        if let Ok(params) =
            serde_json::from_value::<DidChangeTextDocumentParams>(not.params.clone())
        {
            if let Some(change) = params.content_changes.into_iter().last() {
                let uri = params.text_document.uri.clone();
                let text = change.text.clone();

                process_document(&uri, &text, cache, connection);
            }
        }
    } else if not.method == notification::DidSaveTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();

            // Re-run full compilation on save to ensure fresh diagnostics
            if let Some(text) = cache.get_text(&uri) {
                let text_owned = text.clone();
                process_document(&uri, &text_owned, cache, connection);
            }
        }
    }
}

/// Process a document: compile it and publish diagnostics
fn process_document(
    uri: &Uri,
    text: &str,
    cache: &mut CompilationCache,
    connection: &Connection,
) {
    let is_markdown = uri.path().as_str().ends_with(".md");

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

    // Publish diagnostics
    publish_diagnostics(connection, uri.clone(), diagnostics);

    // Try to parse for completion/hover even if full compilation failed
    let (program, symbols) = parse_for_features(text, is_markdown);

    // Update cache
    cache.update(uri.clone(), text.to_string(), program, symbols);
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
        let directives: Vec<lumen_compiler::compiler::ast::Directive> = extracted
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

fn handle_request(req: &Request, connection: &Connection, cache: &CompilationCache) {
    match req.method.as_str() {
        request::GotoDefinition::METHOD => {
            if let Ok(params) = serde_json::from_value::<GotoDefinitionParams>(req.params.clone())
            {
                let uri = params.text_document_position_params.text_document.uri.clone();
                let text = cache.get_text(&uri).map(|s| s.as_str()).unwrap_or("");
                let program = cache.get_program(&uri);

                let result = goto_definition::build_goto_definition(params, text, program, &uri);

                let response = Response {
                    id: req.id.clone(),
                    result: serde_json::to_value(result).ok(),
                    error: None,
                };
                let _ = connection.sender.send(Message::Response(response));
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
                    result: serde_json::to_value(result).ok(),
                    error: None,
                };
                let _ = connection.sender.send(Message::Response(response));
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
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                };
                let _ = connection.sender.send(Message::Response(response));
            }
        }
        request::SemanticTokensFullRequest::METHOD => {
            if let Ok(params) = serde_json::from_value::<SemanticTokensParams>(req.params.clone())
            {
                let uri = &params.text_document.uri;
                let text = cache.get_text(uri).map(|s| s.as_str()).unwrap_or("");
                let is_markdown = uri.path().as_str().ends_with(".md");

                let result = semantic_tokens::build_semantic_tokens(text, is_markdown);

                let response = Response {
                    id: req.id.clone(),
                    result: serde_json::to_value(result).ok(),
                    error: None,
                };
                let _ = connection.sender.send(Message::Response(response));
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
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                };
                let _ = connection.sender.send(Message::Response(response));
            }
        }
        // Stub handlers for other features to avoid errors
        request::DocumentSymbolRequest::METHOD => {
            send_empty_response(req, connection);
        }
        request::SignatureHelpRequest::METHOD => {
            send_empty_response(req, connection);
        }
        request::CodeActionRequest::METHOD => {
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(Vec::<CodeAction>::new()).unwrap()),
                error: None,
            };
            let _ = connection.sender.send(Message::Response(response));
        }
        request::FoldingRangeRequest::METHOD => {
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(Vec::<FoldingRange>::new()).unwrap()),
                error: None,
            };
            let _ = connection.sender.send(Message::Response(response));
        }
        request::References::METHOD => {
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(Vec::<Location>::new()).unwrap()),
                error: None,
            };
            let _ = connection.sender.send(Message::Response(response));
        }
        request::WorkspaceSymbolRequest::METHOD => {
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(Vec::<SymbolInformation>::new()).unwrap()),
                error: None,
            };
            let _ = connection.sender.send(Message::Response(response));
        }
        _ => {}
    }
}

fn send_empty_response(req: &Request, connection: &Connection) {
    let response = Response {
        id: req.id.clone(),
        result: serde_json::to_value(()).ok(),
        error: None,
    };
    let _ = connection.sender.send(Message::Response(response));
}
