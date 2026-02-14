//! Lumen linter — style and correctness checks beyond type checking
//!
//! Implements 10 lint rules:
//! - Style: unused-variable, naming-convention, empty-block, redundant-return, long-cell, missing-type-annotation
//! - Correctness: unreachable-code, infinite-loop, unused-import, shadowed-builtin

use lumen_compiler::compiler::ast::*;
use lumen_compiler::markdown::extract::extract_blocks;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// ANSI color codes
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    #[allow(dead_code)]
    Info,
}

#[derive(Debug, Clone)]
pub struct LintWarning {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub line: usize,
    pub suggestion: Option<String>,
}

impl LintWarning {
    fn new(
        rule: &str,
        severity: Severity,
        message: String,
        file: &str,
        line: usize,
        suggestion: Option<String>,
    ) -> Self {
        Self {
            rule: rule.to_string(),
            severity,
            message,
            file: file.to_string(),
            line,
            suggestion,
        }
    }
}

/// Lint a single source file
pub fn lint_file(source: &str, filename: &str) -> Vec<LintWarning> {
    // Extract code blocks from markdown
    let extracted = extract_blocks(source);
    let mut full_code = String::new();
    for block in &extracted.code_blocks {
        if !full_code.is_empty() {
            full_code.push('\n');
        }
        full_code.push_str(&block.code);
    }

    if full_code.trim().is_empty() {
        return vec![];
    }

    // Lex and parse
    let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(&full_code, 1, 0);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return vec![], // Parse failed, skip linting
    };

    let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
    let program = match parser.parse_program(vec![]) {
        Ok(p) => p,
        Err(_) => return vec![], // Parse failed, skip linting
    };

    // Run all lint rules
    let mut linter = Linter::new(filename);
    linter.lint_program(&program);
    linter.warnings
}

/// Main linter struct that tracks state across rules
struct Linter {
    warnings: Vec<LintWarning>,
    filename: String,
    builtins: HashSet<String>,
}

impl Linter {
    fn new(filename: &str) -> Self {
        let mut builtins = HashSet::new();
        // Built-in functions from the language
        for name in &[
            "print",
            "len",
            "push",
            "pop",
            "keys",
            "values",
            "contains",
            "split",
            "join",
            "trim",
            "replace",
            "to_upper",
            "to_lower",
            "starts_with",
            "ends_with",
            "parse_int",
            "parse_float",
            "parallel",
            "race",
            "vote",
            "select",
            "timeout",
            "uuid",
            "timestamp",
            "hash",
            "encode_base64",
            "decode_base64",
            "encode_json",
            "decode_json",
        ] {
            builtins.insert(name.to_string());
        }

        Self {
            warnings: Vec::new(),
            filename: filename.to_string(),
            builtins,
        }
    }

    fn warn(&mut self, warning: LintWarning) {
        self.warnings.push(warning);
    }

    fn lint_program(&mut self, program: &Program) {
        // Check unused imports
        self.check_unused_imports(program);

        // Check items
        for item in &program.items {
            match item {
                Item::Cell(cell) => self.lint_cell(cell),
                Item::Record(record) => self.check_naming_record(record),
                Item::Enum(enum_def) => self.check_naming_enum(enum_def),
                Item::Import(_) => {} // handled separately
                _ => {}
            }
        }
    }

