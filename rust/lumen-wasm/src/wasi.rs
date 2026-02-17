//! WASI Preview1 host bindings for Lumen WASM targets.
//!
//! This module provides the bridge between WASI system calls and Lumen's
//! built-in functions when running in a WASI environment (e.g., Wasmtime,
//! Wasmer). It defines the WASI function import signatures, a `WasiContext`
//! to track runtime state, and wrapper functions that translate between
//! WASI's low-level interface and Lumen's higher-level builtins.
//!
//! ## Architecture
//!
//! When Lumen compiles to `wasm32-wasi`, the generated WASM module imports
//! WASI functions from the `wasi_snapshot_preview1` namespace. This module
//! provides:
//!
//! 1. **Type definitions** matching WASI preview1 function signatures
//! 2. **`WasiContext`** — runtime state (file descriptors, env vars, args)
//! 3. **Wrapper functions** — bridge WASI calls to Lumen builtins
//!
//! ## Usage
//!
//! ```rust,no_run
//! use lumen_wasm::wasi::{WasiContext, WasiConfig};
//!
//! let config = WasiConfig::new()
//!     .with_args(vec!["my-program".into(), "--flag".into()])
//!     .with_env("HOME", "/home/user")
//!     .with_preopen("/data", "/host/data");
//! let ctx = WasiContext::from_config(config);
//! ```

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// WASI error codes (errno)
// ---------------------------------------------------------------------------

/// WASI error numbers (subset of `errno` from WASI preview1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum WasiErrno {
    /// No error.
    Success = 0,
    /// Bad file descriptor.
    Badf = 8,
    /// Invalid argument.
    Inval = 28,
    /// Not a directory.
    Notdir = 54,
    /// No such file or directory.
    Noent = 44,
    /// Permission denied.
    Acces = 2,
    /// Function not supported.
    Nosys = 52,
    /// I/O error.
    Io = 29,
}

impl WasiErrno {
    /// Convert to the raw u16 value used in WASI return codes.
    pub fn raw(self) -> u16 {
        self as u16
    }
}

impl std::fmt::Display for WasiErrno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasiErrno::Success => write!(f, "success"),
            WasiErrno::Badf => write!(f, "bad file descriptor"),
            WasiErrno::Inval => write!(f, "invalid argument"),
            WasiErrno::Notdir => write!(f, "not a directory"),
            WasiErrno::Noent => write!(f, "no such file or directory"),
            WasiErrno::Acces => write!(f, "permission denied"),
            WasiErrno::Nosys => write!(f, "function not supported"),
            WasiErrno::Io => write!(f, "I/O error"),
        }
    }
}

// ---------------------------------------------------------------------------
// WASI clock IDs
// ---------------------------------------------------------------------------

/// WASI clock identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WasiClockId {
    /// Wall-clock time (real time).
    Realtime = 0,
    /// Monotonic clock (for measuring elapsed time).
    Monotonic = 1,
    /// CPU time of the current process.
    ProcessCputime = 2,
    /// CPU time of the current thread.
    ThreadCputime = 3,
}

impl WasiClockId {
    /// Parse a clock ID from a raw u32 value.
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(WasiClockId::Realtime),
            1 => Some(WasiClockId::Monotonic),
            2 => Some(WasiClockId::ProcessCputime),
            3 => Some(WasiClockId::ThreadCputime),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// WASI file descriptor types
// ---------------------------------------------------------------------------

/// Rights associated with a file descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasiRights(pub u64);

impl WasiRights {
    pub const FD_READ: WasiRights = WasiRights(1 << 1);
    pub const FD_WRITE: WasiRights = WasiRights(1 << 6);
    pub const PATH_OPEN: WasiRights = WasiRights(1 << 13);
    pub const FD_READDIR: WasiRights = WasiRights(1 << 14);

    pub fn contains(self, other: WasiRights) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn union(self, other: WasiRights) -> WasiRights {
        WasiRights(self.0 | other.0)
    }
}

/// Type of a file descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WasiFileType {
    /// Unknown type.
    Unknown = 0,
    /// Block device.
    BlockDevice = 1,
    /// Character device (e.g., terminal).
    CharacterDevice = 2,
    /// Directory.
    Directory = 3,
    /// Regular file.
    RegularFile = 4,
    /// Symbolic link.
    SymbolicLink = 7,
}

/// An open file descriptor in the WASI context.
#[derive(Debug, Clone)]
pub struct WasiFd {
    /// The file descriptor number.
    pub fd: u32,
    /// The type of this fd.
    pub file_type: WasiFileType,
    /// Rights granted to this fd.
    pub rights: WasiRights,
    /// Optional host path this fd is bound to (for preopened dirs).
    pub host_path: Option<String>,
    /// Optional guest path this fd is bound to (for preopened dirs).
    pub guest_path: Option<String>,
    /// Internal buffer for reads/writes on virtual fds.
    pub buffer: Vec<u8>,
    /// Read cursor position in the buffer.
    pub cursor: usize,
}

