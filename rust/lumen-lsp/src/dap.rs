//! Debug Adapter Protocol (DAP) server for Lumen.
//!
//! Implements a subset of the [DAP specification](https://microsoft.github.io/debug-adapter-protocol/)
//! sufficient for breakpoint-based debugging with value inspection.  The
//! protocol layer translates between DAP JSON messages and the runtime's
//! [`DebugSession`] / [`DebugState`] types defined in `lumen-runtime`.
//!
//! # Architecture
//!
//! ```text
//!  Editor (VS Code, etc.)
//!       │ DAP JSON over stdio/socket
//!       ▼
//!  ┌─────────────┐
//!  │  DapServer   │  ← this module
//!  │  (protocol)  │
//!  └──────┬───────┘
//!         │ DapRequest / DapResponse / DapEvent
//!         ▼
//!  ┌─────────────────┐
//!  │  DebugSession    │  ← lumen-runtime::debugger
//!  │  (state mgmt)    │
//!  └─────────────────┘
//! ```
//!
//! # Capabilities
//!
//! The server advertises:
//! - `supportsConfigurationDoneRequest` — editor can signal "done configuring"
//! - `supportsFunctionBreakpoints` — break on cell entry
//! - `supportsEvaluateForHovers` — evaluate expressions for hover tooltips
//! - `supportsStepInTargetsRequest: false` — kept simple for now
//!
//! # Value Inspection
//!
//! The `Variables` request returns structured children for complex values:
//! - **Records** → fields as named children
//! - **Lists / Tuples** → indexed children (`[0]`, `[1]`, ...)
//! - **Maps** → key-value pairs as named children
//! - **Sets** → indexed elements
//! - **Unions** → tag + payload
//! - **Primitives** → direct string representation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// DAP protocol types
// ---------------------------------------------------------------------------

/// Source location descriptor sent by the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSource {
    /// Optional human-readable name.
    pub name: Option<String>,
    /// Absolute file path.
    pub path: Option<String>,
}

/// A breakpoint location in source code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSourceBreakpoint {
    /// 1-indexed line number.
    pub line: i64,
    /// Optional column.
    pub column: Option<i64>,
    /// Optional condition expression.
    pub condition: Option<String>,
}

/// A resolved breakpoint returned to the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpoint {
    /// Unique breakpoint identifier.
    pub id: Option<i64>,
    /// Whether the breakpoint could be set at the requested location.
    pub verified: bool,
    /// The actual line where the breakpoint was placed.
    pub line: Option<i64>,
    /// Human-readable message (e.g. why it was not verified).
    pub message: Option<String>,
}

/// A variable visible in a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariable {
    /// Variable name.
    pub name: String,
    /// String representation of the value.
    pub value: String,
    /// Type name for display.
    #[serde(rename = "type")]
    pub ty: String,
    /// Non-zero reference for structured (expandable) values.
    pub variables_reference: i64,
    /// Number of named children (for records/maps).
    pub named_variables: Option<i64>,
    /// Number of indexed children (for lists/tuples/sets).
    pub indexed_variables: Option<i64>,
}

/// A single stack frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapStackFrame {
    /// Frame ID (unique within a stopped event).
    pub id: i64,
    /// Display name (cell name or "<unknown>").
    pub name: String,
    /// Source location.
    pub source: Option<DapSource>,
    /// 1-indexed line number.
    pub line: i64,
    /// 1-indexed column (0 = unknown).
    pub column: i64,
}

/// A scope within a stack frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapScope {
    /// Scope name (e.g. "Locals", "Registers").
    pub name: String,
    /// Reference to the variables in this scope.
    pub variables_reference: i64,
    /// Whether this scope is expensive to fetch.
    pub expensive: bool,
}

/// A thread in the debuggee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapThread {
    /// Thread identifier.
    pub id: i64,
    /// Display name.
    pub name: String,
}

/// Server capabilities advertised during initialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapCapabilities {
    pub supports_configuration_done_request: bool,
    pub supports_function_breakpoints: bool,
    pub supports_evaluate_for_hovers: bool,
    pub supports_step_in_targets_request: bool,
    pub supports_set_variable: bool,
    pub supports_restart_request: bool,
}

// ---------------------------------------------------------------------------
// Request / Response / Event envelopes
// ---------------------------------------------------------------------------

