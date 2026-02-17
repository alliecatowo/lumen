//! Property-based testing, snapshot testing, and assertion helpers for the Lumen compiler.
//!
//! Provides a deterministic PRNG, value generators, snapshot registry,
//! and compile/typecheck/parse assertion utilities.

use std::collections::HashMap;
use std::fmt;

// ════════════════════════════════════════════════════════════════════
// SimpleRng — deterministic xorshift64 PRNG
// ════════════════════════════════════════════════════════════════════

/// A simple deterministic pseudo-random number generator using xorshift64.
///
/// Same seed always produces the same sequence.
#[derive(Debug, Clone)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Create a new RNG from a seed. Zero seeds are remapped to avoid degenerate state.
    pub fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x5EED_DEAD_BEEF_CAFE
        } else {
            seed
        };
        SimpleRng { state }
    }

    /// Produce the next u64 value using xorshift64.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Random i64 in `[min, max]` (inclusive).
    pub fn next_i64_range(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        let range = (max as u128).wrapping_sub(min as u128).wrapping_add(1) as u64;
        let val = self.next_u64() % range;
        min.wrapping_add(val as i64)
    }

    /// Random f64 in `[0.0, 1.0)`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Random boolean.
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Random ASCII alphabetic character (`a-z`, `A-Z`).
    pub fn next_char_alpha(&mut self) -> char {
        let chars = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let idx = (self.next_u64() as usize) % chars.len();
        chars[idx] as char
    }

    /// Random usize in `[0, max)`.
    fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() as usize) % max
    }
}

// ════════════════════════════════════════════════════════════════════
// TestValue — dynamically typed test values
// ════════════════════════════════════════════════════════════════════

/// A dynamically typed test value used by property-based testing.
#[derive(Debug, Clone, PartialEq)]
pub enum TestValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<TestValue>),
    Null,
}

impl fmt::Display for TestValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestValue::Int(v) => write!(f, "{v}"),
            TestValue::Float(v) => write!(f, "{v}"),
            TestValue::String(v) => write!(f, "\"{v}\""),
            TestValue::Bool(v) => write!(f, "{v}"),
            TestValue::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            TestValue::Null => write!(f, "null"),
        }
    }
}

impl TestValue {
    /// Return a Lumen source literal representation of this value.
    pub fn to_lumen_literal(&self) -> String {
        match self {
            TestValue::Int(v) => format!("{v}"),
            TestValue::Float(v) => {
                let s = format!("{v}");
                if s.contains('.') {
                    s
                } else {
                    format!("{s}.0")
                }
            }
            TestValue::String(v) => format!("\"{v}\""),
            TestValue::Bool(v) => format!("{v}"),
            TestValue::List(items) => {
                let inner: Vec<String> = items.iter().map(|i| i.to_lumen_literal()).collect();
                format!("[{}]", inner.join(", "))
            }
            TestValue::Null => "null".to_string(),
        }
    }

    /// Attempt to shrink a value toward a simpler form.
    /// Returns a list of candidate shrinks, smallest first.
    pub fn shrink(&self) -> Vec<TestValue> {
        match self {
            TestValue::Int(v) => shrink_int(*v),
            TestValue::Float(v) => shrink_float(*v),
            TestValue::String(s) => shrink_string(s),
            TestValue::Bool(_) => vec![TestValue::Bool(false)],
            TestValue::List(items) => shrink_list(items),
            TestValue::Null => vec![],
        }
    }
}

fn shrink_int(v: i64) -> Vec<TestValue> {
    if v == 0 {
        return vec![];
    }
    let mut candidates = vec![TestValue::Int(0)];
    let half = v / 2;
    if half != 0 && half != v {
        candidates.push(TestValue::Int(half));
    }
    if v > 0 {
        candidates.push(TestValue::Int(v - 1));
    } else {
        candidates.push(TestValue::Int(v + 1));
    }
    candidates
}

