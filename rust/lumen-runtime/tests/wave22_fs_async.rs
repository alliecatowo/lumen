//! Comprehensive tests for the `lumen_runtime::fs_async` module.

use lumen_runtime::fs_async::*;
use std::fs;
use std::path::Path;

/// Helper: create a fresh temporary directory for a test.
fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lumen_fs_async_test_{name}_{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).ok();
    }
    fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

// ---------------------------------------------------------------------------
// AsyncFileOp construction
// ---------------------------------------------------------------------------

#[test]
fn fs_async_op_read_construction() {
    let op = AsyncFileOp::Read {
        path: "/tmp/a.txt".into(),
    };
    assert_eq!(
        op,
        AsyncFileOp::Read {
            path: "/tmp/a.txt".into()
        }
    );
}

#[test]
fn fs_async_op_write_construction() {
    let op = AsyncFileOp::Write {
        path: "b.txt".into(),
        content: "hi".into(),
    };
    if let AsyncFileOp::Write { path, content } = &op {
        assert_eq!(path, "b.txt");
        assert_eq!(content, "hi");
    } else {
        panic!("expected Write");
    }
}

#[test]
fn fs_async_op_append_construction() {
    let op = AsyncFileOp::Append {
        path: "c.txt".into(),
        content: "more".into(),
    };
    assert!(matches!(op, AsyncFileOp::Append { .. }));
}

#[test]
fn fs_async_op_delete_construction() {
    let op = AsyncFileOp::Delete {
        path: "d.txt".into(),
    };
    assert!(matches!(op, AsyncFileOp::Delete { .. }));
}

#[test]
fn fs_async_op_copy_construction() {
    let op = AsyncFileOp::Copy {
        src: "a".into(),
        dst: "b".into(),
    };
    assert!(matches!(op, AsyncFileOp::Copy { .. }));
}

#[test]
fn fs_async_op_move_construction() {
    let op = AsyncFileOp::Move {
        src: "a".into(),
        dst: "b".into(),
    };
    assert!(matches!(op, AsyncFileOp::Move { .. }));
}

#[test]
fn fs_async_op_exists_construction() {
    let op = AsyncFileOp::Exists {
        path: "e.txt".into(),
    };
    assert!(matches!(op, AsyncFileOp::Exists { .. }));
}

#[test]
fn fs_async_op_metadata_construction() {
    let op = AsyncFileOp::Metadata {
        path: "f.txt".into(),
    };
    assert!(matches!(op, AsyncFileOp::Metadata { .. }));
}

