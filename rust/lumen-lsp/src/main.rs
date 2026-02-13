use lsp_server::{Connection, Message, Notification};
use lsp_types::notification::Notification as _;
use lsp_types::*;
use lumen_compiler::compiler::constraints::ConstraintError;
use lumen_compiler::compiler::lexer::LexError;
use lumen_compiler::compiler::parser::ParseError;
use lumen_compiler::compiler::resolve::ResolveError;
use lumen_compiler::compiler::typecheck::TypeError;
use lumen_compiler::CompileError;

fn main() {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::FULL,
        )),
        ..Default::default()
    };

    let caps_json = serde_json::to_value(capabilities).unwrap();
    let _init_params = connection.initialize(caps_json).unwrap();

    for msg in &connection.receiver {
        match msg {
            Message::Notification(not) => {
                if not.method == notification::DidOpenTextDocument::METHOD {
                    if let Ok(params) =
                        serde_json::from_value::<DidOpenTextDocumentParams>(not.params)
                    {
                        let diagnostics = diagnose(&params.text_document.text);
                        publish(&connection, params.text_document.uri, diagnostics);
                    }
                } else if not.method == notification::DidChangeTextDocument::METHOD {
                    if let Ok(params) =
                        serde_json::from_value::<DidChangeTextDocumentParams>(not.params)
                    {
                        if let Some(change) = params.content_changes.into_iter().last() {
                            let diagnostics = diagnose(&change.text);
                            publish(&connection, params.text_document.uri, diagnostics);
                        }
                    }
                }
            }
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap() {
                    break;
                }
            }
            _ => {}
        }
    }

    io_threads.join().unwrap();
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
    let not = Notification::new(
        notification::PublishDiagnostics::METHOD.to_string(),
        params,
    );
    connection.sender.send(Message::Notification(not)).unwrap();
}