fn shrink_float(v: f64) -> Vec<TestValue> {
    if v == 0.0 {
        return vec![];
    }
    let mut candidates = vec![TestValue::Float(0.0)];
    let half = v / 2.0;
    if half != 0.0 && (half - v).abs() > f64::EPSILON {
        candidates.push(TestValue::Float(half));
    }
    candidates
}

fn shrink_string(s: &str) -> Vec<TestValue> {
    if s.is_empty() {
        return vec![];
    }
    let mut candidates = vec![TestValue::String(String::new())];
    if s.len() > 1 {
        // Remove last character
        candidates.push(TestValue::String(s[..s.len() - 1].to_string()));
        // Remove first character
        candidates.push(TestValue::String(s[1..].to_string()));
    }
    candidates
}

fn shrink_list(items: &[TestValue]) -> Vec<TestValue> {
    if items.is_empty() {
        return vec![];
    }
    let mut candidates = vec![TestValue::List(vec![])];
    // Remove last element
    if items.len() > 1 {
        candidates.push(TestValue::List(items[..items.len() - 1].to_vec()));
    }
    // Remove first element
    if items.len() > 1 {
        candidates.push(TestValue::List(items[1..].to_vec()));
    }
    // Shrink individual elements
    for (i, item) in items.iter().enumerate() {
        for shrunk in item.shrink() {
            let mut new_list = items.to_vec();
            new_list[i] = shrunk;
            candidates.push(TestValue::List(new_list));
        }
    }
    candidates
}

// ════════════════════════════════════════════════════════════════════
// ValueGenerator — random value generation
// ════════════════════════════════════════════════════════════════════

/// Generator for producing random `TestValue`s.
#[derive(Debug, Clone)]
pub enum ValueGenerator {
    /// Random integer in `[min, max]`.
    IntRange(i64, i64),
    /// Random float in `[min, max)`.
    FloatRange(f64, f64),
    /// Random alphabetic string up to the given max length.
    StringAlpha(usize),
    /// Random string with any printable ASCII characters, up to max length.
    StringAny(usize),
    /// Random boolean.
    Bool,
    /// Random list of values from the inner generator, up to max length.
    ListOf(Box<ValueGenerator>, usize),
    /// Pick one value from the given list.
    OneOf(Vec<TestValue>),
    /// Always produce the same constant value.
    Constant(TestValue),
}

