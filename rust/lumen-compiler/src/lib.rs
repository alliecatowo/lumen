//! Lumen Compiler
//!
//! Transforms Lumen source files (`.lm`, `.lumen`, `.lm.md`, `.lumen.md`) into LIR modules.

pub mod compiler;
pub mod diagnostics;
pub mod lang_ref;
pub mod markdown;

use compiler::ast::{Directive, ImportDecl, ImportList, Item};
use compiler::lir::LirModule;
use compiler::resolve::SymbolTable;
use std::collections::HashSet;

use thiserror::Error;

// ── Compile options ─────────────────────────────────────────────────

/// Controls when ownership analysis violations are treated as hard errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OwnershipCheckMode {
    /// Ownership analysis is completely skipped.
    Off,
    /// Ownership violations are detected but do not block compilation (default).
    #[default]
    Warn,
    /// Ownership violations are treated as compile errors.
    Error,
}

/// Options controlling optional analysis passes in the compile pipeline.
///
/// All fields have sensible defaults: ownership analysis defaults to `Warn`
/// mode, and typestate / session type checking are opt-in (disabled by default
/// since they require explicit declarations).
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Ownership analysis mode. Default: `Warn`.
    pub ownership_mode: OwnershipCheckMode,
    /// Typestate declarations to check against (opt-in per type).
    /// Map from type name → typestate declaration.
    pub typestate_declarations:
        std::collections::HashMap<String, compiler::typestate::TypestateDecl>,
    /// Session protocol declarations to check against (opt-in per protocol).
    /// Map from protocol name → session type.
    pub session_protocols: std::collections::HashMap<String, compiler::session::SessionType>,
    /// Session action sequences to verify (opt-in per protocol).
    /// Map from protocol name → sequence of (action, span) pairs.
    pub session_actions:
        std::collections::HashMap<String, Vec<(compiler::session::Action, compiler::tokens::Span)>>,
    /// Allow unstable features without errors. Default: `false`.
    pub allow_unstable: bool,
    /// Language edition for forward-compatibility. Default: `"2026"`.
    pub edition: String,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            ownership_mode: OwnershipCheckMode::default(),
            typestate_declarations: std::collections::HashMap::new(),
            session_protocols: std::collections::HashMap::new(),
            session_actions: std::collections::HashMap::new(),
            allow_unstable: false,
            edition: "2026".to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("lex error: {0}")]
    Lex(#[from] compiler::lexer::LexError),
    #[error("parse errors: {0:?}")]
    Parse(Vec<compiler::parser::ParseError>),
    #[error("resolve errors: {0:?}")]
    Resolve(Vec<compiler::resolve::ResolveError>),
    #[error("type errors: {0:?}")]
    Type(Vec<compiler::typecheck::TypeError>),
    #[error("constraint errors: {0:?}")]
    Constraint(Vec<compiler::constraints::ConstraintError>),
    #[error("ownership errors: {0:?}")]
    Ownership(Vec<compiler::ownership::OwnershipError>),
    #[error("typestate errors: {0:?}")]
    Typestate(Vec<compiler::typestate::TypestateError>),
    #[error("session type errors: {0:?}")]
    Session(Vec<compiler::session::SessionError>),
    #[error("lowering error: {0}")]
    Lower(String),
    #[error("multiple errors: {0:?}")]
    Multiple(Vec<CompileError>),
}

impl CompileError {
    /// Construct a `CompileError` from a list of errors.
    /// Returns `None` if the list is empty, unwraps single-element lists,
    /// and flattens nested `Multiple` variants.
    pub fn from_multiple(errors: Vec<CompileError>) -> Option<CompileError> {
        let flattened: Vec<CompileError> = errors
            .into_iter()
            .flat_map(|e| match e {
                CompileError::Multiple(inner) => inner,
                other => vec![other],
            })
            .collect();
        match flattened.len() {
            0 => None,
            1 => Some(flattened.into_iter().next().unwrap()),
            _ => Some(CompileError::Multiple(flattened)),
        }
    }
}

impl From<compiler::parser::ParseError> for CompileError {
    fn from(err: compiler::parser::ParseError) -> Self {
        CompileError::Parse(vec![err])
    }
}

