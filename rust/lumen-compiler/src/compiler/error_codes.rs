//! Stable error codes for all `CompileError` variants.
//!
//! Code ranges:
//!   E0001–E0099  Lex / Parse errors
//!   E0100–E0199  Resolve errors
//!   E0200–E0299  Type errors
//!   E0300–E0399  Constraint errors
//!   E0400–E0499  Ownership errors
//!   E0500–E0599  Lowering errors

use crate::compiler::constraints::ConstraintError;
use crate::compiler::lexer::LexError;
use crate::compiler::ownership::OwnershipError;
use crate::compiler::parser::ParseError;
use crate::compiler::resolve::ResolveError;
use crate::compiler::typecheck::TypeError;
use crate::CompileError;

// ── Lex error codes (E0001–E0009) ──────────────────────────────────

fn lex_error_code(e: &LexError) -> &'static str {
    match e {
        LexError::UnexpectedChar { .. } => "E0001",
        LexError::UnterminatedString { .. } => "E0002",
        LexError::InconsistentIndent { .. } => "E0003",
        LexError::InvalidNumber { .. } => "E0004",
        LexError::InvalidBytesLiteral { .. } => "E0005",
        LexError::InvalidUnicodeEscape { .. } => "E0006",
        LexError::UnterminatedMarkdownBlock { .. } => "E0007",
    }
}

// ── Parse error codes (E0010–E0099) ────────────────────────────────

fn parse_error_code(e: &ParseError) -> &'static str {
    match e {
        ParseError::Unexpected { .. } => "E0010",
        ParseError::UnexpectedEof => "E0011",
        ParseError::UnclosedBracket { .. } => "E0012",
        ParseError::MissingEnd { .. } => "E0013",
        ParseError::MissingType { .. } => "E0014",
        ParseError::IncompleteExpression { .. } => "E0015",
        ParseError::MalformedConstruct { .. } => "E0016",
    }
}

// ── Resolve error codes (E0100–E0199) ──────────────────────────────

fn resolve_error_code(e: &ResolveError) -> &'static str {
    match e {
        ResolveError::UndefinedType { .. } => "E0100",
        ResolveError::GenericArityMismatch { .. } => "E0101",
        ResolveError::UndefinedCell { .. } => "E0102",
        ResolveError::UndefinedTrait { .. } => "E0103",
        ResolveError::UndefinedTool { .. } => "E0104",
        ResolveError::Duplicate { .. } => "E0105",
        ResolveError::MissingEffectGrant { .. } => "E0106",
        ResolveError::UndeclaredEffect { .. } => "E0107",
        ResolveError::EffectContractViolation { .. } => "E0108",
        ResolveError::NondeterministicOperation { .. } => "E0109",
        ResolveError::MachineUnknownInitial { .. } => "E0110",
        ResolveError::MachineUnknownTransition { .. } => "E0111",
        ResolveError::MachineUnreachableState { .. } => "E0112",
        ResolveError::MachineMissingTerminal { .. } => "E0113",
        ResolveError::MachineTransitionArgCount { .. } => "E0114",
        ResolveError::MachineTransitionArgType { .. } => "E0115",
        ResolveError::MachineUnsupportedExpr { .. } => "E0116",
        ResolveError::MachineGuardType { .. } => "E0117",
        ResolveError::PipelineUnknownStage { .. } => "E0118",
        ResolveError::PipelineStageArity { .. } => "E0119",
        ResolveError::PipelineStageTypeMismatch { .. } => "E0120",
        ResolveError::CircularImport { .. } => "E0121",
        ResolveError::ModuleNotFound { .. } => "E0122",
        ResolveError::ImportedSymbolNotFound { .. } => "E0123",
        ResolveError::TraitMissingMethods { .. } => "E0124",
        ResolveError::TraitMethodSignatureMismatch { .. } => "E0125",
        ResolveError::UnstableFeature { .. } => "E0126",
        ResolveError::DeprecatedUsage { .. } => "E0127",
    }
}

// ── Type error codes (E0200–E0299) ─────────────────────────────────