impl ValueGenerator {
    /// Generate a random `TestValue` using the supplied RNG.
    pub fn generate(&self, rng: &mut SimpleRng) -> TestValue {
        match self {
            ValueGenerator::IntRange(min, max) => TestValue::Int(rng.next_i64_range(*min, *max)),
            ValueGenerator::FloatRange(min, max) => {
                let t = rng.next_f64();
                TestValue::Float(*min + t * (*max - *min))
            }
            ValueGenerator::StringAlpha(max_len) => {
                let len = rng.next_usize(*max_len + 1);
                let s: String = (0..len).map(|_| rng.next_char_alpha()).collect();
                TestValue::String(s)
            }
            ValueGenerator::StringAny(max_len) => {
                let len = rng.next_usize(*max_len + 1);
                // Printable ASCII: 0x20..0x7E
                let s: String = (0..len)
                    .map(|_| {
                        let code = 0x20 + (rng.next_u64() as u8 % 95);
                        code as char
                    })
                    .collect();
                TestValue::String(s)
            }
            ValueGenerator::Bool => TestValue::Bool(rng.next_bool()),
            ValueGenerator::ListOf(inner, max_len) => {
                let len = rng.next_usize(*max_len + 1);
                let items: Vec<TestValue> = (0..len).map(|_| inner.generate(rng)).collect();
                TestValue::List(items)
            }
            ValueGenerator::OneOf(choices) => {
                if choices.is_empty() {
                    return TestValue::Null;
                }
                let idx = rng.next_usize(choices.len());
                choices[idx].clone()
            }
            ValueGenerator::Constant(val) => val.clone(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// PropertyTest — property-based test runner
// ════════════════════════════════════════════════════════════════════

/// A property-based test configuration.
#[derive(Debug, Clone)]
pub struct PropertyTest {
    pub name: String,
    pub iterations: usize,
    pub seed: u64,
    pub generators: Vec<ValueGenerator>,
}

/// Result of running a single property test iteration.
#[derive(Debug, Clone)]
pub struct PropertyFailure {
    pub iteration: usize,
    pub inputs: Vec<TestValue>,
    pub shrunk_inputs: Option<Vec<TestValue>>,
    pub message: String,
}

impl fmt::Display for PropertyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Property failed at iteration {}: {}",
            self.iteration, self.message
        )?;
        write!(f, "\n  Inputs: [")?;
        for (i, v) in self.inputs.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{v}")?;
        }
        write!(f, "]")?;
        if let Some(shrunk) = &self.shrunk_inputs {
            write!(f, "\n  Shrunk: [")?;
            for (i, v) in shrunk.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{v}")?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

impl PropertyTest {
    /// Create a new property test with defaults.
    pub fn new(name: &str) -> Self {
        PropertyTest {
            name: name.to_string(),
            iterations: 100,
            seed: 12345,
            generators: Vec::new(),
        }
    }

    /// Run the property test with a checker function.
    ///
    /// The checker receives a slice of generated values and returns `Ok(())` on
    /// success or `Err(message)` on failure. On failure, shrinking is attempted.
    pub fn run<F>(&self, checker: F) -> Result<usize, PropertyFailure>
    where
        F: Fn(&[TestValue]) -> Result<(), String>,
    {
        let mut rng = SimpleRng::new(self.seed);

        for iteration in 0..self.iterations {
            let inputs: Vec<TestValue> = self
                .generators
                .iter()
                .map(|g| g.generate(&mut rng))
                .collect();

            if let Err(msg) = checker(&inputs) {
                // Attempt to shrink
                let shrunk = self.shrink_inputs(&inputs, &checker);
                return Err(PropertyFailure {
                    iteration,
                    inputs,
                    shrunk_inputs: shrunk,
                    message: msg,
                });
            }
        }

        Ok(self.iterations)
    }

    /// Try to find a minimal failing input by shrinking each value.
    fn shrink_inputs<F>(&self, inputs: &[TestValue], checker: &F) -> Option<Vec<TestValue>>
    where
        F: Fn(&[TestValue]) -> Result<(), String>,
    {
        let mut current = inputs.to_vec();
        let mut improved = false;
        let max_shrink_rounds = 50;

        for _ in 0..max_shrink_rounds {
            let mut round_improved = false;
            for i in 0..current.len() {
                let candidates = current[i].shrink();
                for candidate in candidates {
                    let mut trial = current.clone();
                    trial[i] = candidate;
                    if checker(&trial).is_err() {
                        current = trial;
                        round_improved = true;
                        improved = true;
                        break;
                    }
                }
            }
            if !round_improved {
                break;
            }
        }

        if improved {
            Some(current)
        } else {
            None
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Snapshot testing
// ════════════════════════════════════════════════════════════════════

/// Result of comparing a snapshot.
#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotResult {
    /// Output matches the stored snapshot.
    Match,
    /// Output differs from the stored snapshot.
    Mismatch {
        expected: String,
        actual: String,
        diff: String,
    },
    /// No stored snapshot exists yet; this is new output.
    New(String),
}

/// A single snapshot test entry.
#[derive(Debug, Clone)]
pub struct SnapshotTest {
    pub name: String,
    pub input: String,
    pub expected_output: String,
}

/// Registry for managing named snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotRegistry {
    snapshots: HashMap<String, String>,
}

impl SnapshotRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        SnapshotRegistry {
            snapshots: HashMap::new(),
        }
    }

    /// Register (store) a snapshot under a name.
    pub fn register(&mut self, name: &str, output: &str) {
        self.snapshots.insert(name.to_string(), output.to_string());
    }

    /// Check whether `actual` matches the stored snapshot for `name`.
    pub fn check(&self, name: &str, actual: &str) -> SnapshotResult {
        match self.snapshots.get(name) {
            None => SnapshotResult::New(actual.to_string()),
            Some(expected) => {
                if expected == actual {
                    SnapshotResult::Match
                } else {
                    SnapshotResult::Mismatch {
                        expected: expected.clone(),
                        actual: actual.to_string(),
                        diff: compute_diff(expected, actual),
                    }
                }
            }
        }
    }

    /// Update (overwrite) a snapshot.
    pub fn update(&mut self, name: &str, output: &str) {
        self.snapshots.insert(name.to_string(), output.to_string());
    }

    /// Return all snapshots as sorted `(name, output)` pairs.
    pub fn all_snapshots(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = self
            .snapshots
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        pairs.sort_by_key(|(k, _)| *k);
        pairs
    }

    /// Serialize all snapshots to a deterministic text format.
    ///
    /// Format:
    /// ```text
    /// --- snapshot: name
    /// content line 1
    /// content line 2
    /// --- end
    /// ```
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for (name, content) in self.all_snapshots() {
            out.push_str(&format!("--- snapshot: {name}\n"));
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("--- end\n");
        }
        out
    }

    /// Deserialize from the text format produced by `serialize`.
    pub fn deserialize(input: &str) -> Result<Self, String> {
        let mut snapshots = HashMap::new();
        let mut current_name: Option<String> = None;
        let mut current_content = String::new();

        for line in input.lines() {
            if let Some(rest) = line.strip_prefix("--- snapshot: ") {
                if current_name.is_some() {
                    return Err("Nested snapshot blocks are not allowed".to_string());
                }
                current_name = Some(rest.to_string());
                current_content.clear();
            } else if line == "--- end" {
                match current_name.take() {
                    Some(name) => {
                        // Remove trailing newline added during serialize
                        let content = if current_content.ends_with('\n') {
                            current_content[..current_content.len() - 1].to_string()
                        } else {
                            current_content.clone()
                        };
                        snapshots.insert(name, content);
                        current_content.clear();
                    }
                    None => {
                        return Err("Found '--- end' without matching '--- snapshot:'".to_string())
                    }
                }
            } else if current_name.is_some() {
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(line);
            }
        }

        if current_name.is_some() {
            return Err("Unclosed snapshot block".to_string());
        }

        Ok(SnapshotRegistry { snapshots })
    }
}

impl Default for SnapshotRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
// Diff computation
// ════════════════════════════════════════════════════════════════════

/// Compute a line-by-line diff between two strings.
///
/// Produces output with `-` lines for expected-only and `+` lines for actual-only.
pub fn compute_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut diff = String::new();
    let max_len = expected_lines.len().max(actual_lines.len());

