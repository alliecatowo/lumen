//! Async file I/O abstractions for the Lumen runtime.
//!
//! This module provides typed file-system primitives — async-style file
//! operations, batch execution, directory listing, file watching (design-only),
//! and path utilities.  Actual I/O is performed through `std::fs`; the "async"
//! qualifier refers to the API shape that can later be wired to a true async
//! executor (e.g. tokio) without changing call-site code.

use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// FsError
// ---------------------------------------------------------------------------

/// Errors that can occur during file-system operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The target path was not found.
    NotFound(String),
    /// Insufficient permissions.
    PermissionDenied(String),
    /// The target already exists.
    AlreadyExists(String),
    /// Expected a file but found a directory.
    IsDirectory(String),
    /// Expected a directory but found a file.
    NotDirectory(String),
    /// Generic I/O error.
    IoError(String),
    /// Invalid or malformed path.
    PathError(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound(p) => write!(f, "not found: {p}"),
            FsError::PermissionDenied(p) => write!(f, "permission denied: {p}"),
            FsError::AlreadyExists(p) => write!(f, "already exists: {p}"),
            FsError::IsDirectory(p) => write!(f, "is a directory: {p}"),
            FsError::NotDirectory(p) => write!(f, "not a directory: {p}"),
            FsError::IoError(msg) => write!(f, "io error: {msg}"),
            FsError::PathError(msg) => write!(f, "path error: {msg}"),
        }
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => FsError::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(err.to_string()),
            std::io::ErrorKind::AlreadyExists => FsError::AlreadyExists(err.to_string()),
            _ => FsError::IoError(err.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// FileMetadata
// ---------------------------------------------------------------------------

/// Metadata about a file-system entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// Size in bytes.
    pub size: u64,
    /// `true` when the entry is a regular file.
    pub is_file: bool,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// `true` when the entry is read-only.
    pub readonly: bool,
    /// Last modification time as milliseconds since the Unix epoch, if available.
    pub modified_ms: Option<u64>,
    /// Creation time as milliseconds since the Unix epoch, if available.
    pub created_ms: Option<u64>,
}