impl WasiFd {
    /// Create a new file descriptor with the given properties.
    pub fn new(fd: u32, file_type: WasiFileType, rights: WasiRights) -> Self {
        Self {
            fd,
            file_type,
            rights,
            host_path: None,
            guest_path: None,
            buffer: Vec::new(),
            cursor: 0,
        }
    }

    /// Create a preopen directory fd.
    pub fn preopen_dir(fd: u32, guest_path: &str, host_path: &str) -> Self {
        Self {
            fd,
            file_type: WasiFileType::Directory,
            rights: WasiRights::PATH_OPEN
                .union(WasiRights::FD_READ)
                .union(WasiRights::FD_READDIR),
            host_path: Some(host_path.to_string()),
            guest_path: Some(guest_path.to_string()),
            buffer: Vec::new(),
            cursor: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// WASI function signatures (import table)
// ---------------------------------------------------------------------------

/// Describes a WASI preview1 function import.
///
/// These are the functions that a WASI-compliant host must provide when
/// instantiating a Lumen WASM module targeting `wasm32-wasi`.
#[derive(Debug, Clone)]
pub struct WasiImport {
    /// WASI module name (always "wasi_snapshot_preview1").
    pub module: &'static str,
    /// Function name.
    pub name: &'static str,
    /// Parameter types (WASM value types).
    pub params: &'static [WasmValType],
    /// Return types (WASM value types).
    pub returns: &'static [WasmValType],
    /// Human-readable description.
    pub description: &'static str,
}

/// WebAssembly value types for import/export signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmValType {
    I32,
    I64,
}

impl std::fmt::Display for WasmValType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmValType::I32 => write!(f, "i32"),
            WasmValType::I64 => write!(f, "i64"),
        }
    }
}

/// The complete set of WASI preview1 imports needed by Lumen programs.
pub static WASI_IMPORTS: &[WasiImport] = &[
    // --- Console I/O ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "fd_write",
        params: &[
            WasmValType::I32, // fd
            WasmValType::I32, // iovs (pointer to iovec array)
            WasmValType::I32, // iovs_len (number of iovecs)
            WasmValType::I32, // nwritten (pointer to output)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Write to a file descriptor (used for console output via stdout/stderr)",
    },
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "fd_read",
        params: &[
            WasmValType::I32, // fd
            WasmValType::I32, // iovs
            WasmValType::I32, // iovs_len
            WasmValType::I32, // nread (pointer to output)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Read from a file descriptor (used for console input via stdin)",
    },
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "fd_close",
        params: &[WasmValType::I32],  // fd
        returns: &[WasmValType::I32], // errno
        description: "Close a file descriptor",
    },
    // --- Clock ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "clock_time_get",
        params: &[
            WasmValType::I32, // clock_id
            WasmValType::I64, // precision
            WasmValType::I32, // time (pointer to output i64)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Get the current time from a clock (maps to Lumen's `timestamp` builtin)",
    },
    // --- Random ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "random_get",
        params: &[
            WasmValType::I32, // buf (pointer)
            WasmValType::I32, // buf_len
        ],
        returns: &[WasmValType::I32], // errno
        description: "Fill a buffer with random bytes (maps to Lumen's `random` builtin)",
    },
    // --- Filesystem ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "path_open",
        params: &[
            WasmValType::I32, // dirfd
            WasmValType::I32, // dirflags (lookupflags)
            WasmValType::I32, // path (pointer)
            WasmValType::I32, // path_len
            WasmValType::I32, // oflags
            WasmValType::I64, // fs_rights_base
            WasmValType::I64, // fs_rights_inheriting
            WasmValType::I32, // fdflags
            WasmValType::I32, // fd (pointer to output)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Open a file or directory (maps to Lumen's `read_file`/`write_file` builtins)",
    },
    // --- CLI arguments ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "args_get",
        params: &[
            WasmValType::I32, // argv (pointer to pointer array)
            WasmValType::I32, // argv_buf (pointer to string data)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Read command-line argument data",
    },
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "args_sizes_get",
        params: &[
            WasmValType::I32, // argc (pointer to output)
            WasmValType::I32, // argv_buf_size (pointer to output)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Get sizes of command-line argument data",
    },
    // --- Environment variables ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "environ_get",
        params: &[
            WasmValType::I32, // environ (pointer to pointer array)
            WasmValType::I32, // environ_buf (pointer to string data)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Read environment variable data (maps to Lumen's `get_env` builtin)",
    },
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "environ_sizes_get",
        params: &[
            WasmValType::I32, // environ_count (pointer to output)
            WasmValType::I32, // environ_buf_size (pointer to output)
        ],
        returns: &[WasmValType::I32], // errno
        description: "Get sizes of environment variable data",
    },
    // --- Process lifecycle ---
    WasiImport {
        module: "wasi_snapshot_preview1",
        name: "proc_exit",
        params: &[WasmValType::I32], // exit_code
        returns: &[],
        description: "Terminate the process with an exit code",
    },
];

