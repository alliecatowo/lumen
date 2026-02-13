//! Interactive REPL for the Lumen language.

use std::io::{self, BufRead, Write};

use lumen_vm::values::Value;

// ANSI color helpers
fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}
fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}
fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}
fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}
fn gray(s: &str) -> String {
    format!("\x1b[90m{}\x1b[0m", s)
}

/// Keywords that open a block and require a matching `end`.
const BLOCK_OPENERS: &[&str] = &[
    "cell", "if", "while", "for", "match", "record", "enum", "loop",
];

/// Keywords that start a top-level item definition (not an expression).
const ITEM_KEYWORDS: &[&str] = &[
    "cell", "record", "enum", "process", "agent", "effect", "bind",
    "handler", "pipeline", "orchestration", "machine", "memory",
    "guardrail", "eval", "pattern", "grant", "import",
];

pub fn run_repl() {
    println!("{}", bold(&cyan("Lumen REPL v0.1.0")));
    println!("{}\n", gray("Type :help for help, :quit to exit"));

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut buffer = String::new();

    loop {
        // Show prompt
        if buffer.is_empty() {
            print!("{} ", green("lumen>"));
        } else {
            print!("{}    ", gray("..."));
        }
        if io::stdout().flush().is_err() {
            break;
        }

        // Read a line
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim();

        // Handle commands only on a fresh prompt (not mid-multiline)
        if buffer.is_empty() {
            if trimmed.is_empty() {
                continue;
            }
            match trimmed {
                ":quit" | ":q" => break,
                ":help" | ":h" => {
                    print_help();
                    continue;
                }
                ":reset" | ":r" => {
                    println!("{}", gray("State reset."));
                    continue;
                }
                _ if trimmed.starts_with(":type ") || trimmed.starts_with(":t ") => {
                    let expr = if trimmed.starts_with(":type ") {
                        &trimmed[6..]
                    } else {
                        &trimmed[3..]
                    };
                    cmd_type(expr);
                    continue;
                }
                _ => {}
            }
        }

        buffer.push_str(&line);

        // Check if we need more lines (unmatched block openers)
        if needs_more_input(&buffer) {
            continue;
        }

        // We have a complete input — evaluate it
        let input = buffer.trim().to_string();
        buffer.clear();

        if input.is_empty() {
            continue;
        }

        eval_input(&input);
    }

    println!("\n{}", cyan("Goodbye!"));
}

/// Determine if input has unmatched block openers that need more lines.
fn needs_more_input(input: &str) -> bool {
    let mut depth: i32 = 0;
    for word in input.split_whitespace() {
        if BLOCK_OPENERS.contains(&word) {
            depth += 1;
        } else if word == "end" {
            depth -= 1;
        }
    }
    depth > 0
}

/// Keywords that start a statement (not a bare expression).
const STMT_KEYWORDS: &[&str] = &[
    "let", "if", "while", "for", "match", "return", "halt",
    "loop", "break", "continue", "emit",
];

/// Check if the input looks like a top-level item definition.
fn is_item_definition(input: &str) -> bool {
    let first_word = input.split_whitespace().next().unwrap_or("");
    ITEM_KEYWORDS.contains(&first_word)
}

/// Check if the input is a statement (starts with a statement keyword).
fn is_statement(input: &str) -> bool {
    let first_word = input.split_whitespace().next().unwrap_or("");
    STMT_KEYWORDS.contains(&first_word)
}

/// Wrap input as a markdown source suitable for the compiler.
fn wrap_as_source(input: &str) -> String {
    if is_item_definition(input) {
        // Top-level item — wrap as-is
        format!("# repl\n\n```lumen\n{}\n```\n", input)
    } else if is_statement(input) {
        // Statement — let the parser wrap in synthetic main
        format!("# repl\n\n```lumen\n{}\n```\n", input)
    } else {
        // Expression — wrap in cell main() with explicit return
        format!(
            "# repl\n\n```lumen\ncell main()\n  return {}\nend\n```\n",
            input
        )
    }
}

/// Evaluate input: compile and run, printing the result.
fn eval_input(input: &str) {
    let source = wrap_as_source(input);

    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", red("Error:"), e);
            return;
        }
    };

    // Find the entry cell — prefer "main" (synthesized from top-level stmts)
    let entry = if module.cells.iter().any(|c| c.name == "main") {
        "main".to_string()
    } else if module.cells.iter().any(|c| c.name == "__script_main") {
        "__script_main".to_string()
    } else if module.cells.len() == 1 {
        module.cells[0].name.clone()
    } else {
        // Definition-only input (records, enums, etc.) — nothing to execute
        println!("{}", gray("(defined)"));
        return;
    };

    let registry = lumen_runtime::tools::ProviderRegistry::new();
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);

    match vm.execute(&entry, vec![]) {
        Ok(result) => {
            // Don't print Null for side-effect-only statements
            if !matches!(result, Value::Null) {
                let type_name = value_type_name(&result);
                println!("{} {}", result, gray(&format!(": {}", type_name)));
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("Runtime error:"), e);
        }
    }
}

/// Handle the :type command — evaluate and report the runtime type.
fn cmd_type(expr: &str) {
    let source = format!(
        "# repl\n\n```lumen\ncell main()\n  return {}\nend\n```\n",
        expr
    );

    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", red("Error:"), e);
            return;
        }
    };

    let registry = lumen_runtime::tools::ProviderRegistry::new();
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);

    match vm.execute("main", vec![]) {
        Ok(result) => println!("{}", cyan(value_type_name(&result))),
        Err(e) => eprintln!("{} {}", red("Error:"), e),
    }
}

/// Return a human-readable type name for a runtime value.
fn value_type_name(val: &Value) -> &'static str {
    match val {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::String(_) => "String",
        Value::Bytes(_) => "Bytes",
        Value::List(_) => "List",
        Value::Tuple(_) => "Tuple",
        Value::Set(_) => "Set",
        Value::Map(_) => "Map",
        Value::Record(_) => "Record",
        Value::Union(_) => "Union",
        Value::Closure(_) => "Closure",
        Value::TraceRef(_) => "TraceRef",
        Value::Future(_) => "Future",
    }
}

fn print_help() {
    println!("{}", bold("Commands:"));
    println!("  {}  {}", cyan(":help, :h"), gray("Show this help"));
    println!("  {}  {}", cyan(":quit, :q"), gray("Exit the REPL"));
    println!("  {}  {}", cyan(":reset, :r"), gray("Reset REPL state"));
    println!("  {}  {}", cyan(":type, :t"), gray("Show the type of an expression"));
    println!();
    println!("{}", gray("Enter Lumen expressions or definitions."));
    println!("{}", gray("Multi-line input is supported — open blocks are"));
    println!("{}", gray("continued until a matching `end` is found."));
}