/// Run the three optional analysis passes (ownership, typestate, session types).
///
/// Returns any hard errors produced by analyses running in `Error` mode.
/// Analyses in `Warn` mode or `Off` mode do not contribute errors.
fn run_optional_analyses(
    program: &compiler::ast::Program,
    symbols: &SymbolTable,
    options: &CompileOptions,
) -> Vec<CompileError> {
    let mut errors = Vec::new();

    // 1. Ownership analysis
    if options.ownership_mode != OwnershipCheckMode::Off {
        let ownership_errors = compiler::ownership::check_program(program, symbols);
        if !ownership_errors.is_empty() && options.ownership_mode == OwnershipCheckMode::Error {
            errors.push(CompileError::Ownership(ownership_errors));
        }
        // In Warn mode, errors are detected but silently discarded.
    }

    // 2. Typestate checking (opt-in: only if declarations are provided)
    if !options.typestate_declarations.is_empty() {
        let mut all_typestate_errors = Vec::new();
        for item in &program.items {
            let cells: Vec<&compiler::ast::CellDef> = match item {
                Item::Cell(c) => vec![c],
                Item::Agent(a) => a.cells.iter().collect(),
                Item::Process(p) => p.cells.iter().collect(),
                _ => vec![],
            };
            for cell in cells {
                let mut checker = compiler::typestate::TypestateChecker::new();
                let cell_errors = checker.check_cell(cell, &options.typestate_declarations);
                all_typestate_errors.extend(cell_errors);
            }
        }
        if !all_typestate_errors.is_empty() {
            errors.push(CompileError::Typestate(all_typestate_errors));
        }
    }

    // 3. Session type checking (opt-in: only if protocols are provided)
    if !options.session_protocols.is_empty() {
        let mut all_session_errors = Vec::new();
        let mut checker = compiler::session::SessionChecker::new();
        for (name, protocol) in &options.session_protocols {
            checker.declare_protocol(name, protocol.clone());
        }
        // Verify action sequences against declared protocols.
        for (protocol_name, actions) in &options.session_actions {
            let end_span = actions
                .last()
                .map(|(_, s)| *s)
                .unwrap_or(compiler::tokens::Span::dummy());
            let session_errors = checker.check_complete_session(protocol_name, actions, end_span);
            all_session_errors.extend(session_errors);
        }
        if !all_session_errors.is_empty() {
            errors.push(CompileError::Session(all_session_errors));
        }
    }

    errors
}

/// Safe wrapper around the lowering pass that converts register allocation
/// panics into proper `CompileError::Lower` errors instead of crashing.
fn lower_safe(
    program: &compiler::ast::Program,
    symbols: &SymbolTable,
    source: &str,
) -> Result<LirModule, CompileError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compiler::lower::lower(program, symbols, source)
    }))
    .map_err(|panic_val| {
        let msg = if let Some(s) = panic_val.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = panic_val.downcast_ref::<&str>() {
            (*s).to_string()
        } else {
            "internal lowering error".to_string()
        };
        CompileError::Lower(msg)
    })
}

/// Compile with access to external modules for import resolution.
///
/// The `resolve_import` callback takes a module path (e.g., "mathlib") and returns
/// the source content of that module if it exists, or None if not found.
pub fn compile_with_imports(
    source: &str,
    resolve_import: &dyn Fn(&str) -> Option<String>,
) -> Result<LirModule, CompileError> {
    compile_with_imports_and_options(source, resolve_import, &CompileOptions::default())
}

/// Compile with access to external modules and optional analysis passes.
pub fn compile_with_imports_and_options(
    source: &str,
    resolve_import: &dyn Fn(&str) -> Option<String>,
    options: &CompileOptions,
) -> Result<LirModule, CompileError> {
    let mut compilation_stack = HashSet::new();
    compile_with_imports_internal(
        source,
        resolve_import,
        &mut compilation_stack,
        None,
        options,
    )
}