    fn lint_cell(&mut self, cell: &CellDef) {
        // Check naming convention for cells
        self.check_naming_cell(cell);

        // Check for long cells
        self.check_long_cell(cell);

        // Build variable usage map
        let mut defined_vars = HashMap::new();
        let mut used_vars = HashSet::new();

        self.collect_vars(&cell.body, &mut defined_vars, &mut used_vars);

        // Check for unused variables and shadowed builtins
        for (name, line) in &defined_vars {
            if !used_vars.contains(name) && !name.starts_with('_') {
                self.warn(LintWarning::new(
                    "unused-variable",
                    Severity::Warning,
                    format!("variable '{}' is defined but never used", name),
                    &self.filename,
                    *line,
                    Some(format!("prefix with underscore if intentional: _{}", name)),
                ));
            }
            if self.builtins.contains(name) {
                self.warn(LintWarning::new(
                    "shadowed-builtin",
                    Severity::Error,
                    format!("variable '{}' shadows a builtin function", name),
                    &self.filename,
                    *line,
                    Some("choose a different name".to_string()),
                ));
            }
        }

        // Check cell body
        self.check_unreachable(&cell.body);
        for stmt in &cell.body {
            self.check_stmt(stmt);
        }

        // Check for redundant return
        if let Some(last_stmt) = cell.body.last() {
            if matches!(last_stmt, Stmt::Return(_)) {
                let line = last_stmt.span().line;
                self.warn(LintWarning::new(
                    "redundant-return",
                    Severity::Warning,
                    "redundant 'return' at end of cell (last expression is implicitly returned)"
                        .to_string(),
                    &self.filename,
                    line,
                    Some("remove the 'return' keyword".to_string()),
                ));
            }
        }
    }

