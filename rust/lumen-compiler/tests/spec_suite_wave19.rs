//! Wave 19 test suite — optional analysis pipeline integration.
//!
//! Tests for:
//! - Ownership analysis wired into compile pipeline (Warn vs Error vs Off modes)
//! - Typestate checking via CompileOptions
//! - Session type checking via CompileOptions
//! - Verifying all three can be disabled without affecting compilation

use lumen_compiler::compiler::session::{Action, SessionType};
use lumen_compiler::compiler::tokens::Span;
use lumen_compiler::compiler::typestate::{Transition, TypestateDecl};
use lumen_compiler::{
    compile_raw_with_options, compile_with_options, CompileError, CompileOptions,
    OwnershipCheckMode,
};
use std::collections::HashMap;

fn markdown(code: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", code.trim())
}

fn dummy_span() -> Span {
    Span::dummy()
}

// ============================================================================
// Ownership analysis — pipeline integration
// ============================================================================

/// Code that triggers a use-after-move: list is Owned, used twice.
const USE_AFTER_MOVE_CODE: &str = r#"
cell main() -> list[Int]
  let xs = [1, 2, 3]
  let a = xs
  let b = xs
  return b
end
"#;

/// Code with no ownership issues: only copy types.
const CLEAN_CODE: &str = r#"
cell main() -> Int
  let x = 42
  let a = x + 1
  let b = x + 2
  return b
end
"#;

#[test]
fn ownership_error_mode_rejects_use_after_move_raw() {
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Error,
        ..Default::default()
    };
    let result = compile_raw_with_options(USE_AFTER_MOVE_CODE, &opts);
    assert!(result.is_err(), "expected ownership error in Error mode");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("ownership"),
        "error should mention ownership: {}",
        msg
    );
}

#[test]
fn ownership_error_mode_rejects_use_after_move_markdown() {
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Error,
        ..Default::default()
    };
    let md = markdown(USE_AFTER_MOVE_CODE);
    let result = compile_with_options(&md, &opts);
    assert!(
        result.is_err(),
        "expected ownership error in Error mode (markdown)"
    );
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("ownership"),
        "error should mention ownership: {}",
        msg
    );
}

#[test]
fn ownership_warn_mode_allows_use_after_move() {
    // Default mode is Warn — ownership violations are detected but don't block compilation.
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Warn,
        ..Default::default()
    };
    let result = compile_raw_with_options(USE_AFTER_MOVE_CODE, &opts);
    assert!(
        result.is_ok(),
        "Warn mode should not block compilation: {:?}",
        result.err()
    );
}

#[test]
fn ownership_off_mode_skips_analysis() {
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Off,
        ..Default::default()
    };
    let result = compile_raw_with_options(USE_AFTER_MOVE_CODE, &opts);
    assert!(
        result.is_ok(),
        "Off mode should skip ownership analysis entirely: {:?}",
        result.err()
    );
}

#[test]
fn ownership_error_mode_passes_clean_code() {
    // Code with only Copy types should pass even in Error mode.
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Error,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(
        result.is_ok(),
        "clean code should pass ownership Error mode: {:?}",
        result.err()
    );
}

#[test]
fn ownership_default_is_warn() {
    // Default CompileOptions should use Warn mode.
    let opts = CompileOptions::default();
    assert_eq!(opts.ownership_mode, OwnershipCheckMode::Warn);
}

// ============================================================================
// Typestate checking — pipeline integration
// ============================================================================

/// Build a File typestate: states Open, Closed; transitions via read/write/close.
fn file_typestate() -> TypestateDecl {
    TypestateDecl {
        type_name: "File".to_string(),
        states: vec!["Open".to_string(), "Closed".to_string()],
        initial_state: "Open".to_string(),
        transitions: vec![
            Transition {
                from_state: "Open".to_string(),
                to_state: "Open".to_string(),
                via_method: "read".to_string(),
            },
            Transition {
                from_state: "Open".to_string(),
                to_state: "Open".to_string(),
                via_method: "write".to_string(),
            },
            Transition {
                from_state: "Open".to_string(),
                to_state: "Closed".to_string(),
                via_method: "close".to_string(),
            },
        ],
    }
}

/// Code that opens a "File" record and calls methods in a valid order.
/// The typestate checker walks the AST for method calls on typestate-tracked vars.
/// This declares a File record, constructs it, and calls close() — a valid transition.
const TYPESTATE_VALID_CODE: &str = r#"
record File
  name: String
end

cell process() -> String
  let f = File(name: "data.txt")
  let result = f.close()
  return "done"
end
"#;