// ---------------------------------------------------------------------------
// execute_batch: Write + Read
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_write_then_read() {
    let dir = tmp_dir("write_read");
    let file = dir.join("hello.txt");
    let file_str = file.to_str().unwrap().to_string();

    let batch = BatchFileOp {
        operations: vec![
            AsyncFileOp::Write {
                path: file_str.clone(),
                content: "hello world".into(),
            },
            AsyncFileOp::Read {
                path: file_str.clone(),
            },
        ],
        parallel: false,
    };
    let results = execute_batch(&batch);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0], AsyncFileResult::Success);
    assert_eq!(results[1], AsyncFileResult::Content("hello world".into()));

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// execute_batch: Delete
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_delete_existing() {
    let dir = tmp_dir("delete");
    let file = dir.join("gone.txt");
    fs::write(&file, "bye").unwrap();
    let file_str = file.to_str().unwrap().to_string();

    let batch = BatchFileOp {
        operations: vec![AsyncFileOp::Delete {
            path: file_str.clone(),
        }],
        parallel: false,
    };
    let results = execute_batch(&batch);
    assert_eq!(results, vec![AsyncFileResult::Success]);
    assert!(!file.exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fs_async_batch_delete_nonexistent() {
    let batch = BatchFileOp {
        operations: vec![AsyncFileOp::Delete {
            path: "/tmp/surely_no_such_file_999".into(),
        }],
        parallel: false,
    };
    let results = execute_batch(&batch);
    assert!(matches!(
        results[0],
        AsyncFileResult::Error(FsError::NotFound(_))
    ));
}

// ---------------------------------------------------------------------------
// execute_batch: Exists
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_exists_true() {
    let dir = tmp_dir("exists_t");
    let file = dir.join("present.txt");
    fs::write(&file, "").unwrap();
    let file_str = file.to_str().unwrap().to_string();

    let results = execute_batch(&BatchFileOp {
        operations: vec![AsyncFileOp::Exists { path: file_str }],
        parallel: false,
    });
    assert_eq!(results, vec![AsyncFileResult::Exists(true)]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fs_async_batch_exists_false() {
    let results = execute_batch(&BatchFileOp {
        operations: vec![AsyncFileOp::Exists {
            path: "/tmp/nope_nope_nope_999".into(),
        }],
        parallel: false,
    });
    assert_eq!(results, vec![AsyncFileResult::Exists(false)]);
}

// ---------------------------------------------------------------------------
// execute_batch: Copy
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_copy() {
    let dir = tmp_dir("copy");
    let src = dir.join("src.txt");
    let dst = dir.join("dst.txt");
    fs::write(&src, "copied").unwrap();

    let results = execute_batch(&BatchFileOp {
        operations: vec![AsyncFileOp::Copy {
            src: src.to_str().unwrap().into(),
            dst: dst.to_str().unwrap().into(),
        }],
        parallel: false,
    });
    assert_eq!(results, vec![AsyncFileResult::Success]);
    assert_eq!(fs::read_to_string(&dst).unwrap(), "copied");

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// execute_batch: Move
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_move() {
    let dir = tmp_dir("mv");
    let src = dir.join("orig.txt");
    let dst = dir.join("moved.txt");
    fs::write(&src, "data").unwrap();

    let results = execute_batch(&BatchFileOp {
        operations: vec![AsyncFileOp::Move {
            src: src.to_str().unwrap().into(),
            dst: dst.to_str().unwrap().into(),
        }],
        parallel: false,
    });
    assert_eq!(results, vec![AsyncFileResult::Success]);
    assert!(!src.exists());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "data");

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// execute_batch: Append
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_append() {
    let dir = tmp_dir("append");
    let file = dir.join("log.txt");
    fs::write(&file, "line1\n").unwrap();
    let file_str = file.to_str().unwrap().to_string();

    let results = execute_batch(&BatchFileOp {
        operations: vec![
            AsyncFileOp::Append {
                path: file_str.clone(),
                content: "line2\n".into(),
            },
            AsyncFileOp::Read { path: file_str },
        ],
        parallel: false,
    });
    assert_eq!(results[0], AsyncFileResult::Success);
    assert_eq!(
        results[1],
        AsyncFileResult::Content("line1\nline2\n".into())
    );

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// execute_batch: Metadata
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_metadata() {
    let dir = tmp_dir("meta");
    let file = dir.join("info.txt");
    fs::write(&file, "12345").unwrap();
    let file_str = file.to_str().unwrap().to_string();

    let results = execute_batch(&BatchFileOp {
        operations: vec![AsyncFileOp::Metadata { path: file_str }],
        parallel: false,
    });
    match &results[0] {
        AsyncFileResult::Metadata(m) => {
            assert_eq!(m.size, 5);
            assert!(m.is_file);
            assert!(!m.is_dir);
            assert!(!m.readonly);
            assert!(m.modified_ms.is_some());
        }
        other => panic!("expected Metadata, got {other:?}"),
    }

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// list_dir
// ---------------------------------------------------------------------------

#[test]
fn fs_async_list_dir_basic() {
    let dir = tmp_dir("listdir");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();

    let entries = list_dir(dir.to_str().unwrap()).unwrap();
    assert_eq!(entries.len(), 3);
    // sorted by name
    assert_eq!(entries[0].name, "a.txt");
    assert!(entries[0].is_file);
    assert_eq!(entries[1].name, "b.txt");
    assert_eq!(entries[2].name, "sub");
    assert!(entries[2].is_dir);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fs_async_list_dir_not_found() {
    let err = list_dir("/tmp/no_such_dir_fs_async_test_999").unwrap_err();
    assert!(matches!(err, FsError::NotFound(_)));
}

#[test]
fn fs_async_list_dir_not_directory() {
    let dir = tmp_dir("listdir_notdir");
    let file = dir.join("file.txt");
    fs::write(&file, "").unwrap();

    let err = list_dir(file.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, FsError::NotDirectory(_)));

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// list_dir_recursive
// ---------------------------------------------------------------------------

#[test]
fn fs_async_list_dir_recursive_depth() {
    let dir = tmp_dir("recursive");
    fs::write(dir.join("root.txt"), "").unwrap();
    fs::create_dir(dir.join("d1")).unwrap();
    fs::write(dir.join("d1").join("child.txt"), "").unwrap();
    fs::create_dir(dir.join("d1").join("d2")).unwrap();
    fs::write(dir.join("d1").join("d2").join("deep.txt"), "").unwrap();

    // depth 0 — only immediate children
    let entries = list_dir_recursive(dir.to_str().unwrap(), 0).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"root.txt"));
    assert!(names.contains(&"d1"));
    assert!(!names.contains(&"child.txt"));

    // depth 1 — includes d1's children
    let entries = list_dir_recursive(dir.to_str().unwrap(), 1).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"child.txt"));
    assert!(names.contains(&"d2"));
    assert!(!names.contains(&"deep.txt"));

    // depth 2 — includes everything
    let entries = list_dir_recursive(dir.to_str().unwrap(), 2).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"deep.txt"));

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// DirEntry fields
// ---------------------------------------------------------------------------

#[test]
fn fs_async_dir_entry_fields() {
    let dir = tmp_dir("direntry");
    fs::write(dir.join("sized.txt"), "abcde").unwrap();

    let entries = list_dir(dir.to_str().unwrap()).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.name, "sized.txt");
    assert!(e.path.ends_with("sized.txt"));
    assert!(e.is_file);
    assert!(!e.is_dir);
    assert_eq!(e.size, 5);

    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// FileWatcher
// ---------------------------------------------------------------------------

#[test]
fn fs_async_file_watcher_builder() {
    let w = FileWatcher::new(vec!["/tmp".into()])
        .recursive(true)
        .debounce(500);
    assert_eq!(w.paths, vec!["/tmp".to_string()]);
    assert!(w.recursive);
    assert_eq!(w.debounce_ms, 500);
}

#[test]
fn fs_async_file_watcher_defaults() {
    let w = FileWatcher::new(vec![]);
    assert!(!w.recursive);
    assert_eq!(w.debounce_ms, 100);
}

#[test]
fn fs_async_file_watcher_poll_empty() {
    let w = FileWatcher::new(vec!["/tmp".into()]);
    assert!(w.poll_events().is_empty());
}

// ---------------------------------------------------------------------------
// FileWatchEvent construction
// ---------------------------------------------------------------------------

#[test]
fn fs_async_file_watch_event_variants() {
    let _c = FileWatchEvent::Created("a".into());
    let _m = FileWatchEvent::Modified("b".into());
    let _d = FileWatchEvent::Deleted("c".into());
    let _r = FileWatchEvent::Renamed {
        from: "x".into(),
        to: "y".into(),
    };
    // just ensure they compile and are Debug-printable
    assert_eq!(format!("{_c:?}"), "Created(\"a\")");
}

// ---------------------------------------------------------------------------
// Path utilities
// ---------------------------------------------------------------------------

#[test]
fn fs_async_normalize_path_dots() {
    assert_eq!(normalize_path("a/./b"), "a/b");
    assert_eq!(normalize_path("a/b/../c"), "a/c");
    assert_eq!(normalize_path("./a/b"), "a/b");
}

#[test]
fn fs_async_normalize_path_multiple_parents() {
    assert_eq!(normalize_path("a/b/c/../../d"), "a/d");
}

#[test]
fn fs_async_normalize_path_only_dot() {
    assert_eq!(normalize_path("."), ".");
}

#[test]
fn fs_async_join_paths_basic() {
    let joined = join_paths("/home/user", "docs/file.txt");
    assert_eq!(joined, "/home/user/docs/file.txt");
}

#[test]
fn fs_async_join_paths_absolute_relative_overrides() {
    // When the second path is absolute it replaces the base (std::path semantics).
    let joined = join_paths("/home", "/etc/passwd");
    assert_eq!(joined, "/etc/passwd");
}

#[test]
fn fs_async_file_extension_basic() {
    assert_eq!(file_extension("photo.jpg"), Some("jpg".into()));
    assert_eq!(file_extension("archive.tar.gz"), Some("gz".into()));
}

#[test]
fn fs_async_file_extension_none() {
    assert_eq!(file_extension("Makefile"), None);
    assert_eq!(file_extension("/path/to/no_ext"), None);
}

#[test]
fn fs_async_file_extension_hidden() {
    // ".gitignore" — stem is ".gitignore", extension is None per std::path
    assert_eq!(file_extension(".gitignore"), None);
}

#[test]
fn fs_async_file_stem_basic() {
    assert_eq!(file_stem("report.pdf"), Some("report".into()));
    assert_eq!(file_stem("archive.tar.gz"), Some("archive.tar".into()));
}

#[test]
fn fs_async_file_stem_no_extension() {
    assert_eq!(file_stem("Makefile"), Some("Makefile".into()));
}

// ---------------------------------------------------------------------------
// FsError Display
// ---------------------------------------------------------------------------

#[test]
fn fs_async_error_display() {
    assert_eq!(FsError::NotFound("/x".into()).to_string(), "not found: /x");
    assert_eq!(
        FsError::PermissionDenied("/y".into()).to_string(),
        "permission denied: /y"
    );
    assert_eq!(
        FsError::AlreadyExists("/z".into()).to_string(),
        "already exists: /z"
    );
    assert_eq!(
        FsError::IsDirectory("/d".into()).to_string(),
        "is a directory: /d"
    );
    assert_eq!(
        FsError::NotDirectory("/f".into()).to_string(),
        "not a directory: /f"
    );
    assert_eq!(
        FsError::IoError("boom".into()).to_string(),
        "io error: boom"
    );
    assert_eq!(
        FsError::PathError("bad".into()).to_string(),
        "path error: bad"
    );
}

// ---------------------------------------------------------------------------
// From<std::io::Error>
// ---------------------------------------------------------------------------

#[test]
fn fs_async_from_io_error_not_found() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    let fs_err: FsError = io_err.into();
    assert!(matches!(fs_err, FsError::NotFound(_)));
}

#[test]
fn fs_async_from_io_error_permission() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
    let fs_err: FsError = io_err.into();
    assert!(matches!(fs_err, FsError::PermissionDenied(_)));
}

#[test]
fn fs_async_from_io_error_already_exists() {
    let io_err = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "dup");
    let fs_err: FsError = io_err.into();
    assert!(matches!(fs_err, FsError::AlreadyExists(_)));
}

#[test]
fn fs_async_from_io_error_generic() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "misc");
    let fs_err: FsError = io_err.into();
    assert!(matches!(fs_err, FsError::IoError(_)));
}

// ---------------------------------------------------------------------------
// FileMetadata construction
// ---------------------------------------------------------------------------

#[test]
fn fs_async_file_metadata_construction() {
    let m = FileMetadata {
        size: 1024,
        is_file: true,
        is_dir: false,
        readonly: false,
        modified_ms: Some(1700000000000),
        created_ms: None,
    };
    assert_eq!(m.size, 1024);
    assert!(m.is_file);
    assert!(!m.is_dir);
    assert!(!m.readonly);
    assert_eq!(m.modified_ms, Some(1700000000000));
    assert_eq!(m.created_ms, None);
}

// ---------------------------------------------------------------------------
// Batch parallel flag
// ---------------------------------------------------------------------------

#[test]
fn fs_async_batch_parallel_flag() {
    let batch = BatchFileOp {
        operations: vec![],
        parallel: true,
    };
    assert!(batch.parallel);
    let results = execute_batch(&batch);
    assert!(results.is_empty());
}
