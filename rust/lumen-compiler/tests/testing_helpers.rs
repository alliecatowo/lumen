//! Tests for T171: Testing helpers — property-based testing, snapshot testing, and assertion helpers.

use lumen_compiler::compiler::testing_helpers::*;

// ════════════════════════════════════════════════════════════════════
// §1  SimpleRng — deterministic PRNG
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_rng_deterministic_sequence() {
    let mut a = SimpleRng::new(42);
    let mut b = SimpleRng::new(42);
    let seq_a: Vec<u64> = (0..200).map(|_| a.next_u64()).collect();
    let seq_b: Vec<u64> = (0..200).map(|_| b.next_u64()).collect();
    assert_eq!(seq_a, seq_b);
}

#[test]
fn testing_rng_different_seeds_diverge() {
    let mut a = SimpleRng::new(1);
    let mut b = SimpleRng::new(2);
    let matches = (0..20).filter(|_| a.next_u64() == b.next_u64()).count();
    assert_eq!(
        matches, 0,
        "Different seeds should produce different values"
    );
}

#[test]
fn testing_rng_zero_seed_safe() {
    let mut rng = SimpleRng::new(0);
    let val = rng.next_u64();
    assert_ne!(
        val, 0,
        "Zero seed must be remapped to avoid degenerate state"
    );
}

#[test]
fn testing_rng_i64_range() {
    let mut rng = SimpleRng::new(99);
    for _ in 0..500 {
        let val = rng.next_i64_range(-10, 10);
        assert!((-10..=10).contains(&val), "Got out-of-range value: {val}");
    }
}

#[test]
fn testing_rng_i64_range_singleton() {
    let mut rng = SimpleRng::new(77);
    for _ in 0..50 {
        assert_eq!(rng.next_i64_range(5, 5), 5);
    }
}

#[test]
fn testing_rng_f64_range() {
    let mut rng = SimpleRng::new(123);
    for _ in 0..500 {
        let val = rng.next_f64();
        assert!((0.0..1.0).contains(&val), "f64 out of [0,1): {val}");
    }
}

#[test]
fn testing_rng_bool_produces_both() {
    let mut rng = SimpleRng::new(42);
    let bools: Vec<bool> = (0..200).map(|_| rng.next_bool()).collect();
    assert!(bools.contains(&true), "Should produce true");
    assert!(bools.contains(&false), "Should produce false");
}

#[test]
fn testing_rng_char_alpha() {
    let mut rng = SimpleRng::new(55);
    for _ in 0..200 {
        let c = rng.next_char_alpha();
        assert!(c.is_ascii_alphabetic(), "Not alpha: {c}");
    }
}

// ════════════════════════════════════════════════════════════════════
// §2  TestValue — display, literals, shrinking
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_value_display_int() {
    assert_eq!(format!("{}", TestValue::Int(42)), "42");
    assert_eq!(format!("{}", TestValue::Int(-7)), "-7");
}

#[test]
fn testing_value_display_float() {
    assert_eq!(format!("{}", TestValue::Float(3.14)), "3.14");
}

#[test]
fn testing_value_display_string() {
    assert_eq!(format!("{}", TestValue::String("hi".into())), "\"hi\"");
}

#[test]
fn testing_value_display_bool() {
    assert_eq!(format!("{}", TestValue::Bool(true)), "true");
    assert_eq!(format!("{}", TestValue::Bool(false)), "false");
}

#[test]
fn testing_value_display_null() {
    assert_eq!(format!("{}", TestValue::Null), "null");
}

#[test]
fn testing_value_display_list() {
    let list = TestValue::List(vec![TestValue::Int(1), TestValue::Int(2)]);
    assert_eq!(format!("{list}"), "[1, 2]");
}

#[test]
fn testing_value_lumen_literal_int() {
    assert_eq!(TestValue::Int(42).to_lumen_literal(), "42");
    assert_eq!(TestValue::Int(-1).to_lumen_literal(), "-1");
}

#[test]
fn testing_value_lumen_literal_float() {
    let lit = TestValue::Float(2.0).to_lumen_literal();
    assert!(lit.contains('.'), "Float literal should contain '.': {lit}");
}

