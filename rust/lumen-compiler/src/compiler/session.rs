//! Session types — protocol verification for agent communication.
//!
//! Session types provide a compile-time guarantee that two communicating
//! parties follow a declared protocol. Each session type describes a
//! sequence of send/receive operations, choices, and recursion.
//!
//! ## Example
//!
//! ```text
//! session LoginProtocol {
//!     Client -> Server: Credentials
//!     Server -> Client: AuthResult
//!     if AuthResult.success {
//!         Client -> Server: Request
//!         Server -> Client: Response
//!     }
//! }
//! ```
//!
//! ## Duality
//!
//! Every session type has a **dual**: the complementary view from the other
//! party. If party A has `Send("Credentials")`, party B must have
//! `Recv("Credentials")`. The checker verifies that two endpoints are duals
//! of each other, ensuring protocol safety.
//!
//! ## Integration
//!
//! This module is **opt-in** — it is not wired into the main `compile()` pipeline.
//! Use [`SessionChecker`] to declare protocols and verify protocol adherence.

use crate::compiler::tokens::Span;

use std::collections::HashMap;
use std::fmt;

// ── Session type AST ────────────────────────────────────────────────

/// A session type: a protocol specification for two-party communication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionType {
    /// Send a message of a given type to the peer.
    Send { msg_type: String },
    /// Receive a message of a given type from the peer.
    Recv { msg_type: String },
    /// Sequential composition: first do `fst`, then do `snd`.
    Then(Box<SessionType>, Box<SessionType>),
    /// Internal choice: the local party picks one of several labeled branches.
    Choose(Vec<(String, SessionType)>),
    /// External choice: the local party handles whichever branch the peer picks.
    Offer(Vec<(String, SessionType)>),
    /// End of session — protocol is complete.
    End,
    /// Recursive session: `label` names the recursion point, `body` is the protocol.
    Rec(String, Box<SessionType>),
    /// Reference to a recursion point (jumps back to `Rec` with matching label).
    Var(String),
}

/// An action taken by a party during protocol execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Sent a message of this type.
    Send(String),
    /// Received a message of this type.
    Recv(String),
    /// Chose a branch by label.
    Choose(String),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Send(t) => write!(f, "send({})", t),
            Action::Recv(t) => write!(f, "recv({})", t),
            Action::Choose(l) => write!(f, "choose({})", l),
        }
    }
}

// ── Errors ──────────────────────────────────────────────────────────

/// Errors produced during session type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// The message type did not match the expected type in the protocol.
    UnexpectedMessage {
        expected: String,
        got: String,
        span: Span,
    },
    /// The session ended before the protocol was fully completed.
    SessionNotComplete { remaining: String, span: Span },
    /// A general protocol violation.
    ProtocolViolation {
        protocol: String,
        detail: String,
        span: Span,
    },
    /// Action kind mismatch (e.g., tried to send when protocol expects receive).
    WrongActionKind {
        expected: String,
        got: String,
        span: Span,
    },
    /// Chose a branch that doesn't exist in the protocol.
    UnknownBranch {
        label: String,
        available: Vec<String>,
        span: Span,
    },
    /// Duality check failed.
    DualityViolation { detail: String, span: Span },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::UnexpectedMessage {
                expected,
                got,
                span,
            } => write!(
                f,
                "unexpected message: expected '{}', got '{}' (line {})",
                expected, got, span.line
            ),
            SessionError::SessionNotComplete { remaining, span } => write!(
                f,
                "session not complete: remaining protocol is '{}' (line {})",
                remaining, span.line
            ),
            SessionError::ProtocolViolation {
                protocol,
                detail,
                span,
            } => write!(
                f,
                "protocol '{}' violation: {} (line {})",
                protocol, detail, span.line
            ),
            SessionError::WrongActionKind {
                expected,
                got,
                span,
            } => write!(
                f,
                "wrong action kind: expected '{}', got '{}' (line {})",
                expected, got, span.line
            ),
            SessionError::UnknownBranch {
                label,
                available,
                span,
            } => write!(
                f,
                "unknown branch '{}', available: [{}] (line {})",
                label,
                available.join(", "),
                span.line
            ),
            SessionError::DualityViolation { detail, span } => {
                write!(f, "duality violation: {} (line {})", detail, span.line)
            }
        }
    }
}

impl std::error::Error for SessionError {}

// ── Core operations ─────────────────────────────────────────────────