/// DAP requests the editor can send.
#[derive(Debug, Clone, PartialEq)]
pub enum DapRequest {
    /// Handshake — client sends its ID, server returns capabilities.
    Initialize { client_id: Option<String> },
    /// Set breakpoints for a source file (replaces previous set).
    SetBreakpoints {
        source: DapSource,
        breakpoints: Vec<DapSourceBreakpoint>,
    },
    /// Editor is done sending initial configuration.
    ConfigurationDone,
    /// List debuggee threads.
    Threads,
    /// Get the call stack for a thread.
    StackTrace { thread_id: i64 },
    /// Get scopes for a stack frame.
    Scopes { frame_id: i64 },
    /// Get variables for a scope/structured reference.
    Variables { variables_reference: i64 },
    /// Resume execution.
    Continue { thread_id: i64 },
    /// Step over (next line).
    Next { thread_id: i64 },
    /// Step into a function call.
    StepIn { thread_id: i64 },
    /// Step out of the current function.
    StepOut { thread_id: i64 },
    /// Evaluate an expression (for hovers or watch).
    Evaluate {
        expression: String,
        frame_id: Option<i64>,
    },
    /// End the debug session.
    Disconnect,
}

/// Body variants for DAP responses.
#[derive(Debug, Clone, PartialEq)]
pub enum DapResponseBody {
    /// `initialize` response — server capabilities.
    Initialize(DapCapabilities),
    /// `setBreakpoints` response — resolved breakpoints.
    SetBreakpoints(Vec<DapBreakpoint>),
    /// Acknowledgement with no body.
    Empty,
    /// `threads` response.
    Threads(Vec<DapThread>),
    /// `stackTrace` response.
    StackTrace(Vec<DapStackFrame>),
    /// `scopes` response.
    Scopes(Vec<DapScope>),
    /// `variables` response.
    Variables(Vec<DapVariable>),
    /// `continue` response.
    Continue { all_threads_continued: bool },
    /// `evaluate` response.
    Evaluate {
        result: String,
        ty: String,
        variables_reference: i64,
    },
    /// Error body.
    Error(String),
}

/// A DAP response sent back to the editor.
#[derive(Debug, Clone, PartialEq)]
pub struct DapResponse {
    /// Whether the request succeeded.
    pub success: bool,
    /// The command this responds to.
    pub command: String,
    /// Response body.
    pub body: DapResponseBody,
}

/// DAP events pushed to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DapEvent {
    /// The debuggee has been initialized.
    Initialized,
    /// Execution has stopped (breakpoint, step, etc.).
    Stopped {
        reason: StopReason,
        thread_id: i64,
        description: Option<String>,
    },
    /// Debuggee produced output text.
    Output {
        category: OutputCategory,
        output: String,
    },
    /// The debug session has ended.
    Terminated,
    /// A thread was created or exited.
    Thread {
        reason: ThreadReason,
        thread_id: i64,
    },
}

/// Why did execution stop?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    Breakpoint,
    Step,
    Pause,
    Entry,
    Exception,
}

impl StopReason {
    /// DAP protocol string.
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::Breakpoint => "breakpoint",
            StopReason::Step => "step",
            StopReason::Pause => "pause",
            StopReason::Entry => "entry",
            StopReason::Exception => "exception",
        }
    }
}

/// Output category for `output` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputCategory {
    Console,
    Stdout,
    Stderr,
}

impl OutputCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputCategory::Console => "console",
            OutputCategory::Stdout => "stdout",
            OutputCategory::Stderr => "stderr",
        }
    }
}

/// Thread lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadReason {
    Started,
    Exited,
}

impl ThreadReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThreadReason::Started => "started",
            ThreadReason::Exited => "exited",
        }
    }
}

// ---------------------------------------------------------------------------
// Value representation for inspection
// ---------------------------------------------------------------------------

/// Mirrors the runtime's `SerializedValue` for DAP variable expansion.
///
/// This is intentionally a separate type from `lumen_runtime::snapshot::SerializedValue`
/// so that the LSP crate does not depend on the runtime crate.  Conversion
/// helpers can be added later when the two are wired together.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<InspectValue>),
    Tuple(Vec<InspectValue>),
    Set(Vec<InspectValue>),
    Map(Vec<(String, InspectValue)>),
    Record {
        type_name: String,
        fields: Vec<(String, InspectValue)>,
    },
    Union {
        tag: String,
        payload: Box<InspectValue>,
    },
}

impl InspectValue {
    /// Short type label for the DAP `type` field.
    pub fn type_name(&self) -> &str {
        match self {
            InspectValue::Null => "null",
            InspectValue::Bool(_) => "Bool",
            InspectValue::Int(_) => "Int",
            InspectValue::Float(_) => "Float",
            InspectValue::String(_) => "String",
            InspectValue::Bytes(_) => "Bytes",
            InspectValue::List(_) => "List",
            InspectValue::Tuple(_) => "Tuple",
            InspectValue::Set(_) => "Set",
            InspectValue::Map(_) => "Map",
            InspectValue::Record { type_name, .. } => type_name.as_str(),
            InspectValue::Union { tag, .. } => tag.as_str(),
        }
    }