#[test]
fn testing_value_lumen_literal_string() {
    assert_eq!(TestValue::String("hi".into()).to_lumen_literal(), "\"hi\"");
}

#[test]
fn testing_value_lumen_literal_list() {
    let list = TestValue::List(vec![TestValue::Bool(true), TestValue::Null]);
    assert_eq!(list.to_lumen_literal(), "[true, null]");
}

#[test]
fn testing_value_shrink_int_zero() {
    assert!(TestValue::Int(0).shrink().is_empty());
}

#[test]
fn testing_value_shrink_int_positive() {
    let shrinks = TestValue::Int(10).shrink();
    assert!(!shrinks.is_empty());
    assert!(shrinks.contains(&TestValue::Int(0)));
}

#[test]
fn testing_value_shrink_int_negative() {
    let shrinks = TestValue::Int(-5).shrink();
    assert!(shrinks.contains(&TestValue::Int(0)));
}

#[test]
fn testing_value_shrink_string_empty() {
    assert!(TestValue::String(String::new()).shrink().is_empty());
}

#[test]
fn testing_value_shrink_string_nonempty() {
    let shrinks = TestValue::String("abc".into()).shrink();
    assert!(shrinks.contains(&TestValue::String(String::new())));
    assert!(shrinks.contains(&TestValue::String("ab".into())));
    assert!(shrinks.contains(&TestValue::String("bc".into())));
}

#[test]
fn testing_value_shrink_list_empty() {
    assert!(TestValue::List(vec![]).shrink().is_empty());
}

#[test]
fn testing_value_shrink_list_nonempty() {
    let list = TestValue::List(vec![TestValue::Int(5), TestValue::Int(10)]);
    let shrinks = list.shrink();
    assert!(shrinks.contains(&TestValue::List(vec![])));
}

#[test]
fn testing_value_shrink_bool() {
    let shrinks = TestValue::Bool(true).shrink();
    assert!(shrinks.contains(&TestValue::Bool(false)));
}

#[test]
fn testing_value_shrink_null() {
    assert!(TestValue::Null.shrink().is_empty());
}

// ════════════════════════════════════════════════════════════════════
// §3  ValueGenerator
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_gen_int_range() {
    let gen = ValueGenerator::IntRange(0, 100);
    let mut rng = SimpleRng::new(42);
    for _ in 0..200 {
        match gen.generate(&mut rng) {
            TestValue::Int(v) => assert!((0..=100).contains(&v)),
            other => panic!("Expected Int, got {other}"),
        }
    }
}

#[test]
fn testing_gen_float_range() {
    let gen = ValueGenerator::FloatRange(-1.0, 1.0);
    let mut rng = SimpleRng::new(42);
    for _ in 0..200 {
        match gen.generate(&mut rng) {
            TestValue::Float(v) => assert!((-1.0..=1.0).contains(&v), "Out of range: {v}"),
            other => panic!("Expected Float, got {other}"),
        }
    }
}

#[test]
fn testing_gen_string_alpha() {
    let gen = ValueGenerator::StringAlpha(10);
    let mut rng = SimpleRng::new(42);
    for _ in 0..100 {
        match gen.generate(&mut rng) {
            TestValue::String(s) => {
                assert!(s.len() <= 10);
                assert!(s.chars().all(|c| c.is_ascii_alphabetic()));
            }
            other => panic!("Expected String, got {other}"),
        }
    }
}

#[test]
fn testing_gen_string_any() {
    let gen = ValueGenerator::StringAny(20);
    let mut rng = SimpleRng::new(42);
    for _ in 0..100 {
        match gen.generate(&mut rng) {
            TestValue::String(s) => {
                assert!(s.len() <= 20);
                assert!(s.chars().all(|c| c.is_ascii() && !c.is_ascii_control()));
            }
            other => panic!("Expected String, got {other}"),
        }
    }
}

#[test]
fn testing_gen_bool() {
    let gen = ValueGenerator::Bool;
    let mut rng = SimpleRng::new(42);
    let vals: Vec<TestValue> = (0..100).map(|_| gen.generate(&mut rng)).collect();
    assert!(vals.contains(&TestValue::Bool(true)));
    assert!(vals.contains(&TestValue::Bool(false)));
}