/// Compute the dual of a session type.
///
/// The dual flips Send ↔ Recv and Choose ↔ Offer, preserving the
/// structure otherwise. This is the fundamental operation for verifying
/// that two endpoints are compatible.
pub fn dual(session: &SessionType) -> SessionType {
    match session {
        SessionType::Send { msg_type } => SessionType::Recv {
            msg_type: msg_type.clone(),
        },
        SessionType::Recv { msg_type } => SessionType::Send {
            msg_type: msg_type.clone(),
        },
        SessionType::Then(fst, snd) => SessionType::Then(Box::new(dual(fst)), Box::new(dual(snd))),
        SessionType::Choose(branches) => SessionType::Offer(
            branches
                .iter()
                .map(|(label, st)| (label.clone(), dual(st)))
                .collect(),
        ),
        SessionType::Offer(branches) => SessionType::Choose(
            branches
                .iter()
                .map(|(label, st)| (label.clone(), dual(st)))
                .collect(),
        ),
        SessionType::End => SessionType::End,
        SessionType::Rec(label, body) => SessionType::Rec(label.clone(), Box::new(dual(body))),
        SessionType::Var(label) => SessionType::Var(label.clone()),
    }
}

/// Check if a session type is at the terminal state (End).
pub fn is_complete(session: &SessionType) -> bool {
    matches!(session, SessionType::End)
}

/// Advance a session type by one action.
///
/// Given the current protocol state and an action, returns the remaining
/// protocol after the action is performed, or an error if the action
/// violates the protocol.
pub fn advance(
    current: &SessionType,
    action: &Action,
    span: Span,
) -> Result<SessionType, SessionError> {
    match (current, action) {
        // Send action on a Send protocol step.
        (SessionType::Send { msg_type }, Action::Send(sent_type)) => {
            if msg_type == sent_type {
                Ok(SessionType::End)
            } else {
                Err(SessionError::UnexpectedMessage {
                    expected: msg_type.clone(),
                    got: sent_type.clone(),
                    span,
                })
            }
        }
        // Recv action on a Recv protocol step.
        (SessionType::Recv { msg_type }, Action::Recv(recv_type)) => {
            if msg_type == recv_type {
                Ok(SessionType::End)
            } else {
                Err(SessionError::UnexpectedMessage {
                    expected: msg_type.clone(),
                    got: recv_type.clone(),
                    span,
                })
            }
        }
        // Then: advance the first step, if it completes move to second.
        (SessionType::Then(fst, snd), _) => {
            let after_fst = advance(fst, action, span)?;
            if is_complete(&after_fst) {
                Ok(*snd.clone())
            } else {
                Ok(SessionType::Then(Box::new(after_fst), snd.clone()))
            }
        }
        // Choose: the local party picks a branch.
        (SessionType::Choose(branches), Action::Choose(label)) => {
            for (branch_label, branch_st) in branches {
                if branch_label == label {
                    return Ok(branch_st.clone());
                }
            }
            Err(SessionError::UnknownBranch {
                label: label.clone(),
                available: branches.iter().map(|(l, _)| l.clone()).collect(),
                span,
            })
        }
        // Offer: the remote party chose, the local party must handle.
        // When we "advance" an Offer, we need a Choose action to select.
        (SessionType::Offer(branches), Action::Choose(label)) => {
            for (branch_label, branch_st) in branches {
                if branch_label == label {
                    return Ok(branch_st.clone());
                }
            }
            Err(SessionError::UnknownBranch {
                label: label.clone(),
                available: branches.iter().map(|(l, _)| l.clone()).collect(),
                span,
            })
        }
        // Rec: unfold the recursion and retry.
        (SessionType::Rec(label, body), _) => {
            let unfolded = unfold_rec(body, label, current);
            advance(&unfolded, action, span)
        }
        // Var: should have been unfolded by Rec handling — protocol error.
        (SessionType::Var(label), _) => Err(SessionError::ProtocolViolation {
            protocol: String::new(),
            detail: format!("unresolved recursion variable '{}'", label),
            span,
        }),
        // End: no more actions allowed.
        (SessionType::End, _) => Err(SessionError::ProtocolViolation {
            protocol: String::new(),
            detail: format!("session is complete, cannot perform {}", action),
            span,
        }),
        // Mismatched action kind.
        (SessionType::Send { msg_type }, Action::Recv(got)) => Err(SessionError::WrongActionKind {
            expected: format!("send({})", msg_type),
            got: format!("recv({})", got),
            span,
        }),
        (SessionType::Recv { msg_type }, Action::Send(got)) => Err(SessionError::WrongActionKind {
            expected: format!("recv({})", msg_type),
            got: format!("send({})", got),
            span,
        }),
        (SessionType::Send { .. }, Action::Choose(label)) => Err(SessionError::WrongActionKind {
            expected: "send".to_string(),
            got: format!("choose({})", label),
            span,
        }),
        (SessionType::Recv { .. }, Action::Choose(label)) => Err(SessionError::WrongActionKind {
            expected: "recv".to_string(),
            got: format!("choose({})", label),
            span,
        }),
        (SessionType::Choose(_), Action::Send(t)) => Err(SessionError::WrongActionKind {
            expected: "choose".to_string(),
            got: format!("send({})", t),
            span,
        }),
        (SessionType::Choose(_), Action::Recv(t)) => Err(SessionError::WrongActionKind {
            expected: "choose".to_string(),
            got: format!("recv({})", t),
            span,
        }),
        (SessionType::Offer(_), Action::Send(t)) => Err(SessionError::WrongActionKind {
            expected: "offer (choose to select)".to_string(),
            got: format!("send({})", t),
            span,
        }),
        (SessionType::Offer(_), Action::Recv(t)) => Err(SessionError::WrongActionKind {
            expected: "offer (choose to select)".to_string(),
            got: format!("recv({})", t),
            span,
        }),
    }
}

