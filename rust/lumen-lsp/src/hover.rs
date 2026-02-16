//! Hover information with type signatures

use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Position};
use lumen_compiler::compiler::ast::{Item, Program};

pub fn build_hover(params: HoverParams, text: &str, program: Option<&Program>) -> Option<Hover> {
    let position = params.text_document_position_params.position;
    let word = extract_word_at_position(text, position)?;

    if let Some(prog) = program {
        for item in &prog.items {
            match item {
                Item::Cell(cell) if cell.name == word => {
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

                    return Some(make_hover(&signature, cell.doc.as_deref()));
                }
                Item::Record(record) if record.name == word => {
                    let fields_str = record
                        .fields
                        .iter()
                        .map(|f| format!("  {}: {}", f.name, type_expr_to_string(&f.ty)))
                        .collect::<Vec<_>>()
                        .join("\n");

                    let signature = format!("record {}\n{}\nend", record.name, fields_str);

                    return Some(make_hover(&signature, record.doc.as_deref()));
                }
                Item::Enum(enum_def) if enum_def.name == word => {
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

                    return Some(make_hover(&signature, enum_def.doc.as_deref()));
                }
                Item::TypeAlias(alias) if alias.name == word => {
                    let signature = format!(
                        "type {} = {}",
                        alias.name,
                        type_expr_to_string(&alias.type_expr)
                    );

                    return Some(make_hover(&signature, alias.doc.as_deref()));
                }
                Item::Process(process) if process.name == word => {
                    let signature = format!("process {} {}", process.kind, process.name);

                    return Some(make_hover(&signature, None));
                }
                Item::Effect(effect) if effect.name == word => {
                    let signature = format!("effect {}", effect.name);

                    return Some(make_hover(&signature, None));
                }
                Item::Handler(handler) if handler.name == word => {
                    let signature = format!("handler {}", handler.name);

                    return Some(make_hover(&signature, handler.doc.as_deref()));
                }
                _ => {}
            }
        }
    }

    // Check for builtins
    get_builtin_hover(&word)
}

/// Build a hover result with an optional docstring prepended above the signature.
fn make_hover(signature: &str, doc: Option<&str>) -> Hover {
    let code_block = format!("```lumen\n{}\n```", signature);
    let value = match doc {
        Some(content) if !content.is_empty() => {
            format!("{}\n\n---\n\n{}", content, code_block)
        }
        _ => code_block,
    };

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

fn get_builtin_hover(name: &str) -> Option<Hover> {
    let builtin_docs = vec![
        ("print", "print(value) -> Void", "Output a value to stdout"),
        (
            "len",
            "len(collection) -> Int",
            "Get the length of a collection",
        ),
        (
            "append",
            "append(list, item) -> list",
            "Append an item to a list",
        ),
        ("sort", "sort(list) -> list", "Sort a list"),
        ("map", "map(list, fn) -> list", "Transform each element"),
        ("filter", "filter(list, fn) -> list", "Filter by predicate"),
        (
            "reduce",
            "reduce(list, init, fn) -> value",
            "Reduce to single value",
        ),
        ("join", "join(list, separator) -> String", "Join to string"),
        (
            "split",
            "split(string, separator) -> list[String]",
            "Split string",
        ),
        ("trim", "trim(string) -> String", "Trim whitespace"),
        (
            "parse_int",
            "parse_int(string) -> result[Int, String]",
            "Parse to int",
        ),
        (
            "parse_float",
            "parse_float(string) -> result[Float, String]",
            "Parse to float",
        ),
        (
            "to_string",
            "to_string(value) -> String",
            "Convert to string",
        ),
        (
            "contains",
            "contains(collection, item) -> Bool",
            "Check membership",
        ),
        ("keys", "keys(map) -> list", "Get map keys"),
        ("values", "values(map) -> list", "Get map values"),
        ("parallel", "parallel(futures) -> list", "Run in parallel"),
        ("race", "race(futures) -> value", "Race futures"),
        (
            "vote",
            "vote(futures, threshold) -> value",
            "Vote on results",
        ),
        ("select", "select(futures) -> value", "Select first"),
        ("timeout", "timeout(future, ms) -> result", "Timeout future"),
    ];

    for (builtin_name, signature, doc) in builtin_docs {
        if builtin_name == name {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```lumen\n{}\n```\n\n{}", signature, doc),
                }),
                range: None,
            });
        }
    }

    None
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

pub fn type_expr_to_string(ty: &lumen_compiler::compiler::ast::TypeExpr) -> String {
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
