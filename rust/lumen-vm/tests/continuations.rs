//! Wave-21 tests: T162 — Multi-shot continuations infrastructure.
//!
//! Tests for `lumen_vm::vm::continuations` module covering all public types
//! and their invariants.

use lumen_vm::vm::continuations::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal `ContinuationSnapshot` for testing.
fn make_snapshot(ip: usize, cell: usize) -> ContinuationSnapshot {
    ContinuationSnapshot {
        frames: vec![SavedFrame {
            cell_index: cell,
            ip,
            base_reg: 0,
            locals: vec![
                ("x".to_string(), SavedValue::Int(42)),
                ("flag".to_string(), SavedValue::Bool(true)),
            ],
        }],
        registers: vec![
            SavedRegister {
                index: 0,
                value: SavedValue::Int(1),
            },
            SavedRegister {
                index: 1,
                value: SavedValue::Str("hello".to_string()),
            },
        ],
        handler_stack_depth: 1,
        resume_point_ip: ip,
        resume_point_cell: cell,
    }
}

/// Build a snapshot with multiple frames.
fn make_multi_frame_snapshot() -> ContinuationSnapshot {
    ContinuationSnapshot {
        frames: vec![
            SavedFrame {
                cell_index: 0,
                ip: 10,
                base_reg: 0,
                locals: vec![("a".to_string(), SavedValue::Int(1))],
            },
            SavedFrame {
                cell_index: 1,
                ip: 20,
                base_reg: 8,
                locals: vec![("b".to_string(), SavedValue::Float(3.14))],
            },
            SavedFrame {
                cell_index: 2,
                ip: 30,
                base_reg: 16,
                locals: vec![],
            },
        ],
        registers: vec![
            SavedRegister {
                index: 0,
                value: SavedValue::Null,
            },
            SavedRegister {
                index: 5,
                value: SavedValue::List(vec![SavedValue::Int(10), SavedValue::Int(20)]),
            },
        ],
        handler_stack_depth: 2,
        resume_point_ip: 30,
        resume_point_cell: 2,
    }
}

// ===========================================================================
// ContinuationMode
// ===========================================================================

#[test]
fn continuation_mode_variants_exist() {
    let _one = ContinuationMode::OneShot;
    let _multi = ContinuationMode::MultiShot;
    assert_ne!(ContinuationMode::OneShot, ContinuationMode::MultiShot);
}

#[test]
fn continuation_mode_is_copy_and_clone() {
    let mode = ContinuationMode::MultiShot;
    let mode2 = mode; // Copy
    let mode3 = mode.clone(); // Clone
    assert_eq!(mode2, mode3);
}

// ===========================================================================
// SavedValue
// ===========================================================================

#[test]
fn saved_value_int_round_trip() {
    let v = SavedValue::Int(42);
    let cloned = v.clone();
    assert_eq!(v, cloned);
    assert_eq!(format!("{}", v), "42");
}

#[test]
fn saved_value_float_round_trip() {
    let v = SavedValue::Float(3.14);
    let cloned = v.clone();
    assert_eq!(v, cloned);
}

#[test]
fn saved_value_bool_round_trip() {
    let v = SavedValue::Bool(false);
    assert_eq!(format!("{}", v), "false");
    assert_eq!(v.clone(), SavedValue::Bool(false));
}

#[test]
fn saved_value_str_round_trip() {
    let v = SavedValue::Str("hello world".to_string());
    assert_eq!(format!("{}", v), "\"hello world\"");
    assert_eq!(v.clone(), SavedValue::Str("hello world".to_string()));
}

#[test]
fn saved_value_null_round_trip() {
    let v = SavedValue::Null;
    assert_eq!(format!("{}", v), "null");
    assert_eq!(v.clone(), SavedValue::Null);
}

#[test]
fn saved_value_list_round_trip() {
    let v = SavedValue::List(vec![
        SavedValue::Int(1),
        SavedValue::Str("two".to_string()),
        SavedValue::Null,
    ]);
    let cloned = v.clone();
    assert_eq!(v, cloned);
    assert_eq!(format!("{}", v), "[1, \"two\", null]");
}

#[test]
fn saved_value_map_round_trip() {
    let v = SavedValue::Map(vec![
        ("key".to_string(), SavedValue::Int(100)),
        ("nested".to_string(), SavedValue::Bool(true)),
    ]);
    let cloned = v.clone();
    assert_eq!(v, cloned);
    assert_eq!(format!("{}", v), "{\"key\": 100, \"nested\": true}");
}