// ---------------------------------------------------------------------------
// WasiConfig — builder for WasiContext
// ---------------------------------------------------------------------------

/// Configuration for constructing a [`WasiContext`].
#[derive(Debug, Clone, Default)]
pub struct WasiConfig {
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables.
    pub env_vars: HashMap<String, String>,
    /// Preopened directories: (guest_path, host_path).
    pub preopens: Vec<(String, String)>,
    /// Whether to allow stdin reads.
    pub allow_stdin: bool,
    /// Whether to allow stdout writes.
    pub allow_stdout: bool,
    /// Whether to allow stderr writes.
    pub allow_stderr: bool,
}

impl WasiConfig {
    /// Create a new default configuration.
    ///
    /// By default, stdout and stderr are allowed but stdin is not.
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            env_vars: HashMap::new(),
            preopens: Vec::new(),
            allow_stdin: false,
            allow_stdout: true,
            allow_stderr: true,
        }
    }

    /// Set the command-line arguments.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Add a preopened directory mapping.
    pub fn with_preopen(mut self, guest_path: &str, host_path: &str) -> Self {
        self.preopens
            .push((guest_path.to_string(), host_path.to_string()));
        self
    }

    /// Allow or disallow stdin reads.
    pub fn with_stdin(mut self, allow: bool) -> Self {
        self.allow_stdin = allow;
        self
    }
}

// ---------------------------------------------------------------------------
// WasiContext — runtime state
// ---------------------------------------------------------------------------

/// Runtime state for WASI host function execution.
///
/// Tracks open file descriptors, environment variables, command-line arguments,
/// and output buffers. This is the bridge between Lumen builtins and WASI
/// system calls.
#[derive(Debug)]
pub struct WasiContext {
    /// Open file descriptors (fd number -> descriptor).
    fds: HashMap<u32, WasiFd>,
    /// Next available file descriptor number.
    next_fd: u32,
    /// Command-line arguments.
    args: Vec<String>,
    /// Environment variables.
    env_vars: HashMap<String, String>,
    /// Captured stdout output.
    stdout_buf: Vec<u8>,
    /// Captured stderr output.
    stderr_buf: Vec<u8>,
    /// Exit code if proc_exit was called.
    exit_code: Option<u32>,
}

impl WasiContext {
    /// Create a new WASI context with default settings.
    ///
    /// Sets up stdin (fd 0), stdout (fd 1), and stderr (fd 2).
    pub fn new() -> Self {
        let mut ctx = Self {
            fds: HashMap::new(),
            next_fd: 3, // 0=stdin, 1=stdout, 2=stderr
            args: Vec::new(),
            env_vars: HashMap::new(),
            stdout_buf: Vec::new(),
            stderr_buf: Vec::new(),
            exit_code: None,
        };

        // Register standard file descriptors.
        ctx.fds.insert(
            0,
            WasiFd::new(0, WasiFileType::CharacterDevice, WasiRights::FD_READ),
        );
        ctx.fds.insert(
            1,
            WasiFd::new(1, WasiFileType::CharacterDevice, WasiRights::FD_WRITE),
        );
        ctx.fds.insert(
            2,
            WasiFd::new(2, WasiFileType::CharacterDevice, WasiRights::FD_WRITE),
        );

        ctx
    }

    /// Create a WASI context from a configuration.
    pub fn from_config(config: WasiConfig) -> Self {
        let mut ctx = Self::new();
        ctx.args = config.args;
        ctx.env_vars = config.env_vars;

        // Register preopened directories starting at fd 3.
        for (guest_path, host_path) in &config.preopens {
            let fd = ctx.next_fd;
            ctx.fds
                .insert(fd, WasiFd::preopen_dir(fd, guest_path, host_path));
            ctx.next_fd += 1;
        }

        ctx
    }

    // --- File descriptor operations ---

    /// Write data to a file descriptor.
    ///
    /// Bridges `fd_write` WASI call. For stdout/stderr, captures output
    /// in internal buffers.
    pub fn fd_write(&mut self, fd: u32, data: &[u8]) -> Result<usize, WasiErrno> {
        let fd_entry = self.fds.get(&fd).ok_or(WasiErrno::Badf)?;
        if !fd_entry.rights.contains(WasiRights::FD_WRITE) {
            return Err(WasiErrno::Acces);
        }

        match fd {
            1 => {
                self.stdout_buf.extend_from_slice(data);
                Ok(data.len())
            }
            2 => {
                self.stderr_buf.extend_from_slice(data);
                Ok(data.len())
            }
            _ => {
                // For other fds, append to their buffer.
                if let Some(fd_entry) = self.fds.get_mut(&fd) {
                    fd_entry.buffer.extend_from_slice(data);
                    Ok(data.len())
                } else {
                    Err(WasiErrno::Badf)
                }
            }
        }
    }