    for i in 0..max_len {
        let exp = expected_lines.get(i).copied();
        let act = actual_lines.get(i).copied();
        match (exp, act) {
            (Some(e), Some(a)) if e == a => {
                diff.push_str(&format!(" {e}\n"));
            }
            (Some(e), Some(a)) => {
                diff.push_str(&format!("-{e}\n"));
                diff.push_str(&format!("+{a}\n"));
            }
            (Some(e), None) => {
                diff.push_str(&format!("-{e}\n"));
            }
            (None, Some(a)) => {
                diff.push_str(&format!("+{a}\n"));
            }
            (None, None) => {}
        }
    }

    diff
}

// ════════════════════════════════════════════════════════════════════
// Compiler assertion helpers
// ════════════════════════════════════════════════════════════════════

/// Assert that the given Lumen source compiles without errors.
pub fn assert_compiles(source: &str) -> Result<(), String> {
    crate::compile_raw(source)
        .map(|_| ())
        .map_err(|e| format!("Expected compilation to succeed, but got error: {e}"))
}

/// Assert that the given Lumen source fails to compile and the error message
/// contains `expected_error`.
pub fn assert_compile_error(source: &str, expected_error: &str) -> Result<(), String> {
    match crate::compile_raw(source) {
        Ok(_) => Err(format!(
            "Expected compile error containing '{expected_error}', but compilation succeeded"
        )),
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains(expected_error) {
                Ok(())
            } else {
                Err(format!(
                    "Expected error containing '{expected_error}', got: {msg}"
                ))
            }
        }
    }
}

