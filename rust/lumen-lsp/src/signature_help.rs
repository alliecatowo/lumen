//! Signature help provider — shows function signatures while typing arguments

use lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureHelpParams, SignatureInformation,
};
use lumen_compiler::compiler::ast::{Item, Program};

use crate::hover::type_expr_to_string;

pub fn build_signature_help(
    params: SignatureHelpParams,
    text: &str,
    program: Option<&Program>,
) -> Option<SignatureHelp> {
    let position = params.text_document_position_params.position;
    let (call_name, active_param) = find_call_context(text, position)?;
    let prog = program?;

    // Search user-defined cells
    for item in &prog.items {
        if let Item::Cell(cell) = item {
            if cell.name == call_name {
                return Some(build_cell_signature(cell, active_param));
            }
        }
    }

    // Search builtins
    build_builtin_signature(&call_name, active_param)
}

/// Walk backwards from the cursor to find the function name before `(` and count commas
/// to determine the active parameter index.
fn find_call_context(text: &str, position: Position) -> Option<(String, u32)> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = (position.character as usize).min(line.len());

    let prefix = &line[..col];

    // Count unmatched commas and find the opening paren
    let mut depth = 0i32;
    let mut commas = 0u32;
    let mut paren_pos = None;

    for (i, ch) in prefix.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    paren_pos = Some(i);
                    break;
                }
                depth -= 1;
            }
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }

    let paren_idx = paren_pos?;

    // Extract the word immediately before the `(`
    let before_paren = &prefix[..paren_idx];
    let name_start = before_paren
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let name = &before_paren[name_start..];
    if name.is_empty() {
        return None;
    }

    Some((name.to_string(), commas))
}

fn build_cell_signature(
    cell: &lumen_compiler::compiler::ast::CellDef,
    active_param: u32,
) -> SignatureHelp {
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

    let label = format!(
        "cell {}({}){}{}",
        cell.name, params_str, return_str, effects_str
    );

    let parameters: Vec<ParameterInformation> = cell
        .params
        .iter()
        .map(|p| {
            let param_label = format!("{}: {}", p.name, type_expr_to_string(&p.ty));
            ParameterInformation {
                label: ParameterLabel::Simple(param_label),
                documentation: None,
            }
        })
        .collect();

    let documentation = cell.doc.as_ref().map(|d| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: d.clone(),
        })
    });

    SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation,
            parameters: Some(parameters),
            active_parameter: Some(active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_param),
    }
}

fn build_builtin_signature(name: &str, active_param: u32) -> Option<SignatureHelp> {
    let builtins: Vec<(&str, &str, &[&str])> = vec![
        ("print", "print(value) -> Void", &["value"]),
        ("len", "len(collection) -> Int", &["collection"]),
        ("append", "append(list, item) -> list", &["list", "item"]),
        ("sort", "sort(list) -> list", &["list"]),
        ("map", "map(list, fn) -> list", &["list", "fn"]),
        ("filter", "filter(list, fn) -> list", &["list", "fn"]),
        (
            "reduce",
            "reduce(list, init, fn) -> value",
            &["list", "init", "fn"],
        ),
        (
            "join",
            "join(list, separator) -> String",
            &["list", "separator"],
        ),
        (
            "split",
            "split(string, separator) -> list[String]",
            &["string", "separator"],
        ),
        ("trim", "trim(string) -> String", &["string"]),
        (
            "parse_int",
            "parse_int(string) -> result[Int, String]",
            &["string"],
        ),
        (
            "parse_float",
            "parse_float(string) -> result[Float, String]",
            &["string"],
        ),
        ("to_string", "to_string(value) -> String", &["value"]),
        (
            "contains",
            "contains(collection, item) -> Bool",
            &["collection", "item"],
        ),
        ("keys", "keys(map) -> list", &["map"]),
        ("values", "values(map) -> list", &["map"]),
        ("parallel", "parallel(futures) -> list", &["futures"]),
        ("race", "race(futures) -> value", &["futures"]),
        (
            "vote",
            "vote(futures, threshold) -> value",
            &["futures", "threshold"],
        ),
        ("select", "select(futures) -> value", &["futures"]),
        (
            "timeout",
            "timeout(future, ms) -> result",
            &["future", "ms"],
        ),
    ];

    for (builtin_name, label, params) in builtins {
        if builtin_name == name {
            let parameters: Vec<ParameterInformation> = params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(p.to_string()),
                    documentation: None,
                })
                .collect();

            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label: label.to_string(),
                    documentation: None,
                    parameters: Some(parameters),
                    active_parameter: Some(active_param),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            });
        }
    }

    None
}