fn type_error_code(e: &TypeError) -> &'static str {
    match e {
        TypeError::Mismatch { .. } => "E0200",
        TypeError::UndefinedVar { .. } => "E0201",
        TypeError::NotCallable { .. } => "E0202",
        TypeError::ArgCount { .. } => "E0203",
        TypeError::UnknownField { .. } => "E0204",
        TypeError::UndefinedType { .. } => "E0205",
        TypeError::MissingReturn { .. } => "E0206",
        TypeError::ImmutableAssign { .. } => "E0207",
        TypeError::IncompleteMatch { .. } => "E0208",
        TypeError::MustUseIgnored { .. } => "E0209",
    }
}

// ── Constraint error codes (E0300–E0399) ───────────────────────────

fn constraint_error_code(e: &ConstraintError) -> &'static str {
    match e {
        ConstraintError::Invalid { .. } => "E0300",
    }
}

// ── Ownership error codes (E0400–E0499) ────────────────────────────

fn ownership_error_code(e: &OwnershipError) -> &'static str {
    match e {
        OwnershipError::UseAfterMove { .. } => "E0400",
        OwnershipError::NotConsumed { .. } => "E0401",
        OwnershipError::AlreadyBorrowed { .. } => "E0402",
        OwnershipError::MoveWhileBorrowed { .. } => "E0403",
    }
}

// ── Public API ─────────────────────────────────────────────────────

/// Return the stable error code for the *first* sub-error inside a
/// `CompileError`.  Compound errors (`Multiple`, `Parse(Vec<..>)`, etc.)
/// return the code of the first element so callers always get a valid
/// string.
pub fn error_code(error: &CompileError) -> &'static str {
    match error {
        CompileError::Lex(e) => lex_error_code(e),
        CompileError::Parse(errors) => errors.first().map_or("E0010", parse_error_code),
        CompileError::Resolve(errors) => errors.first().map_or("E0100", resolve_error_code),
        CompileError::Type(errors) => errors.first().map_or("E0200", type_error_code),
        CompileError::Constraint(errors) => errors.first().map_or("E0300", constraint_error_code),
        CompileError::Ownership(errors) => errors.first().map_or("E0400", ownership_error_code),
        CompileError::Lower(_) => "E0500",
        CompileError::Multiple(errors) => errors.first().map_or("E0500", error_code),
        CompileError::Typestate(_) => "E0600",
        CompileError::Session(_) => "E0700",
    }
}

/// Return the stable error code for a single `LexError`.
pub fn lex_code(e: &LexError) -> &'static str {
    lex_error_code(e)
}

/// Return the stable error code for a single `ParseError`.
pub fn parse_code(e: &ParseError) -> &'static str {
    parse_error_code(e)
}

/// Return the stable error code for a single `ResolveError`.
pub fn resolve_code(e: &ResolveError) -> &'static str {
    resolve_error_code(e)
}

/// Return the stable error code for a single `TypeError`.
pub fn type_code(e: &TypeError) -> &'static str {
    type_error_code(e)
}

/// Return the stable error code for a single `ConstraintError`.
pub fn constraint_code(e: &ConstraintError) -> &'static str {
    constraint_error_code(e)
}

/// Return the stable error code for a single `OwnershipError`.
pub fn ownership_code(e: &OwnershipError) -> &'static str {
    ownership_error_code(e)
}