    /// Display string for the value (summary for complex types).
    pub fn display_value(&self) -> String {
        match self {
            InspectValue::Null => "null".into(),
            InspectValue::Bool(b) => b.to_string(),
            InspectValue::Int(n) => n.to_string(),
            InspectValue::Float(f) => format!("{f}"),
            InspectValue::String(s) => format!("\"{s}\""),
            InspectValue::Bytes(b) => format!("<{} bytes>", b.len()),
            InspectValue::List(items) => format!("[{} items]", items.len()),
            InspectValue::Tuple(items) => format!("({} items)", items.len()),
            InspectValue::Set(items) => format!("{{{} items}}", items.len()),
            InspectValue::Map(entries) => format!("{{{} entries}}", entries.len()),
            InspectValue::Record { type_name, fields } => {
                format!("{type_name}({} fields)", fields.len())
            }
            InspectValue::Union { tag, .. } => format!("{tag}(...)"),
        }
    }

    /// Whether this value has children that can be expanded.
    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            InspectValue::List(_)
                | InspectValue::Tuple(_)
                | InspectValue::Set(_)
                | InspectValue::Map(_)
                | InspectValue::Record { .. }
                | InspectValue::Union { .. }
        )
    }

    /// Extract children as `(name, value)` pairs for the Variables request.
    pub fn children(&self) -> Vec<(String, InspectValue)> {
        match self {
            InspectValue::List(items) | InspectValue::Tuple(items) | InspectValue::Set(items) => {
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (format!("[{i}]"), v.clone()))
                    .collect()
            }
            InspectValue::Map(entries) => entries.clone(),
            InspectValue::Record { fields, .. } => fields.clone(),
            InspectValue::Union { tag, payload } => {
                vec![(tag.clone(), *payload.clone())]
            }
            _ => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// DapServer
// ---------------------------------------------------------------------------

/// DAP protocol handler.
///
/// Manages breakpoints, variable reference expansion, and translates DAP
/// requests into responses.  The server is designed to be driven by an
/// external message loop (stdio or socket).  It does not own a VM instance;
/// execution control requests (`Continue`, `Next`, etc.) return acknowledgements
/// and the host is responsible for driving the actual VM.
pub struct DapServer {
    /// Source path → breakpoints (the editor sends the full set per file).
    breakpoints: HashMap<String, Vec<DapSourceBreakpoint>>,
    /// Variable references for structured value expansion.
    /// Key: reference ID → children variables.
    variable_refs: HashMap<i64, Vec<DapVariable>>,
    /// Monotonic counter for variable reference IDs.
    next_var_ref: i64,
    /// Whether the server has been initialized.
    initialized: bool,
    /// Whether configuration is done.
    configuration_done: bool,
    /// Stack frames for the current stopped state.
    stack_frames: Vec<DapStackFrame>,
    /// Scopes indexed by frame ID → list of scopes.
    frame_scopes: HashMap<i64, Vec<DapScope>>,
    /// Next breakpoint ID for assignment.
    next_bp_id: i64,
}

impl DapServer {
    /// Create a new DAP server.
    pub fn new() -> Self {
        DapServer {
            breakpoints: HashMap::new(),
            variable_refs: HashMap::new(),
            next_var_ref: 1,
            initialized: false,
            configuration_done: false,
            stack_frames: Vec::new(),
            frame_scopes: HashMap::new(),
            next_bp_id: 1,
        }
    }

    /// Return the capabilities this server advertises.
    pub fn capabilities() -> DapCapabilities {
        DapCapabilities {
            supports_configuration_done_request: true,
            supports_function_breakpoints: true,
            supports_evaluate_for_hovers: true,
            supports_step_in_targets_request: false,
            supports_set_variable: false,
            supports_restart_request: false,
        }
    }

    /// Process a DAP request and return a response.
    pub fn handle_request(&mut self, req: DapRequest) -> DapResponse {
        match req {
            DapRequest::Initialize { .. } => {
                self.initialized = true;
                DapResponse {
                    success: true,
                    command: "initialize".into(),
                    body: DapResponseBody::Initialize(Self::capabilities()),
                }
            }

            DapRequest::SetBreakpoints {
                source,
                breakpoints,
            } => {
                let path = source.path.clone().unwrap_or_default();
                let resolved: Vec<DapBreakpoint> = breakpoints
                    .iter()
                    .map(|sb| {
                        let id = self.next_bp_id;
                        self.next_bp_id += 1;
                        DapBreakpoint {
                            id: Some(id),
                            verified: true,
                            line: Some(sb.line),
                            message: None,
                        }
                    })
                    .collect();
                self.breakpoints.insert(path, breakpoints);
                DapResponse {
                    success: true,
                    command: "setBreakpoints".into(),
                    body: DapResponseBody::SetBreakpoints(resolved),
                }
            }

            DapRequest::ConfigurationDone => {
                self.configuration_done = true;
                DapResponse {
                    success: true,
                    command: "configurationDone".into(),
                    body: DapResponseBody::Empty,
                }
            }

            DapRequest::Threads => DapResponse {
                success: true,
                command: "threads".into(),
                body: DapResponseBody::Threads(vec![DapThread {
                    id: 1,
                    name: "main".into(),
                }]),
            },

            DapRequest::StackTrace { .. } => DapResponse {
                success: true,
                command: "stackTrace".into(),
                body: DapResponseBody::StackTrace(self.stack_frames.clone()),
            },

            DapRequest::Scopes { frame_id } => {
                let scopes = self
                    .frame_scopes
                    .get(&frame_id)
                    .cloned()
                    .unwrap_or_default();
                DapResponse {
                    success: true,
                    command: "scopes".into(),
                    body: DapResponseBody::Scopes(scopes),
                }
            }

            DapRequest::Variables {
                variables_reference,
            } => {
                let vars = self
                    .variable_refs
                    .get(&variables_reference)
                    .cloned()
                    .unwrap_or_default();
                DapResponse {
                    success: true,
                    command: "variables".into(),
                    body: DapResponseBody::Variables(vars),
                }
            }

            DapRequest::Continue { .. } => DapResponse {
                success: true,
                command: "continue".into(),
                body: DapResponseBody::Continue {
                    all_threads_continued: true,
                },
            },

            DapRequest::Next { .. } => DapResponse {
                success: true,
                command: "next".into(),
                body: DapResponseBody::Empty,
            },

            DapRequest::StepIn { .. } => DapResponse {
                success: true,
                command: "stepIn".into(),
                body: DapResponseBody::Empty,
            },

            DapRequest::StepOut { .. } => DapResponse {
                success: true,
                command: "stepOut".into(),
                body: DapResponseBody::Empty,
            },

            DapRequest::Evaluate {
                expression,
                frame_id: _,
            } => {
                // In the absence of a live VM connection, return the expression
                // as an unevaluated string.  When wired to the runtime, this
                // will delegate to DebugSession::execute(Inspect(..)).
                DapResponse {
                    success: true,
                    command: "evaluate".into(),
                    body: DapResponseBody::Evaluate {
                        result: expression,
                        ty: "String".into(),
                        variables_reference: 0,
                    },
                }
            }

            DapRequest::Disconnect => {
                self.initialized = false;
                self.configuration_done = false;
                DapResponse {
                    success: true,
                    command: "disconnect".into(),
                    body: DapResponseBody::Empty,
                }
            }
        }
    }

    /// Whether the server has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Whether configuration is done.
    pub fn is_configuration_done(&self) -> bool {
        self.configuration_done
    }

    /// Get the current set of breakpoints for a source path.
    pub fn get_breakpoints(&self, path: &str) -> Option<&Vec<DapSourceBreakpoint>> {
        self.breakpoints.get(path)
    }

    // -- State injection (called by the host driving the VM) ----------------

    /// Set the current stack frames (called when execution stops).
    pub fn set_stack_frames(&mut self, frames: Vec<DapStackFrame>) {
        self.stack_frames = frames;
    }

    /// Set scopes for a given frame ID.
    pub fn set_frame_scopes(&mut self, frame_id: i64, scopes: Vec<DapScope>) {
        self.frame_scopes.insert(frame_id, scopes);
    }

    /// Register a set of variables under a reference ID.
    /// Returns the reference ID assigned.
    pub fn register_variables(&mut self, vars: Vec<DapVariable>) -> i64 {
        let ref_id = self.next_var_ref;
        self.next_var_ref += 1;
        self.variable_refs.insert(ref_id, vars);
        ref_id
    }

    /// Expand an [`InspectValue`] into the variable reference table.
    ///
    /// Returns a [`DapVariable`] with a `variables_reference` that can be
    /// used in subsequent `Variables` requests to drill into children.
    pub fn expand_value(&mut self, name: &str, value: &InspectValue) -> DapVariable {
        let var_ref = if value.is_structured() {
            let children: Vec<DapVariable> = value
                .children()
                .into_iter()
                .map(|(child_name, child_val)| self.expand_value(&child_name, &child_val))
                .collect();
            self.register_variables(children)
        } else {
            0
        };

        let (named, indexed) = match value {
            InspectValue::Record { fields, .. } => (Some(fields.len() as i64), None),
            InspectValue::Map(entries) => (Some(entries.len() as i64), None),
            InspectValue::List(items) => (None, Some(items.len() as i64)),
            InspectValue::Tuple(items) => (None, Some(items.len() as i64)),
            InspectValue::Set(items) => (None, Some(items.len() as i64)),
            InspectValue::Union { .. } => (Some(1), None),
            _ => (None, None),
        };

        DapVariable {
            name: name.to_string(),
            value: value.display_value(),
            ty: value.type_name().to_string(),
            variables_reference: var_ref,
            named_variables: named,
            indexed_variables: indexed,
        }
    }

    /// Clear all variable references (call when a new stop occurs).
    pub fn clear_variable_refs(&mut self) {
        self.variable_refs.clear();
        self.next_var_ref = 1;
    }

    /// Build a stopped event for a breakpoint hit.
    pub fn stopped_event(reason: StopReason, thread_id: i64) -> DapEvent {
        DapEvent::Stopped {
            reason,
            thread_id,
            description: None,
        }
    }

    /// Build a stopped event with a description.
    pub fn stopped_event_with_description(
        reason: StopReason,
        thread_id: i64,
        description: String,
    ) -> DapEvent {
        DapEvent::Stopped {
            reason,
            thread_id,
            description: Some(description),
        }
    }

    /// Build an output event.
    pub fn output_event(category: OutputCategory, output: String) -> DapEvent {
        DapEvent::Output { category, output }
    }
}

impl Default for DapServer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Capabilities -------------------------------------------------------

    #[test]
    fn capabilities_advertise_expected_features() {
        let caps = DapServer::capabilities();
        assert!(caps.supports_configuration_done_request);
        assert!(caps.supports_function_breakpoints);
        assert!(caps.supports_evaluate_for_hovers);
        assert!(!caps.supports_step_in_targets_request);
        assert!(!caps.supports_set_variable);
        assert!(!caps.supports_restart_request);
    }

    // -- Lifecycle ----------------------------------------------------------

    #[test]
    fn initialize_sets_state_and_returns_capabilities() {
        let mut server = DapServer::new();
        assert!(!server.is_initialized());

        let resp = server.handle_request(DapRequest::Initialize {
            client_id: Some("vscode".into()),
        });
        assert!(resp.success);
        assert_eq!(resp.command, "initialize");
        assert!(server.is_initialized());

        match resp.body {
            DapResponseBody::Initialize(caps) => {
                assert!(caps.supports_configuration_done_request);
            }
            _ => panic!("expected Initialize body"),
        }
    }

    #[test]
    fn configuration_done_updates_state() {
        let mut server = DapServer::new();
        server.handle_request(DapRequest::Initialize { client_id: None });
        assert!(!server.is_configuration_done());

        let resp = server.handle_request(DapRequest::ConfigurationDone);
        assert!(resp.success);
        assert_eq!(resp.command, "configurationDone");
        assert!(server.is_configuration_done());
    }

    #[test]
    fn disconnect_resets_state() {
        let mut server = DapServer::new();
        server.handle_request(DapRequest::Initialize { client_id: None });
        server.handle_request(DapRequest::ConfigurationDone);
        assert!(server.is_initialized());
        assert!(server.is_configuration_done());

        let resp = server.handle_request(DapRequest::Disconnect);
        assert!(resp.success);
        assert_eq!(resp.command, "disconnect");
        assert!(!server.is_initialized());
        assert!(!server.is_configuration_done());
    }

    // -- Breakpoints --------------------------------------------------------

    #[test]
    fn set_breakpoints_stores_and_resolves() {
        let mut server = DapServer::new();
        server.handle_request(DapRequest::Initialize { client_id: None });

        let resp = server.handle_request(DapRequest::SetBreakpoints {
            source: DapSource {
                name: Some("main.lm".into()),
                path: Some("/project/main.lm".into()),
            },
            breakpoints: vec![
                DapSourceBreakpoint {
                    line: 5,
                    column: None,
                    condition: None,
                },
                DapSourceBreakpoint {
                    line: 12,
                    column: Some(1),
                    condition: Some("x > 0".into()),
                },
            ],
        });

        assert!(resp.success);
        match resp.body {
            DapResponseBody::SetBreakpoints(bps) => {
                assert_eq!(bps.len(), 2);
                assert!(bps[0].verified);
                assert_eq!(bps[0].line, Some(5));
                assert!(bps[1].verified);
                assert_eq!(bps[1].line, Some(12));
                // IDs should be unique
                assert_ne!(bps[0].id, bps[1].id);
            }
            _ => panic!("expected SetBreakpoints body"),
        }

        // Verify stored breakpoints
        let stored = server.get_breakpoints("/project/main.lm").unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].line, 5);
    }

    #[test]
    fn set_breakpoints_replaces_previous() {
        let mut server = DapServer::new();
        server.handle_request(DapRequest::Initialize { client_id: None });

        // Set initial breakpoints
        server.handle_request(DapRequest::SetBreakpoints {
            source: DapSource {
                name: None,
                path: Some("/a.lm".into()),
            },
            breakpoints: vec![
                DapSourceBreakpoint {
                    line: 1,
                    column: None,
                    condition: None,
                },
                DapSourceBreakpoint {
                    line: 2,
                    column: None,
                    condition: None,
                },
            ],
        });
        assert_eq!(server.get_breakpoints("/a.lm").unwrap().len(), 2);

        // Replace with one breakpoint
        server.handle_request(DapRequest::SetBreakpoints {
            source: DapSource {
                name: None,
                path: Some("/a.lm".into()),
            },
            breakpoints: vec![DapSourceBreakpoint {
                line: 10,
                column: None,
                condition: None,
            }],
        });
        let stored = server.get_breakpoints("/a.lm").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].line, 10);
    }

    // -- Threads ------------------------------------------------------------

    #[test]
    fn threads_returns_main_thread() {
        let mut server = DapServer::new();
        let resp = server.handle_request(DapRequest::Threads);
        assert!(resp.success);
        match resp.body {
            DapResponseBody::Threads(threads) => {
                assert_eq!(threads.len(), 1);
                assert_eq!(threads[0].id, 1);
                assert_eq!(threads[0].name, "main");
            }
            _ => panic!("expected Threads body"),
        }
    }

    // -- Stack trace / scopes -----------------------------------------------

    #[test]
    fn stack_trace_returns_injected_frames() {
        let mut server = DapServer::new();
        server.set_stack_frames(vec![
            DapStackFrame {
                id: 0,
                name: "main".into(),
                source: Some(DapSource {
                    name: Some("main.lm".into()),
                    path: Some("/project/main.lm".into()),
                }),
                line: 10,
                column: 1,
            },
            DapStackFrame {
                id: 1,
                name: "helper".into(),
                source: None,
                line: 20,
                column: 1,
            },
        ]);

        let resp = server.handle_request(DapRequest::StackTrace { thread_id: 1 });
        match resp.body {
            DapResponseBody::StackTrace(frames) => {
                assert_eq!(frames.len(), 2);
                assert_eq!(frames[0].name, "main");
                assert_eq!(frames[1].name, "helper");
            }
            _ => panic!("expected StackTrace body"),
        }
    }

    #[test]
    fn scopes_returns_injected_scopes() {
        let mut server = DapServer::new();
        let scope_ref = server.register_variables(vec![DapVariable {
            name: "x".into(),
            value: "42".into(),
            ty: "Int".into(),
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
        }]);

        server.set_frame_scopes(
            0,
            vec![DapScope {
                name: "Locals".into(),
                variables_reference: scope_ref,
                expensive: false,
            }],
        );

        let resp = server.handle_request(DapRequest::Scopes { frame_id: 0 });
        match resp.body {
            DapResponseBody::Scopes(scopes) => {
                assert_eq!(scopes.len(), 1);
                assert_eq!(scopes[0].name, "Locals");
            }
            _ => panic!("expected Scopes body"),
        }
    }

    // -- Variables / Value inspection (T104) --------------------------------

    #[test]
    fn variables_returns_registered_vars() {
        let mut server = DapServer::new();
        let ref_id = server.register_variables(vec![
            DapVariable {
                name: "x".into(),
                value: "42".into(),
                ty: "Int".into(),
                variables_reference: 0,
                named_variables: None,
                indexed_variables: None,
            },
            DapVariable {
                name: "name".into(),
                value: "\"hello\"".into(),
                ty: "String".into(),
                variables_reference: 0,
                named_variables: None,
                indexed_variables: None,
            },
        ]);

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: ref_id,
        });
        match resp.body {
            DapResponseBody::Variables(vars) => {
                assert_eq!(vars.len(), 2);
                assert_eq!(vars[0].name, "x");
                assert_eq!(vars[0].value, "42");
                assert_eq!(vars[1].name, "name");
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn variables_unknown_ref_returns_empty() {
        let mut server = DapServer::new();
        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: 999,
        });
        match resp.body {
            DapResponseBody::Variables(vars) => assert!(vars.is_empty()),
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_record_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Record {
            type_name: "Point".into(),
            fields: vec![
                ("x".into(), InspectValue::Int(10)),
                ("y".into(), InspectValue::Int(20)),
            ],
        };

        let var = server.expand_value("point", &value);
        assert_eq!(var.name, "point");
        assert_eq!(var.ty, "Point");
        assert_eq!(var.value, "Point(2 fields)");
        assert_eq!(var.named_variables, Some(2));
        assert!(var.variables_reference > 0);

        // Drill into children
        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].name, "x");
                assert_eq!(children[0].value, "10");
                assert_eq!(children[0].ty, "Int");
                assert_eq!(children[0].variables_reference, 0);
                assert_eq!(children[1].name, "y");
                assert_eq!(children[1].value, "20");
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_list_value() {
        let mut server = DapServer::new();
        let value = InspectValue::List(vec![
            InspectValue::Int(1),
            InspectValue::Int(2),
            InspectValue::Int(3),
        ]);

        let var = server.expand_value("items", &value);
        assert_eq!(var.ty, "List");
        assert_eq!(var.value, "[3 items]");
        assert_eq!(var.indexed_variables, Some(3));
        assert!(var.variables_reference > 0);

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 3);
                assert_eq!(children[0].name, "[0]");
                assert_eq!(children[0].value, "1");
                assert_eq!(children[1].name, "[1]");
                assert_eq!(children[2].name, "[2]");
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_nested_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Record {
            type_name: "Outer".into(),
            fields: vec![(
                "inner".into(),
                InspectValue::Record {
                    type_name: "Inner".into(),
                    fields: vec![("val".into(), InspectValue::String("hello".into()))],
                },
            )],
        };

        let var = server.expand_value("outer", &value);
        assert!(var.variables_reference > 0);

        // First level: inner field
        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        let inner_ref = match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].name, "inner");
                assert_eq!(children[0].ty, "Inner");
                assert!(children[0].variables_reference > 0);
                children[0].variables_reference
            }
            _ => panic!("expected Variables body"),
        };

        // Second level: val field
        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: inner_ref,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].name, "val");
                assert_eq!(children[0].value, "\"hello\"");
                assert_eq!(children[0].ty, "String");
                assert_eq!(children[0].variables_reference, 0);
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_tuple_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Tuple(vec![InspectValue::Bool(true), InspectValue::Null]);

        let var = server.expand_value("pair", &value);
        assert_eq!(var.ty, "Tuple");
        assert_eq!(var.indexed_variables, Some(2));

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].name, "[0]");
                assert_eq!(children[0].value, "true");
                assert_eq!(children[0].ty, "Bool");
                assert_eq!(children[1].name, "[1]");
                assert_eq!(children[1].value, "null");
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_map_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Map(vec![
            ("key1".into(), InspectValue::Int(100)),
            ("key2".into(), InspectValue::String("val".into())),
        ]);

        let var = server.expand_value("config", &value);
        assert_eq!(var.ty, "Map");
        assert_eq!(var.named_variables, Some(2));
        assert_eq!(var.value, "{2 entries}");

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].name, "key1");
                assert_eq!(children[0].value, "100");
                assert_eq!(children[1].name, "key2");
                assert_eq!(children[1].value, "\"val\"");
            }
            _ => panic!("expected Variables body"),
        }
    }

    #[test]
    fn expand_union_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Union {
            tag: "Some".into(),
            payload: Box::new(InspectValue::Int(42)),
        };

        let var = server.expand_value("opt", &value);
        assert_eq!(var.ty, "Some");
        assert_eq!(var.value, "Some(...)");
        assert_eq!(var.named_variables, Some(1));

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].name, "Some");
                assert_eq!(children[0].value, "42");
            }
            _ => panic!("expected Variables body"),
        }
    }

    // -- InspectValue -------------------------------------------------------

    #[test]
    fn inspect_value_primitives_not_structured() {
        assert!(!InspectValue::Null.is_structured());
        assert!(!InspectValue::Bool(true).is_structured());
        assert!(!InspectValue::Int(0).is_structured());
        assert!(!InspectValue::Float(1.5).is_structured());
        assert!(!InspectValue::String("hi".into()).is_structured());
        assert!(!InspectValue::Bytes(vec![]).is_structured());
    }

    #[test]
    fn inspect_value_complex_is_structured() {
        assert!(InspectValue::List(vec![]).is_structured());
        assert!(InspectValue::Tuple(vec![]).is_structured());
        assert!(InspectValue::Set(vec![]).is_structured());
        assert!(InspectValue::Map(vec![]).is_structured());
        assert!(InspectValue::Record {
            type_name: "R".into(),
            fields: vec![]
        }
        .is_structured());
        assert!(InspectValue::Union {
            tag: "T".into(),
            payload: Box::new(InspectValue::Null)
        }
        .is_structured());
    }

    #[test]
    fn inspect_value_display_values() {
        assert_eq!(InspectValue::Null.display_value(), "null");
        assert_eq!(InspectValue::Bool(false).display_value(), "false");
        assert_eq!(InspectValue::Int(42).display_value(), "42");
        assert_eq!(InspectValue::String("hi".into()).display_value(), "\"hi\"");
        assert_eq!(
            InspectValue::Bytes(vec![1, 2, 3]).display_value(),
            "<3 bytes>"
        );
        assert_eq!(
            InspectValue::List(vec![InspectValue::Int(1)]).display_value(),
            "[1 items]"
        );
        assert_eq!(InspectValue::Set(vec![]).display_value(), "{0 items}");
    }

    #[test]
    fn inspect_value_type_names() {
        assert_eq!(InspectValue::Null.type_name(), "null");
        assert_eq!(InspectValue::Bool(true).type_name(), "Bool");
        assert_eq!(InspectValue::Int(0).type_name(), "Int");
        assert_eq!(InspectValue::Float(0.0).type_name(), "Float");
        assert_eq!(InspectValue::String("".into()).type_name(), "String");
        assert_eq!(InspectValue::Bytes(vec![]).type_name(), "Bytes");
        assert_eq!(InspectValue::List(vec![]).type_name(), "List");
        assert_eq!(InspectValue::Tuple(vec![]).type_name(), "Tuple");
        assert_eq!(InspectValue::Set(vec![]).type_name(), "Set");
        assert_eq!(InspectValue::Map(vec![]).type_name(), "Map");
        assert_eq!(
            InspectValue::Record {
                type_name: "Point".into(),
                fields: vec![]
            }
            .type_name(),
            "Point"
        );
        assert_eq!(
            InspectValue::Union {
                tag: "Ok".into(),
                payload: Box::new(InspectValue::Null)
            }
            .type_name(),
            "Ok"
        );
    }

    // -- Evaluate -----------------------------------------------------------

    #[test]
    fn evaluate_returns_expression_as_string() {
        let mut server = DapServer::new();
        let resp = server.handle_request(DapRequest::Evaluate {
            expression: "x + 1".into(),
            frame_id: Some(0),
        });
        assert!(resp.success);
        match resp.body {
            DapResponseBody::Evaluate {
                result,
                ty,
                variables_reference,
            } => {
                assert_eq!(result, "x + 1");
                assert_eq!(ty, "String");
                assert_eq!(variables_reference, 0);
            }
            _ => panic!("expected Evaluate body"),
        }
    }

    // -- Stepping commands --------------------------------------------------

    #[test]
    fn step_commands_return_success() {
        let mut server = DapServer::new();
        let next = server.handle_request(DapRequest::Next { thread_id: 1 });
        assert!(next.success);
        assert_eq!(next.command, "next");

        let step_in = server.handle_request(DapRequest::StepIn { thread_id: 1 });
        assert!(step_in.success);
        assert_eq!(step_in.command, "stepIn");

        let step_out = server.handle_request(DapRequest::StepOut { thread_id: 1 });
        assert!(step_out.success);
        assert_eq!(step_out.command, "stepOut");

        let cont = server.handle_request(DapRequest::Continue { thread_id: 1 });
        assert!(cont.success);
        match cont.body {
            DapResponseBody::Continue {
                all_threads_continued,
            } => assert!(all_threads_continued),
            _ => panic!("expected Continue body"),
        }
    }

    // -- Events -------------------------------------------------------------

    #[test]
    fn stopped_event_creation() {
        let event = DapServer::stopped_event(StopReason::Breakpoint, 1);
        match event {
            DapEvent::Stopped {
                reason,
                thread_id,
                description,
            } => {
                assert_eq!(reason, StopReason::Breakpoint);
                assert_eq!(thread_id, 1);
                assert!(description.is_none());
            }
            _ => panic!("expected Stopped event"),
        }

        let event =
            DapServer::stopped_event_with_description(StopReason::Step, 1, "step over".into());
        match event {
            DapEvent::Stopped { description, .. } => {
                assert_eq!(description, Some("step over".into()));
            }
            _ => panic!("expected Stopped event"),
        }
    }

    #[test]
    fn clear_variable_refs_resets_state() {
        let mut server = DapServer::new();
        let ref1 = server.register_variables(vec![DapVariable {
            name: "a".into(),
            value: "1".into(),
            ty: "Int".into(),
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
        }]);
        assert!(ref1 > 0);

        server.clear_variable_refs();

        // After clearing, old reference should return empty
        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: ref1,
        });
        match resp.body {
            DapResponseBody::Variables(vars) => assert!(vars.is_empty()),
            _ => panic!("expected Variables body"),
        }
    }

    // -- StopReason / OutputCategory / ThreadReason string representations ---

    #[test]
    fn enum_as_str_values() {
        assert_eq!(StopReason::Breakpoint.as_str(), "breakpoint");
        assert_eq!(StopReason::Step.as_str(), "step");
        assert_eq!(StopReason::Pause.as_str(), "pause");
        assert_eq!(StopReason::Entry.as_str(), "entry");
        assert_eq!(StopReason::Exception.as_str(), "exception");

        assert_eq!(OutputCategory::Console.as_str(), "console");
        assert_eq!(OutputCategory::Stdout.as_str(), "stdout");
        assert_eq!(OutputCategory::Stderr.as_str(), "stderr");

        assert_eq!(ThreadReason::Started.as_str(), "started");
        assert_eq!(ThreadReason::Exited.as_str(), "exited");
    }

    // -- Expand set value ---------------------------------------------------

    #[test]
    fn expand_set_value() {
        let mut server = DapServer::new();
        let value = InspectValue::Set(vec![
            InspectValue::String("a".into()),
            InspectValue::String("b".into()),
        ]);

        let var = server.expand_value("tags", &value);
        assert_eq!(var.ty, "Set");
        assert_eq!(var.indexed_variables, Some(2));
        assert_eq!(var.value, "{2 items}");

        let resp = server.handle_request(DapRequest::Variables {
            variables_reference: var.variables_reference,
        });
        match resp.body {
            DapResponseBody::Variables(children) => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].name, "[0]");
                assert_eq!(children[0].value, "\"a\"");
                assert_eq!(children[1].name, "[1]");
                assert_eq!(children[1].value, "\"b\"");
            }
            _ => panic!("expected Variables body"),
        }
    }
}