#[test]
fn testing_gen_list_of() {
    let gen = ValueGenerator::ListOf(Box::new(ValueGenerator::IntRange(0, 10)), 5);
    let mut rng = SimpleRng::new(42);
    for _ in 0..100 {
        match gen.generate(&mut rng) {
            TestValue::List(items) => {
                assert!(items.len() <= 5);
                for item in &items {
                    assert!(matches!(item, TestValue::Int(v) if (0..=10).contains(v)));
                }
            }
            other => panic!("Expected List, got {other}"),
        }
    }
}

#[test]
fn testing_gen_one_of() {
    let choices = vec![TestValue::Int(1), TestValue::Int(2), TestValue::Int(3)];
    let gen = ValueGenerator::OneOf(choices.clone());
    let mut rng = SimpleRng::new(42);
    for _ in 0..100 {
        let val = gen.generate(&mut rng);
        assert!(choices.contains(&val), "Got unexpected value: {val}");
    }
}

#[test]
fn testing_gen_one_of_empty() {
    let gen = ValueGenerator::OneOf(vec![]);
    let mut rng = SimpleRng::new(42);
    assert_eq!(gen.generate(&mut rng), TestValue::Null);
}

#[test]
fn testing_gen_constant() {
    let gen = ValueGenerator::Constant(TestValue::String("hello".into()));
    let mut rng = SimpleRng::new(42);
    for _ in 0..10 {
        assert_eq!(gen.generate(&mut rng), TestValue::String("hello".into()));
    }
}

// ════════════════════════════════════════════════════════════════════
// §4  PropertyTest — run and shrink
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_property_all_pass() {
    let mut pt = PropertyTest::new("all_positive");
    pt.iterations = 50;
    pt.seed = 99;
    pt.generators = vec![ValueGenerator::IntRange(1, 1000)];

    let result = pt.run(|inputs| match &inputs[0] {
        TestValue::Int(v) if *v > 0 => Ok(()),
        other => Err(format!("Expected positive, got {other}")),
    });
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 50);
}

#[test]
fn testing_property_failure_detected() {
    let mut pt = PropertyTest::new("find_negative");
    pt.iterations = 1000;
    pt.seed = 42;
    pt.generators = vec![ValueGenerator::IntRange(-100, 100)];

    let result = pt.run(|inputs| match &inputs[0] {
        TestValue::Int(v) if *v >= 0 => Ok(()),
        TestValue::Int(v) => Err(format!("Negative: {v}")),
        _ => Ok(()),
    });
    assert!(result.is_err());
}

#[test]
fn testing_property_shrinking_finds_smaller_input() {
    let mut pt = PropertyTest::new("shrink_test");
    pt.iterations = 200;
    pt.seed = 42;
    pt.generators = vec![ValueGenerator::IntRange(-1000, 1000)];

    let result = pt.run(|inputs| match &inputs[0] {
        TestValue::Int(v) if *v < 0 => Err(format!("Negative: {v}")),
        _ => Ok(()),
    });
    let failure = result.unwrap_err();
    // Shrinking should find a value closer to 0 than the original
    if let Some(shrunk) = &failure.shrunk_inputs {
        if let TestValue::Int(v) = &shrunk[0] {
            assert!(*v < 0, "Shrunk value should still fail: {v}");
            assert_eq!(*v, -1, "Shrunk should reach -1");
        }
    }
}

#[test]
fn testing_property_failure_display() {
    let failure = PropertyFailure {
        iteration: 3,
        inputs: vec![TestValue::Int(42)],
        shrunk_inputs: Some(vec![TestValue::Int(1)]),
        message: "too big".to_string(),
    };
    let s = format!("{failure}");
    assert!(s.contains("iteration 3"));
    assert!(s.contains("too big"));
    assert!(s.contains("Shrunk"));
}

#[test]
fn testing_property_multiple_generators() {
    let mut pt = PropertyTest::new("multi_gen");
    pt.iterations = 100;
    pt.seed = 42;
    pt.generators = vec![
        ValueGenerator::IntRange(0, 100),
        ValueGenerator::StringAlpha(5),
    ];

    let result = pt.run(|inputs| {
        assert_eq!(inputs.len(), 2);
        assert!(matches!(&inputs[0], TestValue::Int(_)));
        assert!(matches!(&inputs[1], TestValue::String(_)));
        Ok(())
    });
    assert!(result.is_ok());
}

