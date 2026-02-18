//! Debug Adapter Protocol (DAP) server for Lumen.
//!
//! Enables VS Code and other DAP-compatible editors to debug Lumen programs
//! with breakpoints, stepping, stack traces, and variable inspection.
//!
//! Since LIR bytecode has no source-line mapping, this DAP server operates at
//! the **cell (function) level**: breakpoints are set on cell entry, and stepping
//! operates instruction-by-instruction or cell-by-cell.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use dap::prelude::*;
use dap::requests::Command;
use dap::responses::ResponseBody;
use dap::server::ServerOutput;
use dap::types::*;

use lumen_compiler::CompileError;
use lumen_core::lir::LirModule;
use lumen_core::values::Value;
use lumen_rt::vm::{self, DebugEvent, VM};

use crate::module_resolver;

// ─── Constants ──────────────────────────────────────────────────────────────

const THREAD_ID: i64 = 1;
const LOCALS_SCOPE_REF: i64 = 1000;

// ─── Messages between DAP I/O thread and VM thread ──────────────────────────

/// Commands sent from the DAP I/O thread to the VM execution thread.
#[derive(Debug)]
enum DapCommand {
    /// Launch and start execution. If `stop_on_entry` is true, stop before first instruction.
    Launch { stop_on_entry: bool },
    /// Continue execution (resume from stopped state).
    Continue,
    /// Step one instruction (step into calls).
    StepIn,
    /// Step over the next instruction (skip over calls).
    Next,
    /// Step out of the current cell.
    StepOut,
    /// Pause execution.
    Pause,
    /// Set breakpoints on cells.
    SetFunctionBreakpoints { names: Vec<String> },
    /// Request stack trace.
    GetStackTrace,
    /// Request scopes for a frame.
    GetScopes { frame_id: i64 },
    /// Request variables for a scope/reference.
    GetVariables { variables_reference: i64 },
    /// Disconnect / terminate.
    Disconnect,
}

/// Events sent from the VM execution thread back to the DAP I/O thread.
#[derive(Debug, Clone)]
enum DapEvent {
    /// VM stopped (breakpoint, step, pause, entry, exception).
    Stopped {
        reason: StoppedReason,
        description: Option<String>,
    },
    /// VM terminated normally.
    Terminated,
    /// VM exited with a code.
    Exited { exit_code: i64 },
    /// Output from the program (print statements, etc.).
    Output {
        output: String,
        category: OutputCategory,
    },
    /// Stack trace response.
    StackTraceResult { frames: Vec<DapStackFrame> },
    /// Scopes response.
    ScopesResult { scopes: Vec<DapScope> },
    /// Variables response.
    VariablesResult { variables: Vec<DapVariable> },
}

#[derive(Debug, Clone)]
enum StoppedReason {
    Entry,
    Breakpoint,
    Step,
    Pause,
    Exception,
}