#[test]
fn saved_value_opaque_round_trip() {
    let v = SavedValue::Opaque("Closure<add>".to_string());
    assert_eq!(format!("{}", v), "<opaque: Closure<add>>");
    assert_eq!(v.clone(), SavedValue::Opaque("Closure<add>".to_string()));
}

#[test]
fn saved_value_nested_list_and_map() {
    let v = SavedValue::List(vec![
        SavedValue::Map(vec![("a".to_string(), SavedValue::Int(1))]),
        SavedValue::List(vec![SavedValue::Bool(true)]),
    ]);
    let cloned = v.clone();
    assert_eq!(v, cloned);
}

// ===========================================================================
// SavedFrame
// ===========================================================================

#[test]
fn saved_frame_construction_and_cloning() {
    let frame = SavedFrame {
        cell_index: 3,
        ip: 42,
        base_reg: 16,
        locals: vec![
            ("x".to_string(), SavedValue::Int(10)),
            ("name".to_string(), SavedValue::Str("test".to_string())),
        ],
    };
    let cloned = frame.clone();
    assert_eq!(frame.cell_index, cloned.cell_index);
    assert_eq!(frame.ip, cloned.ip);
    assert_eq!(frame.base_reg, cloned.base_reg);
    assert_eq!(frame.locals.len(), 2);
    assert_eq!(frame, cloned);
}

#[test]
fn saved_frame_empty_locals() {
    let frame = SavedFrame {
        cell_index: 0,
        ip: 0,
        base_reg: 0,
        locals: vec![],
    };
    assert!(frame.locals.is_empty());
    let cloned = frame.clone();
    assert_eq!(frame, cloned);
}

// ===========================================================================
// SavedRegister
// ===========================================================================

#[test]
fn saved_register_construction_and_cloning() {
    let reg = SavedRegister {
        index: 7,
        value: SavedValue::Float(2.718),
    };
    let cloned = reg.clone();
    assert_eq!(reg.index, 7);
    assert_eq!(reg, cloned);
}

// ===========================================================================
// ContinuationSnapshot
// ===========================================================================

#[test]
fn continuation_snapshot_clone_for_resume_produces_independent_copy() {
    let original = make_snapshot(10, 0);
    let mut cloned = original.clone_for_resume();

    // Mutate the clone — original must not be affected.
    cloned.resume_point_ip = 999;
    cloned.frames[0].ip = 999;
    cloned.registers[0] = SavedRegister {
        index: 0,
        value: SavedValue::Bool(false),
    };

    assert_eq!(original.resume_point_ip, 10);
    assert_eq!(original.frames[0].ip, 10);
    assert_eq!(original.registers[0].value, SavedValue::Int(1));
}

#[test]
fn continuation_snapshot_clone_for_resume_multi_frame() {
    let original = make_multi_frame_snapshot();
    let cloned = original.clone_for_resume();
    assert_eq!(original.frames.len(), cloned.frames.len());
    assert_eq!(original.registers.len(), cloned.registers.len());
    assert_eq!(original, cloned);
}

#[test]
fn continuation_snapshot_debug_impl() {
    let snap = make_snapshot(5, 1);
    let debug = format!("{:?}", snap);
    assert!(debug.contains("ContinuationSnapshot"));
    assert!(debug.contains("resume_point_ip: 5"));
}

// ===========================================================================
// ContinuationState — OneShot
// ===========================================================================

#[test]
fn continuation_state_oneshot_first_resume_succeeds() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::OneShot);

    assert!(state.can_resume());
    assert_eq!(state.resume_count(), 0);
    assert_eq!(state.mode(), ContinuationMode::OneShot);

    let result = state.prepare_resume();
    assert!(result.is_ok());
    assert_eq!(state.resume_count(), 1);
}

#[test]
fn continuation_state_oneshot_second_resume_fails() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::OneShot);

    let _ = state.prepare_resume().unwrap();
    assert!(!state.can_resume());

    let err = state.prepare_resume().unwrap_err();
    assert_eq!(err, ContinuationError::AlreadyResumed);
}

#[test]
fn continuation_state_oneshot_consumes_snapshot() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::OneShot);

    let resumed = state.prepare_resume().unwrap();
    assert_eq!(resumed.resume_point_ip, 10);

    // The internal snapshot is consumed.
    assert!(!state.can_resume());
}

// ===========================================================================
// ContinuationState — MultiShot
// ===========================================================================

