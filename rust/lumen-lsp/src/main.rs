use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::*;
use lumen_compiler::compiler::ast::{Item, Program, Stmt};
use lumen_compiler::compiler::constraints::ConstraintError;
use lumen_compiler::compiler::lexer::{LexError, Lexer};
use lumen_compiler::compiler::parser::{ParseError, Parser};
use lumen_compiler::compiler::resolve::ResolveError;
use lumen_compiler::compiler::typecheck::TypeError;
use lumen_compiler::markdown::extract::extract_blocks;
use lumen_compiler::CompileError;
use std::collections::HashMap;

/// Symbol definition with location and type information
#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    kind: SymbolKind,
    location: Location,
    signature: String,
}

#[derive(Debug, Clone)]
enum SymbolKind {
    Cell,
    Record,
    Enum,
    TypeAlias,
    Process,
    Effect,
}

/// Stores open documents and their symbol indices
struct DocumentStore {
    documents: HashMap<Uri, String>,
    symbols: HashMap<Uri, Vec<Symbol>>,
}

impl DocumentStore {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
            symbols: HashMap::new(),
        }
    }

    fn update(&mut self, uri: Uri, text: String) {
        self.documents.insert(uri.clone(), text.clone());

        // Build symbol index from the source
        let symbols = build_symbol_index(&text, &uri);
        self.symbols.insert(uri, symbols);
    }

    fn get_text(&self, uri: &Uri) -> Option<&String> {
        self.documents.get(uri)
    }

    fn get_symbols(&self, uri: &Uri) -> Option<&Vec<Symbol>> {
        self.symbols.get(uri)
    }

    fn all_symbols(&self) -> Vec<Symbol> {
        self.symbols.values().flat_map(|v| v.clone()).collect()
    }
}

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
        // 1. Document Symbols
        document_symbol_provider: Some(OneOf::Left(true)),
        // 2. Semantic Tokens
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: vec![
                        SemanticTokenType::KEYWORD,
                        SemanticTokenType::TYPE,
                        SemanticTokenType::FUNCTION,
                        SemanticTokenType::VARIABLE,
                        SemanticTokenType::PARAMETER,
                        SemanticTokenType::OPERATOR,
                        SemanticTokenType::STRING,
                        SemanticTokenType::NUMBER,
                        SemanticTokenType::COMMENT,
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenType::STRUCT,
                        SemanticTokenType::ENUM,
                        SemanticTokenType::INTERFACE,
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
        // 3. Signature Help
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".into(), ",".into()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        // 4. Inlay Hints
        inlay_hint_provider: Some(OneOf::Left(true)),
        // 5. Code Actions
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        // 6. Folding Ranges
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        // 7. Document Formatting
        document_formatting_provider: Some(OneOf::Left(true)),
        // 8. Find References
        references_provider: Some(OneOf::Left(true)),
        // 9. Workspace Symbols
        workspace_symbol_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };

    let caps_json = serde_json::to_value(capabilities).unwrap();
    let _init_params = connection.initialize(caps_json).unwrap();

    let mut document_store = DocumentStore::new();

    for msg in &connection.receiver {
        match msg {
            Message::Notification(not) => {
                handle_notification(&not, &connection, &mut document_store);
            }
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap() {
                    break;
                }
                handle_request(&req, &connection, &document_store);
            }
            _ => {}
        }
    }

    io_threads.join().unwrap();
}

fn handle_notification(not: &Notification, connection: &Connection, store: &mut DocumentStore) {
    if not.method == notification::DidOpenTextDocument::METHOD {
        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(not.params.clone())
        {
            let uri = params.text_document.uri.clone();
            let text = params.text_document.text.clone();

            store.update(uri.clone(), text.clone());

            let diagnostics = diagnose(&text);
            publish(connection, uri, diagnostics);
        }
    } else if not.method == notification::DidChangeTextDocument::METHOD {
        if let Ok(params) =
            serde_json::from_value::<DidChangeTextDocumentParams>(not.params.clone())
        {
            if let Some(change) = params.content_changes.into_iter().last() {
                let uri = params.text_document.uri.clone();
                let text = change.text.clone();

                store.update(uri.clone(), text.clone());

                let diagnostics = diagnose(&text);
                publish(connection, uri, diagnostics);
            }
        }
    }
}