impl StoppedReason {
    fn as_str(&self) -> &'static str {
        match self {
            StoppedReason::Entry => "entry",
            StoppedReason::Breakpoint => "breakpoint",
            StoppedReason::Step => "step",
            StoppedReason::Pause => "pause",
            StoppedReason::Exception => "exception",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum OutputCategory {
    Console,
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DapStackFrame {
    id: i64,
    name: String,
    instruction_pointer: usize,
    source_file: Option<String>,
}

#[derive(Debug, Clone)]
struct DapScope {
    name: String,
    variables_reference: i64,
    expensive: bool,
}

#[derive(Debug, Clone)]
struct DapVariable {
    name: String,
    value: String,
    ty: String,
    variables_reference: i64,
}

// ─── Debugger State (owned by VM thread) ────────────────────────────────────

/// The state of the debugger, held exclusively by the VM execution thread.
struct DebuggerState {
    /// The compiled module.
    module: LirModule,
    /// Source file path.
    source_path: PathBuf,
    /// Source text.
    #[allow(dead_code)]
    source_text: String,
    /// Cell names that have breakpoints set on entry.
    function_breakpoints: HashSet<String>,
    /// Whether we should stop on next instruction (single-step mode).
    #[allow(dead_code)]
    step_mode: StepMode,
    /// Current call depth when step-over/step-out was initiated.
    #[allow(dead_code)]
    step_depth: usize,
    /// Whether a pause was requested.
    #[allow(dead_code)]
    pause_requested: bool,
    /// Whether we should stop on entry.
    stop_on_entry: bool,
    /// Cached frame data for variable inspection.
    cached_frames: Vec<CachedFrame>,
    /// Cached variables by variables_reference.
    cached_variables: HashMap<i64, Vec<DapVariable>>,
    /// Channel to send events back to DAP I/O thread.
    event_tx: mpsc::Sender<DapEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum StepMode {
    /// Run freely until breakpoint or pause.
    Run,
    /// Stop after one instruction (step in).
    StepIn,
    /// Stop when call depth returns to `step_depth` (step over).
    StepOver,
    /// Stop when call depth decreases below `step_depth` (step out).
    StepOut,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CachedFrame {
    cell_name: String,
    cell_idx: usize,
    base_register: usize,
    ip: usize,
}

// ─── Public Entry Point ─────────────────────────────────────────────────────

/// Run the DAP server, reading from stdin and writing to stdout.
/// This is the entry point called by `lumen debug <file>`.
pub fn run_dap_server(file: &Path, allow_unstable: bool) {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());
    let writer = BufWriter::new(stdout.lock());

    run_dap_server_on(reader, writer, file, allow_unstable);
}

/// Run the DAP server on arbitrary Read/Write streams (testable).
pub fn run_dap_server_on<R: Read, W: Write>(
    reader: BufReader<R>,
    writer: BufWriter<W>,
    file: &Path,
    allow_unstable: bool,
) {
    let mut server = Server::new(reader, writer);

    // Channels for communication between DAP I/O and VM threads.
    let (cmd_tx, cmd_rx) = mpsc::channel::<DapCommand>();
    let (event_tx, event_rx) = mpsc::channel::<DapEvent>();

    // Clone server output handle for sending events from this thread.
    // server.output is Arc<Mutex<ServerOutput<W>>>
    let server_output = server.output.clone();

    // Read and compile source.
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            send_output_event(
                &server_output,
                &format!("Failed to read source file: {}\n", e),
                "stderr",
            );
            return;
        }
    };

    let module = match compile_for_debug(file, &source, allow_unstable) {
        Ok(m) => m,
        Err(e) => {
            let formatted = lumen_compiler::format_error(&e, &source, &file.display().to_string());
            send_output_event(&server_output, &formatted, "stderr");
            return;
        }
    };

    let file_path = file.to_path_buf();
    let source_for_thread = source.clone();
    let module_for_thread = module.clone();

    // Spawn VM execution thread.
    let vm_handle = thread::spawn(move || {
        run_vm_thread(
            module_for_thread,
            file_path,
            source_for_thread,
            cmd_rx,
            event_tx,
        );
    });

    // DAP I/O loop on the main thread.
    run_dap_io_loop(
        &mut server,
        &server_output,
        &cmd_tx,
        &event_rx,
        file,
        &source,
        &module,
    );

    // Wait for VM thread to finish.
    let _ = vm_handle.join();
}

// ─── DAP I/O Loop (main thread) ────────────────────────────────────────────

fn run_dap_io_loop<R: Read, W: Write>(
    server: &mut Server<R, W>,
    server_output: &Arc<Mutex<ServerOutput<W>>>,
    cmd_tx: &mpsc::Sender<DapCommand>,
    event_rx: &mpsc::Receiver<DapEvent>,
    file: &Path,
    _source: &str,
    module: &LirModule,
) {
    let mut initialized = false;
    let mut running = true;

    while running {
        // Drain any pending events from the VM thread (non-blocking).
        loop {
            match event_rx.try_recv() {
                Ok(event) => handle_vm_event(server_output, &event),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    running = false;
                    break;
                }
            }
        }

        // Poll for a DAP request from the client (non-blocking approach: use timeout).
        match server.poll_request() {
            Ok(Some(req)) => {
                let resp = handle_request(
                    &req,
                    server_output,
                    cmd_tx,
                    event_rx,
                    file,
                    module,
                    &mut initialized,
                    &mut running,
                );
                if let Some(resp) = resp {
                    if server.respond(resp).is_err() {
                        running = false;
                    }
                }
            }
            Ok(None) => {
                // Client disconnected.
                running = false;
            }
            Err(_) => {
                // Read error — try to continue.
                running = false;
            }
        }
    }

    // Signal VM thread to disconnect.
    let _ = cmd_tx.send(DapCommand::Disconnect);
}

fn handle_request<W: Write>(
    req: &Request,
    server_output: &Arc<Mutex<ServerOutput<W>>>,
    cmd_tx: &mpsc::Sender<DapCommand>,
    event_rx: &mpsc::Receiver<DapEvent>,
    file: &Path,
    module: &LirModule,
    initialized: &mut bool,
    running: &mut bool,
) -> Option<Response> {
    match &req.command {
        Command::Initialize(_args) => {
            let caps = types::Capabilities {
                supports_configuration_done_request: Some(true),
                supports_function_breakpoints: Some(true),
                supports_step_in_targets_request: Some(false),
                supports_terminate_request: Some(true),
                supports_loaded_sources_request: Some(true),
                ..Default::default()
            };
            *initialized = true;

            // Send Initialized event inline.
            send_initialized_event(server_output);

            Some(req.clone().success(ResponseBody::Initialize(caps)))
        }

        Command::Launch(args) => {
            let stop_on_entry = args
                .additional_data
                .as_ref()
                .and_then(|v| v.get("stopOnEntry"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let _ = cmd_tx.send(DapCommand::Launch { stop_on_entry });

            Some(req.clone().success(ResponseBody::Launch))
        }

        Command::ConfigurationDone => Some(req.clone().success(ResponseBody::ConfigurationDone)),

        Command::SetBreakpoints(args) => {
            // We don't support source-line breakpoints (no source map in LIR).
            // Return empty breakpoints (all unverified).
            let breakpoints = if let Some(ref bps) = args.breakpoints {
                bps.iter()
                    .map(|bp| Breakpoint {
                        verified: false,
                        message: Some(
                            "Source-line breakpoints not supported. Use function breakpoints."
                                .to_string(),
                        ),
                        line: Some(bp.line),
                        ..Default::default()
                    })
                    .collect()
            } else {
                vec![]
            };

            Some(req.clone().success(ResponseBody::SetBreakpoints(
                responses::SetBreakpointsResponse { breakpoints },
            )))
        }

        Command::SetFunctionBreakpoints(args) => {
            let names: Vec<String> = args.breakpoints.iter().map(|bp| bp.name.clone()).collect();

            // Verify which cell names exist in the module.
            let breakpoints: Vec<Breakpoint> = names
                .iter()
                .map(|name| {
                    let exists = module.cells.iter().any(|c| c.name == *name);
                    Breakpoint {
                        verified: exists,
                        message: if exists {
                            None
                        } else {
                            Some(format!("Cell '{}' not found in module", name))
                        },
                        ..Default::default()
                    }
                })
                .collect();

            let _ = cmd_tx.send(DapCommand::SetFunctionBreakpoints {
                names: names
                    .into_iter()
                    .filter(|n| module.cells.iter().any(|c| c.name == *n))
                    .collect(),
            });

            Some(req.clone().success(ResponseBody::SetFunctionBreakpoints(
                responses::SetFunctionBreakpointsResponse { breakpoints },
            )))
        }

        Command::Threads => {
            let threads = vec![Thread {
                id: THREAD_ID,
                name: "main".to_string(),
            }];
            Some(
                req.clone()
                    .success(ResponseBody::Threads(responses::ThreadsResponse {
                        threads,
                    })),
            )
        }

        Command::StackTrace(_args) => {
            let _ = cmd_tx.send(DapCommand::GetStackTrace);

            // Wait for stack trace result from VM thread.
            match wait_for_event(event_rx, |e| matches!(e, DapEvent::StackTraceResult { .. })) {
                Some(DapEvent::StackTraceResult { frames }) => {
                    let stack_frames: Vec<StackFrame> = frames
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let source = f.source_file.as_ref().map(|p| Source {
                                name: Some(
                                    Path::new(p)
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                ),
                                path: Some(p.clone()),
                                ..Default::default()
                            });
                            StackFrame {
                                id: i as i64,
                                name: f.name.clone(),
                                source,
                                line: 0, // No source mapping.
                                column: 0,
                                instruction_pointer_reference: Some(format!(
                                    "0x{:04x}",
                                    f.instruction_pointer
                                )),
                                ..Default::default()
                            }
                        })
                        .collect();

                    let total = stack_frames.len() as i64;
                    Some(req.clone().success(ResponseBody::StackTrace(
                        responses::StackTraceResponse {
                            stack_frames,
                            total_frames: Some(total),
                        },
                    )))
                }
                _ => Some(req.clone().success(ResponseBody::StackTrace(
                    responses::StackTraceResponse {
                        stack_frames: vec![],
                        total_frames: Some(0),
                    },
                ))),
            }
        }

        Command::Scopes(args) => {
            let frame_id = args.frame_id;
            let _ = cmd_tx.send(DapCommand::GetScopes { frame_id });

            match wait_for_event(event_rx, |e| matches!(e, DapEvent::ScopesResult { .. })) {
                Some(DapEvent::ScopesResult { scopes }) => {
                    let dap_scopes: Vec<Scope> = scopes
                        .iter()
                        .map(|s| Scope {
                            name: s.name.clone(),
                            variables_reference: s.variables_reference,
                            expensive: s.expensive,
                            ..Default::default()
                        })
                        .collect();

                    Some(
                        req.clone()
                            .success(ResponseBody::Scopes(responses::ScopesResponse {
                                scopes: dap_scopes,
                            })),
                    )
                }
                _ => Some(
                    req.clone()
                        .success(ResponseBody::Scopes(responses::ScopesResponse {
                            scopes: vec![],
                        })),
                ),
            }
        }

        Command::Variables(args) => {
            let variables_reference = args.variables_reference;
            let _ = cmd_tx.send(DapCommand::GetVariables {
                variables_reference,
            });

            match wait_for_event(event_rx, |e| matches!(e, DapEvent::VariablesResult { .. })) {
                Some(DapEvent::VariablesResult { variables }) => {
                    let dap_vars: Vec<Variable> = variables
                        .iter()
                        .map(|v| Variable {
                            name: v.name.clone(),
                            value: v.value.clone(),
                            type_field: Some(v.ty.clone()),
                            variables_reference: v.variables_reference,
                            ..Default::default()
                        })
                        .collect();

                    Some(req.clone().success(ResponseBody::Variables(
                        responses::VariablesResponse {
                            variables: dap_vars,
                        },
                    )))
                }
                _ => Some(req.clone().success(ResponseBody::Variables(
                    responses::VariablesResponse { variables: vec![] },
                ))),
            }
        }

        Command::Continue(_) => {
            let _ = cmd_tx.send(DapCommand::Continue);
            Some(
                req.clone()
                    .success(ResponseBody::Continue(responses::ContinueResponse {
                        all_threads_continued: Some(true),
                    })),
            )
        }

        Command::Next(_) => {
            let _ = cmd_tx.send(DapCommand::Next);
            Some(req.clone().success(ResponseBody::Next))
        }

        Command::StepIn(_) => {
            let _ = cmd_tx.send(DapCommand::StepIn);
            Some(req.clone().success(ResponseBody::StepIn))
        }

        Command::StepOut(_) => {
            let _ = cmd_tx.send(DapCommand::StepOut);
            Some(req.clone().success(ResponseBody::StepOut))
        }

        Command::Pause(_) => {
            let _ = cmd_tx.send(DapCommand::Pause);
            Some(req.clone().success(ResponseBody::Pause))
        }

        Command::Disconnect(_) => {
            let _ = cmd_tx.send(DapCommand::Disconnect);
            *running = false;
            Some(req.clone().success(ResponseBody::Disconnect))
        }

        Command::Terminate(_) => {
            let _ = cmd_tx.send(DapCommand::Disconnect);
            *running = false;
            Some(req.clone().success(ResponseBody::Terminate))
        }

        Command::LoadedSources => {
            let sources = vec![Source {
                name: Some(
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                ),
                path: Some(file.display().to_string()),
                ..Default::default()
            }];
            Some(req.clone().success(ResponseBody::LoadedSources(
                responses::LoadedSourcesResponse { sources },
            )))
        }

        // Unhandled commands — acknowledge them.
        _ => req.clone().ack().ok(),
    }
}

// ─── VM Execution Thread ────────────────────────────────────────────────────

fn run_vm_thread(
    module: LirModule,
    file_path: PathBuf,
    source: String,
    cmd_rx: mpsc::Receiver<DapCommand>,
    event_tx: mpsc::Sender<DapEvent>,
) {
    let mut state = DebuggerState {
        module: module.clone(),
        source_path: file_path,
        source_text: source,
        function_breakpoints: HashSet::new(),
        step_mode: StepMode::Run,
        step_depth: 0,
        pause_requested: false,
        stop_on_entry: false,
        cached_frames: Vec::new(),
        cached_variables: HashMap::new(),
        event_tx: event_tx.clone(),
    };

    // Wait for commands from DAP I/O thread.
    loop {
        match cmd_rx.recv() {
            Ok(DapCommand::Launch { stop_on_entry }) => {
                state.stop_on_entry = stop_on_entry;
                run_debuggee(&mut state, &cmd_rx);
            }
            Ok(DapCommand::SetFunctionBreakpoints { names }) => {
                state.function_breakpoints = names.into_iter().collect();
            }
            Ok(DapCommand::Disconnect) => {
                break;
            }
            Ok(DapCommand::GetStackTrace) => {
                // No VM running yet — empty stack.
                let _ = event_tx.send(DapEvent::StackTraceResult { frames: vec![] });
            }
            Ok(DapCommand::GetScopes { .. }) => {
                let _ = event_tx.send(DapEvent::ScopesResult { scopes: vec![] });
            }
            Ok(DapCommand::GetVariables { .. }) => {
                let _ = event_tx.send(DapEvent::VariablesResult { variables: vec![] });
            }
            Ok(_) => {
                // Ignore other commands before launch.
            }
            Err(_) => break, // Channel closed.
        }
    }
}

/// Run the debuggee program with debug hooks. This is the core execution loop.
fn run_debuggee(state: &mut DebuggerState, cmd_rx: &mpsc::Receiver<DapCommand>) {
    // Create VM and load module.
    let mut vm = VM::new();

    // Set up provider registry (basic, no config).
    let registry = lumen_rt::services::tools::ProviderRegistry::new();
    vm.set_provider_registry(registry);
    vm.load(state.module.clone());

    // Shared state for the debug callback.
    // We use Arc<Mutex<>> to share state between the callback and the main loop.
    let break_flag = Arc::new(Mutex::new(false));
    let pause_flag = Arc::new(Mutex::new(false));
    let cell_breakpoints: Arc<Mutex<HashSet<String>>> =
        Arc::new(Mutex::new(state.function_breakpoints.clone()));
    let stop_reason: Arc<Mutex<Option<StoppedReason>>> = Arc::new(Mutex::new(None));

    // Step tracking
    let step_mode_flag = Arc::new(Mutex::new(StepMode::Run));
    let step_depth_flag = Arc::new(Mutex::new(0usize));
    let call_depth = Arc::new(Mutex::new(0usize));

    // Set up the debug callback.
    {
        let break_flag = Arc::clone(&break_flag);
        let pause_flag = Arc::clone(&pause_flag);
        let cell_breakpoints = Arc::clone(&cell_breakpoints);
        let stop_reason = Arc::clone(&stop_reason);
        let step_mode_flag = Arc::clone(&step_mode_flag);
        let step_depth_flag = Arc::clone(&step_depth_flag);
        let call_depth = Arc::clone(&call_depth);

        vm.debug_callback = Some(Box::new(move |event| {
            match event {
                DebugEvent::CallEnter { cell_name } => {
                    if let Ok(mut depth) = call_depth.lock() {
                        *depth += 1;
                    }

                    // Check function breakpoints.
                    if let Ok(bps) = cell_breakpoints.lock() {
                        if bps.contains(cell_name.as_str()) {
                            if let Ok(mut bf) = break_flag.lock() {
                                *bf = true;
                            }
                            if let Ok(mut sr) = stop_reason.lock() {
                                *sr = Some(StoppedReason::Breakpoint);
                            }
                        }
                    }
                }
                DebugEvent::CallExit { .. } => {
                    if let Ok(mut depth) = call_depth.lock() {
                        *depth = depth.saturating_sub(1);
                    }
                }
                DebugEvent::Step { .. } => {
                    // Check pause request.
                    if let Ok(mut pf) = pause_flag.lock() {
                        if *pf {
                            *pf = false;
                            if let Ok(mut bf) = break_flag.lock() {
                                *bf = true;
                            }
                            if let Ok(mut sr) = stop_reason.lock() {
                                *sr = Some(StoppedReason::Pause);
                            }
                        }
                    }

                    // Check step mode.
                    if let Ok(mode) = step_mode_flag.lock() {
                        match *mode {
                            StepMode::StepIn => {
                                if let Ok(mut bf) = break_flag.lock() {
                                    *bf = true;
                                }
                                if let Ok(mut sr) = stop_reason.lock() {
                                    *sr = Some(StoppedReason::Step);
                                }
                            }
                            StepMode::StepOver => {
                                if let Ok(depth) = call_depth.lock() {
                                    if let Ok(target) = step_depth_flag.lock() {
                                        if *depth <= *target {
                                            if let Ok(mut bf) = break_flag.lock() {
                                                *bf = true;
                                            }
                                            if let Ok(mut sr) = stop_reason.lock() {
                                                *sr = Some(StoppedReason::Step);
                                            }
                                        }
                                    }
                                }
                            }
                            StepMode::StepOut => {
                                if let Ok(depth) = call_depth.lock() {
                                    if let Ok(target) = step_depth_flag.lock() {
                                        if *depth < *target {
                                            if let Ok(mut bf) = break_flag.lock() {
                                                *bf = true;
                                            }
                                            if let Ok(mut sr) = stop_reason.lock() {
                                                *sr = Some(StoppedReason::Step);
                                            }
                                        }
                                    }
                                }
                            }
                            StepMode::Run => {}
                        }
                    }
                }
                _ => {}
            }
        }));
    }

    // If stop_on_entry, send stopped event immediately.
    if state.stop_on_entry {
        let _ = state.event_tx.send(DapEvent::Stopped {
            reason: StoppedReason::Entry,
            description: Some("Stopped on entry".to_string()),
        });

        // Process commands while stopped on entry.
        process_stopped_commands(
            state,
            cmd_rx,
            &mut vm,
            &cell_breakpoints,
            &step_mode_flag,
            &step_depth_flag,
            &call_depth,
        );
    }

    // Execute the program using fuel-based stepping.
    // We run in a loop: execute some instructions, check for break conditions, handle commands.
    loop {
        // Reset break flag.
        if let Ok(mut bf) = break_flag.lock() {
            *bf = false;
        }

        // Set fuel for one batch of instructions.
        vm.set_fuel(1024);

        match vm.execute_continue() {
            Ok(Some(result)) => {
                // Program completed normally.
                let _ = state.event_tx.send(DapEvent::Output {
                    output: format!("{}\n", result),
                    category: OutputCategory::Stdout,
                });
                // Flush captured stdout.
                for line in vm.output.drain(..) {
                    let _ = state.event_tx.send(DapEvent::Output {
                        output: format!("{}\n", line),
                        category: OutputCategory::Stdout,
                    });
                }
                let _ = state.event_tx.send(DapEvent::Terminated);
                let _ = state.event_tx.send(DapEvent::Exited { exit_code: 0 });
                return;
            }
            Ok(None) => {
                // Fuel exhausted or break requested — check break flag.
                // Flush any output.
                for line in vm.output.drain(..) {
                    let _ = state.event_tx.send(DapEvent::Output {
                        output: format!("{}\n", line),
                        category: OutputCategory::Stdout,
                    });
                }

                let should_stop = break_flag.lock().map(|bf| *bf).unwrap_or(false);
                if should_stop {
                    let reason = stop_reason
                        .lock()
                        .ok()
                        .and_then(|mut sr| sr.take())
                        .unwrap_or(StoppedReason::Step);

                    // Reset step mode.
                    if let Ok(mut mode) = step_mode_flag.lock() {
                        *mode = StepMode::Run;
                    }

                    let _ = state.event_tx.send(DapEvent::Stopped {
                        reason: reason.clone(),
                        description: Some(format!("Stopped: {}", reason.as_str())),
                    });

                    // Cache frame state for inspection.
                    cache_vm_state(state, &vm);

                    // Process commands while stopped.
                    process_stopped_commands(
                        state,
                        cmd_rx,
                        &mut vm,
                        &cell_breakpoints,
                        &step_mode_flag,
                        &step_depth_flag,
                        &call_depth,
                    );
                }
                // Otherwise, fuel just ran out — continue running.
            }
            Err(e) => {
                // Runtime error.
                let _ = state.event_tx.send(DapEvent::Output {
                    output: format!("Runtime error: {}\n", e),
                    category: OutputCategory::Stderr,
                });
                // Flush output.
                for line in vm.output.drain(..) {
                    let _ = state.event_tx.send(DapEvent::Output {
                        output: format!("{}\n", line),
                        category: OutputCategory::Stdout,
                    });
                }
                let _ = state.event_tx.send(DapEvent::Stopped {
                    reason: StoppedReason::Exception,
                    description: Some(format!("Runtime error: {}", e)),
                });

                cache_vm_state(state, &vm);

                // Process commands while stopped on exception.
                process_stopped_commands(
                    state,
                    cmd_rx,
                    &mut vm,
                    &cell_breakpoints,
                    &step_mode_flag,
                    &step_depth_flag,
                    &call_depth,
                );
            }
        }
    }
}

/// Process commands while the VM is stopped (breakpoint, step, entry, exception).
/// Blocks until a Continue, Step, or Disconnect command is received.
fn process_stopped_commands(
    state: &mut DebuggerState,
    cmd_rx: &mpsc::Receiver<DapCommand>,
    vm: &mut VM,
    cell_breakpoints: &Arc<Mutex<HashSet<String>>>,
    step_mode_flag: &Arc<Mutex<StepMode>>,
    step_depth_flag: &Arc<Mutex<usize>>,
    call_depth: &Arc<Mutex<usize>>,
) {
    loop {
        match cmd_rx.recv() {
            Ok(DapCommand::Continue) => {
                if let Ok(mut mode) = step_mode_flag.lock() {
                    *mode = StepMode::Run;
                }
                return; // Resume execution.
            }
            Ok(DapCommand::StepIn) => {
                if let Ok(mut mode) = step_mode_flag.lock() {
                    *mode = StepMode::StepIn;
                }
                return; // Resume for one step.
            }
            Ok(DapCommand::Next) => {
                let depth = call_depth.lock().map(|d| *d).unwrap_or(0);
                if let Ok(mut target) = step_depth_flag.lock() {
                    *target = depth;
                }
                if let Ok(mut mode) = step_mode_flag.lock() {
                    *mode = StepMode::StepOver;
                }
                return;
            }
            Ok(DapCommand::StepOut) => {
                let depth = call_depth.lock().map(|d| *d).unwrap_or(0);
                if let Ok(mut target) = step_depth_flag.lock() {
                    *target = depth;
                }
                if let Ok(mut mode) = step_mode_flag.lock() {
                    *mode = StepMode::StepOut;
                }
                return;
            }
            Ok(DapCommand::SetFunctionBreakpoints { names }) => {
                state.function_breakpoints = names.iter().cloned().collect();
                if let Ok(mut bps) = cell_breakpoints.lock() {
                    *bps = state.function_breakpoints.clone();
                }
            }
            Ok(DapCommand::GetStackTrace) => {
                let frames = build_stack_frames(state, vm);
                let _ = state.event_tx.send(DapEvent::StackTraceResult { frames });
            }
            Ok(DapCommand::GetScopes { frame_id }) => {
                let scopes = build_scopes(frame_id);
                let _ = state.event_tx.send(DapEvent::ScopesResult { scopes });
            }
            Ok(DapCommand::GetVariables {
                variables_reference,
            }) => {
                let variables = build_variables(vm, variables_reference);
                let _ = state.event_tx.send(DapEvent::VariablesResult { variables });
            }
            Ok(DapCommand::Pause) => {
                // Already stopped; ignore.
            }
            Ok(DapCommand::Disconnect) => {
                return; // Let the outer loop handle cleanup.
            }
            Ok(_) => {
                // Ignore unknown commands while stopped.
            }
            Err(_) => return, // Channel closed.
        }
    }
}

// ─── VM State Inspection ────────────────────────────────────────────────────

fn cache_vm_state(state: &mut DebuggerState, vm: &VM) {
    state.cached_frames.clear();
    state.cached_variables.clear();

    if let Some(module) = vm.module() {
        for (_i, frame) in vm.frames().iter().enumerate() {
            let cell_name = if frame.cell_idx < module.cells.len() {
                module.cells[frame.cell_idx].name.clone()
            } else {
                format!("<unknown-cell-{}>", frame.cell_idx)
            };
            state.cached_frames.push(CachedFrame {
                cell_name,
                cell_idx: frame.cell_idx,
                base_register: frame.base_register,
                ip: frame.ip,
            });
        }
    }
}

fn build_stack_frames(state: &DebuggerState, vm: &VM) -> Vec<DapStackFrame> {
    let module = match vm.module() {
        Some(m) => m,
        None => return vec![],
    };

    vm.frames()
        .iter()
        .rev() // Most recent frame first (DAP convention).
        .enumerate()
        .map(|(i, frame)| {
            let cell_name = if frame.cell_idx < module.cells.len() {
                module.cells[frame.cell_idx].name.clone()
            } else {
                format!("<unknown-cell-{}>", frame.cell_idx)
            };
            DapStackFrame {
                id: i as i64,
                name: cell_name,
                instruction_pointer: frame.ip,
                source_file: Some(state.source_path.display().to_string()),
            }
        })
        .collect()
}

fn build_scopes(frame_id: i64) -> Vec<DapScope> {
    // One scope per frame: "Locals" containing the cell's registers.
    // Variables reference encodes frame_id: LOCALS_SCOPE_REF + frame_id.
    vec![DapScope {
        name: "Locals".to_string(),
        variables_reference: LOCALS_SCOPE_REF + frame_id,
        expensive: false,
    }]
}

fn build_variables(vm: &VM, variables_reference: i64) -> Vec<DapVariable> {
    let module = match vm.module() {
        Some(m) => m,
        None => return vec![],
    };

    // Decode frame index from variables_reference.
    let frame_idx = (variables_reference - LOCALS_SCOPE_REF) as usize;

    // Frames are reversed in the stack trace (most recent first), but vm.frames()
    // stores them oldest-first. Convert back.
    let frames = vm.frames();
    let actual_frame_idx = if frame_idx < frames.len() {
        frames.len() - 1 - frame_idx
    } else {
        return vec![];
    };

    let frame = match frames.get(actual_frame_idx) {
        Some(f) => f,
        None => return vec![],
    };

    let cell = match module.cells.get(frame.cell_idx) {
        Some(c) => c,
        None => return vec![],
    };

    let base = frame.base_register;
    let num_regs = cell.registers as usize;
    let mut variables = Vec::new();

    // First, show named parameters.
    let mut named_regs: HashSet<usize> = HashSet::new();
    for param in &cell.params {
        let reg = param.register as usize;
        named_regs.insert(reg);
        let abs_reg = base + reg;
        let value = vm.read_register(abs_reg);
        variables.push(DapVariable {
            name: param.name.clone(),
            value: format_value_short(&value),
            ty: param.ty.clone(),
            variables_reference: if is_composite(&value) {
                // Encode: frame * 100000 + register for drill-down.
                (frame_idx as i64) * 100_000 + reg as i64 + 10_000
            } else {
                0
            },
        });
    }

    // Then show remaining registers (unnamed).
    for reg in 0..num_regs {
        if named_regs.contains(&reg) {
            continue;
        }
        let abs_reg = base + reg;
        let value = vm.read_register(abs_reg);
        // Skip null registers to reduce noise.
        if matches!(&value, Value::Null) {
            continue;
        }
        variables.push(DapVariable {
            name: format!("r{}", reg),
            value: format_value_short(&value),
            ty: value.type_name().to_string(),
            variables_reference: if is_composite(&value) {
                (frame_idx as i64) * 100_000 + reg as i64 + 10_000
            } else {
                0
            },
        });
    }

    variables
}

// ─── Value Formatting ───────────────────────────────────────────────────────

fn format_value_short(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{}", f),
        Value::String(sr) => {
            let s = match sr {
                lumen_core::values::StringRef::Owned(s) => s.as_str(),
                lumen_core::values::StringRef::Interned(_) => "<interned>",
            };
            if s.len() > 80 {
                format!("\"{}...\"", &s[..77])
            } else {
                format!("\"{}\"", s)
            }
        }
        Value::List(items) => {
            let items = items.as_ref();
            if items.len() <= 5 {
                let parts: Vec<String> = items.iter().map(|v| format_value_short(v)).collect();
                format!("[{}]", parts.join(", "))
            } else {
                format!("[...{} items]", items.len())
            }
        }
        Value::Tuple(items) => {
            let items = items.as_ref();
            let parts: Vec<String> = items.iter().map(|v| format_value_short(v)).collect();
            format!("({})", parts.join(", "))
        }
        Value::Map(m) => {
            let m = m.as_ref();
            if m.len() <= 3 {
                let parts: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, format_value_short(v)))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            } else {
                format!("{{...{} entries}}", m.len())
            }
        }
        Value::Set(s) => {
            let s = s.as_ref();
            format!("{{...{} items}}", s.len())
        }
        Value::Record(rv) => {
            let rv = rv.as_ref();
            format!("{}(...)", rv.type_name)
        }
        Value::Union(uv) => {
            // uv.tag is u32 (interned string ID), uv.payload is Arc<Value>
            format!("variant#{}({})", uv.tag, format_value_short(&uv.payload))
        }
        Value::Closure(_) => "<closure>".to_string(),
        Value::Future(_) => "<future>".to_string(),
        Value::BigInt(n) => format!("{}", n),
        Value::Bytes(b) => format!("b\"...{} bytes\"", b.len()),
        _ => format!("{}", value),
    }
}