// ════════════════════════════════════════════════════════════════════
// §5  SnapshotRegistry
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_snapshot_register_and_check_match() {
    let mut reg = SnapshotRegistry::new();
    reg.register("test1", "hello world");
    assert_eq!(reg.check("test1", "hello world"), SnapshotResult::Match);
}

#[test]
fn testing_snapshot_check_mismatch() {
    let mut reg = SnapshotRegistry::new();
    reg.register("test1", "hello");
    match reg.check("test1", "world") {
        SnapshotResult::Mismatch {
            expected,
            actual,
            diff,
        } => {
            assert_eq!(expected, "hello");
            assert_eq!(actual, "world");
            assert!(!diff.is_empty());
        }
        other => panic!("Expected Mismatch, got {other:?}"),
    }
}

#[test]
fn testing_snapshot_check_new() {
    let reg = SnapshotRegistry::new();
    match reg.check("unknown", "content") {
        SnapshotResult::New(content) => assert_eq!(content, "content"),
        other => panic!("Expected New, got {other:?}"),
    }
}

#[test]
fn testing_snapshot_update() {
    let mut reg = SnapshotRegistry::new();
    reg.register("test1", "old");
    reg.update("test1", "new");
    assert_eq!(reg.check("test1", "new"), SnapshotResult::Match);
}

#[test]
fn testing_snapshot_all_sorted() {
    let mut reg = SnapshotRegistry::new();
    reg.register("z_last", "3");
    reg.register("a_first", "1");
    reg.register("m_middle", "2");
    let all = reg.all_snapshots();
    assert_eq!(all[0].0, "a_first");
    assert_eq!(all[1].0, "m_middle");
    assert_eq!(all[2].0, "z_last");
}

#[test]
fn testing_snapshot_serialize_round_trip() {
    let mut reg = SnapshotRegistry::new();
    reg.register("alpha", "line1\nline2");
    reg.register("beta", "single line");
    reg.register("gamma", "multi\nline\ncontent");

    let serialized = reg.serialize();
    let deserialized = SnapshotRegistry::deserialize(&serialized).unwrap();
    assert_eq!(deserialized.all_snapshots(), reg.all_snapshots());
}

#[test]
fn testing_snapshot_serialize_empty() {
    let reg = SnapshotRegistry::new();
    let serialized = reg.serialize();
    assert_eq!(serialized, "");
    let deserialized = SnapshotRegistry::deserialize(&serialized).unwrap();
    assert!(deserialized.all_snapshots().is_empty());
}

#[test]
fn testing_snapshot_deserialize_error_nested() {
    let bad = "--- snapshot: a\n--- snapshot: b\ncontent\n--- end\n--- end\n";
    assert!(SnapshotRegistry::deserialize(bad).is_err());
}

#[test]
fn testing_snapshot_deserialize_error_orphan_end() {
    let bad = "--- end\n";
    assert!(SnapshotRegistry::deserialize(bad).is_err());
}

#[test]
fn testing_snapshot_deserialize_error_unclosed() {
    let bad = "--- snapshot: a\ncontent\n";
    assert!(SnapshotRegistry::deserialize(bad).is_err());
}

// ════════════════════════════════════════════════════════════════════
// §6  Diff computation
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_diff_identical() {
    let diff = compute_diff("hello\nworld", "hello\nworld");
    assert!(!diff.contains('-'));
    assert!(!diff.contains('+'));
}

#[test]
fn testing_diff_single_change() {
    let diff = compute_diff("hello", "world");
    assert!(diff.contains("-hello"));
    assert!(diff.contains("+world"));
}

#[test]
fn testing_diff_added_line() {
    let diff = compute_diff("a", "a\nb");
    assert!(diff.contains("+b"));
}

#[test]
fn testing_diff_removed_line() {
    let diff = compute_diff("a\nb", "a");
    assert!(diff.contains("-b"));
}

