//! Auto-generate documentation from Lumen source files.

use lumen_compiler::compiler::ast::{
    CellDef, EnumDef, Item, Program, RecordDef, TypeAliasDef, TypeExpr,
};
use lumen_compiler::markdown::extract::{extract_blocks, CodeBlock};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

/// Documentation for a module
#[derive(Debug)]
struct ModuleDoc {
    name: String,
    cells: Vec<CellDoc>,
    records: Vec<RecordDoc>,
    enums: Vec<EnumDoc>,
    type_aliases: Vec<TypeAliasDoc>,
}

#[derive(Debug)]
struct CellDoc {
    name: String,
    params: Vec<(String, String)>, // (name, type)
    return_type: String,
    effects: Vec<String>,
    doc_comment: String,
}

#[derive(Debug)]
struct RecordDoc {
    name: String,
    fields: Vec<(String, String)>, // (name, type)
    doc_comment: String,
}

#[derive(Debug)]
struct EnumDoc {
    name: String,
    variants: Vec<String>,
    doc_comment: String,
}

#[derive(Debug)]
struct TypeAliasDoc {
    name: String,
    target_type: String,
    doc_comment: String,
}

/// Generate documentation for a single file
pub fn cmd_doc(path: &Path, format: &str, output: Option<&Path>) -> Result<(), String> {
    if path.is_dir() {
        // Generate docs for all supported Lumen source files in directory
        generate_directory_docs(path, format, output)
    } else {
        // Generate docs for single file
        generate_file_docs(path, format, output)
    }
}

fn generate_directory_docs(dir: &Path, format: &str, output: Option<&Path>) -> Result<(), String> {
    let mut all_docs = Vec::new();

    // Find all supported Lumen source files
    for entry in std::fs::read_dir(dir).map_err(|e| format!("Cannot read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Cannot read entry: {}", e))?;
        let path = entry.path();
        if is_lumen_source(&path) {
            let source = std::fs::read_to_string(&path)
                .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
            let filename = module_name_for_path(&path);
            let doc = extract_module_doc(&source, filename)?;
            all_docs.push(doc);
        }
    }

    let rendered = match format {
        "json" => render_docs_json(&all_docs),
        _ => render_docs_markdown(&all_docs),
    };

    write_output(&rendered, output)
}

fn generate_file_docs(path: &Path, format: &str, output: Option<&Path>) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    let filename = module_name_for_path(path);

    let doc = extract_module_doc(&source, filename)?;

    let rendered = match format {
        "json" => render_doc_json(&doc),
        _ => render_doc_markdown(&doc),
    };

    write_output(&rendered, output)
}

fn is_lumen_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            name.ends_with(".lm")
                || name.ends_with(".lumen")
                || name.ends_with(".lm.md")
                || name.ends_with(".lumen.md")
        })
        .unwrap_or(false)
}

fn module_name_for_path(path: &Path) -> &str {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    if let Some(stripped) = name.strip_suffix(".lm.md") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(".lumen.md") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(".lm") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(".lumen") {
        stripped
    } else {
        name
    }
}

fn write_output(content: &str, output: Option<&Path>) -> Result<(), String> {
    if let Some(out_path) = output {
        std::fs::write(out_path, content)
            .map_err(|e| format!("Cannot write to {}: {}", out_path.display(), e))?;
    } else {
        println!("{}", content);
    }
    Ok(())
}

/// Extract documentation from source
fn extract_module_doc(source: &str, module_name: &str) -> Result<ModuleDoc, String> {
    let extracted = extract_blocks(source);

    // Extract prose comments that precede code blocks
    let prose_map = extract_prose_comments(source, &extracted.code_blocks);

    // Concatenate all code blocks
    let mut combined_code = String::new();
    for block in &extracted.code_blocks {
        if !combined_code.is_empty() {
            combined_code.push('\n');
        }
        combined_code.push_str(&block.code);
    }

    // Extract declarations with their doc comments
    let mut cells = Vec::new();
    let mut records = Vec::new();
    let mut enums = Vec::new();
    let mut type_aliases = Vec::new();

    // Parse to AST
    let ast_program = parse_to_ast(&combined_code)?;

    for item in &ast_program.items {
        match item {
            Item::Cell(c) => {
                let doc_comment = find_doc_for_span(c.span.start, &prose_map);
                cells.push(extract_cell_doc(c, &doc_comment));
            }
            Item::Record(r) => {
                let doc_comment = find_doc_for_span(r.span.start, &prose_map);
                records.push(extract_record_doc(r, &doc_comment));
            }
            Item::Enum(e) => {
                let doc_comment = find_doc_for_span(e.span.start, &prose_map);
                enums.push(extract_enum_doc(e, &doc_comment));
            }
            Item::TypeAlias(t) => {
                let doc_comment = find_doc_for_span(t.span.start, &prose_map);
                type_aliases.push(extract_type_alias_doc(t, &doc_comment));
            }
            _ => {}
        }
    }

    Ok(ModuleDoc {
        name: module_name.to_string(),
        cells,
        records,
        enums,
        type_aliases,
    })
}