fn handle_request(req: &Request, connection: &Connection, store: &DocumentStore) {
    if req.method == request::GotoDefinition::METHOD {
        if let Ok(params) = serde_json::from_value::<GotoDefinitionParams>(req.params.clone()) {
            let result = handle_goto_definition(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::HoverRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<HoverParams>(req.params.clone()) {
            let result = handle_hover(params, store);
            let response = Response {
                id: req.id.clone(),
                result: serde_json::to_value(result).ok(),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::Completion::METHOD {
        if let Ok(params) = serde_json::from_value::<CompletionParams>(req.params.clone()) {
            let result = handle_completion(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::DocumentSymbolRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<DocumentSymbolParams>(req.params.clone()) {
            let result = handle_document_symbols(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::SemanticTokensFullRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<SemanticTokensParams>(req.params.clone()) {
            let result = handle_semantic_tokens(params, store);
            let response = Response {
                id: req.id.clone(),
                result: serde_json::to_value(result).ok(),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::SignatureHelpRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<SignatureHelpParams>(req.params.clone()) {
            let result = handle_signature_help(params, store);
            let response = Response {
                id: req.id.clone(),
                result: serde_json::to_value(result).ok(),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::InlayHintRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<InlayHintParams>(req.params.clone()) {
            let result = handle_inlay_hints(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::CodeActionRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<CodeActionParams>(req.params.clone()) {
            let result = handle_code_actions(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::FoldingRangeRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<FoldingRangeParams>(req.params.clone()) {
            let result = handle_folding_ranges(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::Formatting::METHOD {
        if let Ok(params) = serde_json::from_value::<DocumentFormattingParams>(req.params.clone()) {
            let result = handle_formatting(params, store);
            let response = Response {
                id: req.id.clone(),
                result: serde_json::to_value(result).ok(),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::References::METHOD {
        if let Ok(params) = serde_json::from_value::<ReferenceParams>(req.params.clone()) {
            let result = handle_references(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    } else if req.method == request::WorkspaceSymbolRequest::METHOD {
        if let Ok(params) = serde_json::from_value::<WorkspaceSymbolParams>(req.params.clone()) {
            let result = handle_workspace_symbols(params, store);
            let response = Response {
                id: req.id.clone(),
                result: Some(serde_json::to_value(result).unwrap()),
                error: None,
            };
            connection.sender.send(Message::Response(response)).unwrap();
        }
    }
}

fn handle_goto_definition(
    params: GotoDefinitionParams,
    store: &DocumentStore,
) -> Option<GotoDefinitionResponse> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let text = store.get_text(uri)?;
    let word = extract_word_at_position(text, position)?;

    // Look up in symbols for this document first
    if let Some(symbols) = store.get_symbols(uri) {
        for symbol in symbols {
            if symbol.name == word {
                return Some(GotoDefinitionResponse::Scalar(symbol.location.clone()));
            }
        }
    }

    None
}

fn handle_hover(params: HoverParams, store: &DocumentStore) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let text = store.get_text(uri)?;
    let word = extract_word_at_position(text, position)?;

    // Look up in symbols
    if let Some(symbols) = store.get_symbols(uri) {
        for symbol in symbols {
            if symbol.name == word {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```lumen\n{}\n```", symbol.signature),
                    }),
                    range: None,
                });
            }
        }
    }

    None
}

fn handle_completion(_params: CompletionParams, store: &DocumentStore) -> CompletionList {
    let mut items = Vec::new();

    // Add keywords
    let keywords = vec![
        "cell", "record", "enum", "if", "else", "match", "for", "while", "loop", "return", "let",
        "mut", "end", "process", "memory", "machine", "pipeline", "grant", "effect", "bind",
        "handler", "addon", "use", "import", "as", "true", "false", "null", "async", "await",
        "break", "continue", "in", "and", "or", "not", "is", "state", "terminal", "to", "where",
        "when", "agent", "trait", "impl", "const", "type", "pub", "macro",
    ];

    for keyword in keywords {
        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        });
    }

    // Add builtin functions
    let builtins = vec![
        ("print", "print(value) -> Void"),
        ("len", "len(collection) -> Int"),
        ("append", "append(list, item) -> list"),
        ("sort", "sort(list) -> list"),
        ("map", "map(list, fn) -> list"),
        ("filter", "filter(list, fn) -> list"),
        ("reduce", "reduce(list, init, fn) -> value"),
        ("join", "join(list, separator) -> String"),
        ("split", "split(string, separator) -> list[String]"),
        ("trim", "trim(string) -> String"),
        ("parse_int", "parse_int(string) -> result[Int, String]"),
        (
            "parse_float",
            "parse_float(string) -> result[Float, String]",
        ),
        ("to_string", "to_string(value) -> String"),
        ("contains", "contains(collection, item) -> Bool"),
        ("keys", "keys(map) -> list"),
        ("values", "values(map) -> list"),
        ("parallel", "parallel(futures) -> list"),
        ("race", "race(futures) -> value"),
        ("vote", "vote(futures, threshold) -> value"),
        ("select", "select(futures) -> value"),
        ("timeout", "timeout(future, ms) -> result"),
    ];

    for (name, signature) in builtins {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(signature.to_string()),
            ..Default::default()
        });
    }

    // Add primitive types
    let types = vec!["String", "Int", "Float", "Bool", "Bytes", "Json", "Void"];
    for ty in types {
        items.push(CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::CLASS),
            ..Default::default()
        });
    }

    // Add symbols from all open documents
    for symbol in store.all_symbols() {
        let kind = match symbol.kind {
            SymbolKind::Cell => CompletionItemKind::FUNCTION,
            SymbolKind::Record => CompletionItemKind::STRUCT,
            SymbolKind::Enum => CompletionItemKind::ENUM,
            SymbolKind::TypeAlias => CompletionItemKind::CLASS,
            SymbolKind::Process => CompletionItemKind::CLASS,
            SymbolKind::Effect => CompletionItemKind::INTERFACE,
        };

        items.push(CompletionItem {
            label: symbol.name.clone(),
            kind: Some(kind),
            detail: Some(symbol.signature.clone()),
            ..Default::default()
        });
    }

    CompletionList {
        is_incomplete: false,
        items,
    }
}

fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;

    if char_pos > line.len() {
        return None;
    }

    // Find word boundaries
    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    if start >= end {
        return None;
    }

    Some(line[start..end].to_string())
}

fn build_symbol_index(source: &str, uri: &Uri) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    // Parse the source to extract symbols
    let extracted = extract_blocks(source);

    // Concatenate code blocks
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
        return symbols;
    }

    // Lex and parse
    let mut lexer = Lexer::new(&full_code, first_block_line, first_block_offset);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return symbols,
    };

    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program(vec![]) {
        Ok(p) => p,
        Err(_) => return symbols,
    };

    // Extract symbols from the AST
    extract_symbols_from_program(&program, uri, &mut symbols);

    symbols
}

fn extract_symbols_from_program(program: &Program, uri: &Uri, symbols: &mut Vec<Symbol>) {
    for item in &program.items {
        match item {
            Item::Cell(cell) => {
                let params_str = cell
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, type_expr_to_string(&p.ty)))
                    .collect::<Vec<_>>()
                    .join(", ");

                let return_str = cell
                    .return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", type_expr_to_string(t)))
                    .unwrap_or_default();

                let effects_str = if !cell.effects.is_empty() {
                    format!(" / {{{}}}", cell.effects.join(", "))
                } else {
                    String::new()
                };

                let signature = format!(
                    "cell {}({}){}{}",
                    cell.name, params_str, return_str, effects_str
                );

                symbols.push(Symbol {
                    name: cell.name.clone(),
                    kind: SymbolKind::Cell,
                    location: span_to_location(cell.span, uri),
                    signature,
                });
            }
            Item::Record(record) => {
                let fields_str = record
                    .fields
                    .iter()
                    .map(|f| format!("  {}: {}", f.name, type_expr_to_string(&f.ty)))
                    .collect::<Vec<_>>()
                    .join("\n");

                let signature = format!("record {}\n{}\nend", record.name, fields_str);

                symbols.push(Symbol {
                    name: record.name.clone(),
                    kind: SymbolKind::Record,
                    location: span_to_location(record.span, uri),
                    signature,
                });
            }
            Item::Enum(enum_def) => {
                let variants_str = enum_def
                    .variants
                    .iter()
                    .map(|v| {
                        if let Some(payload) = &v.payload {
                            format!("  {}({})", v.name, type_expr_to_string(payload))
                        } else {
                            format!("  {}", v.name)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let signature = format!("enum {}\n{}\nend", enum_def.name, variants_str);

                symbols.push(Symbol {
                    name: enum_def.name.clone(),
                    kind: SymbolKind::Enum,
                    location: span_to_location(enum_def.span, uri),
                    signature,
                });
            }
            Item::TypeAlias(alias) => {
                let signature = format!(
                    "type {} = {}",
                    alias.name,
                    type_expr_to_string(&alias.type_expr)
                );

                symbols.push(Symbol {
                    name: alias.name.clone(),
                    kind: SymbolKind::TypeAlias,
                    location: span_to_location(alias.span, uri),
                    signature,
                });
            }
            Item::Process(process) => {
                let signature = format!("process {} {}", process.kind, process.name);

                symbols.push(Symbol {
                    name: process.name.clone(),
                    kind: SymbolKind::Process,
                    location: span_to_location(process.span, uri),
                    signature,
                });
            }
            Item::Effect(effect) => {
                let signature = format!("effect {}", effect.name);

                symbols.push(Symbol {
                    name: effect.name.clone(),
                    kind: SymbolKind::Effect,
                    location: span_to_location(effect.span, uri),
                    signature,
                });
            }
            _ => {}
        }
    }
}

fn type_expr_to_string(ty: &lumen_compiler::compiler::ast::TypeExpr) -> String {
    use lumen_compiler::compiler::ast::TypeExpr;

    match ty {
        TypeExpr::Named(name, _) => name.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", type_expr_to_string(inner)),
        TypeExpr::Map(k, v, _) => format!(
            "map[{}, {}]",
            type_expr_to_string(k),
            type_expr_to_string(v)
        ),
        TypeExpr::Result(ok, err, _) => format!(
            "result[{}, {}]",
            type_expr_to_string(ok),
            type_expr_to_string(err)
        ),
        TypeExpr::Union(types, _) => types
            .iter()
            .map(type_expr_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Null(_) => "null".to_string(),
        TypeExpr::Tuple(types, _) => {
            let inner = types
                .iter()
                .map(type_expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
        TypeExpr::Set(inner, _) => format!("set[{}]", type_expr_to_string(inner)),
        TypeExpr::Fn(params, ret, effects, _) => {
            let params_str = params
                .iter()
                .map(type_expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let ret_str = type_expr_to_string(ret);
            let effects_str = if !effects.is_empty() {
                format!(" / {{{}}}", effects.join(", "))
            } else {
                String::new()
            };
            format!("fn({}) -> {}{}", params_str, ret_str, effects_str)
        }
        TypeExpr::Generic(name, args, _) => {
            let args_str = args
                .iter()
                .map(type_expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}[{}]", name, args_str)
        }
    }
}

fn span_to_location(span: lumen_compiler::compiler::tokens::Span, uri: &Uri) -> Location {
    Location {
        uri: uri.clone(),
        range: Range {
            start: Position {
                line: if span.line > 0 {
                    (span.line - 1) as u32
                } else {
                    0
                },
                character: 0,
            },
            end: Position {
                line: if span.line > 0 {
                    (span.line - 1) as u32
                } else {
                    0
                },
                character: u32::MAX,
            },
        },
    }
}

fn diagnose(source: &str) -> Vec<Diagnostic> {
    match lumen_compiler::compile(source) {
        Ok(_) => vec![],
        Err(err) => compile_error_to_diagnostics(&err),
    }
}

fn compile_error_to_diagnostics(err: &CompileError) -> Vec<Diagnostic> {
    match err {
        CompileError::Lex(e) => vec![make_diagnostic(lex_error_line(e), &e.to_string())],
        CompileError::Parse(e) => vec![make_diagnostic(parse_error_line(e), &e.to_string())],
        CompileError::Resolve(errors) => errors
            .iter()
            .map(|e| make_diagnostic(resolve_error_line(e), &e.to_string()))
            .collect(),
        CompileError::Type(errors) => errors
            .iter()
            .map(|e| make_diagnostic(type_error_line(e), &e.to_string()))
            .collect(),
        CompileError::Constraint(errors) => errors
            .iter()
            .map(|e| make_diagnostic(constraint_error_line(e), &e.to_string()))
            .collect(),
    }
}

fn make_diagnostic(line: usize, message: &str) -> Diagnostic {
    let line_zero = if line > 0 { line - 1 } else { 0 };
    Diagnostic {
        range: Range {
            start: Position {
                line: line_zero as u32,
                character: 0,
            },
            end: Position {
                line: line_zero as u32,
                character: u32::MAX,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: message.to_string(),
        ..Default::default()
    }
}

fn lex_error_line(e: &LexError) -> usize {
    match e {
        LexError::UnexpectedChar { line, .. } => *line,
        LexError::UnterminatedString { line, .. } => *line,
        LexError::InconsistentIndent { line } => *line,
        LexError::InvalidNumber { line, .. } => *line,
        LexError::InvalidBytesLiteral { line, .. } => *line,
        LexError::InvalidUnicodeEscape { line, .. } => *line,
    }
}

fn parse_error_line(e: &ParseError) -> usize {
    match e {
        ParseError::Unexpected { line, .. } => *line,
        ParseError::UnexpectedEof => 1,
    }
}

fn resolve_error_line(e: &ResolveError) -> usize {
    match e {
        ResolveError::UndefinedType { line, .. } => *line,
        ResolveError::UndefinedCell { line, .. } => *line,
        ResolveError::UndefinedTool { line, .. } => *line,
        ResolveError::Duplicate { line, .. } => *line,
        ResolveError::MissingEffectGrant { line, .. } => *line,
        ResolveError::UndeclaredEffect { line, .. } => *line,
        ResolveError::EffectContractViolation { line, .. } => *line,
        ResolveError::NondeterministicOperation { line, .. } => *line,
        ResolveError::MachineUnknownInitial { line, .. } => *line,
        ResolveError::MachineUnknownTransition { line, .. } => *line,
        ResolveError::MachineUnreachableState { line, .. } => *line,
        ResolveError::MachineMissingTerminal { line, .. } => *line,
        ResolveError::MachineTransitionArgCount { line, .. } => *line,
        ResolveError::MachineTransitionArgType { line, .. } => *line,
        ResolveError::MachineUnsupportedExpr { line, .. } => *line,
        ResolveError::MachineGuardType { line, .. } => *line,
        ResolveError::PipelineUnknownStage { line, .. } => *line,
        ResolveError::PipelineStageArity { line, .. } => *line,
        ResolveError::PipelineStageTypeMismatch { line, .. } => *line,
        ResolveError::CircularImport { .. } => 1,
        ResolveError::ModuleNotFound { line, .. } => *line,
        ResolveError::ImportedSymbolNotFound { line, .. } => *line,
        ResolveError::GenericArityMismatch { line, .. } => *line,
        ResolveError::UndefinedTrait { line, .. } => *line,
        ResolveError::TraitMissingMethods { line, .. } => *line,
    }
}

fn type_error_line(e: &TypeError) -> usize {
    match e {
        TypeError::Mismatch { line, .. } => *line,
        TypeError::UndefinedVar { line, .. } => *line,
        TypeError::NotCallable { line } => *line,
        TypeError::ArgCount { line, .. } => *line,
        TypeError::UnknownField { line, .. } => *line,
        TypeError::UndefinedType { line, .. } => *line,
        TypeError::MissingReturn { line, .. } => *line,
        TypeError::ImmutableAssign { line, .. } => *line,
        TypeError::IncompleteMatch { line, .. } => *line,
    }
}

fn constraint_error_line(e: &ConstraintError) -> usize {
    match e {
        ConstraintError::Invalid { line, .. } => *line,
    }
}

fn publish(connection: &Connection, uri: Uri, diagnostics: Vec<Diagnostic>) {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let not = Notification::new(notification::PublishDiagnostics::METHOD.to_string(), params);
    connection.sender.send(Message::Notification(not)).unwrap();
}

// ── 1. Document Symbols ──

fn handle_document_symbols(
    params: DocumentSymbolParams,
    store: &DocumentStore,
) -> DocumentSymbolResponse {
    let uri = &params.text_document.uri;

    if let Some(symbols) = store.get_symbols(uri) {
        let doc_symbols: Vec<DocumentSymbol> = symbols
            .iter()
            .map(|sym| {
                let kind = match sym.kind {
                    SymbolKind::Cell => lsp_types::SymbolKind::FUNCTION,
                    SymbolKind::Record => lsp_types::SymbolKind::STRUCT,
                    SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
                    SymbolKind::TypeAlias => lsp_types::SymbolKind::TYPE_PARAMETER,
                    SymbolKind::Process => lsp_types::SymbolKind::CLASS,
                    SymbolKind::Effect => lsp_types::SymbolKind::INTERFACE,
                };

                #[allow(deprecated)]
                DocumentSymbol {
                    name: sym.name.clone(),
                    detail: Some(sym.signature.clone()),
                    kind,
                    tags: None,
                    deprecated: None,
                    range: sym.location.range,
                    selection_range: sym.location.range,
                    children: None,
                }
            })
            .collect();

        DocumentSymbolResponse::Nested(doc_symbols)
    } else {
        DocumentSymbolResponse::Nested(vec![])
    }
}

// ── 2. Semantic Tokens ──

fn handle_semantic_tokens(
    params: SemanticTokensParams,
    store: &DocumentStore,
) -> Option<SemanticTokensResult> {
    let uri = &params.text_document.uri;
    let text = store.get_text(uri)?;

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
        return Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: vec![],
        }));
    }

    let mut lexer = Lexer::new(&full_code, first_block_line, first_block_offset);
    let tokens = lexer.tokenize().ok()?;

    let mut semantic_tokens = Vec::new();
    let mut prev_line = 0;
    let mut prev_char = 0;

    for token in tokens {
        let token_type = match &token.kind {
            // Keywords
            lumen_compiler::compiler::tokens::TokenKind::Record
            | lumen_compiler::compiler::tokens::TokenKind::Enum
            | lumen_compiler::compiler::tokens::TokenKind::Cell
            | lumen_compiler::compiler::tokens::TokenKind::Let
            | lumen_compiler::compiler::tokens::TokenKind::If
            | lumen_compiler::compiler::tokens::TokenKind::Else
            | lumen_compiler::compiler::tokens::TokenKind::For
            | lumen_compiler::compiler::tokens::TokenKind::While
            | lumen_compiler::compiler::tokens::TokenKind::Loop
            | lumen_compiler::compiler::tokens::TokenKind::Match
            | lumen_compiler::compiler::tokens::TokenKind::Return
            | lumen_compiler::compiler::tokens::TokenKind::Halt
            | lumen_compiler::compiler::tokens::TokenKind::End
            | lumen_compiler::compiler::tokens::TokenKind::Use
            | lumen_compiler::compiler::tokens::TokenKind::As
            | lumen_compiler::compiler::tokens::TokenKind::Grant
            | lumen_compiler::compiler::tokens::TokenKind::Tool
            | lumen_compiler::compiler::tokens::TokenKind::Where
            | lumen_compiler::compiler::tokens::TokenKind::And
            | lumen_compiler::compiler::tokens::TokenKind::Or
            | lumen_compiler::compiler::tokens::TokenKind::Not => 0, // KEYWORD

            lumen_compiler::compiler::tokens::TokenKind::Ident(_) => 3, // VARIABLE
            lumen_compiler::compiler::tokens::TokenKind::StringLit(_)
            | lumen_compiler::compiler::tokens::TokenKind::RawStringLit(_)
            | lumen_compiler::compiler::tokens::TokenKind::StringInterpLit(_) => 6, // STRING
            lumen_compiler::compiler::tokens::TokenKind::IntLit(_) => 7, // NUMBER
            lumen_compiler::compiler::tokens::TokenKind::FloatLit(_) => 7, // NUMBER
            lumen_compiler::compiler::tokens::TokenKind::Plus
            | lumen_compiler::compiler::tokens::TokenKind::Minus
            | lumen_compiler::compiler::tokens::TokenKind::Star
            | lumen_compiler::compiler::tokens::TokenKind::Slash
            | lumen_compiler::compiler::tokens::TokenKind::Percent
            | lumen_compiler::compiler::tokens::TokenKind::Assign
            | lumen_compiler::compiler::tokens::TokenKind::Eq
            | lumen_compiler::compiler::tokens::TokenKind::NotEq
            | lumen_compiler::compiler::tokens::TokenKind::Lt
            | lumen_compiler::compiler::tokens::TokenKind::Gt
            | lumen_compiler::compiler::tokens::TokenKind::LtEq
            | lumen_compiler::compiler::tokens::TokenKind::GtEq
            | lumen_compiler::compiler::tokens::TokenKind::Bang => 5, // OPERATOR
            _ => continue,                                              // Skip other tokens
        };

        let line = if token.span.line > 0 {
            token.span.line - 1
        } else {
            0
        };
        let char = token.span.start as u32;

        let delta_line = if line >= prev_line {
            (line - prev_line) as u32
        } else {
            0
        };
        let delta_char = if delta_line == 0 && char >= prev_char {
            char - prev_char
        } else {
            char
        };

        semantic_tokens.push(SemanticToken {
            delta_line,
            delta_start: delta_char,
            length: 1, // Default length, could be improved
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_char = char;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: semantic_tokens,
    }))
}

// ── 3. Signature Help ──

fn handle_signature_help(
    params: SignatureHelpParams,
    store: &DocumentStore,
) -> Option<SignatureHelp> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let text = store.get_text(uri)?;

    // Find the function call at or before the cursor
    let (func_name, param_index) = find_function_call_at_position(text, position)?;

    // Look up the function in symbols
    let symbols = store.get_symbols(uri)?;
    let func_symbol = symbols
        .iter()
        .find(|s| matches!(s.kind, SymbolKind::Cell) && s.name == func_name)?;

    // Parse the signature to extract parameters
    let signature_info = SignatureInformation {
        label: func_symbol.signature.clone(),
        documentation: None,
        parameters: None, // Could be improved to parse actual parameters
        active_parameter: Some(param_index),
    };

    Some(SignatureHelp {
        signatures: vec![signature_info],
        active_signature: Some(0),
        active_parameter: Some(param_index),
    })
}

fn find_function_call_at_position(text: &str, position: Position) -> Option<(String, u32)> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;

    // Simple heuristic: look backwards for '(' and find the identifier before it
    let before_cursor = &line[..char_pos.min(line.len())];

    // Count commas to determine parameter index
    let param_index = before_cursor.matches(',').count() as u32;

    // Find the last '(' before cursor
    let open_paren = before_cursor.rfind('(')?;
    let before_paren = &before_cursor[..open_paren].trim_end();

    // Extract the function name (alphanumeric + underscore)
    let func_name_start = before_paren
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let func_name = before_paren[func_name_start..].to_string();

    if func_name.is_empty() {
        None
    } else {
        Some((func_name, param_index))
    }
}

// ── 4. Inlay Hints ──

fn handle_inlay_hints(params: InlayHintParams, store: &DocumentStore) -> Vec<InlayHint> {
    let uri = &params.text_document.uri;
    let text = match store.get_text(uri) {
        Some(t) => t,
        None => return vec![],
    };

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
        return vec![];
    }

    let mut lexer = Lexer::new(&full_code, first_block_line, first_block_offset);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program(vec![]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let mut hints = Vec::new();

    // Extract inlay hints from the program
    for item in &program.items {
        if let Item::Cell(cell) = item {
            extract_inlay_hints_from_cell(cell, &mut hints);
        }
    }

    hints
}

fn extract_inlay_hints_from_cell(
    cell: &lumen_compiler::compiler::ast::CellDef,
    hints: &mut Vec<InlayHint>,
) {
    // Add inlay hints for let bindings without explicit types
    for stmt in &cell.body {
        extract_hints_from_stmt(stmt, hints);
    }
}

fn extract_hints_from_stmt(stmt: &Stmt, hints: &mut Vec<InlayHint>) {
    match stmt {
        Stmt::Let(let_stmt) => {
            // If no type annotation, we could infer and show it
            if let_stmt.ty.is_none() {
                // Use the name from the let statement
                let name = &let_stmt.name;
                let line = if let_stmt.span.line > 0 {
                    let_stmt.span.line - 1
                } else {
                    0
                };
                hints.push(InlayHint {
                    position: Position {
                        line: line as u32,
                        character: (let_stmt.span.start + name.len()) as u32,
                    },
                    label: InlayHintLabel::String(": <inferred>".to_string()),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: None,
                    data: None,
                });
            }
        }
        Stmt::If(if_stmt) => {
            for s in &if_stmt.then_body {
                extract_hints_from_stmt(s, hints);
            }
            if let Some(else_stmts) = &if_stmt.else_body {
                for s in else_stmts {
                    extract_hints_from_stmt(s, hints);
                }
            }
        }
        Stmt::While(while_stmt) => {
            for s in &while_stmt.body {
                extract_hints_from_stmt(s, hints);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                extract_hints_from_stmt(s, hints);
            }
        }
        Stmt::For(for_stmt) => {
            for s in &for_stmt.body {
                extract_hints_from_stmt(s, hints);
            }
        }
        Stmt::Match(match_stmt) => {
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    extract_hints_from_stmt(s, hints);
                }
            }
        }
        _ => {}
    }
}

// ── 5. Code Actions ──

fn handle_code_actions(params: CodeActionParams, store: &DocumentStore) -> CodeActionResponse {
    let uri = &params.text_document.uri;
    let _text = store.get_text(uri);

    let mut actions = Vec::new();

    // Check diagnostics for quick fixes
    for diagnostic in &params.context.diagnostics {
        // Quick fix: Add missing return type
        if diagnostic.message.contains("missing return")
            || diagnostic.message.contains("return type")
        {
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Add return type annotation".to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: None, // Would need to construct WorkspaceEdit
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            });
            actions.push(action);
        }

        // Quick fix: Remove unused variable
        if diagnostic.message.contains("unused") && diagnostic.message.contains("variable") {
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Remove unused variable".to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: None,
                command: None,
                is_preferred: Some(false),
                disabled: None,
                data: None,
            });
            actions.push(action);
        }
    }

    actions
}

// ── 6. Folding Ranges ──

fn handle_folding_ranges(params: FoldingRangeParams, store: &DocumentStore) -> Vec<FoldingRange> {
    let uri = &params.text_document.uri;
    let text = match store.get_text(uri) {
        Some(t) => t,
        None => return vec![],
    };

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
        return vec![];
    }

    let mut lexer = Lexer::new(&full_code, first_block_line, first_block_offset);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program(vec![]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let mut ranges = Vec::new();

    // Create folding ranges for top-level items
    for item in &program.items {
        let (start_line, kind) = match item {
            Item::Cell(cell) => {
                let line = if cell.span.line > 0 {
                    cell.span.line - 1
                } else {
                    0
                };
                (line, Some(FoldingRangeKind::Region))
            }
            Item::Record(record) => {
                let line = if record.span.line > 0 {
                    record.span.line - 1
                } else {
                    0
                };
                (line, Some(FoldingRangeKind::Region))
            }
            Item::Enum(enum_def) => {
                let line = if enum_def.span.line > 0 {
                    enum_def.span.line - 1
                } else {
                    0
                };
                (line, Some(FoldingRangeKind::Region))
            }
            Item::Process(process) => {
                let line = if process.span.line > 0 {
                    process.span.line - 1
                } else {
                    0
                };
                (line, Some(FoldingRangeKind::Region))
            }
            _ => continue,
        };

        // For now, assume each item is multiple lines (could be improved)
        ranges.push(FoldingRange {
            start_line: start_line as u32,
            start_character: None,
            end_line: (start_line + 5) as u32, // Placeholder - would need proper end detection
            end_character: None,
            kind,
            collapsed_text: None,
        });
    }

    ranges
}

// ── 7. Document Formatting ──

fn handle_formatting(
    _params: DocumentFormattingParams,
    _store: &DocumentStore,
) -> Option<Vec<TextEdit>> {
    // Format using lumen fmt command
    // For now, we'll just return None since formatting would require
    // either shelling out to `lumen fmt` or duplicating formatting logic.
    // The VS Code extension already provides a `lumen.fmt` command that users can invoke.

    // Future improvement: Move formatter to a shared library crate
    // that both lumen-cli and lumen-lsp can depend on.

    // Placeholder: return no edits
    None
}

// ── 8. Find References ──

fn handle_references(params: ReferenceParams, store: &DocumentStore) -> Vec<Location> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let text = match store.get_text(uri) {
        Some(t) => t,
        None => return vec![],
    };

    let word = match extract_word_at_position(text, position) {
        Some(w) => w,
        None => return vec![],
    };

    let mut locations = Vec::new();

    // Search in the current document
    for (line_num, line) in text.lines().enumerate() {
        let mut start = 0;
        while let Some(pos) = line[start..].find(&word) {
            let actual_pos = start + pos;
            // Check if it's a whole word match
            let is_start_boundary =
                actual_pos == 0 || !line.chars().nth(actual_pos - 1).unwrap().is_alphanumeric();
            let end_pos = actual_pos + word.len();
            let is_end_boundary =
                end_pos >= line.len() || !line.chars().nth(end_pos).unwrap().is_alphanumeric();

            if is_start_boundary && is_end_boundary {
                locations.push(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line: line_num as u32,
                            character: actual_pos as u32,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: end_pos as u32,
                        },
                    },
                });
            }

            start = actual_pos + 1;
        }
    }

    // Could also search in other open documents
    for (other_uri, other_text) in &store.documents {
        if other_uri == uri {
            continue; // Already searched
        }

        for (line_num, line) in other_text.lines().enumerate() {
            if line.contains(&word) {
                locations.push(Location {
                    uri: other_uri.clone(),
                    range: Range {
                        start: Position {
                            line: line_num as u32,
                            character: 0,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: line.len() as u32,
                        },
                    },
                });
            }
        }
    }

    locations
}

// ── 9. Workspace Symbols ──

fn handle_workspace_symbols(
    params: WorkspaceSymbolParams,
    store: &DocumentStore,
) -> Vec<SymbolInformation> {
    let query = params.query.to_lowercase();

    let all_symbols = store.all_symbols();

    all_symbols
        .iter()
        .filter(|sym| {
            if query.is_empty() {
                true
            } else {
                sym.name.to_lowercase().contains(&query)
            }
        })
        .map(|sym| {
            let kind = match sym.kind {
                SymbolKind::Cell => lsp_types::SymbolKind::FUNCTION,
                SymbolKind::Record => lsp_types::SymbolKind::STRUCT,
                SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
                SymbolKind::TypeAlias => lsp_types::SymbolKind::TYPE_PARAMETER,
                SymbolKind::Process => lsp_types::SymbolKind::CLASS,
                SymbolKind::Effect => lsp_types::SymbolKind::INTERFACE,
            };

            #[allow(deprecated)]
            SymbolInformation {
                name: sym.name.clone(),
                kind,
                tags: None,
                deprecated: None,
                location: sym.location.clone(),
                container_name: None,
            }
        })
        .collect()
}
