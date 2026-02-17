//! Lumen code formatter
//!
//! AST-aware formatter that produces beautiful, consistent output like rustfmt or prettier.
//!
//! Supports two file modes:
//! - **`.lm.md` files** (markdown-first): Preserves markdown structure, formats code inside
//!   `` ```lumen ... ``` `` fenced blocks.
//! - **`.lm` / `.lumen` files** (code-first): Formats Lumen code, preserves `` ``` ... ``` ``
//!   markdown blocks verbatim. Keeps docstrings attached to their declarations.

use lumen_compiler::compiler::ast::*;
use lumen_compiler::markdown::extract::extract_blocks;
use std::path::PathBuf;

const INDENT_SPACES: usize = 2;

/// ANSI color codes for CLI output
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Format a complete .lm.md file
pub fn format_file(content: &str) -> String {
    let mut output = String::new();
    let mut in_code_block = false;
    let mut code_block = String::new();

    for line in content.lines() {
        let trimmed = line.trim_start();

        if !in_code_block && trimmed.starts_with("```lumen") {
            // Start of lumen code block
            in_code_block = true;
            output.push_str(line);
            output.push('\n');
            code_block.clear();
        } else if in_code_block && trimmed.starts_with("```") {
            // End of code block — format and emit
            let formatted = format_lumen_code(&code_block);
            output.push_str(&formatted);
            if !formatted.is_empty() && !formatted.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(line);
            output.push('\n');
            in_code_block = false;
        } else if in_code_block {
            // Accumulate code
            code_block.push_str(line);
            code_block.push('\n');
        } else {
            // Regular markdown - preserve as-is
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

/// Format a .lm/.lumen file (code-first mode with embedded markdown blocks)
///
/// In code-first mode, everything outside triple-backtick fences is Lumen code,
/// and ``` ... ``` blocks are markdown comments/docstrings. The formatter:
/// - Preserves markdown blocks verbatim
/// - Formats code sections using the AST-based pretty printer
/// - Maintains blank lines around markdown blocks
/// - Keeps docstrings attached to their declarations (no added blank line)
pub fn format_lm_source(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut output = String::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim().starts_with("```") {
            // Markdown block — preserve verbatim through closing ```
            output.push_str(lines[i]);
            output.push('\n');
            i += 1;
            while i < lines.len() {
                output.push_str(lines[i]);
                output.push('\n');
                if lines[i].trim().starts_with("```") {
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else {
            // Code section — collect everything until next ``` or EOF
            let mut code_lines: Vec<&str> = Vec::new();
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }

            // Count leading blank lines (preserve spacing after markdown blocks)
            let mut leading = 0;
            for line in &code_lines {
                if line.trim().is_empty() {
                    leading += 1;
                } else {
                    break;
                }
            }

            // Count trailing blank lines (preserve spacing before markdown blocks)
            let mut trailing = 0;
            for line in code_lines.iter().rev() {
                if line.trim().is_empty() {
                    trailing += 1;
                } else {
                    break;
                }
            }

            let code_end = code_lines.len().saturating_sub(trailing);

            // Emit leading blank lines
            for _ in 0..leading {
                output.push('\n');
            }

            // Format actual code (if any non-blank lines exist)
            if leading < code_end {
                let actual_code: String = code_lines[leading..code_end]
                    .iter()
                    .map(|l| format!("{}\n", l))
                    .collect();
                let formatted = format_lumen_code(actual_code.trim());
                output.push_str(&formatted);
            }

            // Emit trailing blank lines
            for _ in 0..trailing {
                output.push('\n');
            }
        }
    }

    output
}

/// Format Lumen code using AST-based pretty printing
pub fn format_lumen_code(code: &str) -> String {
    if code.trim().is_empty() {
        return String::new();
    }

    // Try to parse the code
    let extracted = extract_blocks(&format!("```lumen\n{}\n```", code));

    let mut full_code = String::new();
    for block in &extracted.code_blocks {
        if !full_code.is_empty() {
            full_code.push('\n');
        }
        full_code.push_str(&block.code);
    }

    if full_code.is_empty() {
        return code.to_string();
    }

    // Lex and parse
    let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(&full_code, 1, 0);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return code.to_string(), // Parse failed, return original
    };

    let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
    let program = match parser.parse_program(vec![]) {
        Ok(p) => p,
        Err(_) => return code.to_string(), // Parse failed, return original
    };

    // If the AST is empty (no declarations), return original code
    if program.items.is_empty() {
        return code.to_string();
    }

    // Pretty-print the AST
    let mut formatter = Formatter::new();
    formatter.fmt_program(&program);

    let mut result = formatter.output;
    // Ensure single trailing newline
    while result.ends_with("\n\n") {
        result.pop();
    }
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Pretty-printer for Lumen AST
struct Formatter {
    output: String,
    indent: usize,
}

impl Formatter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    fn indent_str(&self) -> String {
        " ".repeat(self.indent * INDENT_SPACES)
    }

    fn push_indent(&mut self) {
        self.indent += 1;
    }

    fn pop_indent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    fn writeln(&mut self, s: &str) {
        if !s.is_empty() {
            self.output.push_str(&self.indent_str());
            self.output.push_str(s);
        }
        self.output.push('\n');
    }

    fn fmt_program(&mut self, program: &Program) {
        let mut first = true;
        for item in &program.items {
            if !first {
                self.writeln(""); // Blank line between top-level items
            }
            first = false;
            self.fmt_item(item);
        }
    }

    fn fmt_item(&mut self, item: &Item) {
        match item {
            Item::Cell(c) => self.fmt_cell(c),
            Item::Record(r) => self.fmt_record(r),
            Item::Enum(e) => self.fmt_enum(e),
            Item::TypeAlias(t) => self.fmt_type_alias(t),
            Item::Effect(eff) => self.fmt_effect(eff),
            Item::EffectBind(b) => self.fmt_effect_bind(b),
            Item::UseTool(u) => self.fmt_use_tool(u),
            Item::Grant(g) => self.fmt_grant(g),
            Item::Import(i) => self.fmt_import(i),
            Item::Process(p) => self.fmt_process(p),
            Item::Agent(a) => self.fmt_agent(a),
            Item::Handler(h) => self.fmt_handler(h),
            Item::Addon(a) => self.fmt_addon(a),
            Item::Trait(t) => self.fmt_trait(t),
            Item::Impl(i) => self.fmt_impl(i),
            Item::ConstDecl(c) => self.fmt_const_decl(c),
            Item::MacroDecl(m) => self.fmt_macro_decl(m),
        }
    }

    fn fmt_cell(&mut self, cell: &CellDef) {
        let mut header = String::new();
        if cell.is_pub {
            header.push_str("pub ");
        }
        header.push_str("cell ");
        header.push_str(&cell.name);

        if !cell.generic_params.is_empty() {
            header.push('[');
            for (i, param) in cell.generic_params.iter().enumerate() {
                if i > 0 {
                    header.push_str(", ");
                }
                header.push_str(&param.name);
                if !param.bounds.is_empty() {
                    header.push_str(": ");
                    header.push_str(&param.bounds.join(" + "));
                }
            }
            header.push(']');
        }

        // Build parameter list
        let mut params_str = String::new();
        for (i, param) in cell.params.iter().enumerate() {
            if i > 0 {
                params_str.push_str(", ");
            }
            params_str.push_str(&param.name);
            params_str.push_str(": ");
            params_str.push_str(&self.fmt_type(&param.ty));
            if let Some(default) = &param.default_value {
                params_str.push_str(" = ");
                params_str.push_str(&self.fmt_expr(default));
            }
        }

        let return_str = if let Some(ret) = &cell.return_type {
            format!(" -> {}", self.fmt_type(ret))
        } else {
            String::new()
        };

        let effects_str = if !cell.effects.is_empty() {
            format!(" / {{{}}}", cell.effects.join(", "))
        } else {
            String::new()
        };

        // Check if the entire signature fits on one line (target 100 chars)
        let one_line = format!("{}({}){}{}", header, params_str, return_str, effects_str);

        if one_line.len() <= 100 {
            self.writeln(&one_line);
        } else if cell.params.len() <= 3 {
            // Short param list — keep on one line even if slightly long
            self.writeln(&one_line);
        } else {
            // Multi-line param list
            header.push('(');
            self.writeln(header.trim_end());
            self.push_indent();
            for (i, param) in cell.params.iter().enumerate() {
                let mut line = param.name.clone();
                line.push_str(": ");
                line.push_str(&self.fmt_type(&param.ty));
                if let Some(default) = &param.default_value {
                    line.push_str(" = ");
                    line.push_str(&self.fmt_expr(default));
                }
                if i < cell.params.len() - 1 {
                    line.push(',');
                }
                self.writeln(&line);
            }
            self.pop_indent();
            let close = format!("){}{}", return_str, effects_str);
            self.writeln(&close);
            self.push_indent();
            for stmt in &cell.body {
                self.fmt_stmt(stmt);
            }
            self.pop_indent();
            self.writeln("end");
            return;
        }

        self.push_indent();
        for stmt in &cell.body {
            self.fmt_stmt(stmt);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_record(&mut self, record: &RecordDef) {
        let mut header = String::new();
        if record.is_pub {
            header.push_str("pub ");
        }
        header.push_str("record ");
        header.push_str(&record.name);

        if !record.generic_params.is_empty() {
            header.push('[');
            for (i, param) in record.generic_params.iter().enumerate() {
                if i > 0 {
                    header.push_str(", ");
                }
                header.push_str(&param.name);
            }
            header.push(']');
        }

        self.writeln(&header);
        self.push_indent();
        for field in &record.fields {
            let mut line = field.name.clone();
            line.push_str(": ");
            line.push_str(&self.fmt_type(&field.ty));
            if let Some(default) = &field.default_value {
                line.push_str(" = ");
                line.push_str(&self.fmt_expr(default));
            }
            if let Some(constraint) = &field.constraint {
                line.push_str(" where ");
                line.push_str(&self.fmt_expr(constraint));
            }
            self.writeln(&line);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_enum(&mut self, enm: &EnumDef) {
        let mut header = String::new();
        if enm.is_pub {
            header.push_str("pub ");
        }
        header.push_str("enum ");
        header.push_str(&enm.name);

        if !enm.generic_params.is_empty() {
            header.push('[');
            for (i, param) in enm.generic_params.iter().enumerate() {
                if i > 0 {
                    header.push_str(", ");
                }
                header.push_str(&param.name);
            }
            header.push(']');
        }

        self.writeln(&header);
        self.push_indent();
        for variant in &enm.variants {
            let mut line = variant.name.clone();
            if let Some(payload) = &variant.payload {
                line.push('(');
                line.push_str(&self.fmt_type(payload));
                line.push(')');
            }
            self.writeln(&line);
        }
        self.pop_indent();
        self.writeln("end");

        // Methods
        for method in &enm.methods {
            self.writeln("");
            self.fmt_cell(method);
        }
    }

    fn fmt_type_alias(&mut self, alias: &TypeAliasDef) {
        let mut line = String::new();
        if alias.is_pub {
            line.push_str("pub ");
        }
        line.push_str("type ");
        line.push_str(&alias.name);

        if !alias.generic_params.is_empty() {
            line.push('[');
            for (i, param) in alias.generic_params.iter().enumerate() {
                if i > 0 {
                    line.push_str(", ");
                }
                line.push_str(&param.name);
            }
            line.push(']');
        }

        line.push_str(" = ");
        line.push_str(&self.fmt_type(&alias.type_expr));
        self.writeln(&line);
    }

    fn fmt_effect(&mut self, effect: &EffectDecl) {
        self.writeln(&format!("effect {}", effect.name));
        self.push_indent();
        for op in &effect.operations {
            self.fmt_cell(op);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_effect_bind(&mut self, bind: &EffectBindDecl) {
        self.writeln(&format!(
            "bind effect {} to {}",
            bind.effect_path, bind.tool_alias
        ));
    }

    fn fmt_use_tool(&mut self, use_tool: &UseToolDecl) {
        let mut line = format!("use tool {} as {}", use_tool.tool_path, use_tool.alias);
        if let Some(url) = &use_tool.mcp_url {
            line.push_str(&format!(" from \"{}\"", url));
        }
        self.writeln(&line);
    }

    fn fmt_grant(&mut self, grant: &GrantDecl) {
        let mut line = format!("grant {}", grant.tool_alias);
        if !grant.constraints.is_empty() {
            line.push_str(" with {");
            for (i, constraint) in grant.constraints.iter().enumerate() {
                if i > 0 {
                    line.push_str(", ");
                }
                line.push_str(&constraint.key);
                line.push_str(": ");
                line.push_str(&self.fmt_expr(&constraint.value));
            }
            line.push('}');
        }
        self.writeln(&line);
    }

    fn fmt_import(&mut self, import: &ImportDecl) {
        let mut line = String::new();
        if import.is_pub {
            line.push_str("pub ");
        }
        line.push_str("import ");

        match &import.names {
            ImportList::Wildcard => {
                line.push_str(&import.path.join("::"));
                line.push_str("::*");
            }
            ImportList::Names(names) => {
                line.push('{');
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        line.push_str(", ");
                    }
                    line.push_str(&name.name);
                    if let Some(alias) = &name.alias {
                        line.push_str(" as ");
                        line.push_str(alias);
                    }
                }
                line.push_str("} from ");
                line.push_str(&import.path.join("::"));
            }
        }

        self.writeln(&line);
    }

    fn fmt_process(&mut self, process: &ProcessDecl) {
        self.writeln(&format!("{} {}", process.kind, process.name));
        self.push_indent();

        if let Some(initial) = &process.machine_initial {
            self.writeln(&format!("initial {}", initial));
        }

        for state in &process.machine_states {
            self.fmt_machine_state(state);
        }

        if !process.pipeline_stages.is_empty() {
            self.writeln(&format!("stages: {}", process.pipeline_stages.join(" -> ")));
        }

        for grant in &process.grants {
            self.fmt_grant(grant);
        }

        for cell in &process.cells {
            self.writeln("");
            self.fmt_cell(cell);
        }

        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_machine_state(&mut self, state: &MachineStateDecl) {
        let mut line = format!("state {}", state.name);
        if !state.params.is_empty() {
            line.push('(');
            for (i, param) in state.params.iter().enumerate() {
                if i > 0 {
                    line.push_str(", ");
                }
                line.push_str(&param.name);
                line.push_str(": ");
                line.push_str(&self.fmt_type(&param.ty));
            }
            line.push(')');
        }
        if state.terminal {
            line.push_str(" [terminal]");
        }
        self.writeln(&line);

        if let Some(guard) = &state.guard {
            self.push_indent();
            self.writeln(&format!("when {}", self.fmt_expr(guard)));
            self.pop_indent();
        }

        if let Some(transition) = &state.transition_to {
            self.push_indent();
            let mut trans = format!("then {}", transition);
            if !state.transition_args.is_empty() {
                trans.push('(');
                for (i, arg) in state.transition_args.iter().enumerate() {
                    if i > 0 {
                        trans.push_str(", ");
                    }
                    trans.push_str(&self.fmt_expr(arg));
                }
                trans.push(')');
            }
            self.writeln(&trans);
            self.pop_indent();
        }
    }

    fn fmt_agent(&mut self, agent: &AgentDecl) {
        self.writeln(&format!("agent {}", agent.name));
        self.push_indent();

        for grant in &agent.grants {
            self.fmt_grant(grant);
        }

        for cell in &agent.cells {
            self.writeln("");
            self.fmt_cell(cell);
        }

        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_handler(&mut self, handler: &HandlerDecl) {
        self.writeln(&format!("handler {}", handler.name));
        self.push_indent();
        for handle in &handler.handles {
            self.fmt_cell(handle);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_addon(&mut self, addon: &AddonDecl) {
        let mut line = format!("addon {}", addon.kind);
        if let Some(name) = &addon.name {
            line.push(' ');
            line.push_str(name);
        }
        self.writeln(&line);
    }

    fn fmt_trait(&mut self, trt: &TraitDef) {
        let mut header = String::new();
        if trt.is_pub {
            header.push_str("pub ");
        }
        header.push_str("trait ");
        header.push_str(&trt.name);
        if !trt.parent_traits.is_empty() {
            header.push_str(": ");
            header.push_str(&trt.parent_traits.join(" + "));
        }

        self.writeln(&header);
        self.push_indent();
        for method in &trt.methods {
            self.fmt_cell(method);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_impl(&mut self, impl_def: &ImplDef) {
        let mut header = format!("impl {} for {}", impl_def.trait_name, impl_def.target_type);
        if !impl_def.generic_params.is_empty() {
            header.push('[');
            for (i, param) in impl_def.generic_params.iter().enumerate() {
                if i > 0 {
                    header.push_str(", ");
                }
                header.push_str(&param.name);
            }
            header.push(']');
        }

        self.writeln(&header);
        self.push_indent();
        for cell in &impl_def.cells {
            self.fmt_cell(cell);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_const_decl(&mut self, const_decl: &ConstDeclDef) {
        let mut line = format!("const {}", const_decl.name);
        if let Some(ty) = &const_decl.type_ann {
            line.push_str(": ");
            line.push_str(&self.fmt_type(ty));
        }
        line.push_str(" = ");
        line.push_str(&self.fmt_expr(&const_decl.value));
        self.writeln(&line);
    }

    fn fmt_macro_decl(&mut self, macro_decl: &MacroDeclDef) {
        let mut header = format!("macro {}", macro_decl.name);
        if !macro_decl.params.is_empty() {
            header.push('(');
            header.push_str(&macro_decl.params.join(", "));
            header.push(')');
        }

        self.writeln(&header);
        self.push_indent();
        for stmt in &macro_decl.body {
            self.fmt_stmt(stmt);
        }
        self.pop_indent();
        self.writeln("end");
    }

    fn fmt_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(s) => {
                let mut line = String::new();
                if s.mutable {
                    line.push_str("let mut ");
                } else {
                    line.push_str("let ");
                }

                if let Some(pattern) = &s.pattern {
                    line.push_str(&self.fmt_pattern(pattern));
                } else {
                    line.push_str(&s.name);
                }

                if let Some(ty) = &s.ty {
                    line.push_str(": ");
                    line.push_str(&self.fmt_type(ty));
                }
                line.push_str(" = ");
                line.push_str(&self.fmt_expr(&s.value));
                self.writeln(&line);
            }
            Stmt::If(s) => {
                self.writeln(&format!("if {}", self.fmt_expr(&s.condition)));
                self.push_indent();
                for stmt in &s.then_body {
                    self.fmt_stmt(stmt);
                }
                self.pop_indent();
                if let Some(else_body) = &s.else_body {
                    self.writeln("else");
                    self.push_indent();
                    for stmt in else_body {
                        self.fmt_stmt(stmt);
                    }
                    self.pop_indent();
                }
                self.writeln("end");
            }
            Stmt::For(s) => {
                let mut header = String::from("for ");
                if let Some(label) = &s.label {
                    header.push('@');
                    header.push_str(label);
                    header.push(' ');
                }
                if let Some(pattern) = &s.pattern {
                    header.push_str(&self.fmt_pattern(pattern));
                } else {
                    header.push_str(&s.var);
                }
                header.push_str(" in ");
                header.push_str(&self.fmt_expr(&s.iter));
                if let Some(filter) = &s.filter {
                    header.push_str(" if ");
                    header.push_str(&self.fmt_expr(filter));
                }
                self.writeln(&header);
                self.push_indent();
                for stmt in &s.body {
                    self.fmt_stmt(stmt);
                }
                self.pop_indent();
                self.writeln("end");
            }
            Stmt::Match(s) => {
                self.writeln(&format!("match {}", self.fmt_expr(&s.subject)));
                self.push_indent();
                for arm in &s.arms {
                    // Check if arm body is a single short statement (return/expr)
                    let is_short_arm = arm.body.len() == 1
                        && match &arm.body[0] {
                            Stmt::Return(r) => self.fmt_expr(&r.value).len() < 40,
                            Stmt::Expr(e) => self.fmt_expr(&e.expr).len() < 40,
                            _ => false,
                        };

                    if is_short_arm {
                        // Format short arms on one line: pattern -> value
                        let value = match &arm.body[0] {
                            Stmt::Return(r) => self.fmt_expr(&r.value),
                            Stmt::Expr(e) => self.fmt_expr(&e.expr),
                            _ => unreachable!(),
                        };
                        self.writeln(&format!("{} -> {}", self.fmt_pattern(&arm.pattern), value));
                    } else {
                        // Format long arms with indentation
                        self.writeln(&format!("{} ->", self.fmt_pattern(&arm.pattern)));
                        self.push_indent();
                        for stmt in &arm.body {
                            self.fmt_stmt(stmt);
                        }
                        self.pop_indent();
                    }
                }
                self.pop_indent();
                self.writeln("end");
            }
            Stmt::Return(s) => {
                self.writeln(&format!("return {}", self.fmt_expr(&s.value)));
            }
            Stmt::Halt(s) => {
                self.writeln(&format!("halt {}", self.fmt_expr(&s.message)));
            }
            Stmt::Assign(s) => {
                self.writeln(&format!("{} = {}", s.target, self.fmt_expr(&s.value)));
            }
            Stmt::Expr(s) => {
                self.writeln(&self.fmt_expr(&s.expr));
            }
            Stmt::While(s) => {
                let label_str = if let Some(label) = &s.label {
                    format!("@{} ", label)
                } else {
                    String::new()
                };
                self.writeln(&format!(
                    "while {}{}",
                    label_str,
                    self.fmt_expr(&s.condition)
                ));
                self.push_indent();
                for stmt in &s.body {
                    self.fmt_stmt(stmt);
                }
                self.pop_indent();
                self.writeln("end");
            }
            Stmt::Loop(s) => {
                if let Some(label) = &s.label {
                    self.writeln(&format!("loop @{}", label));
                } else {
                    self.writeln("loop");
                }
                self.push_indent();
                for stmt in &s.body {
                    self.fmt_stmt(stmt);
                }
                self.pop_indent();
                self.writeln("end");
            }
            Stmt::Break(s) => {
                if let Some(value) = &s.value {
                    self.writeln(&format!("break {}", self.fmt_expr(value)));
                } else {
                    self.writeln("break");
                }
            }
            Stmt::Continue(_) => {
                self.writeln("continue");
            }
            Stmt::Emit(s) => {
                self.writeln(&format!("emit {}", self.fmt_expr(&s.value)));
            }
            Stmt::CompoundAssign(s) => {
                let op = match s.op {
                    CompoundOp::AddAssign => "+=",
                    CompoundOp::SubAssign => "-=",
                    CompoundOp::MulAssign => "*=",
                    CompoundOp::DivAssign => "/=",
                    CompoundOp::FloorDivAssign => "//=",
                    CompoundOp::ModAssign => "%=",
                    CompoundOp::PowAssign => "**=",
                    CompoundOp::BitAndAssign => "&=",
                    CompoundOp::BitOrAssign => "|=",
                    CompoundOp::BitXorAssign => "^=",
                };
                self.writeln(&format!("{} {} {}", s.target, op, self.fmt_expr(&s.value)));
            }
            Stmt::Defer(s) => {
                self.writeln("defer");
                self.indent += 1;
                for stmt in &s.body {
                    self.fmt_stmt(stmt);
                }
                self.indent -= 1;
                self.writeln("end");
            }
            Stmt::Yield(s) => {
                self.writeln(&format!("yield {}", self.fmt_expr(&s.value)));
            }
        }
    }

    fn fmt_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n, _) => n.to_string(),
            Expr::BigIntLit(n, _) => n.to_string(),
            Expr::FloatLit(f, _) => f.to_string(),
            Expr::StringLit(s, _) => format!("\"{}\"", escape_string(s)),
            Expr::StringInterp(segments, _) => {
                let mut result = String::from("\"");
                for segment in segments {
                    match segment {
                        StringSegment::Literal(s) => result.push_str(&escape_string(s)),
                        StringSegment::Interpolation(e) => {
                            result.push('{');
                            result.push_str(&self.fmt_expr(e));
                            result.push('}');
                        }
                        StringSegment::FormattedInterpolation(e, spec) => {
                            result.push('{');
                            result.push_str(&self.fmt_expr(e));
                            result.push(':');
                            result.push_str(&spec.raw);
                            result.push('}');
                        }
                    }
                }
                result.push('"');
                result
            }
            Expr::BoolLit(b, _) => b.to_string(),
            Expr::NullLit(_) => "null".to_string(),
            Expr::RawStringLit(s, _) => format!("r\"{}\"", s),
            Expr::BytesLit(_, _) => "b\"...\"".to_string(),
            Expr::Ident(s, _) => s.clone(),
            Expr::ListLit(items, _) => {
                let mut result = String::from("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_expr(item));
                }
                result.push(']');
                result
            }
            Expr::MapLit(pairs, _) => {
                let mut result = String::from("{");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_expr(k));
                    result.push_str(": ");
                    result.push_str(&self.fmt_expr(v));
                }
                result.push('}');
                result
            }
            Expr::RecordLit(name, fields, _) => {
                let mut result = name.clone();
                result.push('(');
                for (i, (fname, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(fname);
                    result.push_str(": ");
                    result.push_str(&self.fmt_expr(value));
                }
                result.push(')');
                result
            }
            Expr::BinOp(left, op, right, _) => {
                format!("{} {} {}", self.fmt_expr(left), op, self.fmt_expr(right))
            }
            Expr::Pipe { left, right, .. } => {
                format!("{} |> {}", self.fmt_expr(left), self.fmt_expr(right))
            }
            Expr::UnaryOp(op, expr, _) => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "not ",
                    UnaryOp::BitNot => "~",
                };
                format!("{}{}", op_str, self.fmt_expr(expr))
            }
            Expr::Call(func, args, _) => {
                let mut result = self.fmt_expr(func);
                result.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    match arg {
                        CallArg::Positional(e) => result.push_str(&self.fmt_expr(e)),
                        CallArg::Named(name, e, _) => {
                            result.push_str(name);
                            result.push_str(": ");
                            result.push_str(&self.fmt_expr(e));
                        }
                        CallArg::Role(_, _, _) => {} // Handled separately in ToolCall
                    }
                }
                result.push(')');
                result
            }
            Expr::ToolCall(func, args, _) => {
                let mut result = self.fmt_expr(func);
                result.push('(');
                let mut first = true;
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => {
                            if !first {
                                result.push_str(", ");
                            }
                            first = false;
                            result.push_str(&self.fmt_expr(e));
                        }
                        CallArg::Named(name, e, _) => {
                            if !first {
                                result.push_str(", ");
                            }
                            first = false;
                            result.push_str(name);
                            result.push_str(": ");
                            result.push_str(&self.fmt_expr(e));
                        }
                        CallArg::Role(_, _, _) => {} // Skip roles in args list
                    }
                }
                result.push(')');
                result
            }
            Expr::DotAccess(expr, field, _) => {
                format!("{}.{}", self.fmt_expr(expr), field)
            }
            Expr::IndexAccess(expr, index, _) => {
                format!("{}[{}]", self.fmt_expr(expr), self.fmt_expr(index))
            }
            Expr::RoleBlock(role, content, _) => {
                format!("role {}: {} end", role, self.fmt_expr(content))
            }
            Expr::ExpectSchema(expr, schema, _) => {
                format!("{} expect schema {}", self.fmt_expr(expr), schema)
            }
            Expr::Lambda {
                params,
                return_type,
                body,
                ..
            } => {
                let mut result = String::from("fn(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&param.name);
                    result.push_str(": ");
                    result.push_str(&self.fmt_type(&param.ty));
                }
                result.push(')');

                if let Some(ret) = return_type {
                    result.push_str(" -> ");
                    result.push_str(&self.fmt_type(ret));
                }

                match body {
                    LambdaBody::Expr(e) => {
                        result.push_str(" => ");
                        result.push_str(&self.fmt_expr(e));
                    }
                    LambdaBody::Block(_) => {
                        result.push_str(" ... end");
                    }
                }

                result
            }
            Expr::TupleLit(items, _) => {
                let mut result = String::from("(");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_expr(item));
                }
                result.push(')');
                result
            }
            Expr::SetLit(items, _) => {
                let mut result = String::from("set[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_expr(item));
                }
                result.push(']');
                result
            }
            Expr::RangeExpr {
                start,
                end,
                inclusive,
                step,
                ..
            } => {
                let mut result = String::new();
                if let Some(s) = start {
                    result.push_str(&self.fmt_expr(s));
                }
                result.push_str(if *inclusive { "..=" } else { ".." });
                if let Some(e) = end {
                    result.push_str(&self.fmt_expr(e));
                }
                if let Some(step) = step {
                    result.push_str(" step ");
                    result.push_str(&self.fmt_expr(step));
                }
                result
            }
            Expr::TryExpr(expr, _) => {
                format!("{}?", self.fmt_expr(expr))
            }
            Expr::TryElse {
                expr,
                error_binding,
                handler,
                ..
            } => {
                format!(
                    "try {} else |{}| {}",
                    self.fmt_expr(expr),
                    error_binding,
                    self.fmt_expr(handler)
                )
            }
            Expr::NullCoalesce(left, right, _) => {
                format!("{} ?? {}", self.fmt_expr(left), self.fmt_expr(right))
            }
            Expr::NullSafeAccess(expr, field, _) => {
                format!("{}?.{}", self.fmt_expr(expr), field)
            }
            Expr::NullSafeIndex(expr, idx, _) => {
                format!("{}?[{}]", self.fmt_expr(expr), self.fmt_expr(idx))
            }
            Expr::NullAssert(expr, _) => {
                format!("{}!", self.fmt_expr(expr))
            }
            Expr::SpreadExpr(expr, _) => {
                format!("...{}", self.fmt_expr(expr))
            }
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                format!(
                    "if {} then {} else {}",
                    self.fmt_expr(cond),
                    self.fmt_expr(then_val),
                    self.fmt_expr(else_val)
                )
            }
            Expr::AwaitExpr(expr, _) => {
                format!("await {}", self.fmt_expr(expr))
            }
            Expr::Comprehension {
                body,
                var,
                iter,
                condition,
                kind,
                ..
            } => {
                let bracket = match kind {
                    ComprehensionKind::List => ("[", "]"),
                    ComprehensionKind::Set => ("set[", "]"),
                    ComprehensionKind::Map => ("{", "}"),
                };
                let mut result = String::from(bracket.0);
                result.push_str(&self.fmt_expr(body));
                result.push_str(" for ");
                result.push_str(var);
                result.push_str(" in ");
                result.push_str(&self.fmt_expr(iter));
                if let Some(cond) = condition {
                    result.push_str(" if ");
                    result.push_str(&self.fmt_expr(cond));
                }
                result.push_str(bracket.1);
                result
            }
            Expr::MatchExpr {
                subject, arms: _, ..
            } => {
                format!("match {} ... end", self.fmt_expr(subject))
            }
            Expr::BlockExpr(_, _) => "block ... end".to_string(),
            Expr::IsType {
                expr, type_name, ..
            } => {
                format!("{} is {}", self.fmt_expr(expr), type_name)
            }
            Expr::TypeCast {
                expr, target_type, ..
            } => {
                format!("{} as {}", self.fmt_expr(expr), target_type)
            }
            Expr::WhenExpr {
                arms, else_body, ..
            } => {
                let mut parts = vec!["when".to_string()];
                for arm in arms {
                    parts.push(format!(
                        "  {} -> {}",
                        self.fmt_expr(&arm.condition),
                        self.fmt_expr(&arm.body)
                    ));
                }
                if let Some(eb) = else_body {
                    parts.push(format!("  _ -> {}", self.fmt_expr(eb)));
                }
                parts.push("end".to_string());
                parts.join("\n")
            }
            Expr::ComptimeExpr(inner, _) => {
                format!("comptime {}", self.fmt_expr(inner))
            }
            Expr::Perform {
                effect_name,
                operation,
                args,
                ..
            } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.fmt_expr(a)).collect();
                format!(
                    "perform {}.{}({})",
                    effect_name,
                    operation,
                    arg_strs.join(", ")
                )
            }
            Expr::HandleExpr { body, handlers, .. } => {
                let mut parts = Vec::new();
                parts.push("handle".to_string());
                for stmt in body {
                    let expr_str = match stmt {
                        Stmt::Expr(es) => format!("  {}", self.fmt_expr(&es.expr)),
                        Stmt::Return(r) => format!("  return {}", self.fmt_expr(&r.value)),
                        Stmt::Let(ls) => {
                            format!("  let {} = {}", ls.name, self.fmt_expr(&ls.value))
                        }
                        _ => "  ...".to_string(),
                    };
                    parts.push(expr_str);
                }
                parts.push("with".to_string());
                for handler in handlers {
                    let params: Vec<String> =
                        handler.params.iter().map(|p| p.name.clone()).collect();
                    parts.push(format!(
                        "  {}.{}({}) =>",
                        handler.effect_name,
                        handler.operation,
                        params.join(", ")
                    ));
                    for stmt in &handler.body {
                        let expr_str = match stmt {
                            Stmt::Expr(es) => format!("    {}", self.fmt_expr(&es.expr)),
                            Stmt::Return(r) => format!("    return {}", self.fmt_expr(&r.value)),
                            _ => "    ...".to_string(),
                        };
                        parts.push(expr_str);
                    }
                }
                parts.push("end".to_string());
                parts.join("\n")
            }
            Expr::ResumeExpr(inner, _) => {
                format!("resume({})", self.fmt_expr(inner))
            }
        }
    }

    fn fmt_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(name, _) => name.clone(),
            TypeExpr::List(inner, _) => format!("list[{}]", self.fmt_type(inner)),
            TypeExpr::Map(k, v, _) => format!("map[{}, {}]", self.fmt_type(k), self.fmt_type(v)),
            TypeExpr::Result(ok, err, _) => {
                format!("result[{}, {}]", self.fmt_type(ok), self.fmt_type(err))
            }
            TypeExpr::Union(types, _) => types
                .iter()
                .map(|t| self.fmt_type(t))
                .collect::<Vec<_>>()
                .join(" | "),
            TypeExpr::Null(_) => "null".to_string(),
            TypeExpr::Tuple(types, _) => {
                let mut result = String::from("(");
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_type(ty));
                }
                result.push(')');
                result
            }
            TypeExpr::Set(inner, _) => format!("set[{}]", self.fmt_type(inner)),
            TypeExpr::Fn(params, ret, effects, _) => {
                let mut result = String::from("fn(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_type(param));
                }
                result.push_str(") -> ");
                result.push_str(&self.fmt_type(ret));
                if !effects.is_empty() {
                    result.push_str(" / {");
                    result.push_str(&effects.join(", "));
                    result.push('}');
                }
                result
            }
            TypeExpr::Generic(name, args, _) => {
                let mut result = name.clone();
                result.push('[');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_type(arg));
                }
                result.push(']');
                result
            }
        }
    }

    fn fmt_pattern(&self, pattern: &Pattern) -> String {
        match pattern {
            Pattern::Literal(expr) => self.fmt_expr(expr),
            Pattern::Variant(name, binding, _) => {
                if let Some(b) = binding {
                    format!("{}({})", name, self.fmt_pattern(b))
                } else {
                    name.clone()
                }
            }
            Pattern::Wildcard(_) => "_".to_string(),
            Pattern::Ident(name, _) => name.clone(),
            Pattern::Guard {
                inner, condition, ..
            } => {
                format!(
                    "{} if {}",
                    self.fmt_pattern(inner),
                    self.fmt_expr(condition)
                )
            }
            Pattern::Or { patterns, .. } => patterns
                .iter()
                .map(|p| self.fmt_pattern(p))
                .collect::<Vec<_>>()
                .join(" | "),
            Pattern::ListDestructure { elements, rest, .. } => {
                let mut result = String::from("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_pattern(elem));
                }
                if let Some(r) = rest {
                    if !elements.is_empty() {
                        result.push_str(", ");
                    }
                    result.push_str("...");
                    result.push_str(r);
                }
                result.push(']');
                result
            }
            Pattern::TupleDestructure { elements, .. } => {
                let mut result = String::from("(");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&self.fmt_pattern(elem));
                }
                result.push(')');
                result
            }
            Pattern::RecordDestructure {
                type_name,
                fields,
                open,
                ..
            } => {
                let mut result = type_name.clone();
                result.push('(');
                for (i, (fname, pat)) in fields.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(fname);
                    if let Some(p) = pat {
                        result.push_str(": ");
                        result.push_str(&self.fmt_pattern(p));
                    }
                }
                if *open {
                    if !fields.is_empty() {
                        result.push_str(", ");
                    }
                    result.push_str("..");
                }
                result.push(')');
                result
            }
            Pattern::TypeCheck {
                name, type_expr, ..
            } => {
                format!("{}: {}", name, self.fmt_type(type_expr))
            }
            Pattern::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let op = if *inclusive { "..=" } else { ".." };
                format!("{}{}{}", self.fmt_expr(start), op, self.fmt_expr(end))
            }
        }
    }
}