fn is_composite(value: &Value) -> bool {
    matches!(
        value,
        Value::List(_) | Value::Tuple(_) | Value::Map(_) | Value::Set(_) | Value::Record(_)
    )
}

// ─── Event Helpers ──────────────────────────────────────────────────────────

fn handle_vm_event<W: Write>(server_output: &Arc<Mutex<ServerOutput<W>>>, event: &DapEvent) {
    match event {
        DapEvent::Stopped {
            reason,
            description,
        } => {
            send_stopped_event(server_output, reason.as_str(), description.as_deref());
        }
        DapEvent::Terminated => {
            send_terminated_event(server_output);
        }
        DapEvent::Exited { exit_code } => {
            send_exited_event(server_output, *exit_code);
        }
        DapEvent::Output { output, category } => {
            let cat = match category {
                OutputCategory::Console => "console",
                OutputCategory::Stdout => "stdout",
                OutputCategory::Stderr => "stderr",
            };
            send_output_event(server_output, output, cat);
        }
        // Stack trace / scopes / variables results are handled synchronously.
        _ => {}
    }
}

fn send_initialized_event<W: Write>(server_output: &Arc<Mutex<ServerOutput<W>>>) {
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Initialized);
    }
}

fn send_stopped_event<W: Write>(
    server_output: &Arc<Mutex<ServerOutput<W>>>,
    reason: &str,
    description: Option<&str>,
) {
    let reason_enum = match reason {
        "step" => StoppedEventReason::Step,
        "breakpoint" => StoppedEventReason::Breakpoint,
        "exception" => StoppedEventReason::Exception,
        "pause" => StoppedEventReason::Pause,
        "entry" => StoppedEventReason::Entry,
        other => StoppedEventReason::String(other.to_string()),
    };
    let body = events::StoppedEventBody {
        reason: reason_enum,
        description: description.map(|s| s.to_string()),
        thread_id: Some(THREAD_ID),
        preserve_focus_hint: Some(false),
        text: None,
        all_threads_stopped: Some(true),
        hit_breakpoint_ids: None,
    };
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Stopped(body));
    }
}