#[test]
fn continuation_state_multishot_multiple_resumes_succeed() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::MultiShot);

    for i in 0..5 {
        assert!(state.can_resume());
        let s = state.prepare_resume().unwrap();
        assert_eq!(s.resume_point_ip, 10);
        assert_eq!(state.resume_count(), i + 1);
    }
}

#[test]
fn continuation_state_multishot_clones_are_independent() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::MultiShot);

    let s1 = state.prepare_resume().unwrap();
    let s2 = state.prepare_resume().unwrap();

    // Both should be equal to the original snapshot content.
    assert_eq!(s1.resume_point_ip, s2.resume_point_ip);
    assert_eq!(s1.frames.len(), s2.frames.len());
    assert_eq!(s1, s2);
}

#[test]
fn continuation_state_multishot_with_max_resumes() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::with_max_resumes(snap, 3);

    assert_eq!(state.max_resumes(), Some(3));
    assert_eq!(state.mode(), ContinuationMode::MultiShot);

    // 3 resumes should succeed.
    for _ in 0..3 {
        assert!(state.can_resume());
        state.prepare_resume().unwrap();
    }

    // 4th should fail.
    assert!(!state.can_resume());
    let err = state.prepare_resume().unwrap_err();
    assert_eq!(err, ContinuationError::MaxResumesExceeded(3));
}

#[test]
fn continuation_state_multishot_unlimited_resumes() {
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::MultiShot);

    assert_eq!(state.max_resumes(), None);

    // Should be able to resume many times.
    for _ in 0..100 {
        assert!(state.can_resume());
        state.prepare_resume().unwrap();
    }
    assert_eq!(state.resume_count(), 100);
}

// ===========================================================================
// ContinuationError
// ===========================================================================

#[test]
fn continuation_error_display_already_resumed() {
    let err = ContinuationError::AlreadyResumed;
    assert_eq!(
        format!("{}", err),
        "continuation already resumed (one-shot)"
    );
}

#[test]
fn continuation_error_display_max_resumes_exceeded() {
    let err = ContinuationError::MaxResumesExceeded(5);
    assert_eq!(
        format!("{}", err),
        "continuation exceeded maximum resume count of 5"
    );
}

#[test]
fn continuation_error_display_invalid_state() {
    let err = ContinuationError::InvalidState("snapshot missing".to_string());
    assert_eq!(
        format!("{}", err),
        "continuation in invalid state: snapshot missing"
    );
}

#[test]
fn continuation_error_is_std_error() {
    let err = ContinuationError::AlreadyResumed;
    // Confirm it implements std::error::Error by calling source().
    let _: &dyn std::error::Error = &err;
    assert!(err.source().is_none());
}

// ===========================================================================
// MultiShotScheduler
// ===========================================================================

#[test]
fn continuation_scheduler_new_starts_empty() {
    let sched = MultiShotScheduler::new();
    assert!(sched.is_complete());
    assert_eq!(sched.pending_count(), 0);
    assert_eq!(sched.result_count(), 0);
    assert!(sched.results().is_empty());
}

#[test]
fn continuation_scheduler_default_starts_empty() {
    let sched = MultiShotScheduler::default();
    assert!(sched.is_complete());
}

#[test]
fn continuation_scheduler_enqueue_and_dequeue_fifo() {
    let mut sched = MultiShotScheduler::new();

    let snap1 = make_snapshot(10, 0);
    let snap2 = make_snapshot(20, 1);
    let snap3 = make_snapshot(30, 2);

    sched.enqueue(snap1, SavedValue::Int(1));
    sched.enqueue(snap2, SavedValue::Int(2));
    sched.enqueue(snap3, SavedValue::Int(3));

    assert!(!sched.is_complete());
    assert_eq!(sched.pending_count(), 3);

    // FIFO order.
    let (s1, v1) = sched.dequeue().unwrap();
    assert_eq!(s1.resume_point_ip, 10);
    assert_eq!(v1, SavedValue::Int(1));

    let (s2, v2) = sched.dequeue().unwrap();
    assert_eq!(s2.resume_point_ip, 20);
    assert_eq!(v2, SavedValue::Int(2));

    let (s3, v3) = sched.dequeue().unwrap();
    assert_eq!(s3.resume_point_ip, 30);
    assert_eq!(v3, SavedValue::Int(3));

    assert!(sched.dequeue().is_none());
    assert!(sched.is_complete());
}

