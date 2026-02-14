//! Context-aware code completion

use lsp_types::{CompletionItem, CompletionItemKind, CompletionList, CompletionParams};
use lumen_compiler::compiler::ast::{Item, Program};

pub fn build_completion(
    _params: CompletionParams,
    _text: &str,
    program: Option<&Program>,
) -> CompletionList {
    let mut items = Vec::new();

    // Always add keywords
    add_keywords(&mut items);

    // Add builtin functions
    add_builtins(&mut items);

    // Add primitive types
    add_types(&mut items);

    // Add symbols from the parsed program
    if let Some(prog) = program {
        add_program_symbols(prog, &mut items);
    }

    CompletionList {
        is_incomplete: false,
        items,
    }
}

fn add_keywords(items: &mut Vec<CompletionItem>) {
    let keywords = vec![
        "cell", "record", "enum", "if", "else", "match", "for", "while", "loop", "return", "let",
        "mut", "end", "process", "memory", "machine", "pipeline", "grant", "effect", "bind",
        "handler", "addon", "use", "import", "as", "true", "false", "null", "async", "await",
        "break", "continue", "in", "and", "or", "not", "is", "state", "terminal", "to", "where",
        "when", "agent", "trait", "impl", "const", "type", "pub", "macro", "fn", "from",
        "orchestration", "schema", "expect", "role", "then", "step", "with", "yield", "emit",
        "try", "extern", "comptime",
    ];

    for keyword in keywords {
        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("keyword".to_string()),
            ..Default::default()
        });
    }
}

fn add_builtins(items: &mut Vec<CompletionItem>) {
    let builtins = vec![
        ("print", "print(value) -> Void", "Output a value to stdout"),
        ("len", "len(collection) -> Int", "Get the length of a collection"),
        (
            "append",
            "append(list, item) -> list",
            "Append an item to a list",
        ),
        ("sort", "sort(list) -> list", "Sort a list"),
        (
            "map",
            "map(list, fn) -> list",
            "Transform each element of a list",
        ),
        (
            "filter",
            "filter(list, fn) -> list",
            "Filter list elements by predicate",
        ),
        (
            "reduce",
            "reduce(list, init, fn) -> value",
            "Reduce list to a single value",
        ),
        (
            "join",
            "join(list, separator) -> String",
            "Join list elements into a string",
        ),
        (
            "split",
            "split(string, separator) -> list[String]",
            "Split string into a list",
        ),
        ("trim", "trim(string) -> String", "Trim whitespace"),
        (
            "parse_int",
            "parse_int(string) -> result[Int, String]",
            "Parse string to integer",
        ),
        (
            "parse_float",
            "parse_float(string) -> result[Float, String]",
            "Parse string to float",
        ),
        (
            "to_string",
            "to_string(value) -> String",
            "Convert value to string",
        ),
        (
            "contains",
            "contains(collection, item) -> Bool",
            "Check if collection contains item",
        ),
        ("keys", "keys(map) -> list", "Get map keys"),
        ("values", "values(map) -> list", "Get map values"),
        (
            "parallel",
            "parallel(futures) -> list",
            "Run futures in parallel",
        ),
        ("race", "race(futures) -> value", "Race futures"),
        (
            "vote",
            "vote(futures, threshold) -> value",
            "Vote on future results",
        ),
        ("select", "select(futures) -> value", "Select first future"),
        (
            "timeout",
            "timeout(future, ms) -> result",
            "Timeout a future",
        ),
        ("range", "range(start, end) -> list[Int]", "Create a range"),
        ("reverse", "reverse(list) -> list", "Reverse a list"),
        ("count", "count(collection) -> Int", "Count elements"),
        ("hash", "hash(value) -> String", "Hash a value"),
        ("abs", "abs(number) -> number", "Absolute value"),
        ("min", "min(a, b) -> number", "Minimum of two values"),
        ("max", "max(a, b) -> number", "Maximum of two values"),
        ("upper", "upper(string) -> String", "Convert to uppercase"),
        ("lower", "lower(string) -> String", "Convert to lowercase"),
        (
            "replace",
            "replace(string, from, to) -> String",
            "Replace substring",
        ),
    ];

    for (name, signature, doc) in builtins {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(signature.to_string()),
            documentation: Some(lsp_types::Documentation::String(doc.to_string())),
            ..Default::default()
        });
    }
}

fn add_types(items: &mut Vec<CompletionItem>) {
    let types = vec![
        ("String", "String type"),
        ("Int", "Integer type"),
        ("Float", "Floating point type"),
        ("Bool", "Boolean type"),
        ("Bytes", "Byte array type"),
        ("Json", "JSON type"),
        ("Void", "Void type"),
        ("list", "List type (generic)"),
        ("map", "Map type (generic)"),
        ("set", "Set type (generic)"),
        ("result", "Result type (generic)"),
    ];

    for (ty, doc) in types {
        items.push(CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("type".to_string()),
            documentation: Some(lsp_types::Documentation::String(doc.to_string())),
            ..Default::default()
        });
    }
}

fn add_program_symbols(program: &Program, items: &mut Vec<CompletionItem>) {
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

                let signature = format!("cell {}({}){}", cell.name, params_str, return_str);

                items.push(CompletionItem {
                    label: cell.name.clone(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some(signature),
                    ..Default::default()
                });
            }
            Item::Record(record) => {
                items.push(CompletionItem {
                    label: record.name.clone(),
                    kind: Some(CompletionItemKind::STRUCT),
                    detail: Some(format!("record {}", record.name)),
                    ..Default::default()
                });
            }
            Item::Enum(enum_def) => {
                items.push(CompletionItem {
                    label: enum_def.name.clone(),
                    kind: Some(CompletionItemKind::ENUM),
                    detail: Some(format!("enum {}", enum_def.name)),
                    ..Default::default()
                });

                // Add variants
                for variant in &enum_def.variants {
                    items.push(CompletionItem {
                        label: variant.name.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(format!("variant of {}", enum_def.name)),
                        ..Default::default()
                    });
                }
            }
            Item::TypeAlias(alias) => {
                items.push(CompletionItem {
                    label: alias.name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some(format!("type {}", alias.name)),
                    ..Default::default()
                });
            }
            Item::Process(process) => {
                items.push(CompletionItem {
                    label: process.name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some(format!("process {} {}", process.kind, process.name)),
                    ..Default::default()
                });
            }
            Item::Effect(effect) => {
                items.push(CompletionItem {
                    label: effect.name.clone(),
                    kind: Some(CompletionItemKind::INTERFACE),
                    detail: Some(format!("effect {}", effect.name)),
                    ..Default::default()
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
