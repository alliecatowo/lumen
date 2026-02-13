//! Interactive REPL for the Lumen language.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use lumen_vm::values::Value;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::{History, SearchDirection};
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

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

/// All keywords for tab completion.
const KEYWORDS: &[&str] = &[
    "cell", "record", "enum", "process", "agent", "effect", "bind",
    "handler", "pipeline", "orchestration", "machine", "memory",
    "guardrail", "eval", "pattern", "grant", "import",
    "if", "else", "while", "for", "match", "loop", "break", "continue",
    "return", "halt", "emit", "let", "use", "tool", "where", "in",
    "and", "or", "not", "is", "as", "type", "alias", "parallel",
    "race", "vote", "select", "timeout", "await", "defer", "true",
    "false", "null", "when", "do", "end", "state", "on", "to",
];

/// Builtin functions for tab completion.
const BUILTINS: &[&str] = &[
    "print", "len", "sort", "map", "filter", "reduce", "append", "join",
    "split", "contains", "starts_with", "ends_with", "replace", "trim",
    "upper", "lower", "reverse", "unique", "zip", "range", "sum", "max",
    "min", "abs", "floor", "ceil", "round", "sqrt", "pow", "log", "exp",
    "sin", "cos", "tan", "uuid", "timestamp", "parse_int", "parse_float",
    "to_string", "to_json", "from_json", "hash", "encode", "decode",
];

/// Type names for tab completion.
const TYPES: &[&str] = &[
    "Int", "Float", "String", "Bool", "Any", "Null", "Bytes", "List",
    "Tuple", "Set", "Map", "Record", "Union", "Future", "result",
];

/// REPL commands for tab completion.
const COMMANDS: &[&str] = &[
    ":help", ":quit", ":reset", ":type", ":clear", ":history",
    ":load", ":env", ":time",
];

/// Completer for the REPL.
struct LumenCompleter;

impl Completer for LumenCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace() || c == '(' || c == '[' || c == '{')
            .map(|i| i + 1)
            .unwrap_or(0);
        let word = &line[start..pos];

        if word.is_empty() {
            return Ok((start, Vec::new()));
        }

        let mut candidates = Vec::new();

        // Match commands (only if at line start)
        if line.trim_start() == word && word.starts_with(':') {
            for &cmd in COMMANDS {
                if cmd.starts_with(word) {
                    candidates.push(Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    });
                }
            }
        } else {
            // Match keywords
            for &kw in KEYWORDS {
                if kw.starts_with(word) {
                    candidates.push(Pair {
                        display: kw.to_string(),
                        replacement: kw.to_string(),
                    });
                }
            }

            // Match builtins
            for &builtin in BUILTINS {
                if builtin.starts_with(word) {
                    candidates.push(Pair {
                        display: builtin.to_string(),
                        replacement: builtin.to_string(),
                    });
                }
            }

            // Match types
            for &ty in TYPES {
                if ty.starts_with(word) {
                    candidates.push(Pair {
                        display: ty.to_string(),
                        replacement: ty.to_string(),
                    });
                }
            }
        }

        Ok((start, candidates))
    }
}

impl Hinter for LumenCompleter {
    type Hint = String;
}

impl Highlighter for LumenCompleter {}

impl Validator for LumenCompleter {}

impl Helper for LumenCompleter {}

/// Session state for tracking defined symbols.
#[derive(Default)]
struct SessionState {
    /// Source code for all defined items (cells, records, enums, etc.)
    definitions: Vec<String>,
    /// Map of symbol names to their definition index
    symbols: HashMap<String, usize>,
}

impl SessionState {
    fn add_definition(&mut self, input: &str) {
        // Extract symbol name if possible
        if let Some(name) = extract_symbol_name(input) {
            let index = self.definitions.len();
            self.symbols.insert(name, index);
        }
        self.definitions.push(input.to_string());
    }

    fn clear(&mut self) {
        self.definitions.clear();
        self.symbols.clear();
    }