    fn collect_vars(
        &self,
        stmts: &[Stmt],
        defined: &mut HashMap<String, usize>,
        used: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            match stmt {
                Stmt::Let(let_stmt) => {
                    defined.insert(let_stmt.name.clone(), let_stmt.span.line);
                    self.collect_expr_vars(&let_stmt.value, used);
                }
                Stmt::Assign(assign) => {
                    self.collect_expr_vars(&assign.value, used);
                }
                Stmt::CompoundAssign(compound) => {
                    self.collect_expr_vars(&compound.value, used);
                }
                Stmt::If(if_stmt) => {
                    self.collect_expr_vars(&if_stmt.condition, used);
                    self.collect_vars(&if_stmt.then_body, defined, used);
                    if let Some(else_body) = &if_stmt.else_body {
                        self.collect_vars(else_body, defined, used);
                    }
                }
                Stmt::For(for_stmt) => {
                    self.collect_expr_vars(&for_stmt.iter, used);
                    self.collect_vars(&for_stmt.body, defined, used);
                }
                Stmt::While(while_stmt) => {
                    self.collect_expr_vars(&while_stmt.condition, used);
                    self.collect_vars(&while_stmt.body, defined, used);
                }
                Stmt::Loop(loop_stmt) => {
                    self.collect_vars(&loop_stmt.body, defined, used);
                }
                Stmt::Match(match_stmt) => {
                    self.collect_expr_vars(&match_stmt.subject, used);
                    for arm in &match_stmt.arms {
                        self.collect_vars(&arm.body, defined, used);
                    }
                }
                Stmt::Return(ret) => {
                    self.collect_expr_vars(&ret.value, used);
                }
                Stmt::Expr(expr_stmt) => {
                    self.collect_expr_vars(&expr_stmt.expr, used);
                }
                _ => {}
            }
        }
    }

    fn collect_expr_vars(&self, expr: &Expr, used: &mut HashSet<String>) {
        match expr {
            Expr::Ident(name, _) => {
                used.insert(name.clone());
            }
            Expr::BinOp(left, _, right, _) => {
                self.collect_expr_vars(left, used);
                self.collect_expr_vars(right, used);
            }
            Expr::UnaryOp(_, e, _) => {
                self.collect_expr_vars(e, used);
            }
            Expr::Call(func, args, _) => {
                self.collect_expr_vars(func, used);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => self.collect_expr_vars(e, used),
                        CallArg::Named(_, e, _) => self.collect_expr_vars(e, used),
                        CallArg::Role(_, e, _) => self.collect_expr_vars(e, used),
                    }
                }
            }
            Expr::DotAccess(obj, _, _) => {
                self.collect_expr_vars(obj, used);
            }
            Expr::IndexAccess(e, idx, _) => {
                self.collect_expr_vars(e, used);
                self.collect_expr_vars(idx, used);
            }
            Expr::ListLit(items, _) => {
                for item in items {
                    self.collect_expr_vars(item, used);
                }
            }
            Expr::RecordLit(_, fields, _) => {
                for (_, val) in fields {
                    self.collect_expr_vars(val, used);
                }
            }
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                self.collect_expr_vars(cond, used);
                self.collect_expr_vars(then_val, used);
                self.collect_expr_vars(else_val, used);
            }
            Expr::Lambda { body, .. } => match body {
                LambdaBody::Block(stmts) => {
                    for stmt in stmts {
                        if let Stmt::Expr(expr_stmt) = stmt {
                            self.collect_expr_vars(&expr_stmt.expr, used);
                        }
                    }
                }
                LambdaBody::Expr(e) => {
                    self.collect_expr_vars(e, used);
                }
            },
            _ => {}
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::If(if_stmt) => {
                self.check_empty_block(&if_stmt.then_body, if_stmt.span.line, "if");
                if let Some(else_body) = &if_stmt.else_body {
                    self.check_empty_block(else_body, if_stmt.span.line, "else");
                }

                // Check for unreachable code after return
                self.check_unreachable(&if_stmt.then_body);
                if let Some(else_body) = &if_stmt.else_body {
                    self.check_unreachable(else_body);
                }
            }
            Stmt::For(for_stmt) => {
                self.check_empty_block(&for_stmt.body, for_stmt.span.line, "for");
                self.check_unreachable(&for_stmt.body);
            }
            Stmt::While(while_stmt) => {
                self.check_empty_block(&while_stmt.body, while_stmt.span.line, "while");
                self.check_unreachable(&while_stmt.body);
                self.check_infinite_loop(&while_stmt.body, while_stmt.span.line);
            }
            Stmt::Loop(loop_stmt) => {
                self.check_empty_block(&loop_stmt.body, loop_stmt.span.line, "loop");
                self.check_unreachable(&loop_stmt.body);
                self.check_infinite_loop(&loop_stmt.body, loop_stmt.span.line);
            }
            Stmt::Match(match_stmt) => {
                for arm in &match_stmt.arms {
                    self.check_unreachable(&arm.body);
                }
            }
            Stmt::Let(let_stmt) => {
                // Check for missing type annotation when value can't be inferred
                if let_stmt.ty.is_none() && self.needs_type_annotation(&let_stmt.value) {
                    self.warn(LintWarning::new(
                        "missing-type-annotation",
                        Severity::Warning,
                        format!(
                            "variable '{}' may benefit from an explicit type annotation",
                            let_stmt.name
                        ),
                        &self.filename,
                        let_stmt.span.line,
                        Some(format!("add type: let {}: Type = ...", let_stmt.name)),
                    ));
                }
            }
            _ => {}
        }
    }

    fn check_empty_block(&mut self, block: &[Stmt], line: usize, kind: &str) {
        if block.is_empty() {
            self.warn(LintWarning::new(
                "empty-block",
                Severity::Warning,
                format!("empty {} block", kind),
                &self.filename,
                line,
                Some("add statements or remove this block".to_string()),
            ));
        }
    }

    fn check_unreachable(&mut self, block: &[Stmt]) {
        let mut found_return = false;
        for stmt in block {
            if found_return {
                self.warn(LintWarning::new(
                    "unreachable-code",
                    Severity::Error,
                    "unreachable code after 'return' statement".to_string(),
                    &self.filename,
                    stmt.span().line,
                    Some("remove this code or restructure control flow".to_string()),
                ));
                break;
            }
            if matches!(stmt, Stmt::Return(_) | Stmt::Halt(_)) {
                found_return = true;
            }
        }
    }

    fn check_infinite_loop(&mut self, block: &[Stmt], line: usize) {
        if !self.has_exit(block) {
            self.warn(LintWarning::new(
                "infinite-loop",
                Severity::Error,
                "loop has no break or return statement (potential infinite loop)".to_string(),
                &self.filename,
                line,
                Some("add 'break' or 'return' to ensure termination".to_string()),
            ));
        }
    }

    fn has_exit(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Break(_) | Stmt::Return(_) | Stmt::Halt(_) => return true,
                Stmt::If(if_stmt) => {
                    if self.has_exit(&if_stmt.then_body) {
                        return true;
                    }
                    if let Some(else_body) = &if_stmt.else_body {
                        if self.has_exit(else_body) {
                            return true;
                        }
                    }
                }
                Stmt::Match(match_stmt) => {
                    for arm in &match_stmt.arms {
                        if self.has_exit(&arm.body) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn needs_type_annotation(&self, expr: &Expr) -> bool {
        // Only warn for complex expressions where type might be ambiguous
        matches!(
            expr,
            Expr::Call(_, _, _) | Expr::ListLit(_, _) | Expr::RecordLit(_, _, _)
        )
    }

    fn check_naming_cell(&mut self, cell: &CellDef) {
        if !is_snake_case(&cell.name) {
            self.warn(LintWarning::new(
                "naming-convention",
                Severity::Warning,
                format!("cell '{}' should use snake_case", cell.name),
                &self.filename,
                cell.span.line,
                Some(format!("rename to '{}'", to_snake_case(&cell.name))),
            ));
        }
    }

    fn check_naming_record(&mut self, record: &RecordDef) {
        if !is_pascal_case(&record.name) {
            self.warn(LintWarning::new(
                "naming-convention",
                Severity::Warning,
                format!("record '{}' should use PascalCase", record.name),
                &self.filename,
                record.span.line,
                Some(format!("rename to '{}'", to_pascal_case(&record.name))),
            ));
        }
    }

    fn check_naming_enum(&mut self, enum_def: &EnumDef) {
        if !is_pascal_case(&enum_def.name) {
            self.warn(LintWarning::new(
                "naming-convention",
                Severity::Warning,
                format!("enum '{}' should use PascalCase", enum_def.name),
                &self.filename,
                enum_def.span.line,
                Some(format!("rename to '{}'", to_pascal_case(&enum_def.name))),
            ));
        }
    }

    fn check_long_cell(&mut self, cell: &CellDef) {
        if cell.body.len() > 50 {
            self.warn(LintWarning::new(
                "long-cell",
                Severity::Warning,
                format!(
                    "cell '{}' has {} statements (exceeds recommended 50)",
                    cell.name,
                    cell.body.len()
                ),
                &self.filename,
                cell.span.line,
                Some("consider refactoring into smaller cells".to_string()),
            ));
        }
    }

    fn check_unused_imports(&mut self, program: &Program) {
        let mut imported_names = HashMap::new();
        let mut used_names = HashSet::new();

        // Collect imports
        for item in &program.items {
            if let Item::Import(import) = item {
                match &import.names {
                    ImportList::Names(names) => {
                        for name in names {
                            let key = name.alias.as_ref().unwrap_or(&name.name).clone();
                            imported_names.insert(key, name.span.line);
                        }
                    }
                    ImportList::Wildcard => {
                        // Can't check wildcards for unused
                    }
                }
            }
        }

        // Collect usage in all cells
        for item in &program.items {
            if let Item::Cell(cell) = item {
                for stmt in &cell.body {
                    self.collect_used_names(stmt, &mut used_names);
                }
            }
        }

        // Check for unused
        for (name, line) in imported_names {
            if !used_names.contains(&name) {
                self.warn(LintWarning::new(
                    "unused-import",
                    Severity::Error,
                    format!("imported name '{}' is never used", name),
                    &self.filename,
                    line,
                    Some("remove this import".to_string()),
                ));
            }
        }
    }

    fn collect_used_names(&self, stmt: &Stmt, used: &mut HashSet<String>) {
        match stmt {
            Stmt::Expr(expr_stmt) => self.collect_expr_names(&expr_stmt.expr, used),
            Stmt::Let(let_stmt) => self.collect_expr_names(&let_stmt.value, used),
            Stmt::Return(ret) => self.collect_expr_names(&ret.value, used),
            Stmt::If(if_stmt) => {
                self.collect_expr_names(&if_stmt.condition, used);
                for stmt in &if_stmt.then_body {
                    self.collect_used_names(stmt, used);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    for stmt in else_body {
                        self.collect_used_names(stmt, used);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_expr_names(&self, expr: &Expr, used: &mut HashSet<String>) {
        match expr {
            Expr::Ident(name, _) => {
                used.insert(name.clone());
            }
            Expr::Call(func, args, _) => {
                self.collect_expr_names(func, used);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => self.collect_expr_names(e, used),
                        CallArg::Named(_, e, _) => self.collect_expr_names(e, used),
                        CallArg::Role(_, e, _) => self.collect_expr_names(e, used),
                    }
                }
            }
            Expr::BinOp(left, _, right, _) => {
                self.collect_expr_names(left, used);
                self.collect_expr_names(right, used);
            }
            _ => {}
        }
    }
}

// Helper functions for naming conventions
fn is_snake_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_lowercase() || c.is_numeric() || c == '_')
}

fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first_char = s.chars().next().unwrap();
    first_char.is_uppercase() && !s.contains('_')
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

#[derive(Debug, Clone, Copy)]
pub struct LintSummary {
    pub total_warnings: usize,
    pub total_errors: usize,
}

/// CLI command entry point
pub fn cmd_lint(files: &[PathBuf], strict: bool) -> Result<LintSummary, String> {
    if files.is_empty() {
        return Err("no files specified".to_string());
    }

    let mut total_warnings = 0;
    let mut total_errors = 0;

    for file in files {
        let source = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read file '{}': {}", file.display(), e))?;

        let warnings = lint_file(&source, &file.display().to_string());

        for w in &warnings {
            total_warnings += 1;
            if w.severity == Severity::Error {
                total_errors += 1;
            }

            print_warning(w, strict);
        }
    }

    if total_warnings > 0 {
        println!();
    }

    Ok(LintSummary {
        total_warnings,
        total_errors,
    })
}

fn print_warning(w: &LintWarning, strict: bool) {
    let severity_color = match w.severity {
        Severity::Error => RED,
        Severity::Warning if strict => RED,
        Severity::Warning => YELLOW,
        Severity::Info => CYAN,
    };

    let severity_label = if strict && w.severity == Severity::Warning {
        "error"
    } else {
        match w.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    };

    println!(
        "{}{}[{}]{}: {}",
        severity_color, severity_label, w.rule, RESET, w.message
    );
    println!("  {}--> {}:{}:{}", BOLD, w.file, w.line, RESET);

    if let Some(suggestion) = &w.suggestion {
        println!("  {}= suggestion:{} {}", CYAN, RESET, suggestion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unused_variable() {
        let source = r#"
```lumen
cell test() -> Int
  let x = 5
  let y = 10
  y
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings
            .iter()
            .any(|w| w.rule == "unused-variable" && w.message.contains("'x'")));
    }

    #[test]
    fn test_naming_convention_cell() {
        let source = r#"
```lumen
cell MyFunc() -> Int
  42
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings
            .iter()
            .any(|w| w.rule == "naming-convention" && w.message.contains("MyFunc")));
    }

    #[test]
    fn test_naming_convention_record() {
        let source = r#"
```lumen
record my_record
  x: Int
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings
            .iter()
            .any(|w| w.rule == "naming-convention" && w.message.contains("my_record")));
    }

    #[test]
    fn test_unreachable_code() {
        let source = r#"
```lumen
cell test() -> Int
  return 5
  let x = 10
  x
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings.iter().any(|w| w.rule == "unreachable-code"));
    }

    #[test]
    fn test_empty_block() {
        let source = r#"
```lumen
cell test() -> Int
  if true
  end
  42
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings.iter().any(|w| w.rule == "empty-block"));
    }

    #[test]
    fn test_no_false_positives() {
        let source = r#"
```lumen
cell calculate(x: Int) -> Int
  let result = x + 5
  result
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        // Should not have unused-variable warning (result is used)
        assert!(!warnings
            .iter()
            .any(|w| w.rule == "unused-variable" && w.message.contains("result")));
    }

    #[test]
    fn test_shadowed_builtin() {
        let source = r#"
```lumen
cell test() -> Int
  let print = 5
  print
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings.iter().any(|w| w.rule == "shadowed-builtin"));
    }

    #[test]
    fn test_redundant_return() {
        let source = r#"
```lumen
cell test() -> Int
  let x = 5
  return x
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings.iter().any(|w| w.rule == "redundant-return"));
    }

    #[test]
    fn test_infinite_loop() {
        let source = r#"
```lumen
cell test() -> Int
  loop
    let x = 5
  end
  42
end
```
"#;
        let warnings = lint_file(source, "test.lm.md");
        assert!(warnings.iter().any(|w| w.rule == "infinite-loop"));
    }
}