/// Substitute all occurrences of `Var(label)` with `replacement` in `body`.
fn unfold_rec(body: &SessionType, label: &str, replacement: &SessionType) -> SessionType {
    match body {
        SessionType::Var(v) if v == label => replacement.clone(),
        SessionType::Var(_) => body.clone(),
        SessionType::Send { .. } | SessionType::Recv { .. } | SessionType::End => body.clone(),
        SessionType::Then(fst, snd) => SessionType::Then(
            Box::new(unfold_rec(fst, label, replacement)),
            Box::new(unfold_rec(snd, label, replacement)),
        ),
        SessionType::Choose(branches) => SessionType::Choose(
            branches
                .iter()
                .map(|(l, st)| (l.clone(), unfold_rec(st, label, replacement)))
                .collect(),
        ),
        SessionType::Offer(branches) => SessionType::Offer(
            branches
                .iter()
                .map(|(l, st)| (l.clone(), unfold_rec(st, label, replacement)))
                .collect(),
        ),
        SessionType::Rec(inner_label, inner_body) => {
            if inner_label == label {
                // Shadow: the inner Rec binds the same label, don't substitute inside.
                body.clone()
            } else {
                SessionType::Rec(
                    inner_label.clone(),
                    Box::new(unfold_rec(inner_body, label, replacement)),
                )
            }
        }
    }
}

/// Produce a short textual summary of a session type (for error messages).
pub fn describe(session: &SessionType) -> String {
    match session {
        SessionType::Send { msg_type } => format!("!{}", msg_type),
        SessionType::Recv { msg_type } => format!("?{}", msg_type),
        SessionType::Then(fst, snd) => format!("{}.{}", describe(fst), describe(snd)),
        SessionType::Choose(branches) => {
            let labels: Vec<_> = branches.iter().map(|(l, _)| l.as_str()).collect();
            format!("+{{{}}}", labels.join(", "))
        }
        SessionType::Offer(branches) => {
            let labels: Vec<_> = branches.iter().map(|(l, _)| l.as_str()).collect();
            format!("&{{{}}}", labels.join(", "))
        }
        SessionType::End => "end".to_string(),
        SessionType::Rec(label, body) => format!("μ{}.{}", label, describe(body)),
        SessionType::Var(label) => label.clone(),
    }
}

// ── Checker ─────────────────────────────────────────────────────────

/// Session type checker — stores protocol declarations and verifies adherence.
pub struct SessionChecker {
    /// Known session declarations, keyed by protocol name.
    protocols: HashMap<String, SessionType>,
    /// Errors accumulated during checking.
    errors: Vec<SessionError>,
}

impl SessionChecker {
    /// Create a new empty checker.
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Declare a named protocol.
    pub fn declare_protocol(&mut self, name: &str, session: SessionType) {
        self.protocols.insert(name.to_string(), session);
    }