fn send_terminated_event<W: Write>(server_output: &Arc<Mutex<ServerOutput<W>>>) {
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Terminated(None));
    }
}

fn send_exited_event<W: Write>(server_output: &Arc<Mutex<ServerOutput<W>>>, exit_code: i64) {
    let body = events::ExitedEventBody { exit_code };
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Exited(body));
    }
}

fn send_output_event<W: Write>(
    server_output: &Arc<Mutex<ServerOutput<W>>>,
    output: &str,
    category: &str,
) {
    let cat = match category {
        "console" => OutputEventCategory::Console,
        "stdout" => OutputEventCategory::Stdout,
        "stderr" => OutputEventCategory::Stderr,
        other => OutputEventCategory::String(other.to_string()),
    };
    let body = events::OutputEventBody {
        category: Some(cat),
        output: output.to_string(),
        ..Default::default()
    };
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Output(body));
    }
}

#[allow(dead_code)]
fn send_thread_event<W: Write>(server_output: &Arc<Mutex<ServerOutput<W>>>, started: bool) {
    let body = events::ThreadEventBody {
        reason: if started {
            ThreadEventReason::Started
        } else {
            ThreadEventReason::Exited
        },
        thread_id: THREAD_ID,
    };
    if let Ok(mut out) = server_output.lock() {
        let _ = out.send_event(Event::Thread(body));
    }
}

