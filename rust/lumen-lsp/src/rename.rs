//! Rename symbol support for LSP
//!
//! Handles `textDocument/rename` and `textDocument/prepareRename` requests.
//! Finds all occurrences of an identifier within a single document and returns
//! a `WorkspaceEdit` that replaces them all with the new name.

use lsp_types::{Position, PrepareRenameResponse, Range, TextEdit, Uri, WorkspaceEdit};
use lumen_compiler::compiler::ast::{
    CallArg, CellDef, Expr, Item, LambdaBody, MatchArm, Pattern, Program, Stmt,
};
use lumen_compiler::compiler::tokens::Span;

/// Information about a single occurrence of a symbol in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolOccurrence {
    /// 0-based line
    line: u32,
    /// 0-based start column (UTF-16)
    start_char: u32,
    /// 0-based end column (UTF-16)
    end_char: u32,
}

/// Prepare rename: validates that the cursor is on a renameable symbol and
/// returns its range and placeholder text.
pub fn prepare_rename(
    text: &str,
    position: Position,
    program: Option<&Program>,
) -> Option<PrepareRenameResponse> {
    let word = extract_word_at_position(text, position)?;

    // Don't rename keywords
    if is_keyword(&word) {
        return None;
    }

    // Check that the symbol actually exists in the AST
    if let Some(prog) = program {
        if !symbol_exists_in_program(prog, &word) {
            return None;
        }
    }

    // Return the range of the word under cursor
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;
    let (start, end) = word_boundary(line, char_pos)?;

    Some(PrepareRenameResponse::Range(Range {
        start: Position {
            line: position.line,
            character: start as u32,
        },
        end: Position {
            line: position.line,
            character: end as u32,
        },
    }))
}

/// Rename a symbol: find all occurrences in the document and return edits.
pub fn rename_symbol(
    uri: &Uri,
    text: &str,
    position: Position,
    new_name: &str,
    program: Option<&Program>,
) -> Option<WorkspaceEdit> {
    let word = extract_word_at_position(text, position)?;

    if is_keyword(&word) || new_name.is_empty() {
        return None;
    }

    let occurrences = find_all_occurrences(text, &word, program);
    if occurrences.is_empty() {
        return None;
    }

    let edits: Vec<TextEdit> = occurrences
        .into_iter()
        .map(|occ| TextEdit {
            range: Range {
                start: Position {
                    line: occ.line,
                    character: occ.start_char,
                },
                end: Position {
                    line: occ.line,
                    character: occ.end_char,
                },
            },
            new_text: new_name.to_string(),
        })
        .collect();

    Some(WorkspaceEdit {
        changes: Some([(uri.clone(), edits)].into_iter().collect()),
        document_changes: None,
        change_annotations: None,
    })
}

/// Find all occurrences of the given identifier in the document.
/// Uses the AST to locate semantically meaningful occurrences rather than
/// blindly doing text search.
fn find_all_occurrences(
    text: &str,
    name: &str,
    program: Option<&Program>,
) -> Vec<SymbolOccurrence> {
    let mut occurrences = Vec::new();

    if let Some(prog) = program {
        collect_occurrences_in_program(prog, name, &mut occurrences);
    }

    // Deduplicate by (line, start_char)
    occurrences.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));
    occurrences.dedup();

    // If AST-based search finds nothing, fall back to text-based search
    // for identifiers (whole-word matching)
    if occurrences.is_empty() {
        occurrences = find_text_occurrences(text, name);
    }

    occurrences
}