// ════════════════════════════════════════════════════════════════════
// §7  Compiler assertion helpers
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_assert_compiles_ok() {
    let src = "cell main() -> Int\n  return 42\nend\n";
    assert!(assert_compiles(src).is_ok());
}

#[test]
fn testing_assert_compiles_fail() {
    let src = "cell main( -> Int\n  return 42\nend\n";
    assert!(assert_compiles(src).is_err());
}

#[test]
fn testing_assert_compile_error_found() {
    let src = "cell main( -> Int\n  return 42\nend\n";
    assert!(assert_compile_error(src, "parse").is_ok());
}

#[test]
fn testing_assert_compile_error_wrong_msg() {
    let src = "cell main( -> Int\n  return 42\nend\n";
    assert!(assert_compile_error(src, "type mismatch xyz123").is_err());
}

#[test]
fn testing_assert_compile_error_unexpected_success() {
    let src = "cell main() -> Int\n  return 42\nend\n";
    assert!(assert_compile_error(src, "anything").is_err());
}

#[test]
fn testing_assert_type_checks_ok() {
    let src = "cell foo() -> Int\n  return 1\nend\n";
    assert!(assert_type_checks(src).is_ok());
}

#[test]
fn testing_assert_parses_ok() {
    let src = "cell main() -> Int\n  return 42\nend\n";
    assert!(assert_parses(src).is_ok());
}

#[test]
fn testing_assert_parses_fail() {
    let src = "cell ( ->\nend\n";
    assert!(assert_parses(src).is_err());
}

#[test]
fn testing_assert_parse_error_found() {
    let src = "cell ( ->\nend\n";
    // Should detect some parse error — Debug format shows variant names
    assert!(assert_parse_error(src, "expected").is_ok());
}

#[test]
fn testing_assert_parse_error_unexpected_success() {
    let src = "cell main() -> Int\n  return 42\nend\n";
    assert!(assert_parse_error(src, "anything").is_err());
}

// ════════════════════════════════════════════════════════════════════
// §8  SnapshotTest struct usage
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_snapshot_test_struct() {
    let st = SnapshotTest {
        name: "test1".to_string(),
        input: "cell main() -> Int\n  return 1\nend".to_string(),
        expected_output: "OK".to_string(),
    };
    assert_eq!(st.name, "test1");
    assert!(!st.input.is_empty());
    assert_eq!(st.expected_output, "OK");
}

#[test]
fn testing_snapshot_default() {
    let reg = SnapshotRegistry::default();
    assert!(reg.all_snapshots().is_empty());
}

// ════════════════════════════════════════════════════════════════════
// §9  Integration: property test with compiler assertions
// ════════════════════════════════════════════════════════════════════

#[test]
fn testing_property_int_literals_compile() {
    let mut pt = PropertyTest::new("int_literals");
    pt.iterations = 20;
    pt.seed = 42;
    pt.generators = vec![ValueGenerator::IntRange(0, 10000)];

    let result = pt.run(|inputs| {
        if let TestValue::Int(v) = &inputs[0] {
            let src = format!("cell test() -> Int\n  return {v}\nend\n");
            assert_compiles(&src)
        } else {
            Err("Expected Int".to_string())
        }
    });
    assert!(result.is_ok(), "All int literal programs should compile");
}

#[test]
fn testing_property_bool_literals_compile() {
    let mut pt = PropertyTest::new("bool_literals");
    pt.iterations = 10;
    pt.seed = 42;
    pt.generators = vec![ValueGenerator::Bool];

    let result = pt.run(|inputs| {
        if let TestValue::Bool(v) = &inputs[0] {
            let src = format!("cell test() -> Bool\n  return {v}\nend\n");
            assert_compiles(&src)
        } else {
            Err("Expected Bool".to_string())
        }
    });
    assert!(result.is_ok());
}

#[test]
fn testing_snapshot_multiline_content() {
    let mut reg = SnapshotRegistry::new();
    let content = "line1\nline2\nline3\nline4\nline5";
    reg.register("multi", content);
    let serialized = reg.serialize();
    let restored = SnapshotRegistry::deserialize(&serialized).unwrap();
    assert_eq!(restored.check("multi", content), SnapshotResult::Match);
}