/// Code that calls close() twice — second close() should be an invalid transition
/// from Closed state.
const TYPESTATE_DOUBLE_CLOSE_CODE: &str = r#"
record File
  name: String
end

cell process() -> String
  let f = File(name: "data.txt")
  let r1 = f.close()
  let r2 = f.close()
  return "done"
end
"#;

/// Code that calls read() after close() — invalid transition from Closed.
const TYPESTATE_READ_AFTER_CLOSE_CODE: &str = r#"
record File
  name: String
end

cell process() -> String
  let f = File(name: "data.txt")
  let r1 = f.close()
  let r2 = f.read()
  return "done"
end
"#;

#[test]
fn typestate_no_declarations_skips_check() {
    // Without typestate declarations, typestate checking is not run.
    let opts = CompileOptions::default();
    assert!(opts.typestate_declarations.is_empty());
    let result = compile_raw_with_options(TYPESTATE_DOUBLE_CLOSE_CODE, &opts);
    // Should compile fine — typestate is opt-in.
    assert!(
        result.is_ok(),
        "without typestate declarations, code should compile: {:?}",
        result.err()
    );
}

#[test]
fn typestate_catches_double_close() {
    let mut decls = HashMap::new();
    decls.insert("File".to_string(), file_typestate());
    let opts = CompileOptions {
        typestate_declarations: decls,
        ..Default::default()
    };
    let result = compile_raw_with_options(TYPESTATE_DOUBLE_CLOSE_CODE, &opts);
    assert!(result.is_err(), "typestate should catch double close");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("typestate") || msg.contains("transition") || msg.contains("invalid"),
        "error should mention typestate issue: {}",
        msg
    );
}

#[test]
fn typestate_catches_read_after_close() {
    let mut decls = HashMap::new();
    decls.insert("File".to_string(), file_typestate());
    let opts = CompileOptions {
        typestate_declarations: decls,
        ..Default::default()
    };
    let result = compile_raw_with_options(TYPESTATE_READ_AFTER_CLOSE_CODE, &opts);
    assert!(result.is_err(), "typestate should catch read after close");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("typestate") || msg.contains("transition") || msg.contains("invalid"),
        "error should mention typestate issue: {}",
        msg
    );
}

#[test]
fn typestate_valid_transitions_compile_ok() {
    let mut decls = HashMap::new();
    decls.insert("File".to_string(), file_typestate());
    let opts = CompileOptions {
        typestate_declarations: decls,
        ..Default::default()
    };
    let result = compile_raw_with_options(TYPESTATE_VALID_CODE, &opts);
    assert!(
        result.is_ok(),
        "valid typestate transitions should compile: {:?}",
        result.err()
    );
}

// ============================================================================
// Session type checking — pipeline integration
// ============================================================================

/// A simple login protocol: Client sends Credentials, Server responds with AuthResult.
fn login_protocol() -> SessionType {
    SessionType::Then(
        Box::new(SessionType::Send {
            msg_type: "Credentials".to_string(),
        }),
        Box::new(SessionType::Recv {
            msg_type: "AuthResult".to_string(),
        }),
    )
}

#[test]
fn session_no_protocols_skips_check() {
    // Without session protocol declarations, session checking is not run.
    let opts = CompileOptions::default();
    assert!(opts.session_protocols.is_empty());
    // Any code should compile without session errors.
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(
        result.is_ok(),
        "without session protocols, code should compile: {:?}",
        result.err()
    );
}

#[test]
fn session_complete_protocol_passes() {
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![
            (Action::Send("Credentials".to_string()), dummy_span()),
            (Action::Recv("AuthResult".to_string()), dummy_span()),
        ],
    );

    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    // We compile a simple valid program — the session check is on the action sequences,
    // not the Lumen source directly.
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(
        result.is_ok(),
        "complete session protocol should pass: {:?}",
        result.err()
    );
}

#[test]
fn session_incomplete_protocol_fails() {
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    // Only send Credentials — missing the Recv AuthResult.
    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![(Action::Send("Credentials".to_string()), dummy_span())],
    );

    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(result.is_err(), "incomplete session should fail");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("session") || msg.contains("complete") || msg.contains("remaining"),
        "error should mention session incompleteness: {}",
        msg
    );
}

#[test]
fn session_wrong_message_type_fails() {
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    // Send wrong message type.
    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![
            (Action::Send("WrongType".to_string()), dummy_span()),
            (Action::Recv("AuthResult".to_string()), dummy_span()),
        ],
    );

    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(result.is_err(), "wrong message type should fail");
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("session") || msg.contains("unexpected") || msg.contains("message"),
        "error should mention message mismatch: {}",
        msg
    );
}