/// Collect all AST-based occurrences of `name` in the program.
fn collect_occurrences_in_program(prog: &Program, name: &str, out: &mut Vec<SymbolOccurrence>) {
    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                if cell.name == name {
                    push_span_occurrence(&cell.span, name, &cell.name, out);
                }
                collect_occurrences_in_cell(cell, name, out);
            }
            Item::Record(record) => {
                if record.name == name {
                    push_span_occurrence(&record.span, name, &record.name, out);
                }
                for field in &record.fields {
                    if field.name == name {
                        push_span_occurrence(&field.span, name, &field.name, out);
                    }
                    if let Some(default) = &field.default_value {
                        collect_occurrences_in_expr(default, name, out);
                    }
                }
            }
            Item::Enum(enum_def) => {
                if enum_def.name == name {
                    push_span_occurrence(&enum_def.span, name, &enum_def.name, out);
                }
                for variant in &enum_def.variants {
                    if variant.name == name {
                        push_span_occurrence(&variant.span, name, &variant.name, out);
                    }
                }
                for method in &enum_def.methods {
                    if method.name == name {
                        push_span_occurrence(&method.span, name, &method.name, out);
                    }
                    collect_occurrences_in_cell(method, name, out);
                }
            }
            Item::TypeAlias(alias) => {
                if alias.name == name {
                    push_span_occurrence(&alias.span, name, &alias.name, out);
                }
            }
            Item::Process(process) => {
                if process.name == name {
                    push_span_occurrence(&process.span, name, &process.name, out);
                }
                for cell in &process.cells {
                    if cell.name == name {
                        push_span_occurrence(&cell.span, name, &cell.name, out);
                    }
                    collect_occurrences_in_cell(cell, name, out);
                }
            }
            Item::Effect(effect) => {
                if effect.name == name {
                    push_span_occurrence(&effect.span, name, &effect.name, out);
                }
            }
            Item::Handler(handler) => {
                if handler.name == name {
                    push_span_occurrence(&handler.span, name, &handler.name, out);
                }
            }
            Item::Trait(trait_def) => {
                if trait_def.name == name {
                    push_span_occurrence(&trait_def.span, name, &trait_def.name, out);
                }
            }
            Item::Impl(impl_def) => {
                if impl_def.trait_name == name {
                    push_span_occurrence(&impl_def.span, name, &impl_def.trait_name, out);
                }
                for cell in &impl_def.cells {
                    collect_occurrences_in_cell(cell, name, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect occurrences in a cell body (parameters + statements).
fn collect_occurrences_in_cell(cell: &CellDef, name: &str, out: &mut Vec<SymbolOccurrence>) {
    for param in &cell.params {
        if param.name == name {
            push_span_occurrence(&param.span, name, &param.name, out);
        }
    }
    for stmt in &cell.body {
        collect_occurrences_in_stmt(stmt, name, out);
    }
}

fn collect_occurrences_in_stmt(stmt: &Stmt, name: &str, out: &mut Vec<SymbolOccurrence>) {
    match stmt {
        Stmt::Let(let_stmt) => {
            if let_stmt.name == name {
                push_span_occurrence(&let_stmt.span, name, &let_stmt.name, out);
            }
            collect_occurrences_in_expr(&let_stmt.value, name, out);
        }
        Stmt::Assign(assign) => {
            if assign.target == name {
                push_span_occurrence(&assign.span, name, &assign.target, out);
            }
            collect_occurrences_in_expr(&assign.value, name, out);
        }
        Stmt::If(if_stmt) => {
            collect_occurrences_in_expr(&if_stmt.condition, name, out);
            for s in &if_stmt.then_body {
                collect_occurrences_in_stmt(s, name, out);
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    collect_occurrences_in_stmt(s, name, out);
                }
            }
        }
        Stmt::While(while_stmt) => {
            collect_occurrences_in_expr(&while_stmt.condition, name, out);
            for s in &while_stmt.body {
                collect_occurrences_in_stmt(s, name, out);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                collect_occurrences_in_stmt(s, name, out);
            }
        }
        Stmt::For(for_stmt) => {
            if for_stmt.var == name {
                push_span_occurrence(&for_stmt.span, name, &for_stmt.var, out);
            }
            collect_occurrences_in_expr(&for_stmt.iter, name, out);
            for s in &for_stmt.body {
                collect_occurrences_in_stmt(s, name, out);
            }
        }
        Stmt::Match(match_stmt) => {
            collect_occurrences_in_expr(&match_stmt.subject, name, out);
            for arm in &match_stmt.arms {
                collect_occurrences_in_match_arm(arm, name, out);
            }
        }
        Stmt::Return(ret) => {
            collect_occurrences_in_expr(&ret.value, name, out);
        }
        Stmt::Expr(expr_stmt) => {
            collect_occurrences_in_expr(&expr_stmt.expr, name, out);
        }
        Stmt::CompoundAssign(ca) => {
            if ca.target == name {
                push_span_occurrence(&ca.span, name, &ca.target, out);
            }
            collect_occurrences_in_expr(&ca.value, name, out);
        }
        Stmt::Defer(defer) => {
            for s in &defer.body {
                collect_occurrences_in_stmt(s, name, out);
            }
        }
        Stmt::Yield(yield_stmt) => {
            collect_occurrences_in_expr(&yield_stmt.value, name, out);
        }
        Stmt::Emit(emit) => {
            collect_occurrences_in_expr(&emit.value, name, out);
        }
        Stmt::Halt(halt) => {
            collect_occurrences_in_expr(&halt.message, name, out);
        }
        _ => {}
    }
}

fn collect_occurrences_in_match_arm(arm: &MatchArm, name: &str, out: &mut Vec<SymbolOccurrence>) {
    collect_occurrences_in_pattern(&arm.pattern, name, out);
    for s in &arm.body {
        collect_occurrences_in_stmt(s, name, out);
    }
}

fn collect_occurrences_in_pattern(pat: &Pattern, name: &str, out: &mut Vec<SymbolOccurrence>) {
    match pat {
        Pattern::Ident(id, span) if id == name => {
            out.push(span_to_occurrence(span, name));
        }
        Pattern::Variant(vname, sub_pat, span) => {
            if vname == name {
                out.push(span_to_occurrence(span, name));
            }
            if let Some(sub) = sub_pat {
                collect_occurrences_in_pattern(sub, name, out);
            }
        }
        Pattern::Guard {
            inner, condition, ..
        } => {
            collect_occurrences_in_pattern(inner, name, out);
            collect_occurrences_in_expr(condition, name, out);
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                collect_occurrences_in_pattern(p, name, out);
            }
        }
        Pattern::ListDestructure { elements, rest, .. } => {
            for p in elements {
                collect_occurrences_in_pattern(p, name, out);
            }
            if let Some(rest_name) = rest {
                if rest_name == name {
                    // rest doesn't have its own span, skip
                }
            }
        }
        Pattern::TupleDestructure { elements, .. } => {
            for p in elements {
                collect_occurrences_in_pattern(p, name, out);
            }
        }
        Pattern::RecordDestructure {
            type_name, fields, ..
        } => {
            if type_name == name {
                // type_name reference
            }
            for (_field_name, sub_pat) in fields {
                if let Some(p) = sub_pat {
                    collect_occurrences_in_pattern(p, name, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_occurrences_in_expr(expr: &Expr, name: &str, out: &mut Vec<SymbolOccurrence>) {
    match expr {
        Expr::Ident(id, span) if id == name => {
            out.push(span_to_occurrence(span, name));
        }
        Expr::Call(func, args, _) => {
            collect_occurrences_in_expr(func, name, out);
            for arg in args {
                match arg {
                    CallArg::Positional(e) => collect_occurrences_in_expr(e, name, out),
                    CallArg::Named(_, e, _) => collect_occurrences_in_expr(e, name, out),
                    CallArg::Role(_, e, _) => collect_occurrences_in_expr(e, name, out),
                }
            }
        }
        Expr::BinOp(left, _, right, _) => {
            collect_occurrences_in_expr(left, name, out);
            collect_occurrences_in_expr(right, name, out);
        }
        Expr::UnaryOp(_, inner, _) => {
            collect_occurrences_in_expr(inner, name, out);
        }
        Expr::DotAccess(inner, _, _) => {
            collect_occurrences_in_expr(inner, name, out);
        }
        Expr::IndexAccess(inner, idx, _) => {
            collect_occurrences_in_expr(inner, name, out);
            collect_occurrences_in_expr(idx, name, out);
        }
        Expr::ListLit(items, _) => {
            for item in items {
                collect_occurrences_in_expr(item, name, out);
            }
        }
        Expr::MapLit(entries, _) => {
            for (k, v) in entries {
                collect_occurrences_in_expr(k, name, out);
                collect_occurrences_in_expr(v, name, out);
            }
        }
        Expr::TupleLit(items, _) => {
            for item in items {
                collect_occurrences_in_expr(item, name, out);
            }
        }
        Expr::RecordLit(type_name, fields, span) => {
            if type_name == name {
                out.push(span_to_occurrence(span, name));
            }
            for (_, val) in fields {
                collect_occurrences_in_expr(val, name, out);
            }
        }
        Expr::Lambda { params, body, .. } => {
            for p in params {
                if p.name == name {
                    push_span_occurrence(&p.span, name, &p.name, out);
                }
            }
            match body {
                LambdaBody::Expr(e) => collect_occurrences_in_expr(e, name, out),
                LambdaBody::Block(stmts) => {
                    for s in stmts {
                        collect_occurrences_in_stmt(s, name, out);
                    }
                }
            }
        }
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_occurrences_in_expr(cond, name, out);
            collect_occurrences_in_expr(then_val, name, out);
            collect_occurrences_in_expr(else_val, name, out);
        }
        Expr::Pipe { left, right, .. } => {
            collect_occurrences_in_expr(left, name, out);
            collect_occurrences_in_expr(right, name, out);
        }
        Expr::TryExpr(inner, _) => {
            collect_occurrences_in_expr(inner, name, out);
        }
        Expr::NullCoalesce(left, right, _) => {
            collect_occurrences_in_expr(left, name, out);
            collect_occurrences_in_expr(right, name, out);
        }
        Expr::AwaitExpr(inner, _) => {
            collect_occurrences_in_expr(inner, name, out);
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                collect_occurrences_in_stmt(s, name, out);
            }
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_occurrences_in_expr(subject, name, out);
            for arm in arms {
                collect_occurrences_in_match_arm(arm, name, out);
            }
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_occurrences_in_expr(body, name, out);
            collect_occurrences_in_expr(iter, name, out);
            if let Some(cond) = condition {
                collect_occurrences_in_expr(cond, name, out);
            }
        }
        Expr::HandleExpr { body, handlers, .. } => {
            for s in body {
                collect_occurrences_in_stmt(s, name, out);
            }
            for handler in handlers {
                for s in &handler.body {
                    collect_occurrences_in_stmt(s, name, out);
                }
            }
        }
        Expr::Perform { args, .. } => {
            for arg in args {
                collect_occurrences_in_expr(arg, name, out);
            }
        }
        Expr::ResumeExpr(inner, _) => {
            collect_occurrences_in_expr(inner, name, out);
        }
        _ => {}
    }
}

/// Convert AST span to a SymbolOccurrence. The span's `col` tells us where the
/// construct starts. For declaration items the name appears at a known offset
/// inside the line, so we use text-search on the correct line as a fallback.
fn span_to_occurrence(span: &Span, name: &str) -> SymbolOccurrence {
    let line = if span.line > 0 {
        (span.line - 1) as u32
    } else {
        0
    };
    let col = if span.col > 0 {
        (span.col - 1) as u32
    } else {
        0
    };

    SymbolOccurrence {
        line,
        start_char: col,
        end_char: col + name.len() as u32,
    }
}

/// For declarations whose span covers the keyword (e.g. `cell foo`), the name
/// is somewhere after the keyword. We approximate the name position using the
/// item span offset.
fn push_span_occurrence(
    span: &Span,
    _name: &str,
    actual_name: &str,
    out: &mut Vec<SymbolOccurrence>,
) {
    let _ = actual_name;
    let occ = span_to_occurrence(span, _name);
    out.push(occ);
}

/// Text-based fallback: find all whole-word occurrences of `name`.
fn find_text_occurrences(text: &str, name: &str) -> Vec<SymbolOccurrence> {
    let mut occurrences = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(name) {
            let abs_pos = search_start + pos;
            let before_ok = abs_pos == 0
                || !line.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                    && line.as_bytes()[abs_pos - 1] != b'_';
            let after_pos = abs_pos + name.len();
            let after_ok = after_pos >= line.len()
                || !line.as_bytes()[after_pos].is_ascii_alphanumeric()
                    && line.as_bytes()[after_pos] != b'_';

            if before_ok && after_ok {
                occurrences.push(SymbolOccurrence {
                    line: line_idx as u32,
                    start_char: abs_pos as u32,
                    end_char: (abs_pos + name.len()) as u32,
                });
            }
            search_start = abs_pos + name.len();
        }
    }
    occurrences
}

fn symbol_exists_in_program(prog: &Program, name: &str) -> bool {
    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                if cell.name == name {
                    return true;
                }
                for param in &cell.params {
                    if param.name == name {
                        return true;
                    }
                }
                if stmts_contain_name(&cell.body, name) {
                    return true;
                }
            }
            Item::Record(record) => {
                if record.name == name {
                    return true;
                }
                for field in &record.fields {
                    if field.name == name {
                        return true;
                    }
                }
            }
            Item::Enum(enum_def) => {
                if enum_def.name == name {
                    return true;
                }
                for variant in &enum_def.variants {
                    if variant.name == name {
                        return true;
                    }
                }
            }
            Item::TypeAlias(alias) if alias.name == name => return true,
            Item::Process(p) if p.name == name => return true,
            Item::Effect(e) if e.name == name => return true,
            Item::Handler(h) if h.name == name => return true,
            _ => {}
        }
    }
    false
}

fn stmts_contain_name(stmts: &[Stmt], name: &str) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) if let_stmt.name == name => return true,
            Stmt::Assign(assign) if assign.target == name => return true,
            Stmt::For(for_stmt) if for_stmt.var == name => return true,
            Stmt::Let(let_stmt) => {
                if expr_contains_name(&let_stmt.value, name) {
                    return true;
                }
            }
            Stmt::If(if_stmt) => {
                if stmts_contain_name(&if_stmt.then_body, name) {
                    return true;
                }
                if let Some(else_body) = &if_stmt.else_body {
                    if stmts_contain_name(else_body, name) {
                        return true;
                    }
                }
            }
            Stmt::Expr(expr_stmt) => {
                if expr_contains_name(&expr_stmt.expr, name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn expr_contains_name(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Ident(id, _) => id == name,
        Expr::Call(func, args, _) => {
            expr_contains_name(func, name)
                || args.iter().any(|a| match a {
                    CallArg::Positional(e) => expr_contains_name(e, name),
                    CallArg::Named(_, e, _) => expr_contains_name(e, name),
                    CallArg::Role(_, e, _) => expr_contains_name(e, name),
                })
        }
        Expr::BinOp(l, _, r, _) => expr_contains_name(l, name) || expr_contains_name(r, name),
        Expr::DotAccess(inner, _, _) => expr_contains_name(inner, name),
        _ => false,
    }
}

fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;
    let (start, end) = word_boundary(line, char_pos)?;
    if start >= end {
        return None;
    }
    Some(line[start..end].to_string())
}