    /// Build a source file with all definitions plus the current input.
    fn build_source(&self, input: &str) -> String {
        let mut src = String::from("# repl\n\n```lumen\n");
        for def in &self.definitions {
            src.push_str(def);
            src.push('\n');
        }
        src.push_str(input);
        src.push_str("\n```\n");
        src
    }
}

/// Extract the primary symbol name from a definition (cell name, record name, etc.).
fn extract_symbol_name(input: &str) -> Option<String> {
    let words: Vec<&str> = input.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    match words[0] {
        "cell" | "record" | "enum" | "process" | "agent" | "effect" | "type" => {
            // Name is second word, possibly followed by generics/parens
            let name = words[1].split(&['(', '<', '['][..]).next()?;
            Some(name.to_string())
        }
        _ => None,
    }
}

pub fn run_repl() {
    println!("{}", bold(&cyan("Lumen REPL v0.1.0")));
    println!("{}\n", gray("Type :help for available commands, :quit to exit."));

    // Set up rustyline editor
    let config = rustyline::Config::builder()
        .auto_add_history(true)
        .build();
    let mut rl = Editor::with_config(config).expect("Failed to create editor");
    rl.set_helper(Some(LumenCompleter));

    // Load history from ~/.lumen/repl_history
    let history_path = get_history_path();
    if let Some(ref path) = history_path {
        if path.exists() {
            let _ = rl.load_history(path);
        }
    }

    let mut session_state = SessionState::default();
    let mut multiline_buffer = String::new();

    loop {
        let prompt = if multiline_buffer.is_empty() {
            format!("{} ", green("lumen>"))
        } else {
            format!("{}    ", gray("..."))
        };

        match rl.readline(&prompt) {
            Ok(line) => {
                // Handle empty lines
                if line.trim().is_empty() {
                    if multiline_buffer.is_empty() {
                        continue;
                    } else {
                        multiline_buffer.push('\n');
                        multiline_buffer.push_str(&line);
                        continue;
                    }
                }

                // Handle commands only on a fresh prompt
                if multiline_buffer.is_empty() {
                    if let Some(result) = handle_command(&line, &mut rl, &mut session_state) {
                        if !result {
                            break; // :quit
                        }
                        continue;
                    }
                }

                // Accumulate input
                if !multiline_buffer.is_empty() {
                    multiline_buffer.push('\n');
                }
                multiline_buffer.push_str(&line);

                // Check if we need more input
                if needs_more_input(&multiline_buffer) {
                    continue;
                }

                // Evaluate complete input
                let input = multiline_buffer.trim().to_string();
                multiline_buffer.clear();

                eval_input(&input, &mut session_state);
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", gray("(Ctrl-C to exit)"));
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("{} {:?}", red("Error:"), err);
                break;
            }
        }
    }

    // Save history
    if let Some(ref path) = history_path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = rl.save_history(path);
    }

    println!("\n{}", cyan("Goodbye!"));
}

/// Get the path to the REPL history file (~/.lumen/repl_history).
fn get_history_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let mut path = PathBuf::from(home);
    path.push(".lumen");
    path.push("repl_history");
    Some(path)
}