    /// Get a protocol by name.
    pub fn get_protocol(&self, name: &str) -> Option<&SessionType> {
        self.protocols.get(name)
    }

    /// Check that two session types are duals of each other.
    ///
    /// Returns `true` if `a` and `b` are complementary (every Send in `a`
    /// corresponds to a Recv in `b`, etc.).
    pub fn check_dual(a: &SessionType, b: &SessionType) -> bool {
        let dual_a = dual(a);
        dual_a == *b
    }

    /// Advance a protocol by one action, returning the remaining protocol.
    pub fn advance_protocol(
        &mut self,
        current: &SessionType,
        action: &Action,
        span: Span,
    ) -> Result<SessionType, SessionError> {
        advance(current, action, span)
    }

    /// Verify a complete sequence of actions against a protocol.
    ///
    /// Returns the remaining protocol state after all actions. If the
    /// protocol should be fully consumed, check with `is_complete`.
    pub fn verify_sequence(
        &mut self,
        protocol: &SessionType,
        actions: &[(Action, Span)],
    ) -> Result<SessionType, SessionError> {
        let mut current = protocol.clone();
        for (action, span) in actions {
            current = advance(&current, action, *span)?;
        }
        Ok(current)
    }

    /// Check that a sequence of actions fully completes a named protocol.
    pub fn check_complete_session(
        &mut self,
        protocol_name: &str,
        actions: &[(Action, Span)],
        end_span: Span,
    ) -> Vec<SessionError> {
        let protocol = match self.protocols.get(protocol_name) {
            Some(p) => p.clone(),
            None => {
                return vec![SessionError::ProtocolViolation {
                    protocol: protocol_name.to_string(),
                    detail: "undeclared protocol".to_string(),
                    span: end_span,
                }];
            }
        };

        match self.verify_sequence(&protocol, actions) {
            Ok(remaining) => {
                if !is_complete(&remaining) {
                    vec![SessionError::SessionNotComplete {
                        remaining: describe(&remaining),
                        span: end_span,
                    }]
                } else {
                    vec![]
                }
            }
            Err(e) => vec![e],
        }
    }

    /// Consume accumulated errors.
    pub fn take_errors(&mut self) -> Vec<SessionError> {
        std::mem::take(&mut self.errors)
    }

    /// Return a reference to accumulated errors.
    pub fn errors(&self) -> &[SessionError] {
        &self.errors
    }
}

impl Default for SessionChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Convenience constructors ────────────────────────────────────────

/// Build a `Send` step.
pub fn send(msg_type: &str) -> SessionType {
    SessionType::Send {
        msg_type: msg_type.to_string(),
    }
}

/// Build a `Recv` step.
pub fn recv(msg_type: &str) -> SessionType {
    SessionType::Recv {
        msg_type: msg_type.to_string(),
    }
}

/// Build a `Then` (sequential composition).
pub fn then(fst: SessionType, snd: SessionType) -> SessionType {
    SessionType::Then(Box::new(fst), Box::new(snd))
}

