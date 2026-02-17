//! Go-to-implementation support for LSP
//!
//! Handles `textDocument/implementation` requests.
//! Given a cursor position on a type name, effect name, or cell name, finds
//! all locations where that symbol is implemented/used:
//!   - Effects → `handle ... with Effect.op ...` blocks
//!   - Record types → construction sites (RecordLit)
//!   - Cells → definition site
//!   - Traits → impl blocks
//!   - Processes → definition site

use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};
use lumen_compiler::compiler::ast::{
    CallArg, CellDef, Expr, Item, LambdaBody, MatchArm, Pattern, Program, Stmt,
};
use lumen_compiler::compiler::tokens::Span;

/// Build a go-to-implementation response for the given position.
pub fn build_implementations(
    position: Position,
    text: &str,
    program: Option<&Program>,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let word = extract_word_at_position(text, position)?;
    let prog = program?;

    // Determine what kind of symbol this is and collect implementation sites
    let mut locations = Vec::new();

    for item in &prog.items {
        match item {
            // If the word is an effect name, find all handle expressions that handle it
            Item::Effect(effect) if effect.name == word => {
                collect_handle_sites_in_program(prog, &word, uri, &mut locations);
                // Also find handler declarations for this effect
                collect_handler_decls(prog, &word, uri, &mut locations);
            }
            // If the word is a record name, find all construction sites
            Item::Record(record) if record.name == word => {
                collect_record_construction_sites(prog, &word, uri, &mut locations);
            }
            // If the word is a cell name, point to its definition
            Item::Cell(cell) if cell.name == word => {
                locations.push(span_to_location(&cell.span, uri));
            }
            // If the word is a trait name, find all impl blocks for it
            Item::Trait(trait_def) if trait_def.name == word => {
                collect_impl_blocks(prog, &word, uri, &mut locations);
            }
            // If the word is a process name, point to its definition
            Item::Process(process) if process.name == word => {
                locations.push(span_to_location(&process.span, uri));
            }
            // If the word is an enum name, find all pattern match sites and construction sites
            Item::Enum(enum_def) if enum_def.name == word => {
                locations.push(span_to_location(&enum_def.span, uri));
            }
            _ => {}
        }
    }

    if locations.is_empty() {
        return None;
    }

    // Deduplicate
    locations.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    locations.dedup_by(|a, b| a.range == b.range);

    if locations.len() == 1 {
        Some(GotoDefinitionResponse::Scalar(locations.remove(0)))
    } else {
        Some(GotoDefinitionResponse::Array(locations))
    }
}