#[test]
fn continuation_scheduler_add_result_and_results() {
    let mut sched = MultiShotScheduler::new();

    sched.add_result(SavedValue::Int(11));
    sched.add_result(SavedValue::Int(21));
    sched.add_result(SavedValue::Int(12));

    assert_eq!(sched.result_count(), 3);
    assert_eq!(
        sched.results(),
        &[
            SavedValue::Int(11),
            SavedValue::Int(21),
            SavedValue::Int(12),
        ]
    );
}

#[test]
fn continuation_scheduler_is_complete_transitions() {
    let mut sched = MultiShotScheduler::new();
    assert!(sched.is_complete()); // starts complete (nothing pending)

    sched.enqueue(make_snapshot(10, 0), SavedValue::Null);
    assert!(!sched.is_complete()); // has pending work

    let _ = sched.dequeue();
    assert!(sched.is_complete()); // all dequeued
}

#[test]
fn continuation_scheduler_interleaved_enqueue_dequeue() {
    let mut sched = MultiShotScheduler::new();

    sched.enqueue(make_snapshot(10, 0), SavedValue::Int(1));
    let (_, v1) = sched.dequeue().unwrap();
    assert_eq!(v1, SavedValue::Int(1));

    sched.enqueue(make_snapshot(20, 1), SavedValue::Int(2));
    sched.enqueue(make_snapshot(30, 2), SavedValue::Int(3));

    let (_, v2) = sched.dequeue().unwrap();
    assert_eq!(v2, SavedValue::Int(2));

    let (_, v3) = sched.dequeue().unwrap();
    assert_eq!(v3, SavedValue::Int(3));

    assert!(sched.is_complete());
}

#[test]
fn continuation_scheduler_clone() {
    let mut sched = MultiShotScheduler::new();
    sched.enqueue(make_snapshot(10, 0), SavedValue::Int(1));
    sched.add_result(SavedValue::Str("done".to_string()));

    let sched2 = sched.clone();
    assert_eq!(sched2.pending_count(), 1);
    assert_eq!(sched2.result_count(), 1);
}

// ===========================================================================
// Integration-style: simulate a Choose.choose handler
// ===========================================================================

#[test]
fn continuation_simulate_choose_handler() {
    // Simulate: perform Choose.choose([1, 2, 3])
    // Handler resumes once per option, collecting results.
    let snap = make_snapshot(10, 0);
    let mut state = ContinuationState::new(snap, ContinuationMode::MultiShot);
    let mut sched = MultiShotScheduler::new();

    let options = vec![SavedValue::Int(1), SavedValue::Int(2), SavedValue::Int(3)];

    // Handler enqueues a resumption for each option.
    for opt in &options {
        let resume_snap = state.prepare_resume().unwrap();
        sched.enqueue(resume_snap, opt.clone());
    }

    assert_eq!(state.resume_count(), 3);
    assert_eq!(sched.pending_count(), 3);

    // Process all pending resumptions.
    while let Some((_snap, value)) = sched.dequeue() {
        // Simulate: the body computes value + 10
        if let SavedValue::Int(n) = value {
            sched.add_result(SavedValue::Int(n + 10));
        }
    }

    assert!(sched.is_complete());
    assert_eq!(
        sched.results(),
        &[
            SavedValue::Int(11),
            SavedValue::Int(12),
            SavedValue::Int(13),
        ]
    );
}

#[test]
fn continuation_simulate_nested_choose() {
    // Simulate: let x = choose([1,2]); let y = choose([10,20]); x + y
    // Should produce [11, 21, 12, 22]
    let snap = make_snapshot(10, 0);
    let mut outer_state = ContinuationState::new(snap.clone(), ContinuationMode::MultiShot);
    let mut results: Vec<SavedValue> = Vec::new();

    let xs = vec![1i64, 2];
    let ys = vec![10i64, 20];

    for x in &xs {
        let _outer_snap = outer_state.prepare_resume().unwrap();
        // For each x, simulate inner choose
        let inner_snap = make_snapshot(20, 0);
        let mut inner_state = ContinuationState::new(inner_snap, ContinuationMode::MultiShot);

        for y in &ys {
            let _inner_snap = inner_state.prepare_resume().unwrap();
            results.push(SavedValue::Int(x + y));
        }
    }

    assert_eq!(
        results,
        vec![
            SavedValue::Int(11),
            SavedValue::Int(21),
            SavedValue::Int(12),
            SavedValue::Int(22),
        ]
    );
}

// We need the use for std::error::Error source() call above
use std::error::Error;