/// Build a chain of sequential steps.
pub fn seq(steps: Vec<SessionType>) -> SessionType {
    match steps.len() {
        0 => SessionType::End,
        1 => steps.into_iter().next().unwrap(),
        _ => {
            let mut iter = steps.into_iter().rev();
            let last = iter.next().unwrap();
            iter.fold(last, |acc, step| then(step, acc))
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn span(line: usize) -> Span {
        Span::new(0, 0, line, 1)
    }

    // ── Duality tests ───────────────────────────────────────────

    #[test]
    fn test_dual_of_send_is_recv() {
        let s = send("Credentials");
        let d = dual(&s);
        assert_eq!(d, recv("Credentials"));
    }

    #[test]
    fn test_dual_of_recv_is_send() {
        let r = recv("Response");
        let d = dual(&r);
        assert_eq!(d, send("Response"));
    }

    #[test]
    fn test_dual_of_end_is_end() {
        assert_eq!(dual(&SessionType::End), SessionType::End);
    }

    #[test]
    fn test_dual_of_then() {
        let protocol = then(send("A"), recv("B"));
        let expected = then(recv("A"), send("B"));
        assert_eq!(dual(&protocol), expected);
    }

    #[test]
    fn test_dual_of_choose_is_offer() {
        let choose = SessionType::Choose(vec![
            ("ok".to_string(), send("Data")),
            ("err".to_string(), send("Error")),
        ]);
        let expected = SessionType::Offer(vec![
            ("ok".to_string(), recv("Data")),
            ("err".to_string(), recv("Error")),
        ]);
        assert_eq!(dual(&choose), expected);
    }

    #[test]
    fn test_dual_of_offer_is_choose() {
        let offer = SessionType::Offer(vec![
            ("a".to_string(), recv("X")),
            ("b".to_string(), recv("Y")),
        ]);
        let expected = SessionType::Choose(vec![
            ("a".to_string(), send("X")),
            ("b".to_string(), send("Y")),
        ]);
        assert_eq!(dual(&offer), expected);
    }

    #[test]
    fn test_dual_involution() {
        // dual(dual(S)) == S
        let protocol = seq(vec![send("A"), recv("B"), send("C")]);
        assert_eq!(dual(&dual(&protocol)), protocol);
    }

    #[test]
    fn test_check_dual_symmetric() {
        let client = seq(vec![send("Credentials"), recv("AuthResult")]);
        let server = dual(&client);
        assert!(SessionChecker::check_dual(&client, &server));
        assert!(SessionChecker::check_dual(&server, &client));
    }

    #[test]
    fn test_check_dual_not_dual() {
        let a = send("Foo");
        let b = send("Foo"); // same, not dual
        assert!(!SessionChecker::check_dual(&a, &b));
    }

    // ── Advance / protocol traversal tests ──────────────────────

    #[test]
    fn test_advance_send_correct_type() {
        let protocol = send("Credentials");
        let result = advance(&protocol, &Action::Send("Credentials".to_string()), span(1));
        assert_eq!(result, Ok(SessionType::End));
    }

    #[test]
    fn test_advance_send_wrong_type() {
        let protocol = send("Credentials");
        let result = advance(&protocol, &Action::Send("Request".to_string()), span(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::UnexpectedMessage { expected, got, .. } => {
                assert_eq!(expected, "Credentials");
                assert_eq!(got, "Request");
            }
            other => panic!("expected UnexpectedMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_advance_recv_correct_type() {
        let protocol = recv("Response");
        let result = advance(&protocol, &Action::Recv("Response".to_string()), span(1));
        assert_eq!(result, Ok(SessionType::End));
    }

    #[test]
    fn test_advance_recv_wrong_type() {
        let protocol = recv("Response");
        let result = advance(&protocol, &Action::Recv("Error".to_string()), span(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_advance_wrong_action_kind_send_vs_recv() {
        let protocol = send("Foo");
        let result = advance(&protocol, &Action::Recv("Foo".to_string()), span(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::WrongActionKind { .. } => {}
            other => panic!("expected WrongActionKind, got {:?}", other),
        }
    }

    #[test]
    fn test_advance_complete_protocol() {
        // Send Credentials → Recv AuthResult → End
        let protocol = seq(vec![send("Credentials"), recv("AuthResult")]);

        let remaining =
            advance(&protocol, &Action::Send("Credentials".to_string()), span(1)).unwrap();
        assert_eq!(remaining, recv("AuthResult"));

        let remaining =
            advance(&remaining, &Action::Recv("AuthResult".to_string()), span(2)).unwrap();
        assert!(is_complete(&remaining));
    }

    #[test]
    fn test_advance_then_chain() {
        let protocol = seq(vec![send("A"), recv("B"), send("C"), recv("D")]);

        let r1 = advance(&protocol, &Action::Send("A".to_string()), span(1)).unwrap();
        let r2 = advance(&r1, &Action::Recv("B".to_string()), span(2)).unwrap();
        let r3 = advance(&r2, &Action::Send("C".to_string()), span(3)).unwrap();
        let r4 = advance(&r3, &Action::Recv("D".to_string()), span(4)).unwrap();
        assert!(is_complete(&r4));
    }

    #[test]
    fn test_advance_on_end_fails() {
        let result = advance(&SessionType::End, &Action::Send("X".to_string()), span(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::ProtocolViolation { detail, .. } => {
                assert!(detail.contains("complete"));
            }
            other => panic!("expected ProtocolViolation, got {:?}", other),
        }
    }

    #[test]
    fn test_incomplete_session_detection() {
        let protocol = seq(vec![send("A"), recv("B")]);
        // Only perform the first action.
        let remaining = advance(&protocol, &Action::Send("A".to_string()), span(1)).unwrap();
        assert!(!is_complete(&remaining));
    }

    // ── Choose / Offer tests ────────────────────────────────────

    #[test]
    fn test_advance_choose_valid_branch() {
        let protocol = SessionType::Choose(vec![
            ("success".to_string(), send("Data")),
            ("failure".to_string(), send("Error")),
        ]);

        let result = advance(&protocol, &Action::Choose("success".to_string()), span(1));
        assert_eq!(result, Ok(send("Data")));
    }

    #[test]
    fn test_advance_choose_unknown_branch() {
        let protocol = SessionType::Choose(vec![("a".to_string(), send("X"))]);

        let result = advance(&protocol, &Action::Choose("z".to_string()), span(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::UnknownBranch {
                label, available, ..
            } => {
                assert_eq!(label, "z");
                assert_eq!(available, vec!["a".to_string()]);
            }
            other => panic!("expected UnknownBranch, got {:?}", other),
        }
    }

    #[test]
    fn test_advance_offer_with_choose() {
        let protocol = SessionType::Offer(vec![
            ("ok".to_string(), recv("Result")),
            ("err".to_string(), recv("Error")),
        ]);

        let result = advance(&protocol, &Action::Choose("ok".to_string()), span(1));
        assert_eq!(result, Ok(recv("Result")));
    }

    // ── Recursion tests ─────────────────────────────────────────

    #[test]
    fn test_recursive_ping_pong() {
        // μX. Send("Ping") . Recv("Pong") . X
        let protocol = SessionType::Rec(
            "X".to_string(),
            Box::new(seq(vec![
                send("Ping"),
                recv("Pong"),
                SessionType::Var("X".to_string()),
            ])),
        );

        // First iteration.
        let r1 = advance(&protocol, &Action::Send("Ping".to_string()), span(1)).unwrap();
        let r2 = advance(&r1, &Action::Recv("Pong".to_string()), span(2)).unwrap();

        // After Pong, we should be back at the beginning (Rec unfolded).
        // The next action should be Send Ping again.
        let r3 = advance(&r2, &Action::Send("Ping".to_string()), span(3)).unwrap();
        let r4 = advance(&r3, &Action::Recv("Pong".to_string()), span(4)).unwrap();

        // Still not complete — recursive protocols never end unless explicitly broken.
        // We can keep going.
        assert!(!is_complete(&r4));
    }

    #[test]
    fn test_dual_of_rec() {
        let protocol = SessionType::Rec(
            "X".to_string(),
            Box::new(then(
                send("Ping"),
                then(recv("Pong"), SessionType::Var("X".to_string())),
            )),
        );

        let d = dual(&protocol);
        // The dual should be Rec(X, Recv(Ping) . Send(Pong) . X)
        let expected = SessionType::Rec(
            "X".to_string(),
            Box::new(then(
                recv("Ping"),
                then(send("Pong"), SessionType::Var("X".to_string())),
            )),
        );
        assert_eq!(d, expected);
    }

    // ── SessionChecker integration tests ────────────────────────

    #[test]
    fn test_checker_verify_complete_session() {
        let mut checker = SessionChecker::new();

        let protocol = seq(vec![send("Request"), recv("Response")]);
        checker.declare_protocol("SimpleRPC", protocol);

        let actions = vec![
            (Action::Send("Request".to_string()), span(1)),
            (Action::Recv("Response".to_string()), span(2)),
        ];

        let errors = checker.check_complete_session("SimpleRPC", &actions, span(3));
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_checker_incomplete_session() {
        let mut checker = SessionChecker::new();

        let protocol = seq(vec![send("Request"), recv("Response")]);
        checker.declare_protocol("RPC", protocol);

        // Only send, don't receive.
        let actions = vec![(Action::Send("Request".to_string()), span(1))];

        let errors = checker.check_complete_session("RPC", &actions, span(2));
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            SessionError::SessionNotComplete { .. } => {}
            other => panic!("expected SessionNotComplete, got {:?}", other),
        }
    }

    #[test]
    fn test_checker_wrong_message() {
        let mut checker = SessionChecker::new();

        let protocol = seq(vec![send("A"), recv("B")]);
        checker.declare_protocol("Proto", protocol);

        let actions = vec![
            (Action::Send("A".to_string()), span(1)),
            (Action::Recv("C".to_string()), span(2)), // wrong!
        ];

        let errors = checker.check_complete_session("Proto", &actions, span(3));
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            SessionError::UnexpectedMessage { expected, got, .. } => {
                assert_eq!(expected, "B");
                assert_eq!(got, "C");
            }
            other => panic!("expected UnexpectedMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_checker_undeclared_protocol() {
        let mut checker = SessionChecker::new();

        let errors = checker.check_complete_session("NonExistent", &[], span(1));
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            SessionError::ProtocolViolation { detail, .. } => {
                assert!(detail.contains("undeclared"));
            }
            other => panic!("expected ProtocolViolation, got {:?}", other),
        }
    }

    #[test]
    fn test_checker_with_choice() {
        let mut checker = SessionChecker::new();

        // Protocol: Send credentials, then branch on success/failure.
        let protocol = seq(vec![
            send("Credentials"),
            recv("AuthResult"),
            SessionType::Choose(vec![
                (
                    "success".to_string(),
                    seq(vec![send("Request"), recv("Response")]),
                ),
                ("failure".to_string(), send("Goodbye")),
            ]),
        ]);
        checker.declare_protocol("Login", protocol);

        // Success path.
        let actions = vec![
            (Action::Send("Credentials".to_string()), span(1)),
            (Action::Recv("AuthResult".to_string()), span(2)),
            (Action::Choose("success".to_string()), span(3)),
            (Action::Send("Request".to_string()), span(4)),
            (Action::Recv("Response".to_string()), span(5)),
        ];

        let errors = checker.check_complete_session("Login", &actions, span(6));
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_checker_failure_path() {
        let mut checker = SessionChecker::new();

        let protocol = seq(vec![
            send("Credentials"),
            recv("AuthResult"),
            SessionType::Choose(vec![
                (
                    "success".to_string(),
                    seq(vec![send("Request"), recv("Response")]),
                ),
                ("failure".to_string(), send("Goodbye")),
            ]),
        ]);
        checker.declare_protocol("Login", protocol);

        // Failure path.
        let actions = vec![
            (Action::Send("Credentials".to_string()), span(1)),
            (Action::Recv("AuthResult".to_string()), span(2)),
            (Action::Choose("failure".to_string()), span(3)),
            (Action::Send("Goodbye".to_string()), span(4)),
        ];

        let errors = checker.check_complete_session("Login", &actions, span(5));
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // ── Describe / display tests ────────────────────────────────

    #[test]
    fn test_describe_send() {
        assert_eq!(describe(&send("Foo")), "!Foo");
    }

    #[test]
    fn test_describe_recv() {
        assert_eq!(describe(&recv("Bar")), "?Bar");
    }

    #[test]
    fn test_describe_end() {
        assert_eq!(describe(&SessionType::End), "end");
    }

    #[test]
    fn test_describe_then() {
        let p = then(send("A"), recv("B"));
        assert_eq!(describe(&p), "!A.?B");
    }

    #[test]
    fn test_describe_rec() {
        let p = SessionType::Rec("X".to_string(), Box::new(send("Ping")));
        assert_eq!(describe(&p), "μX.!Ping");
    }

    #[test]
    fn test_seq_empty() {
        assert_eq!(seq(vec![]), SessionType::End);
    }

    #[test]
    fn test_seq_single() {
        assert_eq!(seq(vec![send("A")]), send("A"));
    }

    #[test]
    fn test_seq_multiple() {
        let p = seq(vec![send("A"), recv("B"), send("C")]);
        // Should be Then(Send(A), Then(Recv(B), Send(C)))
        let expected = then(send("A"), then(recv("B"), send("C")));
        assert_eq!(p, expected);
    }

    // ── Error display tests ─────────────────────────────────────

    #[test]
    fn test_error_display_unexpected_message() {
        let err = SessionError::UnexpectedMessage {
            expected: "A".to_string(),
            got: "B".to_string(),
            span: span(5),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("A"));
        assert!(msg.contains("B"));
        assert!(msg.contains("line 5"));
    }

    #[test]
    fn test_error_display_session_not_complete() {
        let err = SessionError::SessionNotComplete {
            remaining: "!Foo".to_string(),
            span: span(3),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("not complete"));
    }

    #[test]
    fn test_error_display_unknown_branch() {
        let err = SessionError::UnknownBranch {
            label: "z".to_string(),
            available: vec!["a".to_string(), "b".to_string()],
            span: span(7),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("z"));
        assert!(msg.contains("a, b"));
    }
}