/// Assert that the given Lumen source passes type checking.
///
/// Runs the full pipeline through typechecking. Since `compile_raw` runs
/// typecheck before lowering, a compile success implies type-check success.
pub fn assert_type_checks(source: &str) -> Result<(), String> {
    // We use compile_raw which runs resolve + typecheck + constraints + lower
    crate::compile_raw(source)
        .map(|_| ())
        .map_err(|e| format!("Expected type checking to succeed, but got error: {e}"))
}

/// Assert that the given Lumen source produces a type error whose message
/// contains `expected`.
pub fn assert_type_error(source: &str, expected: &str) -> Result<(), String> {
    match crate::compile_raw(source) {
        Ok(_) => Err(format!(
            "Expected type error containing '{expected}', but compilation succeeded"
        )),
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains(expected) {
                Ok(())
            } else {
                Err(format!(
                    "Expected type error containing '{expected}', got: {msg}"
                ))
            }
        }
    }
}

/// Assert that the given Lumen source parses successfully.
pub fn assert_parses(source: &str) -> Result<(), String> {
    let mut lexer = crate::compiler::lexer::Lexer::new(source, 1, 0);
    let tokens = lexer.tokenize().map_err(|e| format!("Lex error: {e}"))?;
    let mut parser = crate::compiler::parser::Parser::new(tokens);
    let (_, errors) = parser.parse_program_with_recovery(vec![]);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Expected parsing to succeed, but got {} error(s): {:?}",
            errors.len(),
            errors
        ))
    }
}

/// Assert that the given Lumen source produces a parse error whose message
/// contains `expected`.
pub fn assert_parse_error(source: &str, expected: &str) -> Result<(), String> {
    let mut lexer = crate::compiler::lexer::Lexer::new(source, 1, 0);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("{e}");
            return if msg.contains(expected) {
                Ok(())
            } else {
                Err(format!(
                    "Got lex error instead of parse error. Lex error: {msg}"
                ))
            };
        }
    };
    let mut parser = crate::compiler::parser::Parser::new(tokens);
    let (_, errors) = parser.parse_program_with_recovery(vec![]);
    if errors.is_empty() {
        return Err(format!(
            "Expected parse error containing '{expected}', but parsing succeeded"
        ));
    }
    let full_msg = format!("{errors:?}");
    if full_msg.contains(expected) {
        Ok(())
    } else {
        Err(format!(
            "Expected parse error containing '{expected}', got: {full_msg}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_deterministic() {
        let mut a = SimpleRng::new(42);
        let mut b = SimpleRng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_different_seeds() {
        let mut a = SimpleRng::new(1);
        let mut b = SimpleRng::new(2);
        // Very unlikely to match
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn rng_zero_seed_handled() {
        let mut rng = SimpleRng::new(0);
        // Should not produce 0
        assert_ne!(rng.next_u64(), 0);
    }

    #[test]
    fn test_value_display() {
        assert_eq!(format!("{}", TestValue::Int(42)), "42");
        assert_eq!(format!("{}", TestValue::Bool(true)), "true");
        assert_eq!(format!("{}", TestValue::Null), "null");
    }

    #[test]
    fn snapshot_round_trip() {
        let mut reg = SnapshotRegistry::new();
        reg.register("a", "hello world");
        reg.register("b", "line1\nline2");
        let serialized = reg.serialize();
        let deserialized = SnapshotRegistry::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.all_snapshots(), reg.all_snapshots());
    }
}