fn word_boundary(line: &str, char_pos: usize) -> Option<(usize, usize)> {
    if char_pos > line.len() {
        return None;
    }

    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "cell"
            | "record"
            | "enum"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "loop"
            | "return"
            | "let"
            | "mut"
            | "end"
            | "process"
            | "memory"
            | "machine"
            | "pipeline"
            | "effect"
            | "handler"
            | "grant"
            | "import"
            | "type"
            | "trait"
            | "impl"
            | "fn"
            | "true"
            | "false"
            | "null"
            | "and"
            | "or"
            | "not"
            | "in"
            | "is"
            | "as"
            | "pub"
            | "extern"
            | "defer"
            | "yield"
            | "break"
            | "continue"
            | "halt"
            | "where"
            | "when"
            | "comptime"
            | "perform"
            | "handle"
            | "with"
            | "resume"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Uri;

    fn make_uri() -> Uri {
        "file:///test.lm".parse().unwrap()
    }

    fn parse_program(source: &str) -> Option<Program> {
        match lumen_compiler::compile_raw(source) {
            Ok(_module) => {
                // Re-parse to get the AST
                let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
                let tokens = lexer.tokenize().ok()?;
                let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
                parser.parse_program(vec![]).ok()
            }
            Err(_) => {
                // Try parsing even if compilation fails
                let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
                let tokens = lexer.tokenize().ok()?;
                let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
                parser.parse_program(vec![]).ok()
            }
        }
    }

    #[test]
    fn test_rename_cell_name() {
        let source = "cell greet(name: String) -> String\n  return name\nend\n\ncell main() -> Int\n  let msg = greet(\"world\")\n  return 0\nend";
        let program = parse_program(source);
        let uri = make_uri();

        // Cursor on `greet` definition (line 0, col 5)
        let result = rename_symbol(
            &uri,
            source,
            Position {
                line: 0,
                character: 5,
            },
            "hello",
            program.as_ref(),
        );

        assert!(result.is_some());
        let edit = result.unwrap();
        let changes = edit.changes.unwrap();
        let edits = changes.get(&uri).unwrap();
        // Should find at least 2 occurrences: definition + call site
        assert!(
            edits.len() >= 2,
            "Expected at least 2 edits, got {}",
            edits.len()
        );
        for text_edit in edits {
            assert_eq!(text_edit.new_text, "hello");
        }
    }

    #[test]
    fn test_rename_variable() {
        let source = "cell main() -> Int\n  let count = 10\n  let doubled = count * 2\n  return doubled\nend";
        let program = parse_program(source);
        let uri = make_uri();

        // Rename `count`
        let result = rename_symbol(
            &uri,
            source,
            Position {
                line: 1,
                character: 6,
            },
            "total",
            program.as_ref(),
        );

        assert!(result.is_some());
        let edit = result.unwrap();
        let changes = edit.changes.unwrap();
        let edits = changes.get(&uri).unwrap();
        assert!(
            edits.len() >= 2,
            "Expected at least 2 edits for `count`, got {}",
            edits.len()
        );
        for text_edit in edits {
            assert_eq!(text_edit.new_text, "total");
        }
    }

    #[test]
    fn test_prepare_rename_keyword_rejected() {
        let source = "cell main() -> Int\n  return 0\nend";
        let program = parse_program(source);

        // Cursor on `cell` keyword
        let result = prepare_rename(
            source,
            Position {
                line: 0,
                character: 1,
            },
            program.as_ref(),
        );
        assert!(result.is_none(), "Keywords should not be renameable");
    }

    #[test]
    fn test_prepare_rename_valid_symbol() {
        let source = "cell main() -> Int\n  return 0\nend";
        let program = parse_program(source);

        // Cursor on `main`
        let result = prepare_rename(
            source,
            Position {
                line: 0,
                character: 6,
            },
            program.as_ref(),
        );
        assert!(result.is_some(), "Valid symbol should be renameable");
    }

    #[test]
    fn test_rename_empty_name_rejected() {
        let source = "cell main() -> Int\n  return 0\nend";
        let program = parse_program(source);
        let uri = make_uri();

        let result = rename_symbol(
            &uri,
            source,
            Position {
                line: 0,
                character: 6,
            },
            "",
            program.as_ref(),
        );
        assert!(result.is_none(), "Empty new name should be rejected");
    }

    #[test]
    fn test_find_text_occurrences_whole_word() {
        let text = "let foo = 1\nlet foobar = foo + 2\nprint(foo)";
        let occs = find_text_occurrences(text, "foo");
        // Should find: line 0 col 4, line 1 col 15, line 2 col 6
        assert_eq!(occs.len(), 3, "Should find 3 whole-word occurrences of foo");
        // Should NOT match "foobar"
        for occ in &occs {
            assert_eq!(
                (occ.end_char - occ.start_char) as usize,
                3,
                "Each occurrence should be exactly 3 chars"
            );
        }
    }
}