/// Internal implementation that tracks the compilation stack for circular import detection
fn compile_with_imports_internal(
    source: &str,
    resolve_import: &dyn Fn(&str) -> Option<String>,
    compilation_stack: &mut HashSet<String>,
    _current_module: Option<&str>,
    options: &CompileOptions,
) -> Result<LirModule, CompileError> {
    // 1. Extract Markdown blocks
    let extracted = markdown::extract::extract_blocks(source);

    // 2. Build directives
    let directives: Vec<Directive> = extracted
        .directives
        .iter()
        .map(|d| Directive {
            name: d.name.clone(),
            value: d.value.clone(),
            span: d.span,
        })
        .collect();

    // 3. Concatenate all code blocks
    // 3. Concatenate all code blocks preserving line numbers
    let mut full_code = String::new();
    let mut current_line = 1;

    for block in extracted.code_blocks.iter() {
        // Pad with newlines to reach the block's start line
        while current_line < block.code_start_line {
            full_code.push('\n');
            current_line += 1;
        }

        full_code.push_str(&block.code);

        let lines_in_block = block.code.chars().filter(|&c| c == '\n').count();
        current_line += lines_in_block;
    }

    if full_code.trim().is_empty() {
        return Ok(LirModule::new("sha256:empty".to_string()));
    }

    // 4. Lex
    // We start at line 1 because we padded the code to match the file structure
    let mut lexer = compiler::lexer::Lexer::new(&full_code, 1, 0);
    let tokens = lexer.tokenize()?;

    // 5. Parse
    let mut parser = compiler::parser::Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_with_recovery(directives);
    if !parse_errors.is_empty() {
        return Err(CompileError::Parse(parse_errors));
    }

    // 6. Process imports before resolution
    let mut base_symbols = SymbolTable::new();
    let mut import_errors = Vec::new();
    let mut imported_modules: Vec<LirModule> = Vec::new();

    // Collect all imports
    let imports: Vec<&ImportDecl> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(imp) = item {
                Some(imp)
            } else {
                None
            }
        })
        .collect();

    // Process each import
    for import in imports {
        let module_path = import.path.join(".");

        // Check for circular imports
        if compilation_stack.contains(&module_path) {
            let chain: Vec<String> = compilation_stack.iter().cloned().collect();
            let chain_str = format!("{} -> {}", chain.join(" -> "), module_path);
            import_errors.push(compiler::resolve::ResolveError::CircularImport {
                module: module_path.clone(),
                chain: chain_str,
            });
            continue;
        }

        // Resolve the module source
        let imported_source = match resolve_import(&module_path) {
            Some(src) => src,
            None => {
                import_errors.push(compiler::resolve::ResolveError::ModuleNotFound {
                    module: module_path.clone(),
                    line: import.span.line,
                });
                continue;
            }
        };

        // Track this module in the compilation stack
        compilation_stack.insert(module_path.clone());

        // Recursively compile the imported module. The markdown pipeline now
        // supports fenced and unfenced source forms.
        let imported_module = compile_with_imports_internal(
            &imported_source,
            resolve_import,
            compilation_stack,
            Some(&module_path),
            options,
        )?;

        // Remove from stack after compilation
        compilation_stack.remove(&module_path);

        // Keep the compiled module for later merging
        imported_modules.push(imported_module);

        // Extract symbols from the imported module by parsing it as markdown if it has
        // fenced lumen blocks, otherwise as raw source.
        let imported_extracted = markdown::extract::extract_blocks(&imported_source);
        let (imported_code, imported_directives, imported_line, imported_offset) =
            if imported_extracted.code_blocks.is_empty() {
                (imported_source.clone(), vec![], 1, 0)
            } else {
                let mut code = String::new();
                let mut first_line = 1;
                let mut first_offset = 0;
                for (i, block) in imported_extracted.code_blocks.iter().enumerate() {
                    if i == 0 {
                        first_line = block.code_start_line;
                        first_offset = block.code_offset;
                    }
                    if !code.is_empty() {
                        code.push('\n');
                    }
                    code.push_str(&block.code);
                }
                let directives: Vec<Directive> = imported_extracted
                    .directives
                    .iter()
                    .map(|d| Directive {
                        name: d.name.clone(),
                        value: d.value.clone(),
                        span: d.span,
                    })
                    .collect();
                (code, directives, first_line, first_offset)
            };

        let mut imported_lexer =
            compiler::lexer::Lexer::new(&imported_code, imported_line, imported_offset);
        if let Ok(imported_tokens) = imported_lexer.tokenize() {
            let mut imported_parser = compiler::parser::Parser::new(imported_tokens);
            if let Ok(imported_program) = imported_parser.parse_program(imported_directives) {
                if let Ok(imported_symbols) = compiler::resolve::resolve(&imported_program) {
                    // Import the requested symbols
                    match &import.names {
                        ImportList::Wildcard => {
                            // Import all top-level definitions
                            for (name, info) in imported_symbols.cells {
                                base_symbols.import_cell(name, info);
                            }
                            for (name, info) in imported_symbols.types {
                                base_symbols.import_type(name, info);
                            }
                            for (name, type_expr) in imported_symbols.type_aliases {
                                base_symbols.import_type_alias(name, type_expr);
                            }
                        }
                        ImportList::Names(names) => {
                            for import_name in names {
                                let symbol_name = &import_name.name;
                                let local_name = import_name.alias.as_ref().unwrap_or(symbol_name);

                                // Try to find the symbol in cells, types, or type aliases
                                let mut found = false;

                                if let Some(cell_info) = imported_symbols.cells.get(symbol_name) {
                                    base_symbols.import_cell(local_name.clone(), cell_info.clone());
                                    found = true;
                                }

                                if let Some(type_info) = imported_symbols.types.get(symbol_name) {
                                    base_symbols.import_type(local_name.clone(), type_info.clone());
                                    found = true;
                                }

                                if let Some(type_expr) =
                                    imported_symbols.type_aliases.get(symbol_name)
                                {
                                    base_symbols
                                        .import_type_alias(local_name.clone(), type_expr.clone());
                                    found = true;
                                }

                                if !found {
                                    import_errors.push(
                                        compiler::resolve::ResolveError::ImportedSymbolNotFound {
                                            symbol: symbol_name.clone(),
                                            module: module_path.clone(),
                                            line: import_name.span.line,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. Resolve with imported symbols pre-populated (collect errors, continue with partial table)
    // Use resolve_with_base_partial so imported symbols are available during resolution
    let (symbols, resolve_errors) =
        compiler::resolve::resolve_with_base_partial(&program, base_symbols);
    let mut all_errors: Vec<CompileError> = Vec::new();
    if !import_errors.is_empty() {
        all_errors.push(CompileError::Resolve(import_errors));
    }
    if !resolve_errors.is_empty() {
        all_errors.push(CompileError::Resolve(resolve_errors));
    }

    // 8. Typecheck (run even if resolve had errors, using partial symbol table)
    if let Err(type_errors) = compiler::typecheck::typecheck(&program, &symbols) {
        all_errors.push(CompileError::Type(type_errors));
    }

    // 9. Validate constraints
    if let Err(constraint_errors) = compiler::constraints::validate_constraints(&program) {
        all_errors.push(CompileError::Constraint(constraint_errors));
    }

    // 10. Run optional analysis passes (ownership, typestate, session types)
    all_errors.extend(run_optional_analyses(&program, &symbols, options));

    // If there were any errors, report them all
    if let Some(combined) = CompileError::from_multiple(all_errors) {
        return Err(combined);
    }

    // 11. Lower to LIR
    let mut module = lower_safe(&program, &symbols, source)?;

    // 11. Merge imported modules
    for imported_module in imported_modules {
        module.merge(&imported_module);
    }

    Ok(module)
}

/// Compile raw .lm source with access to external modules for import resolution.
///
/// The `resolve_import` callback takes a module path (e.g., "mathlib") and returns
/// the source content of that module if it exists, or None if not found.
pub fn compile_raw_with_imports(
    source: &str,
    resolve_import: &dyn Fn(&str) -> Option<String>,
) -> Result<LirModule, CompileError> {
    let mut compilation_stack = HashSet::new();
    compile_raw_with_imports_internal(source, resolve_import, &mut compilation_stack, None)
}

/// Internal implementation for raw source compilation with imports
fn compile_raw_with_imports_internal(
    source: &str,
    resolve_import: &dyn Fn(&str) -> Option<String>,
    compilation_stack: &mut HashSet<String>,
    _current_module: Option<&str>,
) -> Result<LirModule, CompileError> {
    if source.trim().is_empty() {
        return Ok(LirModule::new("sha256:empty".to_string()));
    }

    // 1. Lex (start at line 1, offset 0)
    let mut lexer = compiler::lexer::Lexer::new(source, 1, 0);
    let tokens = lexer.tokenize()?;

    // 2. Parse (no directives for raw source)
    let mut parser = compiler::parser::Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_with_recovery(vec![]);
    if !parse_errors.is_empty() {
        return Err(CompileError::Parse(parse_errors));
    }

    // 3. Process imports before resolution
    let mut base_symbols = SymbolTable::new();
    let mut import_errors = Vec::new();
    let mut imported_modules: Vec<LirModule> = Vec::new();

    // Collect all imports
    let imports: Vec<&ImportDecl> = program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(imp) = item {
                Some(imp)
            } else {
                None
            }
        })
        .collect();

    // Process each import
    for import in imports {
        let module_path = import.path.join(".");

        // Check for circular imports
        if compilation_stack.contains(&module_path) {
            let chain: Vec<String> = compilation_stack.iter().cloned().collect();
            let chain_str = format!("{} -> {}", chain.join(" -> "), module_path);
            import_errors.push(compiler::resolve::ResolveError::CircularImport {
                module: module_path.clone(),
                chain: chain_str,
            });
            continue;
        }

        // Resolve the module source
        let imported_source = match resolve_import(&module_path) {
            Some(src) => src,
            None => {
                import_errors.push(compiler::resolve::ResolveError::ModuleNotFound {
                    module: module_path.clone(),
                    line: import.span.line,
                });
                continue;
            }
        };

        // Track this module in the compilation stack
        compilation_stack.insert(module_path.clone());

        // Recursively compile the imported module through the markdown pipeline,
        // which also handles unfenced source.
        let imported_module = compile_with_imports_internal(
            &imported_source,
            resolve_import,
            compilation_stack,
            Some(&module_path),
            &CompileOptions::default(),
        )?;

        // Remove from stack after compilation
        compilation_stack.remove(&module_path);

        // Keep the compiled module for later merging
        imported_modules.push(imported_module);

        // Extract symbols from the imported module by parsing it as markdown if it has
        // fenced lumen blocks, otherwise as raw source.
        let imported_extracted = markdown::extract::extract_blocks(&imported_source);
        let (imported_code, imported_directives, imported_line, imported_offset) =
            if imported_extracted.code_blocks.is_empty() {
                (imported_source.clone(), vec![], 1, 0)
            } else {
                let mut code = String::new();
                let mut first_line = 1;
                let mut first_offset = 0;
                for (i, block) in imported_extracted.code_blocks.iter().enumerate() {
                    if i == 0 {
                        first_line = block.code_start_line;
                        first_offset = block.code_offset;
                    }
                    if !code.is_empty() {
                        code.push('\n');
                    }
                    code.push_str(&block.code);
                }
                let directives: Vec<Directive> = imported_extracted
                    .directives
                    .iter()
                    .map(|d| Directive {
                        name: d.name.clone(),
                        value: d.value.clone(),
                        span: d.span,
                    })
                    .collect();
                (code, directives, first_line, first_offset)
            };

        let mut imported_lexer =
            compiler::lexer::Lexer::new(&imported_code, imported_line, imported_offset);
        if let Ok(imported_tokens) = imported_lexer.tokenize() {
            let mut imported_parser = compiler::parser::Parser::new(imported_tokens);
            if let Ok(imported_program) = imported_parser.parse_program(imported_directives) {
                if let Ok(imported_symbols) = compiler::resolve::resolve(&imported_program) {
                    // Import the requested symbols
                    match &import.names {
                        ImportList::Wildcard => {
                            // Import all top-level definitions
                            for (name, info) in imported_symbols.cells {
                                base_symbols.import_cell(name, info);
                            }
                            for (name, info) in imported_symbols.types {
                                base_symbols.import_type(name, info);
                            }
                            for (name, type_expr) in imported_symbols.type_aliases {
                                base_symbols.import_type_alias(name, type_expr);
                            }
                        }
                        ImportList::Names(names) => {
                            for import_name in names {
                                let symbol_name = &import_name.name;
                                let local_name = import_name.alias.as_ref().unwrap_or(symbol_name);

                                // Try to find the symbol in cells, types, or type aliases
                                let mut found = false;

                                if let Some(cell_info) = imported_symbols.cells.get(symbol_name) {
                                    base_symbols.import_cell(local_name.clone(), cell_info.clone());
                                    found = true;
                                }

                                if let Some(type_info) = imported_symbols.types.get(symbol_name) {
                                    base_symbols.import_type(local_name.clone(), type_info.clone());
                                    found = true;
                                }

                                if let Some(type_expr) =
                                    imported_symbols.type_aliases.get(symbol_name)
                                {
                                    base_symbols
                                        .import_type_alias(local_name.clone(), type_expr.clone());
                                    found = true;
                                }

                                if !found {
                                    import_errors.push(
                                        compiler::resolve::ResolveError::ImportedSymbolNotFound {
                                            symbol: symbol_name.clone(),
                                            module: module_path.clone(),
                                            line: import_name.span.line,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Resolve with imported symbols pre-populated (collect errors, continue with partial table)
    let (symbols, resolve_errors) =
        compiler::resolve::resolve_with_base_partial(&program, base_symbols);
    let mut all_errors: Vec<CompileError> = Vec::new();
    if !import_errors.is_empty() {
        all_errors.push(CompileError::Resolve(import_errors));
    }
    if !resolve_errors.is_empty() {
        all_errors.push(CompileError::Resolve(resolve_errors));
    }

    // 5. Typecheck (run even if resolve had errors, using partial symbol table)
    if let Err(type_errors) = compiler::typecheck::typecheck(&program, &symbols) {
        all_errors.push(CompileError::Type(type_errors));
    }

    // 6. Validate constraints
    if let Err(constraint_errors) = compiler::constraints::validate_constraints(&program) {
        all_errors.push(CompileError::Constraint(constraint_errors));
    }

    // If there were any errors, report them all
    if let Some(combined) = CompileError::from_multiple(all_errors) {
        return Err(combined);
    }

    // 7. Lower to LIR
    let mut module = lower_safe(&program, &symbols, source)?;

    // 8. Merge imported modules
    for imported_module in imported_modules {
        module.merge(&imported_module);
    }

    Ok(module)
}

/// Compile a `.lm` raw Lumen source file to a LIR module.
/// This skips markdown extraction and processes the source directly.
pub fn compile_raw(source: &str) -> Result<LirModule, CompileError> {
    compile_raw_with_options(source, &CompileOptions::default())
}

/// Compile a `.lm` raw Lumen source file with optional analysis passes.
pub fn compile_raw_with_options(
    source: &str,
    options: &CompileOptions,
) -> Result<LirModule, CompileError> {
    if source.trim().is_empty() {
        return Ok(LirModule::new("sha256:empty".to_string()));
    }

    // 1. Lex (start at line 1, offset 0)
    let mut lexer = compiler::lexer::Lexer::new(source, 1, 0);
    let tokens = lexer.tokenize()?;

    // 2. Parse (no directives for raw source)
    let mut parser = compiler::parser::Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_with_recovery(vec![]);
    if !parse_errors.is_empty() {
        return Err(CompileError::Parse(parse_errors));
    }

    // 3. Resolve (collect errors but continue with partial symbol table)
    let (symbols, resolve_errors) = compiler::resolve::resolve_partial(&program);
    let mut all_errors: Vec<CompileError> = Vec::new();
    if !resolve_errors.is_empty() {
        all_errors.push(CompileError::Resolve(resolve_errors));
    }

    // 4. Typecheck (run even if resolve had errors, using partial symbol table)
    if let Err(type_errors) = compiler::typecheck::typecheck(&program, &symbols) {
        all_errors.push(CompileError::Type(type_errors));
    }

    // 5. Validate constraints
    if let Err(constraint_errors) = compiler::constraints::validate_constraints(&program) {
        all_errors.push(CompileError::Constraint(constraint_errors));
    }

    // 6. Run optional analysis passes (ownership, typestate, session types)
    all_errors.extend(run_optional_analyses(&program, &symbols, options));

    // If there were any errors, report them all
    if let Some(combined) = CompileError::from_multiple(all_errors) {
        return Err(combined);
    }

    // 7. Lower to LIR
    let module = lower_safe(&program, &symbols, source)?;

    Ok(module)
}

pub fn compile(source: &str) -> Result<LirModule, CompileError> {
    compile_with_options(source, &CompileOptions::default())
}

/// Compile a markdown Lumen source file with optional analysis passes.
pub fn compile_with_options(
    source: &str,
    options: &CompileOptions,
) -> Result<LirModule, CompileError> {
    // 1. Extract Markdown blocks
    let extracted = markdown::extract::extract_blocks(source);

    // 2. Build directives
    let directives: Vec<Directive> = extracted
        .directives
        .iter()
        .map(|d| Directive {
            name: d.name.clone(),
            value: d.value.clone(),
            span: d.span,
        })
        .collect();

    // 3. Concatenate all code blocks preserving line numbers
    let mut full_code = String::new();
    let mut current_line = 1;

    for block in extracted.code_blocks.iter() {
        while current_line < block.code_start_line {
            full_code.push('\n');
            current_line += 1;
        }
        full_code.push_str(&block.code);
        let lines_in_block = block.code.chars().filter(|&c| c == '\n').count();
        current_line += lines_in_block;
    }

    if full_code.trim().is_empty() {
        return Ok(LirModule::new("sha256:empty".to_string()));
    }

    // 4. Lex
    let mut lexer = compiler::lexer::Lexer::new(&full_code, 1, 0);
    let tokens = lexer.tokenize()?;

    // 5. Parse
    let mut parser = compiler::parser::Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_with_recovery(directives);
    if !parse_errors.is_empty() {
        return Err(CompileError::Parse(parse_errors));
    }

    // 6. Resolve (collect errors but continue with partial symbol table)
    let (symbols, resolve_errors) = compiler::resolve::resolve_partial(&program);
    let mut all_errors: Vec<CompileError> = Vec::new();
    if !resolve_errors.is_empty() {
        all_errors.push(CompileError::Resolve(resolve_errors));
    }

    // 7. Typecheck (run even if resolve had errors, using partial symbol table)
    if let Err(type_errors) = compiler::typecheck::typecheck(&program, &symbols) {
        all_errors.push(CompileError::Type(type_errors));
    }

    // 8. Validate constraints
    if let Err(constraint_errors) = compiler::constraints::validate_constraints(&program) {
        all_errors.push(CompileError::Constraint(constraint_errors));
    }

    // 9. Run optional analysis passes (ownership, typestate, session types)
    all_errors.extend(run_optional_analyses(&program, &symbols, options));

    // If there were any errors, report them all
    if let Some(combined) = CompileError::from_multiple(all_errors) {
        return Err(combined);
    }

    // 10. Lower to LIR
    let module = lower_safe(&program, &symbols, source)?;

    Ok(module)
}

/// Format a compile error with rich diagnostics (colors, source snippets, suggestions).
///
/// This is a convenience function that wraps `diagnostics::format_compile_error`
/// and renders all diagnostics with ANSI colors for terminal display.
pub fn format_error(error: &CompileError, source: &str, filename: &str) -> String {
    diagnostics::format_compile_error(error, source, filename)
        .iter()
        .map(|d| d.render_ansi())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple() {
        let src = r#"# Test

```lumen
cell main() -> Int
  return 42
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.cells.len(), 1);
        assert_eq!(module.cells[0].name, "main");
    }

    #[test]
    fn test_compile_with_record() {
        let src = r#"# Test

```lumen
record Point
  x: Int
  y: Int
end
```

```lumen
cell origin() -> Point
  return Point(x: 0, y: 0)
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.cells.len(), 1);
    }

    #[test]
    fn test_compile_full_example() {
        let src = r#"@lumen 1
@package "test"

# Hello World

```lumen
record Greeting
  message: String
end
```

```lumen
cell greet(name: String) -> Greeting
  let msg = "Hello, " + name
  return Greeting(message: msg)
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.cells.len(), 1);
        assert_eq!(module.version, "1.0.0");
    }

    #[test]
    fn test_compile_raw_collects_multiple_parse_errors() {
        let src = r#"
cell bad1() -> Int
  let x =
  return 1
end

cell bad2(param Int) -> Int
  return param
end

record Broken
  x:
end

cell bad3() -> Int
  return
end
"#;

        let err = compile_raw(src).expect_err("expected parse errors");
        match err {
            CompileError::Parse(errors) => {
                assert!(
                    errors.len() >= 3,
                    "expected at least 3 parse errors, got {}",
                    errors.len()
                );
            }
            other => panic!("expected parse errors, got {:?}", other),
        }
    }

    #[test]
    fn test_compile_markdown_collects_multiple_parse_errors() {
        let src = r#"# Broken

```lumen
cell bad1() -> Int
  let x =
  return 1
end

cell bad2(param Int) -> Int
  return param
end

record Broken
  x:
end

cell bad3() -> Int
  return
end
```
"#;

        let err = compile(src).expect_err("expected parse errors");
        match err {
            CompileError::Parse(errors) => {
                assert!(
                    errors.len() >= 3,
                    "expected at least 3 parse errors, got {}",
                    errors.len()
                );
            }
            other => panic!("expected parse errors, got {:?}", other),
        }
    }
}