    /// Read data from a file descriptor.
    ///
    /// Bridges `fd_read` WASI call.
    pub fn fd_read(&mut self, fd: u32, buf: &mut [u8]) -> Result<usize, WasiErrno> {
        let fd_entry = self.fds.get(&fd).ok_or(WasiErrno::Badf)?;
        if !fd_entry.rights.contains(WasiRights::FD_READ) {
            return Err(WasiErrno::Acces);
        }

        let fd_entry = self.fds.get_mut(&fd).ok_or(WasiErrno::Badf)?;
        let available = fd_entry.buffer.len() - fd_entry.cursor;
        let to_read = buf.len().min(available);

        if to_read > 0 {
            buf[..to_read]
                .copy_from_slice(&fd_entry.buffer[fd_entry.cursor..fd_entry.cursor + to_read]);
            fd_entry.cursor += to_read;
        }

        Ok(to_read)
    }

    /// Close a file descriptor.
    pub fn fd_close(&mut self, fd: u32) -> Result<(), WasiErrno> {
        // Don't allow closing stdin/stdout/stderr.
        if fd < 3 {
            return Err(WasiErrno::Badf);
        }
        self.fds.remove(&fd).ok_or(WasiErrno::Badf)?;
        Ok(())
    }

    /// Open a file relative to a preopened directory.
    ///
    /// Returns the new file descriptor number.
    pub fn path_open(&mut self, dirfd: u32, path: &str) -> Result<u32, WasiErrno> {
        // Validate the directory fd exists and is a directory.
        let dir = self.fds.get(&dirfd).ok_or(WasiErrno::Badf)?;
        if dir.file_type != WasiFileType::Directory {
            return Err(WasiErrno::Notdir);
        }

        // Resolve host path.
        let host_base = dir.host_path.as_deref().ok_or(WasiErrno::Noent)?;
        let full_path = format!("{}/{}", host_base, path);

        let fd = self.next_fd;
        self.next_fd += 1;

        let mut new_fd = WasiFd::new(
            fd,
            WasiFileType::RegularFile,
            WasiRights::FD_READ.union(WasiRights::FD_WRITE),
        );
        new_fd.host_path = Some(full_path);
        self.fds.insert(fd, new_fd);

        Ok(fd)
    }

    // --- Clock operations ---

    /// Get the current time in nanoseconds.
    ///
    /// Bridges `clock_time_get` WASI call to Lumen's `timestamp` builtin.
    pub fn clock_time_get(&self, clock_id: WasiClockId) -> Result<u64, WasiErrno> {
        match clock_id {
            WasiClockId::Realtime | WasiClockId::Monotonic => {
                // Return a timestamp in nanoseconds.
                // On native targets we use std::time; on wasm32 this would
                // be provided by the host.
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| WasiErrno::Io)?;
                Ok(now.as_nanos() as u64)
            }
            _ => Err(WasiErrno::Inval),
        }
    }

    // --- Random ---

    /// Fill a buffer with random bytes.
    ///
    /// Bridges `random_get` WASI call to Lumen's `random` builtin.
    pub fn random_get(&self, buf: &mut [u8]) -> Result<(), WasiErrno> {
        // Simple deterministic "random" for testing; in a real WASI host
        // this would use the host's CSPRNG.
        for (i, byte) in buf.iter_mut().enumerate() {
            // LCG-based pseudo-random for deterministic testing
            let seed = (i as u64).wrapping_mul(6364136223846793005).wrapping_add(1);
            *byte = (seed >> 33) as u8;
        }
        Ok(())
    }

    // --- Args and environment ---

    /// Get the command-line arguments.
    pub fn args_get(&self) -> &[String] {
        &self.args
    }

    /// Get the total sizes of argument data.
    ///
    /// Returns (argc, total_buf_size) where total_buf_size is the sum of
    /// all argument string lengths plus null terminators.
    pub fn args_sizes_get(&self) -> (usize, usize) {
        let argc = self.args.len();
        let buf_size: usize = self.args.iter().map(|a| a.len() + 1).sum(); // +1 for null terminator
        (argc, buf_size)
    }

    /// Get the environment variables.
    pub fn environ_get(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// Get the total sizes of environment variable data.
    ///
    /// Returns (count, total_buf_size) where total_buf_size is the sum of
    /// all "KEY=VALUE\0" string lengths.
    pub fn environ_sizes_get(&self) -> (usize, usize) {
        let count = self.env_vars.len();
        let buf_size: usize = self
            .env_vars
            .iter()
            .map(|(k, v)| k.len() + 1 + v.len() + 1) // "KEY=VALUE\0"
            .sum();
        (count, buf_size)
    }

    // --- Process lifecycle ---

    /// Record a process exit.
    pub fn proc_exit(&mut self, code: u32) {
        self.exit_code = Some(code);
    }

    /// Check if the process has exited.
    pub fn exit_code(&self) -> Option<u32> {
        self.exit_code
    }

    // --- Output accessors ---

    /// Get captured stdout output as a string.
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout_buf).to_string()
    }

    /// Get captured stderr output as a string.
    pub fn stderr_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr_buf).to_string()
    }

    /// Get captured stdout output as raw bytes.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_buf
    }

    /// Get captured stderr output as raw bytes.
    pub fn stderr_bytes(&self) -> &[u8] {
        &self.stderr_buf
    }

    /// Get the file descriptor table (for inspection/debugging).
    pub fn fd_table(&self) -> &HashMap<u32, WasiFd> {
        &self.fds
    }

    /// Check if a file descriptor is open.
    pub fn is_fd_open(&self, fd: u32) -> bool {
        self.fds.contains_key(&fd)
    }
}