#[test]
fn session_wrong_action_kind_fails() {
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    // Protocol expects Send first, but we try Recv.
    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![
            (Action::Recv("Credentials".to_string()), dummy_span()),
            (Action::Recv("AuthResult".to_string()), dummy_span()),
        ],
    );

    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(result.is_err(), "wrong action kind should fail");
}

#[test]
fn session_undeclared_protocol_fails() {
    // Declare one protocol but reference a different (non-existent) one in actions.
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    let mut actions = HashMap::new();
    actions.insert(
        "Ghost".to_string(),
        vec![(Action::Send("Data".to_string()), dummy_span())],
    );

    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(
        result.is_err(),
        "referencing an undeclared protocol should fail"
    );
    let msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        msg.contains("session") || msg.contains("undeclared") || msg.contains("protocol"),
        "error should mention undeclared protocol: {}",
        msg
    );
}

// ============================================================================
// Combined: all three analyses can coexist
// ============================================================================

#[test]
fn all_analyses_enabled_clean_code_passes() {
    let mut decls = HashMap::new();
    decls.insert("File".to_string(), file_typestate());

    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());

    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![
            (Action::Send("Credentials".to_string()), dummy_span()),
            (Action::Recv("AuthResult".to_string()), dummy_span()),
        ],
    );

    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Error,
        typestate_declarations: decls,
        session_protocols: protocols,
        session_actions: actions,
    };
    // Clean code with no File usage and a complete session — should pass all three.
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    assert!(
        result.is_ok(),
        "all analyses enabled on clean code should pass: {:?}",
        result.err()
    );
}

#[test]
fn default_options_do_not_break_existing_code() {
    // Verify that CompileOptions::default() produces no regressions on arbitrary code.
    let opts = CompileOptions::default();
    let code = r#"
record Point
  x: Int
  y: Int
end

cell distance(a: Point, b: Point) -> Float
  let dx = a.x - b.x
  let dy = a.y - b.y
  return to_float(dx * dx + dy * dy)
end

cell main() -> Float
  let p1 = Point(x: 0, y: 0)
  let p2 = Point(x: 3, y: 4)
  return distance(p1, p2)
end
"#;
    let result = compile_raw_with_options(code, &opts);
    assert!(
        result.is_ok(),
        "default options should not break existing code: {:?}",
        result.err()
    );
}

// ============================================================================
// Error variant discrimination
// ============================================================================

#[test]
fn ownership_error_produces_ownership_variant() {
    let opts = CompileOptions {
        ownership_mode: OwnershipCheckMode::Error,
        ..Default::default()
    };
    let result = compile_raw_with_options(USE_AFTER_MOVE_CODE, &opts);
    let err = result.unwrap_err();
    // The error should be an Ownership variant (possibly wrapped in Multiple).
    let has_ownership = match &err {
        CompileError::Ownership(_) => true,
        CompileError::Multiple(errs) => {
            errs.iter().any(|e| matches!(e, CompileError::Ownership(_)))
        }
        _ => false,
    };
    assert!(
        has_ownership,
        "expected CompileError::Ownership variant, got: {:?}",
        err
    );
}

#[test]
fn typestate_error_produces_typestate_variant() {
    let mut decls = HashMap::new();
    decls.insert("File".to_string(), file_typestate());
    let opts = CompileOptions {
        typestate_declarations: decls,
        ..Default::default()
    };
    let result = compile_raw_with_options(TYPESTATE_DOUBLE_CLOSE_CODE, &opts);
    let err = result.unwrap_err();
    let has_typestate = match &err {
        CompileError::Typestate(_) => true,
        CompileError::Multiple(errs) => {
            errs.iter().any(|e| matches!(e, CompileError::Typestate(_)))
        }
        _ => false,
    };
    assert!(
        has_typestate,
        "expected CompileError::Typestate variant, got: {:?}",
        err
    );
}

#[test]
fn session_error_produces_session_variant() {
    let mut protocols = HashMap::new();
    protocols.insert("Login".to_string(), login_protocol());
    let mut actions = HashMap::new();
    actions.insert(
        "Login".to_string(),
        vec![(Action::Send("Credentials".to_string()), dummy_span())],
    );
    let opts = CompileOptions {
        session_protocols: protocols,
        session_actions: actions,
        ..Default::default()
    };
    let result = compile_raw_with_options(CLEAN_CODE, &opts);
    let err = result.unwrap_err();
    let has_session = match &err {
        CompileError::Session(_) => true,
        CompileError::Multiple(errs) => errs.iter().any(|e| matches!(e, CompileError::Session(_))),
        _ => false,
    };
    assert!(
        has_session,
        "expected CompileError::Session variant, got: {:?}",
        err
    );
}