/// Wait for a specific event type from the VM thread, with a timeout.
fn wait_for_event(
    event_rx: &mpsc::Receiver<DapEvent>,
    predicate: impl Fn(&DapEvent) -> bool,
) -> Option<DapEvent> {
    // Use a short timeout to avoid hanging if the VM thread is busy.
    let timeout = std::time::Duration::from_secs(5);
    match event_rx.recv_timeout(timeout) {
        Ok(event) if predicate(&event) => Some(event),
        Ok(_other) => {
            // Got a different event — try once more.
            match event_rx.recv_timeout(timeout) {
                Ok(event) if predicate(&event) => Some(event),
                _ => None,
            }
        }
        Err(_) => None,
    }
}

// ─── Compilation ────────────────────────────────────────────────────────────

fn compile_for_debug(
    path: &Path,
    source: &str,
    allow_unstable: bool,
) -> Result<LirModule, CompileError> {
    let source_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut resolver = module_resolver::ModuleResolver::new(source_dir.clone());

    if let Some(project_root) = find_project_root(&source_dir) {
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && src_dir != source_dir {
            resolver.add_root(src_dir);
        }
        if project_root != source_dir {
            resolver.add_root(project_root);
        }
    }

    let resolver = RefCell::new(resolver);
    let resolve_import = |module_path: &str| resolver.borrow_mut().resolve(module_path);

    let opts = lumen_compiler::CompileOptions {
        allow_unstable,
        ..Default::default()
    };
    lumen_compiler::compile_with_imports_and_options(source, &resolve_import, &opts)
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("lumen.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

// ─── VM Extension ───────────────────────────────────────────────────────────
// We need a way to resume execution after fuel is exhausted.
// The VM already has fuel support — when fuel reaches 0, `run_until` returns
// an error "fuel exhausted". We need to differentiate this from real errors
// and also support continuing execution.

/// Extension trait for VM to support debug-style execution.
trait VmDebugExt {
    /// Execute (or continue) the program. Returns:
    /// - Ok(Some(value)) if the program completed normally
    /// - Ok(None) if fuel was exhausted (should check break flag and resume)
    /// - Err(e) if a runtime error occurred
    fn execute_continue(&mut self) -> Result<Option<Value>, vm::VmError>;
}

impl VmDebugExt for VM {
    fn execute_continue(&mut self) -> Result<Option<Value>, vm::VmError> {
        // If no frames are active, start execution from "main".
        if self.frames_is_empty() {
            match self.execute("main", vec![]) {
                Ok(val) => Ok(Some(val)),
                Err(vm::VmError::Runtime(msg)) if msg == "fuel exhausted" => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            // Continue from where we left off.
            match self.run_until(0) {
                Ok(val) => Ok(Some(val)),
                Err(vm::VmError::Runtime(msg)) if msg == "fuel exhausted" => Ok(None),
                Err(e) => Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_value_short() {
        assert_eq!(format_value_short(&Value::Null), "null");
        assert_eq!(format_value_short(&Value::Bool(true)), "true");
        assert_eq!(format_value_short(&Value::Int(42)), "42");
        assert_eq!(format_value_short(&Value::Float(3.14)), "3.14");
    }

    #[test]
    fn test_is_composite() {
        assert!(!is_composite(&Value::Int(1)));
        assert!(!is_composite(&Value::Null));
        assert!(is_composite(&Value::new_list(vec![Value::Int(1)])));
        assert!(is_composite(&Value::new_tuple(vec![Value::Int(1)])));
    }

    #[test]
    fn test_stopped_reason_as_str() {
        assert_eq!(StoppedReason::Entry.as_str(), "entry");
        assert_eq!(StoppedReason::Breakpoint.as_str(), "breakpoint");
        assert_eq!(StoppedReason::Step.as_str(), "step");
        assert_eq!(StoppedReason::Pause.as_str(), "pause");
        assert_eq!(StoppedReason::Exception.as_str(), "exception");
    }
}