/// Handle REPL commands. Returns Some(true) to continue, Some(false) to quit, None if not a command.
fn handle_command<H: Helper>(
    line: &str,
    rl: &mut Editor<H, rustyline::history::DefaultHistory>,
    session_state: &mut SessionState,
) -> Option<bool> {
    let trimmed = line.trim();

    match trimmed {
        ":quit" | ":q" => return Some(false),
        ":help" | ":h" => {
            print_help();
            return Some(true);
        }
        ":reset" | ":r" => {
            session_state.clear();
            println!("{}", gray("Session state reset."));
            return Some(true);
        }
        ":clear" | ":c" => {
            print!("\x1b[2J\x1b[H"); // Clear screen and move cursor to top
            io::stdout().flush().ok();
            return Some(true);
        }
        ":history" => {
            let history = rl.history();
            for i in 0..history.len() {
                if let Ok(Some(result)) = history.get(i, SearchDirection::Forward) {
                    println!("{:4} {}", gray(&format!("{}", i + 1)), result.entry);
                }
            }
            return Some(true);
        }
        ":env" => {
            cmd_env(session_state);
            return Some(true);
        }
        _ if trimmed.starts_with(":type ") || trimmed.starts_with(":t ") => {
            let expr = if let Some(stripped) = trimmed.strip_prefix(":type ") {
                stripped
            } else if let Some(stripped) = trimmed.strip_prefix(":t ") {
                stripped
            } else {
                unreachable!()
            };
            cmd_type(expr, session_state);
            return Some(true);
        }
        _ if trimmed.starts_with(":load ") => {
            let path = trimmed.strip_prefix(":load ").unwrap().trim();
            cmd_load(path);
            return Some(true);
        }
        _ if trimmed.starts_with(":time ") => {
            let expr = trimmed.strip_prefix(":time ").unwrap();
            cmd_time(expr, session_state);
            return Some(true);
        }
        _ => None,
    }
}