impl FileMetadata {
    /// Build a `FileMetadata` from `std::fs::Metadata`.
    fn from_std(m: &std::fs::Metadata) -> Self {
        let modified_ms = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        let created_ms = m
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        FileMetadata {
            size: m.len(),
            is_file: m.is_file(),
            is_dir: m.is_dir(),
            readonly: m.permissions().readonly(),
            modified_ms,
            created_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// AsyncFileOp / AsyncFileResult
// ---------------------------------------------------------------------------

/// A single file-system operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncFileOp {
    /// Read the entire file as a UTF-8 string.
    Read { path: String },
    /// Write (create/overwrite) a file with the given content.
    Write { path: String, content: String },
    /// Append content to an existing file (or create it).
    Append { path: String, content: String },
    /// Delete a file.
    Delete { path: String },
    /// Copy a file from `src` to `dst`.
    Copy { src: String, dst: String },
    /// Move (rename) a file from `src` to `dst`.
    Move { src: String, dst: String },
    /// Check whether a path exists.
    Exists { path: String },
    /// Retrieve metadata for a path.
    Metadata { path: String },
}

/// The result of executing an [`AsyncFileOp`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncFileResult {
    /// File content returned by a `Read` operation.
    Content(String),
    /// The operation completed successfully (Write, Append, Delete, Copy, Move).
    Success,
    /// Whether the path exists (returned by `Exists`).
    Exists(bool),
    /// File metadata (returned by `Metadata`).
    Metadata(FileMetadata),
    /// An error occurred.
    Error(FsError),
}

/// Execute a single [`AsyncFileOp`] and return its result.
fn execute_op(op: &AsyncFileOp) -> AsyncFileResult {
    match op {
        AsyncFileOp::Read { path } => match std::fs::read_to_string(path) {
            Ok(content) => AsyncFileResult::Content(content),
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
        AsyncFileOp::Write { path, content } => match std::fs::write(path, content) {
            Ok(()) => AsyncFileResult::Success,
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
        AsyncFileOp::Append { path, content } => {
            use std::io::Write;
            let result = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(content.as_bytes()));
            match result {
                Ok(()) => AsyncFileResult::Success,
                Err(e) => AsyncFileResult::Error(FsError::from(e)),
            }
        }
        AsyncFileOp::Delete { path } => match std::fs::remove_file(path) {
            Ok(()) => AsyncFileResult::Success,
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
        AsyncFileOp::Copy { src, dst } => match std::fs::copy(src, dst) {
            Ok(_) => AsyncFileResult::Success,
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
        AsyncFileOp::Move { src, dst } => match std::fs::rename(src, dst) {
            Ok(()) => AsyncFileResult::Success,
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
        AsyncFileOp::Exists { path } => AsyncFileResult::Exists(Path::new(path).exists()),
        AsyncFileOp::Metadata { path } => match std::fs::metadata(path) {
            Ok(m) => AsyncFileResult::Metadata(FileMetadata::from_std(&m)),
            Err(e) => AsyncFileResult::Error(FsError::from(e)),
        },
    }
}

// ---------------------------------------------------------------------------
// BatchFileOp
// ---------------------------------------------------------------------------

/// A batch of file-system operations that can be executed together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchFileOp {
    /// The operations to execute.
    pub operations: Vec<AsyncFileOp>,
    /// If `true`, operations *may* be executed in parallel in the future.
    /// Currently all operations are executed sequentially.
    pub parallel: bool,
}

/// Execute every operation in a [`BatchFileOp`] and collect the results.
///
/// Operations are always executed sequentially in the current implementation
/// regardless of the `parallel` flag.
pub fn execute_batch(batch: &BatchFileOp) -> Vec<AsyncFileResult> {
    batch.operations.iter().map(execute_op).collect()
}

// ---------------------------------------------------------------------------
// DirEntry / directory listing
// ---------------------------------------------------------------------------

/// A single entry inside a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The entry name (file or directory name only, no path prefix).
    pub name: String,
    /// The full path to the entry.
    pub path: String,
    /// `true` when the entry is a regular file.
    pub is_file: bool,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
}

fn dir_entry_from_std(entry: &std::fs::DirEntry) -> Result<DirEntry, FsError> {
    let meta = entry.metadata().map_err(FsError::from)?;
    let name = entry.file_name().to_str().unwrap_or("").to_string();
    let path = entry.path().to_str().unwrap_or("").to_string();
    Ok(DirEntry {
        name,
        path,
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        size: meta.len(),
    })
}

/// List the immediate children of a directory.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, FsError> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(FsError::NotFound(path.to_string()));
    }
    if !p.is_dir() {
        return Err(FsError::NotDirectory(path.to_string()));
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(p).map_err(FsError::from)? {
        let entry = entry.map_err(FsError::from)?;
        entries.push(dir_entry_from_std(&entry)?);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Recursively list directory contents up to `max_depth` levels deep.
///
/// A `max_depth` of `0` lists only the immediate children (equivalent to
/// [`list_dir`]).  A depth of `1` includes children of subdirectories, etc.
pub fn list_dir_recursive(path: &str, max_depth: usize) -> Result<Vec<DirEntry>, FsError> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(FsError::NotFound(path.to_string()));
    }
    if !p.is_dir() {
        return Err(FsError::NotDirectory(path.to_string()));
    }
    let mut entries = Vec::new();
    collect_dir_recursive(p, max_depth, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn collect_dir_recursive(
    dir: &Path,
    depth_remaining: usize,
    out: &mut Vec<DirEntry>,
) -> Result<(), FsError> {
    for entry in std::fs::read_dir(dir).map_err(FsError::from)? {
        let entry = entry.map_err(FsError::from)?;
        let de = dir_entry_from_std(&entry)?;
        let is_dir = de.is_dir;
        out.push(de);
        if is_dir && depth_remaining > 0 {
            collect_dir_recursive(&entry.path(), depth_remaining - 1, out)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// FileWatcher (design stub)
// ---------------------------------------------------------------------------

/// Events that a file watcher can report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWatchEvent {
    /// A new file was created at the given path.
    Created(String),
    /// An existing file was modified.
    Modified(String),
    /// A file was deleted.
    Deleted(String),
    /// A file was renamed from one path to another.
    Renamed { from: String, to: String },
}

/// A builder / handle for watching file-system changes.
///
/// This is currently a **design stub** — [`FileWatcher::poll_events`] always
/// returns an empty vector.  A full implementation would use OS-level
/// notification APIs (inotify, kqueue, ReadDirectoryChangesW).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWatcher {
    /// The paths being watched.
    pub paths: Vec<String>,
    /// Whether subdirectories are watched recursively.
    pub recursive: bool,
    /// Debounce interval in milliseconds.
    pub debounce_ms: u64,
}

impl FileWatcher {
    /// Create a new `FileWatcher` for the given paths.
    pub fn new(paths: Vec<String>) -> Self {
        FileWatcher {
            paths,
            recursive: false,
            debounce_ms: 100,
        }
    }

    /// Enable or disable recursive watching.
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set the debounce interval in milliseconds.
    pub fn debounce(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Poll for pending file-system events.
    ///
    /// **Stub implementation** — always returns an empty vector.
    pub fn poll_events(&self) -> Vec<FileWatchEvent> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Path utilities
// ---------------------------------------------------------------------------

/// Normalize a path by resolving `.` and `..` components and converting
/// separators to the platform default.
///
/// This is a purely lexical operation and does **not** hit the file system.
pub fn normalize_path(path: &str) -> String {
    let p = PathBuf::from(path);
    let mut components: Vec<std::path::Component<'_>> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => { /* skip `.` */ }
            std::path::Component::ParentDir => {
                // Pop the last normal component if there is one; otherwise keep `..`
                if let Some(last) = components.last() {
                    if matches!(last, std::path::Component::Normal(_)) {
                        components.pop();
                        continue;
                    }
                }
                components.push(c);
            }
            _ => components.push(c),
        }
    }
    if components.is_empty() {
        return ".".to_string();
    }
    let result: PathBuf = components.iter().collect();
    result.to_string_lossy().to_string()
}

/// Join a `base` path with a `relative` path.
pub fn join_paths(base: &str, relative: &str) -> String {
    let joined = Path::new(base).join(relative);
    joined.to_string_lossy().to_string()
}

/// Return the file extension (without the leading dot), if any.
pub fn file_extension(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string())
}

/// Return the file stem (file name without its final extension), if any.
pub fn file_stem(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}
