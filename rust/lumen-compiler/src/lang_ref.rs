//! Auto-generated language reference module.
//!
//! Extracts metadata from the compiler's own data structures to produce
//! a comprehensive, always-up-to-date language reference.

use serde::{Deserialize, Serialize};

use crate::compiler::lir::{IntrinsicId, OpCode};
use strum::IntoEnumIterator;

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageReference {
    pub version: String,
    pub keywords: Vec<KeywordEntry>,
    pub operators: Vec<OperatorEntry>,
    pub builtin_types: Vec<TypeEntry>,
    pub builtin_functions: Vec<BuiltinEntry>,
    pub opcodes: Vec<OpcodeEntry>,
    pub intrinsics: Vec<IntrinsicEntry>,
    pub statement_forms: Vec<SyntaxEntry>,
    pub expression_forms: Vec<SyntaxEntry>,
    pub cli_commands: Vec<CliEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeywordEntry {
    pub keyword: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OperatorEntry {
    pub symbol: String,
    pub name: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TypeEntry {
    pub name: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuiltinEntry {
    pub name: String,
    pub description: String,
    pub return_type: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpcodeEntry {
    pub name: String,
    pub hex: String,
    pub encoding: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntrinsicEntry {
    pub name: String,
    pub id: u8,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyntaxEntry {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CliEntry {
    pub command: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Opcode metadata
// ---------------------------------------------------------------------------

fn opcode_info(op: &OpCode) -> (&str, &str, &str) {
    use OpCode::*;
    match op {
        // Misc
        Nop => ("Ax", "No operation", "Misc"),

        // Load/Move
        LoadK => ("A, Bx", "Load constant Bx into register A", "Load/Move"),
        LoadNil => ("A, B", "Set registers A..A+B to nil", "Load/Move"),
        LoadBool => (
            "A, B, C",
            "Load bool B into A; if C, skip next instruction",
            "Load/Move",
        ),
        LoadInt => ("A, sB", "Load small signed integer sB into register A", "Load/Move"),
        Move => ("A, B", "Copy register B to register A", "Load/Move"),

        // Data Construction
        NewList => ("A, B", "Create list from B values starting at A+1", "Data Construction"),
        NewMap => (
            "A, B",
            "Create map from B key-value pairs starting at A+1",
            "Data Construction",
        ),
        NewRecord => ("A, Bx", "Create record of type index Bx", "Data Construction"),
        NewUnion => (
            "A, B, C",
            "Create union with tag B and payload register C",
            "Data Construction",
        ),
        NewTuple => ("A, B", "Create tuple from B values starting at A+1", "Data Construction"),
        NewSet => ("A, B", "Create set from B values starting at A+1", "Data Construction"),

        // Access
        GetField => ("A, B, C", "A = B.field[C]", "Access"),
        SetField => ("A, B, C", "A.field[B] = C", "Access"),
        GetIndex => ("A, B, C", "A = B[C]", "Access"),
        SetIndex => ("A, B, C", "A[B] = C", "Access"),
        GetTuple => ("A, B, C", "A = R[B].elements[C]", "Access"),

        // Arithmetic
        Add => ("A, B, C", "A = B + C", "Arithmetic"),
        Sub => ("A, B, C", "A = B - C", "Arithmetic"),
        Mul => ("A, B, C", "A = B * C", "Arithmetic"),
        Div => ("A, B, C", "A = B / C", "Arithmetic"),
        Mod => ("A, B, C", "A = B % C", "Arithmetic"),
        Pow => ("A, B, C", "A = B ** C (exponentiation)", "Arithmetic"),
        Neg => ("A, B", "A = -B (arithmetic negation)", "Arithmetic"),
        Concat => ("A, B, C", "A = B ++ C (string/list concatenation)", "Arithmetic"),
        FloorDiv => ("A, B, C", "A = B // C (floor division)", "Arithmetic"),

        // Bitwise
        BitOr => ("A, B, C", "A = B | C (bitwise OR)", "Bitwise"),
        BitAnd => ("A, B, C", "A = B & C (bitwise AND)", "Bitwise"),
        BitXor => ("A, B, C", "A = B ^ C (bitwise XOR)", "Bitwise"),
        BitNot => ("A, B", "A = ~B (bitwise NOT)", "Bitwise"),
        Shl => ("A, B, C", "A = B << C (left shift)", "Bitwise"),
        Shr => ("A, B, C", "A = B >> C (right shift)", "Bitwise"),

        // Comparison
        Eq => (
            "A, B, C",
            "If (B == C) != A then skip next instruction",
            "Comparison",
        ),
        Lt => (
            "A, B, C",
            "If (B < C) != A then skip next instruction",
            "Comparison",
        ),
        Le => (
            "A, B, C",
            "If (B <= C) != A then skip next instruction",
            "Comparison",
        ),
        Not => ("A, B", "A = not B (logical negation)", "Comparison"),
        And => ("A, B, C", "A = B and C (logical AND)", "Comparison"),
        Or => ("A, B, C", "A = B or C (logical OR)", "Comparison"),
        In => ("A, B, C", "A = B in C (membership test)", "Comparison"),
        Is => (
            "A, B, C",
            "A = typeof(B) == type(C) (type check)",
            "Comparison",
        ),
        NullCo => (
            "A, B, C",
            "A = if B != null then B else C (null coalescing)",
            "Comparison",
        ),
        Test => (
            "A, C",
            "If (Reg[A] is truthy) != C then skip next instruction",
            "Comparison",
        ),

        // Control Flow
        Jmp => ("sAx", "Jump by signed offset", "Control Flow"),
        Call => ("A, B, C", "Call function A with B args, C results", "Control Flow"),
        TailCall => ("A, B, C", "Tail-call function A with B args", "Control Flow"),
        Return => ("A, B", "Return B values starting from register A", "Control Flow"),
        Halt => ("A", "Halt execution with error message in A", "Control Flow"),
        Loop => ("sAx", "Decrement loop counter, jump if > 0", "Control Flow"),
        ForPrep => ("A, sB", "Prepare numeric for-loop", "Control Flow"),
        ForLoop => ("A, sB", "Iterate numeric for-loop", "Control Flow"),
        ForIn => ("A, B, C", "For-in iterator step", "Control Flow"),
        Break => ("sAx", "Break from enclosing loop", "Control Flow"),
        Continue => ("sAx", "Continue to next loop iteration", "Control Flow"),

        // Intrinsics
        Intrinsic => (
            "A, B, C",
            "A = intrinsic[B](args starting at C)",
            "Intrinsics",
        ),

        // Closures
        Closure => (
            "A, Bx",
            "R[A] = closure(proto=Bx, captures upvalues)",
            "Closures",
        ),
        GetUpval => ("A, B", "R[A] = upvalue[B]", "Closures"),
        SetUpval => ("A, B", "upvalue[B] = R[A]", "Closures"),

        // Effects
        ToolCall => (
            "A, Bx",
            "Invoke tool Bx with args from subsequent registers",
            "Effects",
        ),
        Schema => ("A, B", "Validate A against schema type B", "Effects"),
        Emit => ("A", "Emit output from register A", "Effects"),
        TraceRef => ("A", "R[A] = current trace reference", "Effects"),
        Await => ("A, B", "R[A] = await future R[B]", "Effects"),
        Spawn => ("A, Bx", "R[A] = spawn async task (proto=Bx)", "Effects"),
        Perform => (
            "A, B, C",
            "Perform effect B, operation C, result to A",
            "Effects",
        ),
        HandlePush => ("Ax", "Push effect handler scope at offset Ax", "Effects"),
        HandlePop => ("–", "Pop current effect handler scope", "Effects"),
        Resume => ("A", "Resume suspended continuation with value in A", "Effects"),

        // List Ops
        Append => ("A, B", "Append value B to list A", "List Ops"),

        // Type Checks
        IsVariant => (
            "A, Bx",
            "If A is union variant with tag Bx, skip next",
            "Type Checks",
        ),
        Unbox => ("A, B", "A = B.payload (unbox union value)", "Type Checks"),
    }
}

// ---------------------------------------------------------------------------
// Intrinsic metadata
// ---------------------------------------------------------------------------

fn intrinsic_description(id: &IntrinsicId) -> &str {
    use IntrinsicId::*;
    match id {
        Length => "Return the length of a collection or string",
        Count => "Count elements matching a predicate",
        Matches => "Test if a string matches a regex pattern",
        Hash => "Compute hash of a value",
        Diff => "Compute diff between two values",
        Patch => "Apply a patch to a value",
        Redact => "Redact sensitive fields from a value",
        Validate => "Validate a value against a schema",
        TraceRef => "Get current trace reference",
        Print => "Print a value to stdout",
        ToString => "Convert a value to its string representation",
        ToInt => "Parse or convert a value to Int",
        ToFloat => "Parse or convert a value to Float",
        TypeOf => "Return the runtime type name of a value",
        Keys => "Return the keys of a map or record",
        Values => "Return the values of a map or record",
        Contains => "Test if a collection contains a value",
        Join => "Join a list of strings with a separator",
        Split => "Split a string by a separator",
        Trim => "Remove leading and trailing whitespace",
        Upper => "Convert a string to uppercase",
        Lower => "Convert a string to lowercase",
        Replace => "Replace occurrences in a string",
        Slice => "Extract a sub-range from a list or string",
        Append => "Append an element to a list",
        Range => "Generate a list of integers in a range",
        Abs => "Absolute value of a number",
        Min => "Return the minimum of two values",
        Max => "Return the maximum of two values",
        Sort => "Sort a list (optionally by key function)",
        Reverse => "Reverse a list",
        Map => "Apply a function to each element of a list",
        Filter => "Keep elements matching a predicate",
        Reduce => "Fold a list with an accumulator function",
        FlatMap => "Map then flatten one level",
        Zip => "Combine two lists into a list of tuples",
        Enumerate => "Pair each element with its index",
        Any => "True if any element matches a predicate",
        All => "True if all elements match a predicate",
        Find => "Return the first element matching a predicate",
        Position => "Return the index of the first match",
        GroupBy => "Group elements by a key function",
        Chunk => "Split a list into fixed-size chunks",
        Window => "Sliding window over a list",
        Flatten => "Flatten one level of nesting",
        Unique => "Remove duplicate elements",
        Take => "Take the first N elements",
        Drop => "Drop the first N elements",
        First => "Return the first element",
        Last => "Return the last element",
        IsEmpty => "True if the collection is empty",
        Chars => "Split a string into a list of characters",
        StartsWith => "True if a string starts with a prefix",
        EndsWith => "True if a string ends with a suffix",
        IndexOf => "Return the index of a substring or element",
        PadLeft => "Pad a string on the left to a given width",
        PadRight => "Pad a string on the right to a given width",
        Round => "Round a float to the nearest integer",
        Ceil => "Round a float up to the nearest integer",
        Floor => "Round a float down to the nearest integer",
        Sqrt => "Square root of a number",
        Pow => "Raise a number to a power",
        Log => "Natural logarithm of a number",
        Sin => "Sine of an angle in radians",
        Cos => "Cosine of an angle in radians",
        Clamp => "Clamp a value between a minimum and maximum",
        Clone => "Deep-clone a value",
        Sizeof => "Return the size/memory footprint of a value",
        Debug => "Return a debug representation of a value",
        ToSet => "Convert a list to a set",
        HasKey => "True if a map contains a key",
        Merge => "Merge two maps or sets",
        Size => "Return the number of entries in a map or set",
        Add => "Add an element to a set",
        Remove => "Remove an element from a collection",
        Entries => "Return map entries as a list of (key, value) tuples",
        Compose => "Compose two functions into a new function (f ~> g)",
        Format => "Format a string with {} placeholders",
        Partition => "Split a collection into (matching, non-matching) by predicate",
        ReadDir => "List directory entries as a list of strings",
        Exists => "Check if a file or directory path exists",
        Mkdir => "Create a directory (and parent directories)",
        Eval => "Evaluate a block of code at runtime",
        Guardrail => "Enforce safety invariants on a value",
        Pattern => "Define a reusable pattern matcher",
        Exit => "Exit the process with a status code",
    }
}

// ---------------------------------------------------------------------------
// Keyword data
// ---------------------------------------------------------------------------

const KEYWORDS: &[(&str, &str, &str)] = &[
    // Control Flow
    ("if", "Conditional branch", "Control Flow"),
    ("else", "Alternate branch of an if expression", "Control Flow"),
    ("for", "Iterate over a collection or range", "Control Flow"),
    ("in", "Membership test or for-loop iterator", "Control Flow"),
    ("while", "Loop while a condition is true", "Control Flow"),
    ("loop", "Unconditional loop", "Control Flow"),
    ("break", "Exit the enclosing loop", "Control Flow"),
    ("continue", "Skip to the next loop iteration", "Control Flow"),
    ("match", "Pattern-match on a value", "Control Flow"),
    ("return", "Return a value from a cell", "Control Flow"),
    ("halt", "Halt execution with an error", "Control Flow"),
    ("when", "Multi-branch conditional expression", "Control Flow"),
    ("then", "Clause separator in when expressions", "Control Flow"),
    // Declarations
    ("cell", "Define a function (cell)", "Declarations"),
    ("record", "Define a record type", "Declarations"),
    ("enum", "Define an enum type", "Declarations"),
    ("let", "Bind a local variable", "Declarations"),
    ("type", "Define a type alias", "Declarations"),
    ("fn", "Anonymous function / lambda keyword", "Declarations"),
    ("trait", "Define a trait", "Declarations"),
    ("impl", "Implement a trait for a type", "Declarations"),
    ("mod", "Module declaration", "Declarations"),
    ("const", "Declare a compile-time constant", "Declarations"),
    ("pub", "Mark a declaration as public", "Declarations"),
    ("import", "Import symbols from another module", "Declarations"),
    ("from", "Specify import source", "Declarations"),
    ("use", "Bring a tool or module into scope", "Declarations"),
    // Effects
    ("perform", "Invoke an algebraic effect operation", "Effects"),
    ("handle", "Install an effect handler", "Effects"),
    ("resume", "Resume a suspended continuation", "Effects"),
    ("emit", "Emit an output value", "Effects"),
    ("yield", "Yield a value from a generator cell", "Effects"),
    ("defer", "Register cleanup code for scope exit (LIFO order)", "Effects"),
    ("async", "Mark a cell as asynchronous", "Effects"),
    ("await", "Await the result of a future", "Effects"),
    ("parallel", "Execute futures in parallel", "Effects"),
    ("try", "Try an expression that may fail", "Effects"),
    // Data
    ("null", "The null value literal", "Data"),
    ("result", "The result[T, E] type constructor", "Data"),
    ("ok", "Success variant of result", "Data"),
    ("err", "Error variant of result", "Data"),
    ("list", "List type constructor", "Data"),
    ("map", "Map type constructor", "Data"),
    ("set", "Set type constructor", "Data"),
    ("tuple", "Tuple type constructor", "Data"),
    ("union", "Union type combinator", "Data"),
    ("bool", "Boolean type name", "Data"),
    ("int", "Integer type name", "Data"),
    ("float", "Float type name", "Data"),
    ("string", "String type name", "Data"),
    ("bytes", "Byte buffer type name", "Data"),
    ("json", "JSON type name", "Data"),
    // AI/Tool
    ("tool", "Declare an external tool", "AI/Tool"),
    ("grant", "Grant tool-call permissions with policy", "AI/Tool"),
    ("expect", "Declare expected output constraints", "AI/Tool"),
    ("schema", "Declare a schema for validation", "AI/Tool"),
    ("role", "Define a role for tool access control", "AI/Tool"),
    ("where", "Field constraint clause", "AI/Tool"),
    ("as", "Alias or type cast keyword", "AI/Tool"),
    ("with", "Attach handler or options", "AI/Tool"),
    ("step", "Pipeline stage declaration", "AI/Tool"),
    // Modifiers
    ("mut", "Mark a binding as mutable", "Modifiers"),
    ("self", "Reference to the current instance", "Modifiers"),
    ("end", "Block terminator", "Modifiers"),
    ("comptime", "Compile-time evaluation block", "Modifiers"),
    ("macro", "Macro definition", "Modifiers"),
    ("extern", "Foreign function interface declaration", "Modifiers"),
    ("and", "Logical AND operator", "Modifiers"),
    ("or", "Logical OR operator", "Modifiers"),
    ("not", "Logical NOT operator", "Modifiers"),
    ("is", "Type test operator", "Modifiers"),
];

// ---------------------------------------------------------------------------
// Operator data
// ---------------------------------------------------------------------------

const OPERATORS: &[(&str, &str, &str, &str)] = &[
    // Arithmetic
    ("+", "Add", "Addition or unary positive", "Arithmetic"),
    ("-", "Sub", "Subtraction or unary negation", "Arithmetic"),
    ("*", "Mul", "Multiplication", "Arithmetic"),
    ("/", "Div", "Division", "Arithmetic"),
    ("%", "Mod", "Remainder (modulo)", "Arithmetic"),
    ("**", "Pow", "Exponentiation", "Arithmetic"),
    ("//", "FloorDiv", "Floor division (integer division)", "Arithmetic"),
    // Comparison
    ("==", "Eq", "Equality test", "Comparison"),
    ("!=", "Neq", "Inequality test", "Comparison"),
    ("<", "Lt", "Less than", "Comparison"),
    ("<=", "Le", "Less than or equal", "Comparison"),
    (">", "Gt", "Greater than", "Comparison"),
    (">=", "Ge", "Greater than or equal", "Comparison"),
    // Logical
    ("and", "And", "Logical AND (short-circuit)", "Logical"),
    ("or", "Or", "Logical OR (short-circuit)", "Logical"),
    ("not", "Not", "Logical negation", "Logical"),
    // Bitwise
    ("&", "BitAnd", "Bitwise AND", "Bitwise"),
    ("|", "BitOr", "Bitwise OR", "Bitwise"),
    ("^", "BitXor", "Bitwise XOR", "Bitwise"),
    ("~", "BitNot", "Bitwise NOT (unary)", "Bitwise"),
    ("<<", "Shl", "Left shift", "Bitwise"),
    (">>", "Shr", "Right shift", "Bitwise"),
    // Assignment
    ("=", "Assign", "Variable assignment", "Assignment"),
    ("+=", "AddAssign", "Add and assign", "Assignment"),
    ("-=", "SubAssign", "Subtract and assign", "Assignment"),
    ("*=", "MulAssign", "Multiply and assign", "Assignment"),
    ("/=", "DivAssign", "Divide and assign", "Assignment"),
    ("%=", "ModAssign", "Modulo and assign", "Assignment"),
    ("**=", "PowAssign", "Exponentiate and assign", "Assignment"),
    ("&=", "BitAndAssign", "Bitwise AND and assign", "Assignment"),
    ("|=", "BitOrAssign", "Bitwise OR and assign", "Assignment"),
    ("^=", "BitXorAssign", "Bitwise XOR and assign", "Assignment"),
    ("//=", "FloorDivAssign", "Floor-divide and assign", "Assignment"),
    // Special
    ("|>", "Pipe", "Pipe value through functions (eager, left-to-right)", "Special"),
    ("~>", "Compose", "Compose functions into a new function (lazy)", "Special"),
    ("??", "NullCoalesce", "Null coalescing — return left if non-null, else right", "Special"),
    ("?.", "NullSafeAccess", "Null-safe field access", "Special"),
    ("?[]", "NullSafeIndex", "Null-safe index access", "Special"),
    ("!", "NullAssert", "Assert value is non-null", "Special"),
    ("?", "Optional", "Optional type suffix (T? = T | Null)", "Special"),
    ("...", "Spread", "Spread / variadic parameter", "Special"),
    ("..", "RangeExcl", "Exclusive range (start..end)", "Special"),
    ("..=", "RangeIncl", "Inclusive range (start..=end)", "Special"),
    ("=>", "FatArrow", "Match arm separator", "Special"),
    ("++", "Concat", "String or list concatenation", "Special"),
    ("->", "Arrow", "Return type annotation", "Special"),
];

// ---------------------------------------------------------------------------
// Builtin type data
// ---------------------------------------------------------------------------

const BUILTIN_TYPES: &[(&str, &str, &str)] = &[
    ("Int", "64-bit signed integer", "Scalar"),
    ("Float", "64-bit IEEE 754 floating point", "Scalar"),
    ("String", "UTF-8 string", "Scalar"),
    ("Bool", "Boolean (true / false)", "Scalar"),
    ("Null", "The null value type", "Scalar"),
    ("Bytes", "Byte buffer", "Scalar"),
    ("Json", "Arbitrary JSON value", "Scalar"),
    ("List", "Ordered, growable sequence — list[T]", "Collection"),
    ("Map", "Key-value mapping — map[K, V]", "Collection"),
    ("Set", "Unordered unique elements — set[T]", "Collection"),
    ("Tuple", "Fixed-size heterogeneous sequence — tuple[T, U, ...]", "Collection"),
    ("Result", "Success or error — result[T, E]", "Composite"),
    ("Any", "Dynamic type — accepts any value", "Composite"),
];

// ---------------------------------------------------------------------------
// Builtin function data
// ---------------------------------------------------------------------------

const BUILTIN_FUNCTIONS: &[(&str, &str, &str, &str)] = &[
    ("print", "Print a value to stdout", "Null", "IO"),
    ("len", "Return length of a collection or string", "Int", "Collection"),
    ("length", "Alias for len", "Int", "Collection"),
    ("append", "Append an element to a list", "T", "Collection"),
    ("range", "Generate a list of integers in a range", "List[Int]", "Collection"),
    ("to_string", "Convert a value to String", "String", "Conversion"),
    ("str", "Alias for to_string", "String", "Conversion"),
    ("to_int", "Convert a value to Int", "Int", "Conversion"),
    ("int", "Alias for to_int", "Int", "Conversion"),
    ("to_float", "Convert a value to Float", "Float", "Conversion"),
    ("float", "Alias for to_float", "Float", "Conversion"),
    ("type_of", "Return the runtime type name of a value", "String", "Reflection"),
    ("keys", "Return the keys of a map or record", "List[String]", "Collection"),
    ("values", "Return the values of a map or record", "List[Any]", "Collection"),
    ("contains", "Test if a collection contains a value", "Bool", "Collection"),
    ("join", "Join a list of strings with a separator", "String", "String"),
    ("split", "Split a string by a separator", "List[String]", "String"),
    ("trim", "Remove leading and trailing whitespace", "String", "String"),
    ("upper", "Convert a string to uppercase", "String", "String"),
    ("lower", "Convert a string to lowercase", "String", "String"),
    ("replace", "Replace occurrences in a string", "String", "String"),
    ("abs", "Absolute value of a number", "T", "Math"),
    ("min", "Return the minimum of two values", "T", "Math"),
    ("max", "Return the maximum of two values", "T", "Math"),
    ("sort", "Sort a list", "T", "Collection"),
    ("reverse", "Reverse a list", "T", "Collection"),
    ("filter", "Keep elements matching a predicate", "T", "Collection"),
    ("map", "Apply a function to each element", "T", "Collection"),
    ("reduce", "Fold a list with an accumulator", "T", "Collection"),
    ("parse_json", "Parse a JSON string into a value", "Any", "IO"),
    ("to_json", "Serialize a value to a JSON string", "String", "IO"),
    ("read_file", "Read a file's contents as a string", "String", "IO"),
    ("write_file", "Write a string to a file", "Null", "IO"),
    ("timestamp", "Current Unix timestamp in seconds", "Float", "IO"),
    ("random", "Generate a random float in [0, 1)", "Float", "Math"),
    ("get_env", "Read an environment variable", "Any", "IO"),
    ("hash", "Compute hash of a value", "Int", "Reflection"),
    ("not", "Logical negation", "Bool", "Logic"),
    ("slice", "Extract a sub-range from a list or string", "T", "Collection"),
    ("count", "Count elements matching a predicate", "Int", "Collection"),
    ("matches", "Test if a string matches a pattern", "Bool", "String"),
    ("starts_with", "True if a string starts with a prefix", "Bool", "String"),
    ("ends_with", "True if a string ends with a suffix", "Bool", "String"),
    ("is_empty", "True if a collection or string is empty", "Bool", "Collection"),
];

// ---------------------------------------------------------------------------
// Statement form data
// ---------------------------------------------------------------------------

const STATEMENT_FORMS: &[(&str, &str)] = &[
    ("Let", "Variable binding: let x = expr"),
    ("If", "Conditional statement: if cond ... else ... end"),
    ("For", "For loop: for x in collection ... end"),
    ("Match", "Pattern matching: match expr ... end"),
    ("Return", "Return a value from a cell"),
    ("Halt", "Halt execution with an error message"),
    ("Assign", "Variable assignment: x = expr"),
    ("While", "While loop: while cond ... end"),
    ("Loop", "Infinite loop: loop ... end"),
    ("Break", "Break from the enclosing loop"),
    ("Continue", "Continue to the next iteration"),
    ("Emit", "Emit an output value"),
    ("CompoundAssign", "Compound assignment: x += expr, x -= expr, etc."),
    ("Defer", "Deferred cleanup: defer ... end (LIFO order)"),
    ("Yield", "Yield a value from a generator cell"),
    ("Expr", "Expression evaluated for side effects"),
];

// ---------------------------------------------------------------------------
// Expression form data
// ---------------------------------------------------------------------------

const EXPRESSION_FORMS: &[(&str, &str)] = &[
    ("IntLit", "Integer literal: 42, 0xFF, 0b1010"),
    ("FloatLit", "Float literal: 3.14, 1e-5"),
    ("StringLit", "String literal: \"hello\""),
    ("StringInterp", "String interpolation: \"Hello, {name}!\""),
    ("BoolLit", "Boolean literal: true, false"),
    ("NullLit", "Null literal: null"),
    ("Ident", "Identifier reference: x, my_var"),
    ("ListLit", "List literal: [1, 2, 3]"),
    ("MapLit", "Map literal: {\"a\": 1, \"b\": 2}"),
    ("RecordLit", "Record constructor: Point(x: 1, y: 2)"),
    ("TupleLit", "Tuple literal: (1, \"hello\", true)"),
    ("SetLit", "Set literal: {1, 2, 3}"),
    ("BinOp", "Binary operation: a + b, x == y"),
    ("UnaryOp", "Unary operation: -x, not flag"),
    ("Call", "Function call: foo(a, b)"),
    ("ToolCall", "Tool invocation: tool_name(args)"),
    ("DotAccess", "Field access: record.field"),
    ("IndexAccess", "Index access: list[0]"),
    ("Lambda", "Anonymous function: fn(x) -> x * 2 end"),
    ("RangeExpr", "Range expression: 1..10, 1..=10"),
    ("TryExpr", "Try expression for error handling"),
    ("NullCoalesce", "Null coalescing: x ?? default"),
    ("NullSafeAccess", "Null-safe access: x?.field"),
    ("NullSafeIndex", "Null-safe index: x?[0]"),
    ("Pipe", "Pipe operator: data |> transform()"),
    ("IsType", "Type test: expr is Type"),
    ("TypeCast", "Type cast: expr as Type"),
    ("WhenExpr", "When expression: when cond -> val ... end"),
    ("ComptimeExpr", "Compile-time evaluation: comptime ... end"),
    ("Perform", "Effect invocation: perform Effect.op(args)"),
    ("HandleExpr", "Effect handler: handle body with ... end"),
    ("ResumeExpr", "Resume continuation: resume(value)"),
    ("AwaitExpr", "Await a future: await expr"),
    ("Comprehension", "List comprehension: [expr for x in list]"),
    ("MatchExpr", "Match as expression"),
    ("BlockExpr", "Block expression evaluated to a value"),
    ("SpreadExpr", "Spread expression: ...items"),
    ("IfExpr", "If as expression: if cond then a else b"),
    ("NullAssert", "Null assertion: expr!"),
    ("RoleBlock", "Role-scoped block"),
    ("ExpectSchema", "Schema expectation"),
];

// ---------------------------------------------------------------------------
// CLI command data
// ---------------------------------------------------------------------------

const CLI_COMMANDS: &[(&str, &str)] = &[
    ("lumen check <file>", "Type-check a Lumen source file"),
    ("lumen run <file>", "Compile and execute (default cell: main)"),
    ("lumen emit <file>", "Emit LIR JSON to stdout"),
    ("lumen repl", "Start the interactive REPL"),
    ("lumen fmt <files>", "Format Lumen source files"),
    ("lumen doc <file>", "Generate documentation"),
    ("lumen lint <files>", "Run linter checks"),
    ("lumen test <file>", "Run tests in a Lumen file"),
    ("lumen ci", "Run all checks for continuous integration"),
    ("lumen build wasm --target <target>", "Build WASM output (web, nodejs, wasi)"),
    ("lumen trace show <run-id>", "Display trace events for a run"),
    ("lumen cache clear", "Clear tool result cache"),
    ("lumen lang-ref", "Print the language reference"),
];

// ---------------------------------------------------------------------------
// generate()
// ---------------------------------------------------------------------------

pub fn generate() -> LanguageReference {
    let version = env!("CARGO_PKG_VERSION").to_string();

    let keywords: Vec<KeywordEntry> = KEYWORDS
        .iter()
        .map(|(kw, desc, cat)| KeywordEntry {
            keyword: kw.to_string(),
            description: desc.to_string(),
            category: cat.to_string(),
        })
        .collect();

    let operators: Vec<OperatorEntry> = OPERATORS
        .iter()
        .map(|(sym, name, desc, cat)| OperatorEntry {
            symbol: sym.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            category: cat.to_string(),
        })
        .collect();

    let builtin_types: Vec<TypeEntry> = BUILTIN_TYPES
        .iter()
        .map(|(name, desc, cat)| TypeEntry {
            name: name.to_string(),
            description: desc.to_string(),
            category: cat.to_string(),
        })
        .collect();

    let builtin_functions: Vec<BuiltinEntry> = BUILTIN_FUNCTIONS
        .iter()
        .map(|(name, desc, ret, cat)| BuiltinEntry {
            name: name.to_string(),
            description: desc.to_string(),
            return_type: ret.to_string(),
            category: cat.to_string(),
        })
        .collect();

    let opcodes: Vec<OpcodeEntry> = OpCode::iter()
        .map(|op| {
            let hex = format!("0x{:02X}", op as u8);
            let name = format!("{:?}", op);
            let (encoding, description, category) = opcode_info(&op);
            OpcodeEntry {
                name,
                hex,
                encoding: encoding.to_string(),
                description: description.to_string(),
                category: category.to_string(),
            }
        })
        .collect();

    let intrinsics: Vec<IntrinsicEntry> = IntrinsicId::iter()
        .map(|id| {
            let name = format!("{:?}", id);
            let description = intrinsic_description(&id).to_string();
            IntrinsicEntry {
                name,
                id: id as u8,
                description,
            }
        })
        .collect();

    let statement_forms: Vec<SyntaxEntry> = STATEMENT_FORMS
        .iter()
        .map(|(name, desc)| SyntaxEntry {
            name: name.to_string(),
            description: desc.to_string(),
        })
        .collect();

    let expression_forms: Vec<SyntaxEntry> = EXPRESSION_FORMS
        .iter()
        .map(|(name, desc)| SyntaxEntry {
            name: name.to_string(),
            description: desc.to_string(),
        })
        .collect();

    let cli_commands: Vec<CliEntry> = CLI_COMMANDS
        .iter()
        .map(|(cmd, desc)| CliEntry {
            command: cmd.to_string(),
            description: desc.to_string(),
        })
        .collect();

    LanguageReference {
        version,
        keywords,
        operators,
        builtin_types,
        builtin_functions,
        opcodes,
        intrinsics,
        statement_forms,
        expression_forms,
        cli_commands,
    }
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

pub fn format_markdown(ref_data: &LanguageReference) -> String {
    let mut md = String::with_capacity(16_384);

    md.push_str(&format!(
        "# Lumen Language Reference\n\n**Version**: {}\n\n",
        ref_data.version
    ));

    // -- Keywords --
    md.push_str("## Keywords\n\n");
    md.push_str("| Keyword | Description | Category |\n");
    md.push_str("|---------|-------------|----------|\n");
    for kw in &ref_data.keywords {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            kw.keyword, kw.description, kw.category
        ));
    }
    md.push('\n');

    // -- Operators --
    md.push_str("## Operators\n\n");
    md.push_str("| Symbol | Name | Description | Category |\n");
    md.push_str("|--------|------|-------------|----------|\n");
    for op in &ref_data.operators {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            op.symbol, op.name, op.description, op.category
        ));
    }
    md.push('\n');

    // -- Builtin Types --
    md.push_str("## Builtin Types\n\n");
    md.push_str("| Type | Description | Category |\n");
    md.push_str("|------|-------------|----------|\n");
    for ty in &ref_data.builtin_types {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            ty.name, ty.description, ty.category
        ));
    }
    md.push('\n');

    // -- Builtin Functions --
    md.push_str("## Builtin Functions\n\n");
    md.push_str("| Function | Description | Return Type | Category |\n");
    md.push_str("|----------|-------------|-------------|----------|\n");
    for f in &ref_data.builtin_functions {
        md.push_str(&format!(
            "| `{}` | {} | `{}` | {} |\n",
            f.name, f.description, f.return_type, f.category
        ));
    }
    md.push('\n');

    // -- Opcodes --
    md.push_str("## Opcodes\n\n");
    md.push_str("| Opcode | Hex | Encoding | Description | Category |\n");
    md.push_str("|--------|-----|----------|-------------|----------|\n");
    for op in &ref_data.opcodes {
        md.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            op.name, op.hex, op.encoding, op.description, op.category
        ));
    }
    md.push('\n');

    // -- Intrinsics --
    md.push_str("## Intrinsics\n\n");
    md.push_str("| Name | ID | Description |\n");
    md.push_str("|------|----|-------------|\n");
    for i in &ref_data.intrinsics {
        md.push_str(&format!("| `{}` | {} | {} |\n", i.name, i.id, i.description));
    }
    md.push('\n');

    // -- Statement Forms --
    md.push_str("## Statement Forms\n\n");
    md.push_str("| Form | Description |\n");
    md.push_str("|------|-------------|\n");
    for s in &ref_data.statement_forms {
        md.push_str(&format!("| `{}` | {} |\n", s.name, s.description));
    }
    md.push('\n');

    // -- Expression Forms --
    md.push_str("## Expression Forms\n\n");
    md.push_str("| Form | Description |\n");
    md.push_str("|------|-------------|\n");
    for e in &ref_data.expression_forms {
        md.push_str(&format!("| `{}` | {} |\n", e.name, e.description));
    }
    md.push('\n');

    // -- CLI Commands --
    md.push_str("## CLI Commands\n\n");
    md.push_str("| Command | Description |\n");
    md.push_str("|---------|-------------|\n");
    for c in &ref_data.cli_commands {
        md.push_str(&format!("| `{}` | {} |\n", c.command, c.description));
    }
    md.push('\n');

    md
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_is_nonempty() {
        let lang_ref = generate();
        assert!(!lang_ref.keywords.is_empty());
        assert!(!lang_ref.operators.is_empty());
        assert!(!lang_ref.opcodes.is_empty());
        assert!(!lang_ref.intrinsics.is_empty());
        assert!(!lang_ref.builtin_types.is_empty());
        assert!(!lang_ref.builtin_functions.is_empty());
        assert!(!lang_ref.statement_forms.is_empty());
        assert!(!lang_ref.expression_forms.is_empty());
    }

    #[test]
    fn test_opcode_count_matches_enum() {
        use strum::EnumCount;
        let lang_ref = generate();
        assert_eq!(lang_ref.opcodes.len(), OpCode::COUNT);
    }

    #[test]
    fn test_intrinsic_count_matches_enum() {
        use strum::EnumCount;
        let lang_ref = generate();
        assert_eq!(lang_ref.intrinsics.len(), IntrinsicId::COUNT);
    }

    #[test]
    fn test_markdown_output() {
        let lang_ref = generate();
        let md = format_markdown(&lang_ref);
        assert!(md.contains("# Lumen Language Reference"));
        assert!(md.contains("## Keywords"));
        assert!(md.contains("## Operators"));
        assert!(md.contains("## Opcodes"));
    }
}