fn parse_to_ast(source: &str) -> Result<Program, String> {
    use lumen_compiler::compiler::{lexer::Lexer, parser::Parser};

    let mut lexer = Lexer::new(source, 1, 0);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Tokenize error: {:?}", e))?;

    let mut parser = Parser::new(tokens);
    parser
        .parse_program(vec![])
        .map_err(|e| format!("Parse error: {:?}", e))
}

fn find_doc_for_span(start: usize, prose_map: &HashMap<usize, String>) -> String {
    prose_map.get(&start).cloned().unwrap_or_default()
}

/// Extract prose that appears before each code block
fn extract_prose_comments(source: &str, blocks: &[CodeBlock]) -> HashMap<usize, String> {
    let mut map = HashMap::new();
    let lines: Vec<&str> = source.lines().collect();

    for block in blocks {
        // Look backward from block start line to find prose
        let block_start_line = block.code_start_line;
        let mut prose_lines = Vec::new();

        // Scan backwards from fence start
        let fence_line = block_start_line.saturating_sub(1);
        if fence_line == 0 {
            continue;
        }

        for line_idx in (0..fence_line.saturating_sub(1)).rev() {
            let line = lines.get(line_idx).unwrap_or(&"");
            let trimmed = line.trim();

            // Stop at empty line, fence, or heading
            if trimmed.is_empty()
                || trimmed.starts_with("```")
                || trimmed.starts_with('#')
                || trimmed.starts_with('@')
            {
                break;
            }

            prose_lines.insert(0, trimmed.to_string());
        }

        if !prose_lines.is_empty() {
            let prose = prose_lines.join(" ");
            map.insert(block.code_offset, prose);
        }
    }

    map
}

fn extract_cell_doc(cell: &CellDef, doc_comment: &str) -> CellDoc {
    let params = cell
        .params
        .iter()
        .map(|p| (p.name.clone(), type_to_string(&p.ty)))
        .collect();

    let return_type = cell
        .return_type
        .as_ref()
        .map(type_to_string)
        .unwrap_or_else(|| "void".to_string());

    let effects = cell
        .effects
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>();

    CellDoc {
        name: cell.name.clone(),
        params,
        return_type,
        effects,
        doc_comment: doc_comment.to_string(),
    }
}

fn extract_record_doc(record: &RecordDef, doc_comment: &str) -> RecordDoc {
    let fields = record
        .fields
        .iter()
        .map(|f| (f.name.clone(), type_to_string(&f.ty)))
        .collect();

    RecordDoc {
        name: record.name.clone(),
        fields,
        doc_comment: doc_comment.to_string(),
    }
}

fn extract_enum_doc(enum_def: &EnumDef, doc_comment: &str) -> EnumDoc {
    let variants = enum_def
        .variants
        .iter()
        .map(|v| {
            if let Some(ref payload) = v.payload {
                format!("{}({})", v.name, type_to_string(payload))
            } else {
                v.name.clone()
            }
        })
        .collect();

    EnumDoc {
        name: enum_def.name.clone(),
        variants,
        doc_comment: doc_comment.to_string(),
    }
}

fn extract_type_alias_doc(alias: &TypeAliasDef, doc_comment: &str) -> TypeAliasDoc {
    TypeAliasDoc {
        name: alias.name.clone(),
        target_type: type_to_string(&alias.type_expr),
        doc_comment: doc_comment.to_string(),
    }
}