/// Determine if input has unmatched block openers or unclosed delimiters.
fn needs_more_input(input: &str) -> bool {
    // Check block depth (keywords vs end)
    let mut depth: i32 = 0;
    for word in input.split_whitespace() {
        if BLOCK_OPENERS.contains(&word) {
            depth += 1;
        } else if word == "end" {
            depth -= 1;
        }
    }
    if depth > 0 {
        return true;
    }

    // Check unclosed delimiters
    let mut parens = 0;
    let mut brackets = 0;
    let mut braces = 0;
    for ch in input.chars() {
        match ch {
            '(' => parens += 1,
            ')' => parens -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '{' => braces += 1,
            '}' => braces -= 1,
            _ => {}
        }
    }

    parens > 0 || brackets > 0 || braces > 0
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
fn wrap_as_source(input: &str, session_state: &SessionState) -> String {
    if is_item_definition(input) {
        // Top-level item — add to session and wrap
        session_state.build_source(input)
    } else if is_statement(input) {
        // Statement — wrap as-is
        session_state.build_source(input)
    } else {
        // Expression — wrap in cell main() with explicit return
        let wrapped = format!("cell main()\n  return {}\nend", input);
        session_state.build_source(&wrapped)
    }
}

/// Evaluate input: compile and run, printing the result.
fn eval_input(input: &str, session_state: &mut SessionState) {
    let source = wrap_as_source(input, session_state);

    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", red("Error:"), e);
            return;
        }
    };

    // If this is a definition, add to session state
    if is_item_definition(input) {
        session_state.add_definition(input);
        println!("{}", gray("(defined)"));
        return;
    }

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
fn cmd_type(expr: &str, session_state: &SessionState) {
    let wrapped = format!("cell main()\n  return {}\nend", expr);
    let source = session_state.build_source(&wrapped);

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

/// Handle the :load command — load and evaluate a .lm.md file.
fn cmd_load(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} Failed to read {}: {}", red("Error:"), path, e);
            return;
        }
    };

    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", red("Compile error:"), e);
            return;
        }
    };

    // Find main or first cell
    let entry = if module.cells.iter().any(|c| c.name == "main") {
        "main".to_string()
    } else if !module.cells.is_empty() {
        module.cells[0].name.clone()
    } else {
        println!("{}", gray("No executable cells found."));
        return;
    };

    let registry = lumen_runtime::tools::ProviderRegistry::new();
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);

    match vm.execute(&entry, vec![]) {
        Ok(result) => {
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

/// Handle the :env command — show all defined symbols.
fn cmd_env(session_state: &SessionState) {
    if session_state.symbols.is_empty() {
        println!("{}", gray("No symbols defined."));
        return;
    }

    println!("{}", bold("Defined symbols:"));
    let mut names: Vec<_> = session_state.symbols.keys().collect();
    names.sort();
    for name in names {
        println!("  {}", cyan(name));
    }
}

/// Handle the :time command — evaluate and show execution time.
fn cmd_time(expr: &str, session_state: &SessionState) {
    let wrapped = format!("cell main()\n  return {}\nend", expr);
    let source = session_state.build_source(&wrapped);

    let compile_start = Instant::now();
    let module = match lumen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", red("Error:"), e);
            return;
        }
    };
    let compile_time = compile_start.elapsed();

    let registry = lumen_runtime::tools::ProviderRegistry::new();
    let mut vm = lumen_vm::vm::VM::new();
    vm.set_provider_registry(registry);
    vm.load(module);

    let exec_start = Instant::now();
    match vm.execute("main", vec![]) {
        Ok(result) => {
            let exec_time = exec_start.elapsed();
            let total_time = compile_start.elapsed();
            if !matches!(result, Value::Null) {
                let type_name = value_type_name(&result);
                println!("{} {}", result, gray(&format!(": {}", type_name)));
            }
            println!(
                "{}",
                gray(&format!(
                    "Compile: {:?}, Execute: {:?}, Total: {:?}",
                    compile_time, exec_time, total_time
                ))
            );
        }
        Err(e) => {
            eprintln!("{} {}", red("Error:"), e);
        }
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
    println!("  {}  {}", cyan(":reset, :r"), gray("Reset session state"));
    println!("  {}  {}", cyan(":clear, :c"), gray("Clear terminal screen"));
    println!("  {}  {}", cyan(":type <expr>, :t <expr>"), gray("Show the type of an expression"));
    println!("  {}  {}", cyan(":load <file>"), gray("Load and execute a .lm.md file"));
    println!("  {}  {}", cyan(":env"), gray("Show all defined symbols"));
    println!("  {}  {}", cyan(":time <expr>"), gray("Evaluate and show execution time"));
    println!("  {}  {}", cyan(":history"), gray("Show command history"));
    println!();
    println!("{}", gray("Features:"));
    println!("  {}", gray("• Arrow keys for navigation"));
    println!("  {}", gray("• Tab completion for keywords, builtins, types, commands"));
    println!("  {}", gray("• History persistence in ~/.lumen/repl_history"));
    println!("  {}", gray("• Multi-line input (open blocks continue until `end`)"));
    println!("  {}", gray("• Session state (define cells/records that persist)"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_more_input_blocks() {
        assert!(needs_more_input("cell foo()"));
        assert!(needs_more_input("if x"));
        assert!(!needs_more_input("cell foo() end"));
        assert!(!needs_more_input("if x end"));
    }

    #[test]
    fn test_needs_more_input_parens() {
        assert!(needs_more_input("print("));
        assert!(needs_more_input("let x = [1, 2"));
        assert!(needs_more_input("let x = {a: 1"));
        assert!(!needs_more_input("print(1)"));
        assert!(!needs_more_input("let x = [1, 2]"));
        assert!(!needs_more_input("let x = {a: 1}"));
    }

    #[test]
    fn test_extract_symbol_name() {
        assert_eq!(extract_symbol_name("cell foo()"), Some("foo".to_string()));
        assert_eq!(extract_symbol_name("record Bar"), Some("Bar".to_string()));
        assert_eq!(extract_symbol_name("enum Baz"), Some("Baz".to_string()));
        assert_eq!(extract_symbol_name("cell square(x: Int)"), Some("square".to_string()));
        assert_eq!(extract_symbol_name("record Point[T]"), Some("Point".to_string()));
        assert_eq!(extract_symbol_name("let x = 1"), None);
    }

    #[test]
    fn test_is_item_definition() {
        assert!(is_item_definition("cell foo()"));
        assert!(is_item_definition("record Bar"));
        assert!(is_item_definition("enum Baz"));
        assert!(!is_item_definition("let x = 1"));
        assert!(!is_item_definition("print(42)"));
    }

    #[test]
    fn test_is_statement() {
        assert!(is_statement("let x = 1"));
        assert!(is_statement("if x"));
        assert!(is_statement("return 42"));
        assert!(!is_statement("cell foo()"));
        assert!(!is_statement("42 + 1"));
    }
}