/// Return a short (2–3 sentence) documentation string for the given
/// error code.
pub fn error_doc(code: &str) -> &'static str {
    match code {
        // Lex
        "E0001" => "An unexpected character was found in the source. Check for misplaced punctuation or non-ASCII characters outside string literals.",
        "E0002" => "A string literal was opened but never closed. Add the missing closing quote on the same line or use a multi-line string.",
        "E0003" => "Indentation is inconsistent with the rest of the file. Ensure every line uses the same number of spaces per indent level.",
        "E0004" => "A numeric literal could not be parsed. Verify the number format (e.g., no double dots, valid hex prefix).",
        "E0005" => "A bytes literal is malformed. Bytes literals must contain an even number of hex digits: b\"48656c6c6f\".",
        "E0006" => "A unicode escape sequence is invalid. Use the format \\u{XXXX} with valid hex codepoints.",
        "E0007" => "A markdown block (``` fence) was opened but never closed. Add a matching ``` line to end the block.",

        // Parse
        "E0010" => "The parser encountered a token it did not expect at this position. Check for typos, missing operators, or incorrect syntax.",
        "E0011" => "The input ended unexpectedly. This usually means a block (cell, record, if, match, etc.) is missing its closing 'end' keyword.",
        "E0012" => "A bracket ('(', '[', or '{') was opened but never closed. Add the matching closing bracket.",
        "E0013" => "A block-level construct (cell, record, if, for, etc.) is missing its closing 'end' keyword.",
        "E0014" => "A type annotation was expected after ':' but was not found. Provide a type such as Int, String, or a custom record name.",
        "E0015" => "An expression was started but is incomplete. Make sure the right-hand side of an assignment or argument is a valid expression.",
        "E0016" => "A language construct (record, enum, cell, etc.) is syntactically malformed. Review the construct's required syntax.",

        // Resolve
        "E0100" => "A type name was used that has not been defined. Ensure the record, enum, or type alias is declared before use, or check for typos.",
        "E0101" => "A generic type was instantiated with the wrong number of type arguments. For example, result[Int] is missing the error type.",
        "E0102" => "A cell (function) name was referenced that has not been defined. Check the spelling or ensure the cell is declared in the current scope.",
        "E0103" => "A trait name was referenced that has not been defined. Declare the trait before implementing or referencing it.",
        "E0104" => "A tool alias was used that has not been declared with 'use tool'. Ensure the tool is imported before granting or calling it.",
        "E0105" => "A name was defined more than once in the same scope. Rename one of the duplicate definitions to resolve the conflict.",
        "E0106" => "A cell requires an effect but no compatible grant is in scope. Add a grant block that covers the required effect.",
        "E0107" => "A cell performs an effect that is not declared in its effect row. Add the effect to the cell's signature: `/ {effect_name}`.",
        "E0108" => "A cell calls another cell whose effects are not a subset of the caller's declared effects. Propagate or handle the missing effect.",
        "E0109" => "A nondeterministic operation was used inside a @deterministic cell. Remove the operation or drop the @deterministic directive.",
        "E0110" => "A machine's initial state name does not match any declared state. Check the state name spelling in the machine definition.",
        "E0111" => "A machine state transitions to a state name that does not exist. Verify the target state name in the transition.",
        "E0112" => "A machine state is unreachable from the initial state. Remove the orphan state or add a transition path to it.",
        "E0113" => "A machine declares no terminal states. At least one state must be marked terminal for the machine to halt.",
        "E0114" => "A machine transition provides the wrong number of arguments to the target state. Match the target state's parameter count.",
        "E0115" => "A machine transition argument type does not match the target state's parameter type. Fix the argument type.",
        "E0116" => "A machine state contains an unsupported expression in a guard or action. Simplify the expression.",
        "E0117" => "A machine state guard must evaluate to Bool. The guard expression returns a different type.",
        "E0118" => "A pipeline references a stage cell that has not been defined. Check the cell name in the stage declaration.",
        "E0119" => "A pipeline stage must accept exactly one data argument. Adjust the cell's signature to take a single parameter.",
        "E0120" => "The output type of one pipeline stage does not match the input type of the next. Align the stage interfaces.",
        "E0121" => "A circular import was detected. Module A imports B which imports A (possibly through intermediaries). Break the cycle.",
        "E0122" => "An imported module could not be found on disk. Check the module path and file extensions (.lm.md, .lm, .lumen).",
        "E0123" => "A named symbol imported from a module does not exist in that module. Verify the symbol name or use a wildcard import.",
        "E0124" => "A trait implementation is missing one or more required methods. Implement all methods declared in the trait.",
        "E0125" => "A trait implementation method has an incompatible signature. The parameter types and return type must match the trait declaration.",
        "E0126" => "An unstable feature was used without opting in. Pass `--allow-unstable` or set `allow_unstable = true` in the compile options.",
        "E0127" => "A deprecated cell, record, or enum was used. The declaration is marked `@deprecated` and may be removed in a future edition.",

        // Type
        "E0200" => "An expression's type does not match the expected type. For example, a cell returning String where Int is declared.",
        "E0201" => "A variable name was used that has not been defined in the current scope. Check for typos or missing let bindings.",
        "E0202" => "An expression was used in call position but is not callable. Only cells (functions) and closures can be called.",
        "E0203" => "A cell was called with the wrong number of arguments. Check the cell's signature for the expected parameter count.",
        "E0204" => "A field was accessed on a record that does not have that field. Check the field name or the record definition.",
        "E0205" => "A type name used in a type annotation is not defined. Ensure the type is declared or imported before use.",
        "E0206" => "A cell with a return type does not have a return statement on every code path. Add a return or ensure all branches return.",
        "E0207" => "An assignment was made to an immutable variable. Declare the variable with 'let mut' to allow reassignment.",
        "E0208" => "A match expression does not cover all variants of the matched enum. Add the missing arms or use a wildcard '_' pattern.",
        "E0209" => "The return value of a @must_use cell was discarded. Assign the result to a variable or use it in an expression.",

        // Constraint
        "E0300" => "A field constraint (where clause) is invalid. Ensure the constraint expression is well-formed and uses supported operations.",

        // Ownership
        "E0400" => "A variable was used after its value had already been moved. Clone the value before moving, or restructure to avoid reuse.",
        "E0401" => "An owned variable went out of scope without being consumed. Use or explicitly drop the value before the scope ends.",
        "E0402" => "A variable was borrowed while it already has an active borrow. End the first borrow before creating another.",
        "E0403" => "A variable was moved while it still has active borrows. End all borrows before moving the value.",

        // Lowering
        "E0500" => "An internal error occurred during LIR lowering. This is usually caused by very large cells exceeding register limits.",

        _ => "Unknown error code.",
    }
}