fn type_to_string(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name, _) => name.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", type_to_string(inner)),
        TypeExpr::Map(k, v, _) => format!("map[{}, {}]", type_to_string(k), type_to_string(v)),
        TypeExpr::Result(ok, err, _) => {
            format!("result[{}, {}]", type_to_string(ok), type_to_string(err))
        }
        TypeExpr::Union(types, _) => types
            .iter()
            .map(type_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Null(_) => "null".to_string(),
        TypeExpr::Tuple(types, _) => {
            let inner = types
                .iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
        TypeExpr::Set(inner, _) => format!("set[{}]", type_to_string(inner)),
        TypeExpr::Fn(params, ret, effects, _) => {
            let param_str = params
                .iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let effect_str = if effects.is_empty() {
                String::new()
            } else {
                format!(" / {{{}}}", effects.join(", "))
            };
            format!("fn({}) -> {}{}", param_str, type_to_string(ret), effect_str)
        }
        TypeExpr::Generic(name, args, _) => {
            let arg_str = args
                .iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}[{}]", name, arg_str)
        }
    }
}

/// Render documentation as Markdown
fn render_doc_markdown(doc: &ModuleDoc) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Module: {}\n\n", doc.name));

    if !doc.cells.is_empty() {
        out.push_str("## Cells\n\n");
        for cell in &doc.cells {
            let params_str = cell
                .params
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, ty))
                .collect::<Vec<_>>()
                .join(", ");

            let sig = if cell.effects.is_empty() {
                format!(
                    "### `{}({}) -> {}`\n",
                    cell.name, params_str, cell.return_type
                )
            } else {
                format!(
                    "### `{}({}) -> {} / {{{}}}`\n",
                    cell.name,
                    params_str,
                    cell.return_type,
                    cell.effects.join(", ")
                )
            };
            out.push_str(&sig);

            if !cell.doc_comment.is_empty() {
                out.push_str(&format!("{}\n", cell.doc_comment));
            }
            out.push('\n');
        }
    }

    if !doc.records.is_empty() {
        out.push_str("## Records\n\n");
        for record in &doc.records {
            out.push_str(&format!("### `{}`\n", record.name));
            if !record.doc_comment.is_empty() {
                out.push_str(&format!("{}\n\n", record.doc_comment));
            }
            out.push_str("| Field | Type |\n");
            out.push_str("|-------|------|\n");
            for (name, ty) in &record.fields {
                out.push_str(&format!("| {} | {} |\n", name, ty));
            }
            out.push('\n');
        }
    }

    if !doc.enums.is_empty() {
        out.push_str("## Enums\n\n");
        for enum_def in &doc.enums {
            out.push_str(&format!("### `{}`\n", enum_def.name));
            if !enum_def.doc_comment.is_empty() {
                out.push_str(&format!("{}\n\n", enum_def.doc_comment));
            }
            for variant in &enum_def.variants {
                out.push_str(&format!("- `{}`\n", variant));
            }
            out.push('\n');
        }
    }

    if !doc.type_aliases.is_empty() {
        out.push_str("## Type Aliases\n\n");
        for alias in &doc.type_aliases {
            out.push_str(&format!("### `{} = {}`\n", alias.name, alias.target_type));
            if !alias.doc_comment.is_empty() {
                out.push_str(&format!("{}\n", alias.doc_comment));
            }
            out.push('\n');
        }
    }

    out
}

fn render_docs_markdown(docs: &[ModuleDoc]) -> String {
    let mut out = String::new();
    for doc in docs {
        out.push_str(&render_doc_markdown(doc));
        out.push_str("\n---\n\n");
    }
    out
}

/// Render documentation as JSON
fn render_doc_json(doc: &ModuleDoc) -> String {
    let cells: Vec<_> = doc
        .cells
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "params": c.params,
                "return_type": c.return_type,
                "effects": c.effects,
                "doc": c.doc_comment,
            })
        })
        .collect();

    let records: Vec<_> = doc
        .records
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "fields": r.fields,
                "doc": r.doc_comment,
            })
        })
        .collect();

    let enums: Vec<_> = doc
        .enums
        .iter()
        .map(|e| {
            json!({
                "name": e.name,
                "variants": e.variants,
                "doc": e.doc_comment,
            })
        })
        .collect();

    let type_aliases: Vec<_> = doc
        .type_aliases
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "target": t.target_type,
                "doc": t.doc_comment,
            })
        })
        .collect();

    let output = json!({
        "module": doc.name,
        "cells": cells,
        "records": records,
        "enums": enums,
        "type_aliases": type_aliases,
    });

    serde_json::to_string_pretty(&output).unwrap()
}

fn render_docs_json(docs: &[ModuleDoc]) -> String {
    let modules: Vec<serde_json::Value> = docs
        .iter()
        .map(|doc| serde_json::from_str(&render_doc_json(doc)).unwrap())
        .collect();

    let output = json!({
        "modules": modules,
    });

    serde_json::to_string_pretty(&output).unwrap()
}
