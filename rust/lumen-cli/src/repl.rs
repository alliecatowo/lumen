//! Interactive REPL for the Lumen language.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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
    "cell",
    "record",
    "enum",
    "process",
    "agent",
    "effect",
    "bind",
    "handler",
    "pipeline",
    "orchestration",
    "machine",
    "memory",
    "guardrail",
    "eval",
    "pattern",
    "grant",
    "import",
];

/// All keywords for tab completion.
const KEYWORDS: &[&str] = &[
    "cell",
    "record",
    "enum",
    "process",
    "agent",
    "effect",
    "bind",
    "handler",
    "pipeline",
    "orchestration",
    "machine",
    "memory",
    "guardrail",
    "eval",
    "pattern",
    "grant",
    "import",
    "if",
    "else",
    "while",
    "for",
    "match",
    "loop",
    "break",
    "continue",
    "return",
    "halt",
    "emit",
    "let",
    "use",
    "tool",
    "where",
    "in",
    "and",
    "or",
    "not",
    "is",
    "as",
    "type",
    "alias",
    "parallel",
    "race",
    "vote",
    "select",
    "timeout",
    "await",
    "defer",
    "true",
    "false",
    "null",
    "when",
    "do",
    "end",
    "state",
    "on",
    "to",
];

/// Builtin functions for tab completion.
const BUILTINS: &[&str] = &[
    "print",
    "len",
    "sort",
    "map",
    "filter",
    "reduce",
    "append",
    "join",
    "split",
    "contains",
    "starts_with",
    "ends_with",
    "replace",
    "trim",
    "upper",
    "lower",
    "reverse",
    "unique",
    "zip",
    "range",
    "sum",
    "max",
    "min",
    "abs",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "log",
    "exp",
    "sin",
    "cos",
    "tan",
    "uuid",
    "timestamp",
    "parse_int",
    "parse_float",
    "to_string",
    "to_json",
    "from_json",
    "hash",
    "encode",
    "decode",
];

/// Canonical intrinsic names recognized by the compiler.
const KNOWN_INTRINSICS: &[&str] = &[
    "print",
    "len",
    "range",
    "to_string",
    "to_int",
    "to_float",
    "type_of",
    "keys",
    "values",
    "join",
    "split",
    "append",
    "contains",
    "slice",
    "min",
    "max",
    "matches",
    "trace_ref",
    "abs",
    "sort",
    "reverse",
    "map",
    "filter",
    "reduce",
    "flat_map",
    "zip",
    "enumerate",
    "any",
    "all",
    "find",
    "position",
    "group_by",
    "chunk",
    "window",
    "flatten",
    "unique",
    "take",
    "drop",
    "first",
    "last",
    "is_empty",
    "chars",
    "starts_with",
    "ends_with",
    "index_of",
    "pad_left",
    "pad_right",
    "trim",
    "upper",
    "lower",
    "replace",
    "round",
    "ceil",
    "floor",
    "sqrt",
    "pow",
    "log",
    "sin",
    "cos",
    "clamp",
    "clone",
    "sizeof",
    "debug",
    "count",
    "hash",
    "diff",
    "patch",
    "redact",
    "validate",
    "has_key",
    "merge",
    "size",
    "add",
    "remove",
    "entries",
];

/// Alias name -> canonical intrinsic name.
const INTRINSIC_ALIASES: &[(&str, &str)] = &[
    ("length", "len"),
    ("str", "to_string"),
    ("string", "to_string"),
    ("int", "to_int"),
    ("float", "to_float"),
    ("type", "type_of"),
    ("has", "contains"),
    ("confirm", "matches"),
];

/// Type names for tab completion.
const TYPES: &[&str] = &[
    "Int", "Float", "String", "Bool", "Any", "Null", "Bytes", "List", "Tuple", "Set", "Map",
    "Record", "Union", "Future", "result",
];

/// REPL commands for tab completion.
const COMMANDS: &[&str] = &[
    ":help", ":quit", ":reset", ":type", ":clear", ":history", ":load", ":env", ":time", ":doc",
];