/// Return all registered error codes with their short description.
pub fn all_error_codes() -> Vec<(&'static str, &'static str)> {
    let codes = [
        "E0001", "E0002", "E0003", "E0004", "E0005", "E0006", "E0007", "E0010", "E0011", "E0012",
        "E0013", "E0014", "E0015", "E0016", "E0100", "E0101", "E0102", "E0103", "E0104", "E0105",
        "E0106", "E0107", "E0108", "E0109", "E0110", "E0111", "E0112", "E0113", "E0114", "E0115",
        "E0116", "E0117", "E0118", "E0119", "E0120", "E0121", "E0122", "E0123", "E0124", "E0125",
        "E0126", "E0127", "E0200", "E0201", "E0202", "E0203", "E0204", "E0205", "E0206", "E0207",
        "E0208", "E0209", "E0300", "E0400", "E0401", "E0402", "E0403", "E0500",
    ];
    codes.iter().map(|&c| (c, error_doc(c))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── error_code on CompileError ─────────────────────────────────

    #[test]
    fn test_lex_error_codes() {
        let e = CompileError::Lex(LexError::UnexpectedChar {
            ch: '@',
            line: 1,
            col: 1,
        });
        assert_eq!(error_code(&e), "E0001");

        let e = CompileError::Lex(LexError::UnterminatedString { line: 1, col: 1 });
        assert_eq!(error_code(&e), "E0002");

        let e = CompileError::Lex(LexError::InconsistentIndent { line: 1 });
        assert_eq!(error_code(&e), "E0003");

        let e = CompileError::Lex(LexError::InvalidNumber { line: 1, col: 1 });
        assert_eq!(error_code(&e), "E0004");

        let e = CompileError::Lex(LexError::InvalidBytesLiteral { line: 1, col: 1 });
        assert_eq!(error_code(&e), "E0005");

        let e = CompileError::Lex(LexError::InvalidUnicodeEscape { line: 1, col: 1 });
        assert_eq!(error_code(&e), "E0006");

        let e = CompileError::Lex(LexError::UnterminatedMarkdownBlock { line: 1, col: 1 });
        assert_eq!(error_code(&e), "E0007");
    }

    #[test]
    fn test_parse_error_codes() {
        let e = CompileError::Parse(vec![ParseError::Unexpected {
            found: "x".into(),
            expected: "y".into(),
            line: 1,
            col: 1,
        }]);
        assert_eq!(error_code(&e), "E0010");

        let e = CompileError::Parse(vec![ParseError::UnexpectedEof]);
        assert_eq!(error_code(&e), "E0011");

        let e = CompileError::Parse(vec![ParseError::UnclosedBracket {
            bracket: '(',
            open_line: 1,
            open_col: 1,
            current_line: 2,
            current_col: 1,
        }]);
        assert_eq!(error_code(&e), "E0012");

        let e = CompileError::Parse(vec![ParseError::MissingEnd {
            construct: "cell".into(),
            open_line: 1,
            open_col: 1,
            current_line: 2,
            current_col: 1,
        }]);
        assert_eq!(error_code(&e), "E0013");
    }

    #[test]
    fn test_resolve_error_codes() {
        let e = CompileError::Resolve(vec![ResolveError::UndefinedType {
            name: "Foo".into(),
            line: 1,
            suggestions: vec![],
        }]);
        assert_eq!(error_code(&e), "E0100");

        let e = CompileError::Resolve(vec![ResolveError::UndeclaredEffect {
            cell: "a".into(),
            effect: "http".into(),
            line: 1,
            cause: String::new(),
        }]);
        assert_eq!(error_code(&e), "E0107");

        let e = CompileError::Resolve(vec![ResolveError::CircularImport {
            module: "a".into(),
            chain: "a -> b -> a".into(),
        }]);
        assert_eq!(error_code(&e), "E0121");
    }

    #[test]
    fn test_type_error_codes() {
        let e = CompileError::Type(vec![TypeError::Mismatch {
            expected: "Int".into(),
            actual: "String".into(),
            line: 1,
        }]);
        assert_eq!(error_code(&e), "E0200");

        let e = CompileError::Type(vec![TypeError::UndefinedVar {
            name: "x".into(),
            line: 1,
        }]);
        assert_eq!(error_code(&e), "E0201");

        let e = CompileError::Type(vec![TypeError::IncompleteMatch {
            enum_name: "Color".into(),
            missing: vec!["Red".into()],
            line: 1,
        }]);
        assert_eq!(error_code(&e), "E0208");
    }

    #[test]
    fn test_constraint_error_code() {
        let e = CompileError::Constraint(vec![ConstraintError::Invalid {
            field: "x".into(),
            line: 1,
            message: "bad".into(),
        }]);
        assert_eq!(error_code(&e), "E0300");
    }

    #[test]
    fn test_ownership_error_codes() {
        use crate::compiler::tokens::Span;
        let span = Span::new(0, 1, 1, 1);
        let e = CompileError::Ownership(vec![OwnershipError::UseAfterMove {
            variable: "x".into(),
            moved_at: span,
            used_at: span,
        }]);
        assert_eq!(error_code(&e), "E0400");

        let e = CompileError::Ownership(vec![OwnershipError::NotConsumed {
            variable: "x".into(),
            declared_at: span,
        }]);
        assert_eq!(error_code(&e), "E0401");
    }

    #[test]
    fn test_lower_error_code() {
        let e = CompileError::Lower("oops".into());
        assert_eq!(error_code(&e), "E0500");
    }

    #[test]
    fn test_multiple_error_code_returns_first() {
        let e = CompileError::Multiple(vec![
            CompileError::Lower("a".into()),
            CompileError::Lex(LexError::UnexpectedChar {
                ch: '!',
                line: 1,
                col: 1,
            }),
        ]);
        assert_eq!(error_code(&e), "E0500");
    }

    // ── error_doc ──────────────────────────────────────────────────

    #[test]
    fn test_error_doc_known_code() {
        let doc = error_doc("E0200");
        assert!(doc.contains("type"));
    }

    #[test]
    fn test_error_doc_unknown_code() {
        assert_eq!(error_doc("E9999"), "Unknown error code.");
    }

    // ── all_error_codes ────────────────────────────────────────────

    #[test]
    fn test_all_error_codes_non_empty() {
        let codes = all_error_codes();
        assert!(
            codes.len() >= 40,
            "expected at least 40 codes, got {}",
            codes.len()
        );
    }

    #[test]
    fn test_all_error_codes_no_unknown_doc() {
        for (code, doc) in all_error_codes() {
            assert_ne!(
                doc, "Unknown error code.",
                "code {} has no documentation",
                code
            );
        }
    }

    // ── per-error-type code helpers ────────────────────────────────

    #[test]
    fn test_lex_code_helper() {
        assert_eq!(
            lex_code(&LexError::UnexpectedChar {
                ch: 'x',
                line: 1,
                col: 1
            }),
            "E0001"
        );
    }

    #[test]
    fn test_parse_code_helper() {
        assert_eq!(parse_code(&ParseError::UnexpectedEof), "E0011");
    }

    #[test]
    fn test_type_code_helper() {
        assert_eq!(type_code(&TypeError::NotCallable { line: 1 }), "E0202");
    }
}