impl Default for WasiContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Builtin bridge: WASI <-> Lumen
// ---------------------------------------------------------------------------

/// Maps Lumen builtin function names to the WASI imports they require.
///
/// This is used during compilation to determine which WASI imports need
/// to be included in the generated WASM module.
pub fn builtins_to_wasi_imports(builtin_name: &str) -> &[&str] {
    match builtin_name {
        "print" => &["fd_write"],
        "read_file" => &["path_open", "fd_read", "fd_close"],
        "write_file" => &["path_open", "fd_write", "fd_close"],
        "timestamp" => &["clock_time_get"],
        "random" => &["random_get"],
        "get_env" => &["environ_get", "environ_sizes_get"],
        _ => &[],
    }
}

/// Collect all WASI imports required by a set of Lumen builtins.
pub fn collect_required_imports(builtin_names: &[&str]) -> Vec<&'static WasiImport> {
    let mut required_names: Vec<&str> = Vec::new();

    for name in builtin_names {
        for wasi_name in builtins_to_wasi_imports(name) {
            if !required_names.contains(wasi_name) {
                required_names.push(wasi_name);
            }
        }
    }

    // Always include proc_exit.
    if !required_names.contains(&"proc_exit") {
        required_names.push("proc_exit");
    }

    WASI_IMPORTS
        .iter()
        .filter(|imp| required_names.contains(&imp.name))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- WasiErrno tests ---------------------------------------------------

    #[test]
    fn errno_raw_values() {
        assert_eq!(WasiErrno::Success.raw(), 0);
        assert_eq!(WasiErrno::Badf.raw(), 8);
        assert_eq!(WasiErrno::Inval.raw(), 28);
        assert_eq!(WasiErrno::Noent.raw(), 44);
        assert_eq!(WasiErrno::Nosys.raw(), 52);
    }

    #[test]
    fn errno_display() {
        assert_eq!(format!("{}", WasiErrno::Success), "success");
        assert_eq!(format!("{}", WasiErrno::Badf), "bad file descriptor");
        assert_eq!(format!("{}", WasiErrno::Noent), "no such file or directory");
    }

    // -- WasiClockId tests -------------------------------------------------

    #[test]
    fn clock_id_from_raw() {
        assert_eq!(WasiClockId::from_raw(0), Some(WasiClockId::Realtime));
        assert_eq!(WasiClockId::from_raw(1), Some(WasiClockId::Monotonic));
        assert_eq!(WasiClockId::from_raw(2), Some(WasiClockId::ProcessCputime));
        assert_eq!(WasiClockId::from_raw(3), Some(WasiClockId::ThreadCputime));
        assert_eq!(WasiClockId::from_raw(99), None);
    }

    // -- WasiRights tests --------------------------------------------------

    #[test]
    fn rights_contains() {
        let rw = WasiRights::FD_READ.union(WasiRights::FD_WRITE);
        assert!(rw.contains(WasiRights::FD_READ));
        assert!(rw.contains(WasiRights::FD_WRITE));
        assert!(!rw.contains(WasiRights::PATH_OPEN));
    }

    // -- WasiConfig tests --------------------------------------------------

    #[test]
    fn config_builder() {
        let config = WasiConfig::new()
            .with_args(vec!["prog".into(), "--help".into()])
            .with_env("HOME", "/home/test")
            .with_env("PATH", "/usr/bin")
            .with_preopen("/data", "/host/data")
            .with_stdin(true);

        assert_eq!(config.args, vec!["prog", "--help"]);
        assert_eq!(config.env_vars.get("HOME"), Some(&"/home/test".to_string()));
        assert_eq!(config.env_vars.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(config.preopens.len(), 1);
        assert!(config.allow_stdin);
        assert!(config.allow_stdout);
        assert!(config.allow_stderr);
    }

    // -- WasiContext tests -------------------------------------------------

    #[test]
    fn context_default_fds() {
        let ctx = WasiContext::new();
        assert!(ctx.is_fd_open(0)); // stdin
        assert!(ctx.is_fd_open(1)); // stdout
        assert!(ctx.is_fd_open(2)); // stderr
        assert!(!ctx.is_fd_open(3));
    }

    #[test]
    fn context_from_config_preopens() {
        let config = WasiConfig::new()
            .with_preopen("/app", "/host/app")
            .with_preopen("/data", "/host/data");

        let ctx = WasiContext::from_config(config);
        assert!(ctx.is_fd_open(3)); // first preopen
        assert!(ctx.is_fd_open(4)); // second preopen

        let fd3 = &ctx.fd_table()[&3];
        assert_eq!(fd3.file_type, WasiFileType::Directory);
        assert_eq!(fd3.guest_path.as_deref(), Some("/app"));
        assert_eq!(fd3.host_path.as_deref(), Some("/host/app"));
    }

    #[test]
    fn fd_write_stdout() {
        let mut ctx = WasiContext::new();
        let written = ctx
            .fd_write(1, b"hello")
            .expect("stdout write should succeed");
        assert_eq!(written, 5);
        assert_eq!(ctx.stdout_string(), "hello");
    }

    #[test]
    fn fd_write_stderr() {
        let mut ctx = WasiContext::new();
        let written = ctx
            .fd_write(2, b"error!")
            .expect("stderr write should succeed");
        assert_eq!(written, 6);
        assert_eq!(ctx.stderr_string(), "error!");
    }

    #[test]
    fn fd_write_multiple() {
        let mut ctx = WasiContext::new();
        ctx.fd_write(1, b"hello ").unwrap();
        ctx.fd_write(1, b"world").unwrap();
        assert_eq!(ctx.stdout_string(), "hello world");
    }

    #[test]
    fn fd_write_bad_fd() {
        let mut ctx = WasiContext::new();
        let err = ctx.fd_write(99, b"data").unwrap_err();
        assert_eq!(err, WasiErrno::Badf);
    }

    #[test]
    fn fd_write_stdin_no_write_rights() {
        let mut ctx = WasiContext::new();
        let err = ctx.fd_write(0, b"data").unwrap_err();
        assert_eq!(err, WasiErrno::Acces);
    }

    #[test]
    fn fd_read_from_buffer() {
        let mut ctx = WasiContext::new();
        // Put some data in stdin's buffer
        ctx.fds.get_mut(&0).unwrap().buffer = b"input data".to_vec();

        let mut buf = [0u8; 5];
        let n = ctx.fd_read(0, &mut buf).expect("read should succeed");
        assert_eq!(n, 5);
        assert_eq!(&buf, b"input");

        // Read more
        let n = ctx.fd_read(0, &mut buf).expect("read should succeed");
        assert_eq!(n, 5);
        assert_eq!(&buf, b" data");

        // No more data
        let n = ctx.fd_read(0, &mut buf).expect("read should succeed");
        assert_eq!(n, 0);
    }

    #[test]
    fn fd_read_no_read_rights() {
        let mut ctx = WasiContext::new();
        let mut buf = [0u8; 5];
        let err = ctx.fd_read(1, &mut buf).unwrap_err(); // stdout has no read rights
        assert_eq!(err, WasiErrno::Acces);
    }

    #[test]
    fn fd_close_regular() {
        let mut ctx = WasiContext::new();
        // Open a file via path_open
        let config = WasiConfig::new().with_preopen("/", "/tmp");
        ctx = WasiContext::from_config(config);

        let fd = ctx.path_open(3, "test.txt").expect("path_open should work");
        assert!(ctx.is_fd_open(fd));

        ctx.fd_close(fd).expect("close should succeed");
        assert!(!ctx.is_fd_open(fd));
    }

    #[test]
    fn fd_close_stdio_rejected() {
        let mut ctx = WasiContext::new();
        assert_eq!(ctx.fd_close(0).unwrap_err(), WasiErrno::Badf);
        assert_eq!(ctx.fd_close(1).unwrap_err(), WasiErrno::Badf);
        assert_eq!(ctx.fd_close(2).unwrap_err(), WasiErrno::Badf);
    }

    #[test]
    fn path_open_success() {
        let config = WasiConfig::new().with_preopen("/data", "/host/data");
        let mut ctx = WasiContext::from_config(config);

        let fd = ctx.path_open(3, "file.txt").expect("path_open should work");
        assert!(ctx.is_fd_open(fd));

        let entry = &ctx.fd_table()[&fd];
        assert_eq!(entry.file_type, WasiFileType::RegularFile);
        assert_eq!(entry.host_path.as_deref(), Some("/host/data/file.txt"));
    }

    #[test]
    fn path_open_bad_dirfd() {
        let mut ctx = WasiContext::new();
        let err = ctx.path_open(99, "file.txt").unwrap_err();
        assert_eq!(err, WasiErrno::Badf);
    }

    #[test]
    fn path_open_not_directory() {
        let mut ctx = WasiContext::new();
        // fd 0 is stdin (CharacterDevice, not Directory)
        let err = ctx.path_open(0, "file.txt").unwrap_err();
        assert_eq!(err, WasiErrno::Notdir);
    }

    #[test]
    fn clock_time_get_realtime() {
        let ctx = WasiContext::new();
        let time = ctx
            .clock_time_get(WasiClockId::Realtime)
            .expect("clock should work");
        assert!(time > 0, "realtime clock should return non-zero");
    }

    #[test]
    fn clock_time_get_monotonic() {
        let ctx = WasiContext::new();
        let time = ctx
            .clock_time_get(WasiClockId::Monotonic)
            .expect("monotonic clock should work");
        assert!(time > 0, "monotonic clock should return non-zero");
    }

    #[test]
    fn random_get_fills_buffer() {
        let ctx = WasiContext::new();
        let mut buf = [0u8; 32];
        ctx.random_get(&mut buf).expect("random_get should work");
        // At least some bytes should be non-zero
        assert!(buf.iter().any(|&b| b != 0), "should produce non-zero bytes");
    }

    #[test]
    fn args_operations() {
        let config =
            WasiConfig::new().with_args(vec!["lumen".into(), "run".into(), "test.lm".into()]);
        let ctx = WasiContext::from_config(config);

        assert_eq!(ctx.args_get(), &["lumen", "run", "test.lm"]);

        let (argc, buf_size) = ctx.args_sizes_get();
        assert_eq!(argc, 3);
        // "lumen\0" (6) + "run\0" (4) + "test.lm\0" (8) = 18
        assert_eq!(buf_size, 18);
    }

    #[test]
    fn environ_operations() {
        let config = WasiConfig::new()
            .with_env("HOME", "/home/user")
            .with_env("LANG", "en_US");
        let ctx = WasiContext::from_config(config);

        let env = ctx.environ_get();
        assert_eq!(env.get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(env.get("LANG"), Some(&"en_US".to_string()));

        let (count, buf_size) = ctx.environ_sizes_get();
        assert_eq!(count, 2);
        // "HOME=/home/user\0" (16) + "LANG=en_US\0" (11) = 27
        assert_eq!(buf_size, 27);
    }

    #[test]
    fn proc_exit_records_code() {
        let mut ctx = WasiContext::new();
        assert_eq!(ctx.exit_code(), None);

        ctx.proc_exit(0);
        assert_eq!(ctx.exit_code(), Some(0));

        ctx.proc_exit(1);
        assert_eq!(ctx.exit_code(), Some(1));
    }

    // -- Import table tests ------------------------------------------------

    #[test]
    fn wasi_imports_correct_module() {
        for import in WASI_IMPORTS {
            assert_eq!(
                import.module, "wasi_snapshot_preview1",
                "all imports should be from wasi_snapshot_preview1"
            );
        }
    }

    #[test]
    fn wasi_imports_have_descriptions() {
        for import in WASI_IMPORTS {
            assert!(
                !import.description.is_empty(),
                "import {} should have a description",
                import.name
            );
        }
    }

    #[test]
    fn wasi_imports_all_return_errno_or_void() {
        for import in WASI_IMPORTS {
            // All WASI functions return either i32 (errno) or void
            assert!(
                import.returns.is_empty()
                    || (import.returns.len() == 1 && import.returns[0] == WasmValType::I32),
                "import {} should return errno (i32) or void, got {:?}",
                import.name,
                import.returns
            );
        }
    }

    #[test]
    fn wasi_imports_required_functions_present() {
        let names: Vec<&str> = WASI_IMPORTS.iter().map(|i| i.name).collect();
        assert!(names.contains(&"fd_write"), "should have fd_write");
        assert!(names.contains(&"fd_read"), "should have fd_read");
        assert!(names.contains(&"fd_close"), "should have fd_close");
        assert!(
            names.contains(&"clock_time_get"),
            "should have clock_time_get"
        );
        assert!(names.contains(&"random_get"), "should have random_get");
        assert!(names.contains(&"path_open"), "should have path_open");
        assert!(names.contains(&"args_get"), "should have args_get");
        assert!(
            names.contains(&"args_sizes_get"),
            "should have args_sizes_get"
        );
        assert!(names.contains(&"environ_get"), "should have environ_get");
        assert!(
            names.contains(&"environ_sizes_get"),
            "should have environ_sizes_get"
        );
        assert!(names.contains(&"proc_exit"), "should have proc_exit");
    }

    // -- Import type signature tests ---------------------------------------

    #[test]
    fn fd_write_signature() {
        let import = WASI_IMPORTS.iter().find(|i| i.name == "fd_write").unwrap();
        assert_eq!(import.params.len(), 4, "fd_write takes 4 params");
        assert!(
            import.params.iter().all(|t| *t == WasmValType::I32),
            "all fd_write params should be i32"
        );
        assert_eq!(import.returns, &[WasmValType::I32]);
    }

    #[test]
    fn fd_read_signature() {
        let import = WASI_IMPORTS.iter().find(|i| i.name == "fd_read").unwrap();
        assert_eq!(import.params.len(), 4, "fd_read takes 4 params");
        assert!(
            import.params.iter().all(|t| *t == WasmValType::I32),
            "all fd_read params should be i32"
        );
        assert_eq!(import.returns, &[WasmValType::I32]);
    }

    #[test]
    fn clock_time_get_signature() {
        let import = WASI_IMPORTS
            .iter()
            .find(|i| i.name == "clock_time_get")
            .unwrap();
        assert_eq!(import.params.len(), 3);
        assert_eq!(import.params[0], WasmValType::I32); // clock_id
        assert_eq!(import.params[1], WasmValType::I64); // precision
        assert_eq!(import.params[2], WasmValType::I32); // time ptr
        assert_eq!(import.returns, &[WasmValType::I32]);
    }

    #[test]
    fn random_get_signature() {
        let import = WASI_IMPORTS
            .iter()
            .find(|i| i.name == "random_get")
            .unwrap();
        assert_eq!(import.params.len(), 2);
        assert_eq!(import.params[0], WasmValType::I32); // buf ptr
        assert_eq!(import.params[1], WasmValType::I32); // buf_len
        assert_eq!(import.returns, &[WasmValType::I32]);
    }

    #[test]
    fn path_open_signature() {
        let import = WASI_IMPORTS.iter().find(|i| i.name == "path_open").unwrap();
        assert_eq!(import.params.len(), 9, "path_open takes 9 params");
        // params: i32, i32, i32, i32, i32, i64, i64, i32, i32
        assert_eq!(import.params[5], WasmValType::I64); // fs_rights_base
        assert_eq!(import.params[6], WasmValType::I64); // fs_rights_inheriting
        assert_eq!(import.returns, &[WasmValType::I32]);
    }

    #[test]
    fn proc_exit_signature() {
        let import = WASI_IMPORTS.iter().find(|i| i.name == "proc_exit").unwrap();
        assert_eq!(import.params.len(), 1);
        assert_eq!(import.params[0], WasmValType::I32); // exit_code
        assert!(import.returns.is_empty(), "proc_exit returns void");
    }

    // -- Builtin bridge tests ----------------------------------------------

    #[test]
    fn builtins_to_wasi_print() {
        let imports = builtins_to_wasi_imports("print");
        assert_eq!(imports, &["fd_write"]);
    }

    #[test]
    fn builtins_to_wasi_read_file() {
        let imports = builtins_to_wasi_imports("read_file");
        assert_eq!(imports, &["path_open", "fd_read", "fd_close"]);
    }

    #[test]
    fn builtins_to_wasi_timestamp() {
        let imports = builtins_to_wasi_imports("timestamp");
        assert_eq!(imports, &["clock_time_get"]);
    }

    #[test]
    fn builtins_to_wasi_unknown() {
        let imports = builtins_to_wasi_imports("unknown_builtin");
        assert!(imports.is_empty());
    }

    #[test]
    fn collect_required_imports_deduplicates() {
        let imports = collect_required_imports(&["print", "read_file", "write_file"]);
        // Should include fd_write, path_open, fd_read, fd_close, proc_exit
        // fd_write should appear only once despite being needed by both print and write_file
        let names: Vec<&str> = imports.iter().map(|i| i.name).collect();
        let fd_write_count = names.iter().filter(|&&n| n == "fd_write").count();
        assert_eq!(fd_write_count, 1, "fd_write should appear exactly once");
        assert!(
            names.contains(&"proc_exit"),
            "should always include proc_exit"
        );
    }

    #[test]
    fn collect_required_imports_empty() {
        let imports = collect_required_imports(&[]);
        // Should still include proc_exit
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].name, "proc_exit");
    }

    // -- WasmValType tests -------------------------------------------------

    #[test]
    fn wasm_val_type_display() {
        assert_eq!(format!("{}", WasmValType::I32), "i32");
        assert_eq!(format!("{}", WasmValType::I64), "i64");
    }

    // -- WasiFd tests ------------------------------------------------------

    #[test]
    fn wasi_fd_new() {
        let fd = WasiFd::new(5, WasiFileType::RegularFile, WasiRights::FD_READ);
        assert_eq!(fd.fd, 5);
        assert_eq!(fd.file_type, WasiFileType::RegularFile);
        assert!(fd.rights.contains(WasiRights::FD_READ));
        assert!(!fd.rights.contains(WasiRights::FD_WRITE));
        assert!(fd.host_path.is_none());
        assert!(fd.buffer.is_empty());
    }

    #[test]
    fn wasi_fd_preopen_dir() {
        let fd = WasiFd::preopen_dir(3, "/guest", "/host");
        assert_eq!(fd.fd, 3);
        assert_eq!(fd.file_type, WasiFileType::Directory);
        assert!(fd.rights.contains(WasiRights::PATH_OPEN));
        assert!(fd.rights.contains(WasiRights::FD_READ));
        assert_eq!(fd.guest_path.as_deref(), Some("/guest"));
        assert_eq!(fd.host_path.as_deref(), Some("/host"));
    }

    // -- Default impl test -------------------------------------------------

    #[test]
    fn context_default_impl() {
        let ctx = WasiContext::default();
        assert!(ctx.is_fd_open(0));
        assert!(ctx.is_fd_open(1));
        assert!(ctx.is_fd_open(2));
        assert_eq!(ctx.exit_code(), None);
    }
}