/// Escape special characters in a string literal
fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\0' => result.push_str("\\0"),
            c => result.push(c),
        }
    }
    result
}

/// Format files in place or check if they need formatting
/// Returns (needs_formatting, reformatted_count)
pub fn format_files(files: &[PathBuf], check_mode: bool) -> Result<(bool, usize), String> {
    let mut needs_formatting = false;
    let mut reformatted_count = 0;

    for file in files {
        let content = std::fs::read_to_string(file)
            .map_err(|e| format!("error reading '{}': {}", file.display(), e))?;

        let is_lm_md = file
            .to_str()
            .map(|s| s.ends_with(".lm.md"))
            .unwrap_or(false);

        let formatted = if is_lm_md {
            format_file(&content)
        } else {
            format_lm_source(&content)
        };

        if content != formatted {
            needs_formatting = true;
            reformatted_count += 1;
            if check_mode {
                println!(
                    "  {}✗{} {}{}{} (would reformat)",
                    YELLOW,
                    RESET,
                    BOLD,
                    file.display(),
                    RESET
                );
            } else {
                std::fs::write(file, &formatted)
                    .map_err(|e| format!("error writing '{}': {}", file.display(), e))?;
                println!(
                    "  {}✓{} {}{}{} (reformatted)",
                    GREEN,
                    RESET,
                    BOLD,
                    file.display(),
                    RESET
                );
            }
        } else if !check_mode {
            println!(
                "  {}✓{} {}{}{} (unchanged)",
                GREEN,
                RESET,
                BOLD,
                file.display(),
                RESET
            );
        }
    }

    Ok((needs_formatting, reformatted_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_formatting() {
        let input = r#"cell foo() -> Int
  return 42
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("cell foo() -> Int"));
        assert!(output.contains("  return 42"));
    }

    #[test]
    fn test_record_formatting() {
        let input = r#"record Point
  x: Int
  y: Int
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("record Point"));
        assert!(output.contains("  x: Int"));
        assert!(output.contains("  y: Int"));
    }

    #[test]
    fn test_if_else_indentation() {
        let input = r#"cell main() -> Int
  if true
    return 1
  else
    return 2
  end
end"#;
        let output = format_lumen_code(input);
        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines[0], "cell main() -> Int");
        assert!(lines[1].starts_with("  if"));
        assert!(lines[2].starts_with("    return 1"));
        assert!(lines[3].starts_with("  else"));
        assert!(lines[4].starts_with("    return 2"));
    }

    #[test]
    fn test_match_short_arms() {
        // Short arms should be on one line: pattern -> value
        let input = r#"cell check(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("match x"));
        assert!(output.contains("1 -> \"one\""));
        assert!(output.contains("2 -> \"two\""));
        assert!(output.contains("_ -> \"other\""));
    }

    #[test]
    fn test_match_long_arms() {
        // Long arms should be indented
        let input = r#"cell process(x: Int) -> String
  match x
    1 ->
      let msg = "one"
      return msg
    _ ->
      return "other"
  end
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("1 ->"));
        assert!(output.contains("    let msg = \"one\""));
    }

    #[test]
    fn test_markdown_preservation() {
        let input = r#"# Hello

Some text here.

```lumen
cell greet() -> String
  return "hello"
end
```

More text.
"#;
        let output = format_file(input);
        assert!(output.contains("# Hello"));
        assert!(output.contains("Some text here."));
        assert!(output.contains("More text."));
        assert!(output.contains("  return \"hello\""));
    }

    #[test]
    fn test_blank_lines_between_items() {
        let input = r#"cell foo() -> Int
  return 1
end
cell bar() -> Int
  return 2
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("end\n\ncell bar"));
    }

    #[test]
    fn test_operator_spacing() {
        let input = "cell calc() -> Int\n  let x = 1 + 2 * 3\n  return x\nend";
        let output = format_lumen_code(input);
        assert!(output.contains("x = 1 + 2 * 3"));
    }

    #[test]
    fn test_parse_error_returns_original() {
        let input = "cell foo( -> Int"; // Missing closing paren
        let output = format_lumen_code(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_enum_formatting() {
        let input = r#"enum Status
  Active
  Inactive
  Pending(String)
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("enum Status"));
        assert!(output.contains("  Active"));
        assert!(output.contains("  Inactive"));
        assert!(output.contains("  Pending(String)"));
    }

    #[test]
    fn test_while_loop() {
        let input = r#"cell count() -> Int
  let i = 0
  while i < 10
    i = i + 1
  end
  return i
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("while i < 10"));
        assert!(output.contains("    i = i + 1"));
    }

    #[test]
    fn test_for_loop() {
        let input = r#"cell sum(items: list[Int]) -> Int
  let total = 0
  for x in items
    total = total + x
  end
  return total
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("for x in items"));
        assert!(output.contains("    total = total + x"));
    }

    #[test]
    fn test_compound_assignment() {
        let input = r#"cell calc() -> Int
  let x = 10
  x += 5
  x -= 3
  x *= 2
  return x
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("x += 5"));
        assert!(output.contains("x -= 3"));
        assert!(output.contains("x *= 2"));
    }

    #[test]
    fn test_string_interpolation() {
        let input = r#"cell greet(name: String) -> String
  return "Hello, {name}!"
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("\"Hello, {name}!\""));
    }

    #[test]
    fn test_list_and_map_literals() {
        let input = r#"cell data() -> tuple[list[Int], map[String, Int]]
  let nums = [1, 2, 3]
  let scores = {"alice": 90, "bob": 85}
  return (nums, scores)
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("[1, 2, 3]"));
        assert!(output.contains("{\"alice\": 90, \"bob\": 85}"));
    }

    #[test]
    fn test_record_with_constraints() {
        let input = r#"record User
  name: String
  age: Int where age >= 0
  email: String = "none@example.com"
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("age: Int where age >= 0"));
        assert!(output.contains("email: String = \"none@example.com\""));
    }

    #[test]
    fn test_idempotent() {
        let input = r#"cell fibonacci(n: Int) -> Int
  if n <= 1
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end"#;
        let output1 = format_lumen_code(input);
        let output2 = format_lumen_code(&output1);
        assert_eq!(output1, output2, "Formatter should be idempotent");
    }

    #[test]
    fn test_empty_file() {
        let input = "";
        let output = format_lumen_code(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_only_comments() {
        // Comments aren't in AST, so file with only comments returns original
        let input = "// Just a comment\n// Another comment";
        let output = format_lumen_code(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_lambda_expression() {
        let input = r#"cell apply(f: fn(Int) -> Int, x: Int) -> Int
  return f(x)
end

cell main() -> Int
  let double = fn(n: Int) -> Int => n * 2
  return apply(double, 5)
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("fn(n: Int) -> Int => n * 2"));
    }

    #[test]
    fn test_effects() {
        let input = r#"cell fetch_data(url: String) -> String / {http}
  return get(url)
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("-> String / {http}"));
    }

    // --- Tests for .lm file markdown block preservation ---

    #[test]
    fn test_lm_preserves_markdown_block() {
        let input = "\
```
# Module Overview
This module does things.
```

cell greet() -> String
  return \"hello\"
end
";
        let output = format_lm_source(input);
        assert!(
            output.contains("# Module Overview"),
            "markdown heading preserved"
        );
        assert!(
            output.contains("This module does things."),
            "markdown body preserved"
        );
        assert!(
            output.contains("```\n# Module Overview"),
            "opening fence preserved"
        );
        assert!(output.contains("  return \"hello\""), "code is formatted");
    }

    #[test]
    fn test_lm_docstring_stays_attached() {
        // No blank line between closing ``` and declaration = docstring
        let input = "\
```
Adds two integers.
```
cell add(a: Int, b: Int) -> Int
  return a+b
end
";
        let output = format_lm_source(input);
        // The docstring closing ``` should be immediately followed by the cell
        assert!(
            output.contains("```\ncell add("),
            "docstring stays attached to declaration, got:\n{}",
            output
        );
    }

    #[test]
    fn test_lm_blank_line_between_markdown_and_code() {
        // Blank line between markdown block and code = NOT a docstring
        let input = "\
```
# Overview
```

cell foo() -> Int
  return 42
end
";
        let output = format_lm_source(input);
        // The blank line between the markdown block and code should be preserved
        assert!(
            output.contains("```\n\ncell foo("),
            "blank line preserved between markdown and code, got:\n{}",
            output
        );
    }

    #[test]
    fn test_lm_multiple_markdown_blocks() {
        let input = "\
```
# Header
```

cell first() -> Int
  return 1
end

```
# Second section
```

cell second() -> Int
  return 2
end
";
        let output = format_lm_source(input);
        assert!(
            output.contains("# Header"),
            "first markdown block preserved"
        );
        assert!(
            output.contains("# Second section"),
            "second markdown block preserved"
        );
        assert!(output.contains("cell first()"), "first cell present");
        assert!(output.contains("cell second()"), "second cell present");
    }

    #[test]
    fn test_lm_code_only_no_markdown() {
        // A .lm file with no markdown blocks should format normally
        let input = "\
cell foo() -> Int
  return 42
end

cell bar() -> Int
  return 99
end
";
        let output = format_lm_source(input);
        assert!(output.contains("cell foo() -> Int"));
        assert!(output.contains("  return 42"));
        assert!(output.contains("cell bar() -> Int"));
    }

    #[test]
    fn test_lm_markdown_block_content_not_formatted() {
        // Markdown content should NOT be run through the code formatter
        let input = "\
```
This has **bold** and *italic* markdown.
- List item 1
- List item 2
```
cell main() -> Int
  return 0
end
";
        let output = format_lm_source(input);
        assert!(
            output.contains("This has **bold** and *italic* markdown."),
            "markdown formatting preserved verbatim"
        );
        assert!(output.contains("- List item 1"), "list preserved");
        assert!(output.contains("- List item 2"), "list preserved");
    }

    #[test]
    fn test_lm_empty_file() {
        let output = format_lm_source("");
        assert_eq!(output, "");
    }

    #[test]
    fn test_lm_only_markdown() {
        let input = "\
```
Just documentation, no code.
```
";
        let output = format_lm_source(input);
        assert!(output.contains("Just documentation, no code."));
    }

    #[test]
    fn test_lm_idempotent() {
        let input = "\
```
Docstring for add.
```
cell add(a: Int, b: Int) -> Int
  return a + b
end

```
# Section Two
```

cell multiply(a: Int, b: Int) -> Int
  return a * b
end
";
        let output1 = format_lm_source(input);
        let output2 = format_lm_source(&output1);
        assert_eq!(output1, output2, "lm formatter should be idempotent");
    }

    #[test]
    fn test_lm_markdown_with_code_fence_info() {
        // Markdown blocks might have info strings like ```markdown
        let input = "\
```markdown
# Title
Some description here.
```
cell main() -> Int
  return 0
end
";
        let output = format_lm_source(input);
        assert!(output.contains("```markdown"), "info string preserved");
        assert!(output.contains("# Title"), "content preserved");
    }
}