/// Find all `handle ... with EffectName.op ...` blocks in the program.
fn collect_handle_sites_in_program(
    prog: &Program,
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                collect_handle_sites_in_stmts(&cell.body, effect_name, uri, out);
            }
            Item::Process(process) => {
                for cell in &process.cells {
                    collect_handle_sites_in_stmts(&cell.body, effect_name, uri, out);
                }
            }
            Item::Enum(enum_def) => {
                for method in &enum_def.methods {
                    collect_handle_sites_in_stmts(&method.body, effect_name, uri, out);
                }
            }
            Item::Impl(impl_def) => {
                for cell in &impl_def.cells {
                    collect_handle_sites_in_stmts(&cell.body, effect_name, uri, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_handle_sites_in_stmts(
    stmts: &[Stmt],
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    for stmt in stmts {
        collect_handle_sites_in_stmt(stmt, effect_name, uri, out);
    }
}

fn collect_handle_sites_in_stmt(
    stmt: &Stmt,
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    match stmt {
        Stmt::Let(let_stmt) => {
            collect_handle_sites_in_expr(&let_stmt.value, effect_name, uri, out);
        }
        Stmt::Assign(assign) => {
            collect_handle_sites_in_expr(&assign.value, effect_name, uri, out);
        }
        Stmt::If(if_stmt) => {
            collect_handle_sites_in_expr(&if_stmt.condition, effect_name, uri, out);
            collect_handle_sites_in_stmts(&if_stmt.then_body, effect_name, uri, out);
            if let Some(else_body) = &if_stmt.else_body {
                collect_handle_sites_in_stmts(else_body, effect_name, uri, out);
            }
        }
        Stmt::While(while_stmt) => {
            collect_handle_sites_in_expr(&while_stmt.condition, effect_name, uri, out);
            collect_handle_sites_in_stmts(&while_stmt.body, effect_name, uri, out);
        }
        Stmt::Loop(loop_stmt) => {
            collect_handle_sites_in_stmts(&loop_stmt.body, effect_name, uri, out);
        }
        Stmt::For(for_stmt) => {
            collect_handle_sites_in_expr(&for_stmt.iter, effect_name, uri, out);
            collect_handle_sites_in_stmts(&for_stmt.body, effect_name, uri, out);
        }
        Stmt::Match(match_stmt) => {
            collect_handle_sites_in_expr(&match_stmt.subject, effect_name, uri, out);
            for arm in &match_stmt.arms {
                collect_handle_sites_in_match_arm(arm, effect_name, uri, out);
            }
        }
        Stmt::Return(ret) => {
            collect_handle_sites_in_expr(&ret.value, effect_name, uri, out);
        }
        Stmt::Expr(expr_stmt) => {
            collect_handle_sites_in_expr(&expr_stmt.expr, effect_name, uri, out);
        }
        Stmt::CompoundAssign(ca) => {
            collect_handle_sites_in_expr(&ca.value, effect_name, uri, out);
        }
        Stmt::Defer(defer) => {
            collect_handle_sites_in_stmts(&defer.body, effect_name, uri, out);
        }
        Stmt::Yield(yield_stmt) => {
            collect_handle_sites_in_expr(&yield_stmt.value, effect_name, uri, out);
        }
        Stmt::Emit(emit) => {
            collect_handle_sites_in_expr(&emit.value, effect_name, uri, out);
        }
        Stmt::Halt(halt) => {
            collect_handle_sites_in_expr(&halt.message, effect_name, uri, out);
        }
        _ => {}
    }
}

fn collect_handle_sites_in_match_arm(
    arm: &MatchArm,
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    collect_handle_sites_in_pattern(&arm.pattern, effect_name, uri, out);
    collect_handle_sites_in_stmts(&arm.body, effect_name, uri, out);
}

fn collect_handle_sites_in_pattern(
    pat: &Pattern,
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    match pat {
        Pattern::Guard {
            inner, condition, ..
        } => {
            collect_handle_sites_in_pattern(inner, effect_name, uri, out);
            collect_handle_sites_in_expr(condition, effect_name, uri, out);
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                collect_handle_sites_in_pattern(p, effect_name, uri, out);
            }
        }
        Pattern::ListDestructure { elements, .. } => {
            for p in elements {
                collect_handle_sites_in_pattern(p, effect_name, uri, out);
            }
        }
        Pattern::TupleDestructure { elements, .. } => {
            for p in elements {
                collect_handle_sites_in_pattern(p, effect_name, uri, out);
            }
        }
        Pattern::RecordDestructure { fields, .. } => {
            for (_field_name, sub_pat) in fields {
                if let Some(p) = sub_pat {
                    collect_handle_sites_in_pattern(p, effect_name, uri, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_handle_sites_in_expr(
    expr: &Expr,
    effect_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    match expr {
        Expr::HandleExpr {
            body,
            handlers,
            span,
        } => {
            // Check if any handler references the target effect
            let has_matching_handler = handlers.iter().any(|h| h.effect_name == effect_name);
            if has_matching_handler {
                out.push(span_to_location(span, uri));
            }
            // Continue searching in nested expressions
            collect_handle_sites_in_stmts(body, effect_name, uri, out);
            for handler in handlers {
                collect_handle_sites_in_stmts(&handler.body, effect_name, uri, out);
            }
        }
        Expr::Call(func, args, _) => {
            collect_handle_sites_in_expr(func, effect_name, uri, out);
            for arg in args {
                match arg {
                    CallArg::Positional(e) => {
                        collect_handle_sites_in_expr(e, effect_name, uri, out);
                    }
                    CallArg::Named(_, e, _) => {
                        collect_handle_sites_in_expr(e, effect_name, uri, out);
                    }
                    CallArg::Role(_, e, _) => {
                        collect_handle_sites_in_expr(e, effect_name, uri, out);
                    }
                }
            }
        }
        Expr::BinOp(left, _, right, _) => {
            collect_handle_sites_in_expr(left, effect_name, uri, out);
            collect_handle_sites_in_expr(right, effect_name, uri, out);
        }
        Expr::UnaryOp(_, inner, _) => {
            collect_handle_sites_in_expr(inner, effect_name, uri, out);
        }
        Expr::DotAccess(inner, _, _) => {
            collect_handle_sites_in_expr(inner, effect_name, uri, out);
        }
        Expr::IndexAccess(inner, idx, _) => {
            collect_handle_sites_in_expr(inner, effect_name, uri, out);
            collect_handle_sites_in_expr(idx, effect_name, uri, out);
        }
        Expr::ListLit(items, _) | Expr::TupleLit(items, _) | Expr::SetLit(items, _) => {
            for item in items {
                collect_handle_sites_in_expr(item, effect_name, uri, out);
            }
        }
        Expr::MapLit(entries, _) => {
            for (k, v) in entries {
                collect_handle_sites_in_expr(k, effect_name, uri, out);
                collect_handle_sites_in_expr(v, effect_name, uri, out);
            }
        }
        Expr::RecordLit(_, fields, _) => {
            for (_, val) in fields {
                collect_handle_sites_in_expr(val, effect_name, uri, out);
            }
        }
        Expr::Lambda {
            params: _, body, ..
        } => match body {
            LambdaBody::Expr(e) => collect_handle_sites_in_expr(e, effect_name, uri, out),
            LambdaBody::Block(stmts) => {
                collect_handle_sites_in_stmts(stmts, effect_name, uri, out);
            }
        },
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_handle_sites_in_expr(cond, effect_name, uri, out);
            collect_handle_sites_in_expr(then_val, effect_name, uri, out);
            collect_handle_sites_in_expr(else_val, effect_name, uri, out);
        }
        Expr::Pipe { left, right, .. } => {
            collect_handle_sites_in_expr(left, effect_name, uri, out);
            collect_handle_sites_in_expr(right, effect_name, uri, out);
        }
        Expr::TryExpr(inner, _) | Expr::AwaitExpr(inner, _) | Expr::ResumeExpr(inner, _) => {
            collect_handle_sites_in_expr(inner, effect_name, uri, out);
        }
        Expr::NullCoalesce(left, right, _) => {
            collect_handle_sites_in_expr(left, effect_name, uri, out);
            collect_handle_sites_in_expr(right, effect_name, uri, out);
        }
        Expr::BlockExpr(stmts, _) => {
            collect_handle_sites_in_stmts(stmts, effect_name, uri, out);
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_handle_sites_in_expr(subject, effect_name, uri, out);
            for arm in arms {
                collect_handle_sites_in_match_arm(arm, effect_name, uri, out);
            }
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_handle_sites_in_expr(body, effect_name, uri, out);
            collect_handle_sites_in_expr(iter, effect_name, uri, out);
            if let Some(cond) = condition {
                collect_handle_sites_in_expr(cond, effect_name, uri, out);
            }
        }
        Expr::Perform { args, .. } => {
            for arg in args {
                collect_handle_sites_in_expr(arg, effect_name, uri, out);
            }
        }
        _ => {}
    }
}

/// Find handler declarations that handle the given effect.
fn collect_handler_decls(prog: &Program, effect_name: &str, uri: &Uri, out: &mut Vec<Location>) {
    for item in &prog.items {
        if let Item::Handler(handler) = item {
            // Handler name often matches or references the effect
            if handler.name == effect_name {
                out.push(span_to_location(&handler.span, uri));
            }
        }
    }
}

/// Find all record construction sites (`RecordName(field: value, ...)`) in the program.
fn collect_record_construction_sites(
    prog: &Program,
    record_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                collect_record_constructions_in_cell(cell, record_name, uri, out);
            }
            Item::Process(process) => {
                for cell in &process.cells {
                    collect_record_constructions_in_cell(cell, record_name, uri, out);
                }
            }
            Item::Enum(enum_def) => {
                for method in &enum_def.methods {
                    collect_record_constructions_in_cell(method, record_name, uri, out);
                }
            }
            Item::Impl(impl_def) => {
                for cell in &impl_def.cells {
                    collect_record_constructions_in_cell(cell, record_name, uri, out);
                }
            }
            Item::Record(record) => {
                // Also check default field values
                for field in &record.fields {
                    if let Some(default) = &field.default_value {
                        collect_record_constructions_in_expr(default, record_name, uri, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_record_constructions_in_cell(
    cell: &CellDef,
    record_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    for stmt in &cell.body {
        collect_record_constructions_in_stmt(stmt, record_name, uri, out);
    }
}

fn collect_record_constructions_in_stmt(
    stmt: &Stmt,
    record_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    match stmt {
        Stmt::Let(let_stmt) => {
            collect_record_constructions_in_expr(&let_stmt.value, record_name, uri, out);
        }
        Stmt::Assign(assign) => {
            collect_record_constructions_in_expr(&assign.value, record_name, uri, out);
        }
        Stmt::If(if_stmt) => {
            collect_record_constructions_in_expr(&if_stmt.condition, record_name, uri, out);
            for s in &if_stmt.then_body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    collect_record_constructions_in_stmt(s, record_name, uri, out);
                }
            }
        }
        Stmt::While(while_stmt) => {
            collect_record_constructions_in_expr(&while_stmt.condition, record_name, uri, out);
            for s in &while_stmt.body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
        }
        Stmt::For(for_stmt) => {
            collect_record_constructions_in_expr(&for_stmt.iter, record_name, uri, out);
            for s in &for_stmt.body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
        }
        Stmt::Match(match_stmt) => {
            collect_record_constructions_in_expr(&match_stmt.subject, record_name, uri, out);
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    collect_record_constructions_in_stmt(s, record_name, uri, out);
                }
            }
        }
        Stmt::Return(ret) => {
            collect_record_constructions_in_expr(&ret.value, record_name, uri, out);
        }
        Stmt::Expr(expr_stmt) => {
            collect_record_constructions_in_expr(&expr_stmt.expr, record_name, uri, out);
        }
        Stmt::CompoundAssign(ca) => {
            collect_record_constructions_in_expr(&ca.value, record_name, uri, out);
        }
        Stmt::Defer(defer) => {
            for s in &defer.body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
        }
        Stmt::Yield(yield_stmt) => {
            collect_record_constructions_in_expr(&yield_stmt.value, record_name, uri, out);
        }
        Stmt::Emit(emit) => {
            collect_record_constructions_in_expr(&emit.value, record_name, uri, out);
        }
        Stmt::Halt(halt) => {
            collect_record_constructions_in_expr(&halt.message, record_name, uri, out);
        }
        _ => {}
    }
}

fn collect_record_constructions_in_expr(
    expr: &Expr,
    record_name: &str,
    uri: &Uri,
    out: &mut Vec<Location>,
) {
    match expr {
        Expr::RecordLit(type_name, fields, span) => {
            if type_name == record_name {
                out.push(span_to_location(span, uri));
            }
            for (_, val) in fields {
                collect_record_constructions_in_expr(val, record_name, uri, out);
            }
        }
        // The parser may represent record construction as a Call with an Ident callee
        // (before type resolution distinguishes records from functions)
        Expr::Call(func, args, span) => {
            if let Expr::Ident(name, _) = func.as_ref() {
                if name == record_name {
                    out.push(span_to_location(span, uri));
                }
            }
            collect_record_constructions_in_expr(func, record_name, uri, out);
            for arg in args {
                match arg {
                    CallArg::Positional(e) => {
                        collect_record_constructions_in_expr(e, record_name, uri, out);
                    }
                    CallArg::Named(_, e, _) => {
                        collect_record_constructions_in_expr(e, record_name, uri, out);
                    }
                    CallArg::Role(_, e, _) => {
                        collect_record_constructions_in_expr(e, record_name, uri, out);
                    }
                }
            }
        }
        Expr::BinOp(left, _, right, _) => {
            collect_record_constructions_in_expr(left, record_name, uri, out);
            collect_record_constructions_in_expr(right, record_name, uri, out);
        }
        Expr::UnaryOp(_, inner, _) => {
            collect_record_constructions_in_expr(inner, record_name, uri, out);
        }
        Expr::DotAccess(inner, _, _) => {
            collect_record_constructions_in_expr(inner, record_name, uri, out);
        }
        Expr::IndexAccess(inner, idx, _) => {
            collect_record_constructions_in_expr(inner, record_name, uri, out);
            collect_record_constructions_in_expr(idx, record_name, uri, out);
        }
        Expr::ListLit(items, _) | Expr::TupleLit(items, _) | Expr::SetLit(items, _) => {
            for item in items {
                collect_record_constructions_in_expr(item, record_name, uri, out);
            }
        }
        Expr::MapLit(entries, _) => {
            for (k, v) in entries {
                collect_record_constructions_in_expr(k, record_name, uri, out);
                collect_record_constructions_in_expr(v, record_name, uri, out);
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => {
                collect_record_constructions_in_expr(e, record_name, uri, out);
            }
            LambdaBody::Block(stmts) => {
                for s in stmts {
                    collect_record_constructions_in_stmt(s, record_name, uri, out);
                }
            }
        },
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_record_constructions_in_expr(cond, record_name, uri, out);
            collect_record_constructions_in_expr(then_val, record_name, uri, out);
            collect_record_constructions_in_expr(else_val, record_name, uri, out);
        }
        Expr::Pipe { left, right, .. } => {
            collect_record_constructions_in_expr(left, record_name, uri, out);
            collect_record_constructions_in_expr(right, record_name, uri, out);
        }
        Expr::TryExpr(inner, _) | Expr::AwaitExpr(inner, _) | Expr::ResumeExpr(inner, _) => {
            collect_record_constructions_in_expr(inner, record_name, uri, out);
        }
        Expr::NullCoalesce(left, right, _) => {
            collect_record_constructions_in_expr(left, record_name, uri, out);
            collect_record_constructions_in_expr(right, record_name, uri, out);
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_record_constructions_in_expr(subject, record_name, uri, out);
            for arm in arms {
                for s in &arm.body {
                    collect_record_constructions_in_stmt(s, record_name, uri, out);
                }
            }
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_record_constructions_in_expr(body, record_name, uri, out);
            collect_record_constructions_in_expr(iter, record_name, uri, out);
            if let Some(cond) = condition {
                collect_record_constructions_in_expr(cond, record_name, uri, out);
            }
        }
        Expr::HandleExpr { body, handlers, .. } => {
            for s in body {
                collect_record_constructions_in_stmt(s, record_name, uri, out);
            }
            for handler in handlers {
                for s in &handler.body {
                    collect_record_constructions_in_stmt(s, record_name, uri, out);
                }
            }
        }
        Expr::Perform { args, .. } => {
            for arg in args {
                collect_record_constructions_in_expr(arg, record_name, uri, out);
            }
        }
        _ => {}
    }
}

/// Find all `impl TraitName for Type` blocks for the given trait name.
fn collect_impl_blocks(prog: &Program, trait_name: &str, uri: &Uri, out: &mut Vec<Location>) {
    for item in &prog.items {
        if let Item::Impl(impl_def) = item {
            if impl_def.trait_name == trait_name {
                out.push(span_to_location(&impl_def.span, uri));
            }
        }
    }
}

/// Convert a compiler Span to an LSP Location.
fn span_to_location(span: &Span, uri: &Uri) -> Location {
    let line = if span.line > 0 {
        (span.line - 1) as u32
    } else {
        0
    };
    Location {
        uri: uri.clone(),
        range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: u32::MAX,
            },
        },
    }
}

fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;

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
        return None;
    }

    Some(line[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Uri;

    fn make_uri() -> Uri {
        "file:///test.lm".parse().unwrap()
    }

    fn parse_program(source: &str) -> Option<Program> {
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
        parser.parse_program(vec![]).ok()
    }

    #[test]
    fn test_cell_definition_found() {
        let source = "cell greet(name: String) -> String\n  return name\nend";
        let program = parse_program(source);
        let uri = make_uri();

        let result = build_implementations(
            Position {
                line: 0,
                character: 5,
            },
            source,
            program.as_ref(),
            &uri,
        );
        assert!(result.is_some(), "Should find cell definition site");
    }

    #[test]
    fn test_record_construction_found() {
        let source = concat!(
            "record Point\n",
            "  x: Int\n",
            "  y: Int\n",
            "end\n",
            "\n",
            "cell make() -> Point\n",
            "  return Point(x: 1, y: 2)\n",
            "end\n",
        );
        let program = parse_program(source);
        assert!(program.is_some(), "Program should parse successfully");
        let prog = program.as_ref().unwrap();

        // Check what the word extraction gives us
        let word = extract_word_at_position(
            source,
            Position {
                line: 0,
                character: 8,
            },
        );
        assert_eq!(
            word,
            Some("Point".to_string()),
            "Should extract 'Point' at position"
        );

        // Check the cell body for RecordLit
        let uri = make_uri();

        // Directly test record construction collection
        let mut locations = Vec::new();
        collect_record_construction_sites(prog, "Point", &uri, &mut locations);
        assert!(
            !locations.is_empty(),
            "Should find record construction locations directly"
        );

        let result = build_implementations(
            Position {
                line: 0,
                character: 8,
            },
            source,
            program.as_ref(),
            &uri,
        );
        assert!(result.is_some(), "Should find record construction site");
        match result.unwrap() {
            GotoDefinitionResponse::Scalar(loc) => {
                // Should point to the RecordLit on line 6
                assert_eq!(loc.range.start.line, 6);
            }
            GotoDefinitionResponse::Array(locs) => {
                // May return multiple if there are multiple construction sites
                assert!(!locs.is_empty());
            }
            _ => panic!("Unexpected response type"),
        }
    }

    #[test]
    fn test_trait_impl_found() {
        let source = concat!(
            "trait Printable\n",
            "  cell to_string(self) -> String\n",
            "end\n",
            "\n",
            "impl Printable for Int\n",
            "  cell to_string(self) -> String\n",
            "    return \"int\"\n",
            "  end\n",
            "end\n",
        );
        let program = parse_program(source);
        let uri = make_uri();

        let result = build_implementations(
            Position {
                line: 0,
                character: 8,
            },
            source,
            program.as_ref(),
            &uri,
        );
        assert!(result.is_some(), "Should find impl block for trait");
    }

    #[test]
    fn test_no_implementations_returns_none() {
        let source = "cell main() -> Int\n  return 0\nend";
        let program = parse_program(source);
        let uri = make_uri();

        // Query for a name that doesn't exist
        let result = build_implementations(
            Position {
                line: 0,
                character: 0,
            },
            source,
            program.as_ref(),
            &uri,
        );
        // "cell" is a keyword, not a symbol name with implementations
        assert!(result.is_none());
    }

    #[test]
    fn test_effect_handle_site_found() {
        let source = concat!(
            "effect Console\n",
            "  cell log(msg: String) -> Null\n",
            "end\n",
            "\n",
            "cell main() -> Int\n",
            "  let result = handle\n",
            "    perform Console.log(\"hello\")\n",
            "    42\n",
            "  with\n",
            "    Console.log(msg) => resume(null)\n",
            "  end\n",
            "  return result\n",
            "end\n",
        );
        let program = parse_program(source);
        assert!(program.is_some(), "Effect program should parse");
        let uri = make_uri();

        let result = build_implementations(
            Position {
                line: 0,
                character: 8,
            },
            source,
            program.as_ref(),
            &uri,
        );
        assert!(
            result.is_some(),
            "Should find handle site for effect Console"
        );
    }
}