/// Environment variable used to override REPL history location.
const REPL_HISTORY_PATH_ENV: &str = "LUMEN_REPL_HISTORY_PATH";

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
    let mut words = input.split_whitespace();
    let keyword = words.next()?;

    let raw_name = match keyword {
        "type" => {
            let next = words.next()?;
            if next == "alias" {
                words.next()?
            } else {
                next
            }
        }
        _ if ITEM_KEYWORDS.contains(&keyword) => words.next()?,
        _ => return None,
    };

    let cleaned = raw_name
        .split(&['(', '<', '[', '{', ':', '=', ',', ';'][..])
        .next()
        .unwrap_or("");
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.to_string())
}

#[derive(Debug, PartialEq, Eq)]
enum ReplCommand<'a> {
    Quit,
    Help,
    Reset,
    Clear,
    History,
    Env,
    Type(&'a str),
    Load(&'a str),
    Time(&'a str),
    Doc(&'a str),
    DocIndex,
}

#[derive(Debug, PartialEq, Eq)]
enum ParsedCommand<'a> {
    NotACommand,
    UnknownCommand,
    InvalidUsage(&'static str),
    Command(ReplCommand<'a>),
}

fn parse_repl_command(line: &str) -> ParsedCommand<'_> {
    let trimmed = line.trim();
    if !trimmed.starts_with(':') {
        return ParsedCommand::NotACommand;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("");
    let arg = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match cmd {
        ":quit" | ":q" => ParsedCommand::Command(ReplCommand::Quit),
        ":help" | ":h" => ParsedCommand::Command(ReplCommand::Help),
        ":reset" | ":r" => ParsedCommand::Command(ReplCommand::Reset),
        ":clear" | ":c" => ParsedCommand::Command(ReplCommand::Clear),
        ":history" => ParsedCommand::Command(ReplCommand::History),
        ":env" => ParsedCommand::Command(ReplCommand::Env),
        ":type" | ":t" => match arg {
            Some(expr) => ParsedCommand::Command(ReplCommand::Type(expr)),
            None => ParsedCommand::InvalidUsage("Usage: :type <expr>"),
        },
        ":load" => match arg {
            Some(path) => ParsedCommand::Command(ReplCommand::Load(path)),
            None => ParsedCommand::InvalidUsage("Usage: :load <file>"),
        },
        ":time" => match arg {
            Some(expr) => ParsedCommand::Command(ReplCommand::Time(expr)),
            None => ParsedCommand::InvalidUsage("Usage: :time <expr>"),
        },
        ":doc" | ":d" => match arg {
            Some(symbol) => ParsedCommand::Command(ReplCommand::Doc(symbol)),
            None => ParsedCommand::Command(ReplCommand::DocIndex),
        },
        _ => ParsedCommand::UnknownCommand,
    }
}

pub fn run_repl() {
    println!("{}", bold(&cyan("Lumen REPL v0.1.0")));
    println!(
        "{}\n",
        gray("Type :help for available commands, :quit to exit.")
    );

    // Set up rustyline editor
    let config = rustyline::Config::builder().auto_add_history(true).build();
    let mut rl = Editor::with_config(config).expect("Failed to create editor");
    rl.set_helper(Some(LumenCompleter));

    // Load history from default or configured path.
    let history_path = get_history_path();
    if let Some(ref path) = history_path {
        if path.exists() {
            if let Err(err) = rl.load_history(path) {
                eprintln!(
                    "{} failed to load history from {}: {}",
                    red("Warning:"),
                    path.display(),
                    err
                );
            }
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
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!(
                    "{} failed to create history directory {}: {}",
                    red("Warning:"),
                    parent.display(),
                    err
                );
            }
        }
        if let Err(err) = rl.save_history(path) {
            eprintln!(
                "{} failed to save history to {}: {}",
                red("Warning:"),
                path.display(),
                err
            );
        }
    }

    println!("\n{}", cyan("Goodbye!"));
}

/// Resolve the path to the history file.
///
/// Rules:
/// - `LUMEN_REPL_HISTORY_PATH` set to an absolute path: use as-is.
/// - `LUMEN_REPL_HISTORY_PATH` set to `~/...`: resolve under HOME.
/// - `LUMEN_REPL_HISTORY_PATH` set to a relative path: resolve under HOME.
/// - Otherwise: `${HOME}/.lumen/repl_history`.
fn resolve_history_path(home: Option<&Path>, override_path: Option<&str>) -> Option<PathBuf> {
    let home_path = || home.map(Path::to_path_buf);

    if let Some(raw) = override_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if raw == "~" {
            return home_path();
        }
        if let Some(rest) = raw.strip_prefix("~/") {
            let mut path = home_path()?;
            path.push(rest);
            return Some(path);
        }

        let configured = PathBuf::from(raw);
        if configured.is_relative() {
            let mut base = home_path()?;
            base.push(configured);
            return Some(base);
        }
        return Some(configured);
    }

    let mut default_path = home_path()?;
    default_path.push(".lumen");
    default_path.push("repl_history");
    Some(default_path)
}

fn get_history_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let override_path = std::env::var(REPL_HISTORY_PATH_ENV).ok();
    resolve_history_path(home.as_deref(), override_path.as_deref())
}

/// Handle REPL commands. Returns Some(true) to continue, Some(false) to quit, None if not a command.
fn handle_command<H: Helper>(
    line: &str,
    rl: &mut Editor<H, rustyline::history::DefaultHistory>,
    session_state: &mut SessionState,
) -> Option<bool> {
    match parse_repl_command(line) {
        ParsedCommand::NotACommand => None,
        ParsedCommand::UnknownCommand => {
            eprintln!("{} unknown command. Type :help for usage.", red("Error:"));
            Some(true)
        }
        ParsedCommand::InvalidUsage(usage) => {
            eprintln!("{} {}", red("Error:"), usage);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Quit) => Some(false),
        ParsedCommand::Command(ReplCommand::Help) => {
            print_help();
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Reset) => {
            session_state.clear();
            println!("{}", gray("Session state reset."));
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Clear) => {
            print!("\x1b[2J\x1b[H"); // Clear screen and move cursor to top
            io::stdout().flush().ok();
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::History) => {
            let history = rl.history();
            for i in 0..history.len() {
                if let Ok(Some(result)) = history.get(i, SearchDirection::Forward) {
                    println!("{:4} {}", gray(&format!("{}", i + 1)), result.entry);
                }
            }
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Env) => {
            cmd_env(session_state);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Type(expr)) => {
            cmd_type(expr, session_state);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Load(path)) => {
            cmd_load(path);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Time(expr)) => {
            cmd_time(expr, session_state);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::Doc(symbol)) => {
            cmd_doc(symbol, session_state);
            Some(true)
        }
        ParsedCommand::Command(ReplCommand::DocIndex) => {
            cmd_doc_index(session_state);
            Some(true)
        }
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
    "let", "if", "while", "for", "match", "return", "halt", "loop", "break", "continue", "emit",
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

    // If this is a definition or a persistent statement (like let), add to session state
    let is_definition = is_item_definition(input);
    let is_persistent_stmt = input.trim_start().starts_with("let ");

    if is_definition || is_persistent_stmt {
        session_state.add_definition(input);
    }

    if is_definition {
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

fn canonical_intrinsic_name(name: &str) -> Option<&str> {
    if KNOWN_INTRINSICS.contains(&name) {
        return Some(name);
    }

    for (alias, canonical) in INTRINSIC_ALIASES {
        if *alias == name {
            return Some(canonical);
        }
    }

    None
}

fn intrinsic_aliases(canonical: &str) -> Vec<&'static str> {
    let mut aliases = Vec::new();
    for (alias, target) in INTRINSIC_ALIASES {
        if *target == canonical {
            aliases.push(*alias);
        }
    }
    aliases
}

fn intrinsic_category(canonical: &str) -> &'static str {
    match canonical {
        "print" | "debug" | "clone" | "sizeof" | "type_of" => "core",
        "len" | "append" | "contains" | "slice" | "count" | "sort" | "reverse" | "map"
        | "filter" | "reduce" | "flat_map" | "zip" | "enumerate" | "any" | "all" | "find"
        | "position" | "group_by" | "chunk" | "window" | "flatten" | "unique" | "take" | "drop"
        | "first" | "last" | "is_empty" => "collections",
        "keys" | "values" | "has_key" | "merge" | "size" | "add" | "remove" | "entries" => {
            "map/set"
        }
        "join" | "split" | "chars" | "starts_with" | "ends_with" | "index_of" | "pad_left"
        | "pad_right" | "trim" | "upper" | "lower" | "replace" => "strings",
        "abs" | "min" | "max" | "round" | "ceil" | "floor" | "sqrt" | "pow" | "log" | "sin"
        | "cos" | "clamp" => "math",
        "diff" | "patch" | "redact" | "validate" | "hash" | "matches" | "trace_ref" => "utility",
        "range" | "to_string" | "to_int" | "to_float" => "conversion",
        _ => "intrinsic",
    }
}

fn intrinsic_summary(canonical: &str) -> &'static str {
    match canonical {
        "print" => "Print a value to stdout.",
        "len" => "Return the length or element count of a value.",
        "range" => "Create an integer range as a list.",
        "to_string" => "Convert a value into a String.",
        "to_int" => "Convert a value into an Int.",
        "to_float" => "Convert a value into a Float.",
        "type_of" => "Return the runtime type name of a value.",
        "append" => "Append an element to a list.",
        "map" => "Transform each element in a list.",
        "filter" => "Keep list elements matching a predicate.",
        "reduce" => "Fold a list into a single value.",
        "sort" => "Sort a list in ascending order.",
        "join" => "Join string/list items with a separator.",
        "split" => "Split a string by a delimiter.",
        "contains" => "Check membership/containment in a collection.",
        "matches" => "Check if a value satisfies a pattern/condition.",
        "hash" => "Compute a stable hash for a value.",
        "validate" => "Validate value shape/content against rules.",
        _ => "Built-in intrinsic recognized by the compiler.",
    }
}

fn intrinsic_signature(canonical: &str) -> String {
    match canonical {
        "print" => "print(value: Any) -> Null".to_string(),
        "len" => "len(value: List|Tuple|Map|Set|String|Bytes) -> Int".to_string(),
        "range" => "range(start: Int, end: Int) -> List[Int]".to_string(),
        "to_string" => "to_string(value: Any) -> String".to_string(),
        "to_int" => "to_int(value: Any) -> Int".to_string(),
        "to_float" => "to_float(value: Any) -> Float".to_string(),
        "type_of" => "type_of(value: Any) -> String".to_string(),
        "append" => "append(list: List[T], item: T) -> List[T]".to_string(),
        "map" => "map(items: List[T], f: Fn(T) -> U) -> List[U]".to_string(),
        "filter" => "filter(items: List[T], p: Fn(T) -> Bool) -> List[T]".to_string(),
        "reduce" => "reduce(items: List[T], init: U, f: Fn(U, T) -> U) -> U".to_string(),
        "sort" => "sort(items: List[T]) -> List[T]".to_string(),
        "join" => "join(items: List[String], sep: String) -> String".to_string(),
        "split" => "split(text: String, sep: String) -> List[String]".to_string(),
        "contains" => "contains(container: Any, value: Any) -> Bool".to_string(),
        "matches" => "matches(value: Any, pattern: Any) -> Bool".to_string(),
        "hash" => "hash(value: Any) -> String".to_string(),
        _ => format!("{canonical}(...)"),
    }
}

fn render_doc(symbol: &str, session_state: &SessionState) -> Option<String> {
    let session_index = session_state.symbols.get(symbol).copied().or_else(|| {
        session_state
            .symbols
            .iter()
            .find_map(|(name, idx)| name.eq_ignore_ascii_case(symbol).then_some(*idx))
    });

    if let Some(index) = session_index {
        let definition = session_state.definitions.get(index)?.trim();
        let signature = definition
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .unwrap_or("<definition>");
        let kind = signature.split_whitespace().next().unwrap_or("definition");

        return Some(format!(
            "Session {} `{}`\nSignature: {}\n\n{}",
            kind, symbol, signature, definition
        ));
    }

    let canonical = canonical_intrinsic_name(symbol)?;
    let mut out = String::new();
    out.push_str(&format!("Intrinsic `{}`\n", canonical));
    out.push_str(&format!("Category: {}\n", intrinsic_category(canonical)));
    out.push_str(&format!("Signature: {}\n", intrinsic_signature(canonical)));
    out.push_str(&format!("Summary: {}", intrinsic_summary(canonical)));

    if canonical != symbol {
        out.push_str(&format!(
            "\nAlias: `{}` resolves to `{}`",
            symbol, canonical
        ));
    }

    let aliases = intrinsic_aliases(canonical);
    if !aliases.is_empty() {
        out.push_str(&format!("\nAliases: {}", aliases.join(", ")));
    }

    Some(out)
}

fn doc_suggestions(symbol: &str, session_state: &SessionState) -> Vec<String> {
    let needle = symbol.to_ascii_lowercase();
    let mut candidates: Vec<String> = session_state.symbols.keys().cloned().collect();
    candidates.extend(KNOWN_INTRINSICS.iter().map(|name| (*name).to_string()));
    candidates.extend(
        INTRINSIC_ALIASES
            .iter()
            .map(|(alias, _)| (*alias).to_string()),
    );
    candidates.sort();
    candidates.dedup();

    let mut starts_with = Vec::new();
    let mut contains = Vec::new();
    for candidate in candidates {
        let lowered = candidate.to_ascii_lowercase();
        if lowered.starts_with(&needle) {
            starts_with.push(candidate);
        } else if lowered.contains(&needle) {
            contains.push(candidate);
        }
    }

    starts_with.extend(contains);
    starts_with.truncate(8);
    starts_with
}

/// Handle the :doc command — show docs for loaded symbols or known intrinsics.
fn cmd_doc(symbol: &str, session_state: &SessionState) {
    if let Some(doc) = render_doc(symbol, session_state) {
        println!("{}", doc);
        return;
    }

    eprintln!("{} no docs found for `{}`", red("Error:"), symbol);
    let suggestions = doc_suggestions(symbol, session_state);
    if !suggestions.is_empty() {
        println!(
            "{}",
            gray(&format!("Did you mean: {}?", suggestions.join(", ")))
        );
    }
}

/// Handle `:doc` with no symbol — show usage and available names.
fn cmd_doc_index(session_state: &SessionState) {
    println!("{}", bold("Doc lookup"));
    println!(
        "  {}",
        gray("Use :doc <symbol> to inspect a session definition or intrinsic.")
    );

    if session_state.symbols.is_empty() {
        println!("  {}", gray("Session symbols: none"));
    } else {
        let mut names: Vec<_> = session_state.symbols.keys().cloned().collect();
        names.sort();
        println!(
            "  {}",
            gray(&format!("Session symbols: {}", names.join(", ")))
        );
    }

    let mut intrinsics = KNOWN_INTRINSICS.to_vec();
    intrinsics.sort();
    const PREVIEW_LIMIT: usize = 20;
    let preview = intrinsics
        .iter()
        .take(PREVIEW_LIMIT)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "  {}",
        gray(&format!(
            "Known intrinsics ({}): {}",
            intrinsics.len(),
            preview
        ))
    );
    if intrinsics.len() > PREVIEW_LIMIT {
        println!(
            "  {}",
            gray(&format!(
                "... plus {} more. Use :doc <name> for details.",
                intrinsics.len() - PREVIEW_LIMIT
            ))
        );
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
    println!(
        "  {}  {}",
        cyan(":clear, :c"),
        gray("Clear terminal screen")
    );
    println!(
        "  {}  {}",
        cyan(":type <expr>, :t <expr>"),
        gray("Show the type of an expression")
    );
    println!(
        "  {}  {}",
        cyan(":load <file>"),
        gray("Load and execute a .lm.md file")
    );
    println!("  {}  {}", cyan(":env"), gray("Show all defined symbols"));
    println!(
        "  {}  {}",
        cyan(":time <expr>"),
        gray("Evaluate and show execution time")
    );
    println!(
        "  {}  {}",
        cyan(":doc <symbol>, :d <symbol>"),
        gray("Show docs for a session symbol or intrinsic")
    );
    println!(
        "  {}  {}",
        cyan(":doc"),
        gray("List available doc lookup symbols")
    );
    println!("  {}  {}", cyan(":history"), gray("Show command history"));
    println!();
    println!("{}", gray("Features:"));
    println!("  {}", gray("• Arrow keys for navigation"));
    println!(
        "  {}",
        gray("• Tab completion for keywords, builtins, types, commands")
    );
    if let Some(path) = get_history_path() {
        println!(
            "  {}",
            gray(&format!(
                "• History persistence in {} (override with ${})",
                path.display(),
                REPL_HISTORY_PATH_ENV
            ))
        );
    } else {
        println!(
            "  {}",
            gray("• History persistence disabled (HOME not set)")
        );
    }
    println!(
        "  {}",
        gray("• Multi-line input (open blocks continue until `end`)")
    );
    println!(
        "  {}",
        gray("• Session state (define cells/records that persist)")
    );
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
        assert_eq!(
            extract_symbol_name("cell square(x: Int)"),
            Some("square".to_string())
        );
        assert_eq!(
            extract_symbol_name("record Point[T]"),
            Some("Point".to_string())
        );
        assert_eq!(
            extract_symbol_name("type alias UserId = Int"),
            Some("UserId".to_string())
        );
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

    #[test]
    fn test_parse_repl_command() {
        assert_eq!(
            parse_repl_command(":doc len"),
            ParsedCommand::Command(ReplCommand::Doc("len"))
        );
        assert_eq!(
            parse_repl_command(":d to_string"),
            ParsedCommand::Command(ReplCommand::Doc("to_string"))
        );
        assert_eq!(
            parse_repl_command(":doc"),
            ParsedCommand::Command(ReplCommand::DocIndex)
        );
        assert_eq!(
            parse_repl_command(":type"),
            ParsedCommand::InvalidUsage("Usage: :type <expr>")
        );
        assert_eq!(parse_repl_command(":nope"), ParsedCommand::UnknownCommand);
        assert_eq!(parse_repl_command("1 + 1"), ParsedCommand::NotACommand);
    }

    #[test]
    fn test_resolve_history_path() {
        let home = Path::new("/home/tester");

        assert_eq!(
            resolve_history_path(Some(home), None),
            Some(PathBuf::from("/home/tester/.lumen/repl_history"))
        );
        assert_eq!(
            resolve_history_path(Some(home), Some("repl/history.log")),
            Some(PathBuf::from("/home/tester/repl/history.log"))
        );
        assert_eq!(
            resolve_history_path(Some(home), Some("~/logs/repl.log")),
            Some(PathBuf::from("/home/tester/logs/repl.log"))
        );
        assert_eq!(
            resolve_history_path(Some(home), Some("/tmp/repl.log")),
            Some(PathBuf::from("/tmp/repl.log"))
        );
        assert_eq!(resolve_history_path(None, Some("relative.log")), None);
    }

    #[test]
    fn test_render_doc_for_session_symbol() {
        let mut state = SessionState::default();
        state.add_definition("cell square(x: Int)\n  return x * x\nend");
        let rendered = render_doc("square", &state).expect("session doc");
        assert!(rendered.contains("Session cell `square`"));
        assert!(rendered.contains("cell square(x: Int)"));
    }

    #[test]
    fn test_render_doc_for_intrinsic_alias() {
        let state = SessionState::default();
        let rendered = render_doc("length", &state).expect("intrinsic doc");
        assert!(rendered.contains("Intrinsic `len`"));
        assert!(rendered.contains("Alias: `length` resolves to `len`"));
    }
}
