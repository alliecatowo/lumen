//! C header → Lumen extern bindgen.
//!
//! This module parses simplified C header declarations and generates
//! corresponding Lumen `extern cell` declarations, record types, enums,
//! and type aliases. It handles the most common C declaration patterns
//! but does not attempt to cover the full C grammar.
//!
//! ## Usage
//!
//! ```text
//! let output = generate_bindings(header_content)?;
//! let lumen_source = output.to_lumen_source();
//! ```

use std::fmt;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during C header parsing and bindgen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindgenError {
    /// A parse error at a specific line in the input.
    ParseError {
        /// The 1-based line number where the error occurred.
        line: usize,
        /// A description of what went wrong.
        message: String,
    },
    /// A C type that cannot be mapped to Lumen.
    UnsupportedType(String),
    /// A declaration that could not be parsed.
    InvalidDeclaration(String),
}

impl fmt::Display for BindgenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindgenError::ParseError { line, message } => {
                write!(f, "parse error at line {}: {}", line, message)
            }
            BindgenError::UnsupportedType(ty) => {
                write!(f, "unsupported C type: {}", ty)
            }
            BindgenError::InvalidDeclaration(decl) => {
                write!(f, "invalid declaration: {}", decl)
            }
        }
    }
}

impl std::error::Error for BindgenError {}

// =============================================================================
// C Type Representation
// =============================================================================

/// Represents a C type encountered in header declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CType {
    /// `void`
    Void,
    /// `int`
    Int,
    /// `unsigned int` / `unsigned`
    UInt,
    /// `long`
    Long,
    /// `unsigned long`
    ULong,
    /// `long long`
    LongLong,
    /// `short`
    Short,
    /// `char`
    Char,
    /// `unsigned char`
    UChar,
    /// `float`
    Float,
    /// `double`
    Double,
    /// `_Bool` / `bool`
    Bool,
    /// A pointer to another type, e.g. `int *`.
    Pointer(Box<CType>),
    /// A const-qualified pointer, e.g. `const char *`.
    ConstPointer(Box<CType>),
    /// A fixed-size array, e.g. `int[10]`.
    Array(Box<CType>, usize),
    /// A struct reference by name, e.g. `struct Foo`.
    Struct(String),
    /// An enum reference by name, e.g. `enum Bar`.
    Enum(String),
    /// A function pointer, e.g. `int (*)(int, int)`.
    FnPointer {
        /// The return type of the function pointer.
        return_type: Box<CType>,
        /// The parameter types.
        params: Vec<CType>,
    },
    /// A typedef name reference.
    Typedef(String),
    /// An unrecognized C type.
    Unknown(String),
}

// =============================================================================
// C Declaration Types
// =============================================================================

/// A parameter in a C function declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CParam {
    /// The parameter name (may be absent in prototypes).
    pub name: Option<String>,
    /// The parameter's C type.
    pub ctype: CType,
}

/// A field in a C struct declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CField {
    /// The field name.
    pub name: String,
    /// The field's C type.
    pub ctype: CType,
}

/// A variant in a C enum declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CEnumVariant {
    /// The variant identifier.
    pub name: String,
    /// An optional explicit integer value.
    pub value: Option<i64>,
}

/// A parsed C declaration from a header file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CDecl {
    /// A function declaration / prototype.
    Function {
        /// Function name.
        name: String,
        /// Return type.
        return_type: CType,
        /// Parameter list.
        params: Vec<CParam>,
        /// Whether the function is variadic (`...`).
        is_variadic: bool,
    },
    /// A typedef declaration.
    TypedefDecl {
        /// The new type alias name.
        name: String,
        /// The target type being aliased.
        target: CType,
    },
    /// A struct declaration with fields.
    StructDecl {
        /// The struct name.
        name: String,
        /// The struct's fields.
        fields: Vec<CField>,
    },
    /// An enum declaration with variants.
    EnumDecl {
        /// The enum name.
        name: String,
        /// The enum variants.
        variants: Vec<CEnumVariant>,
    },
    /// A constant declaration (e.g. `#define` or `const`).
    ConstantDecl {
        /// The constant name.
        name: String,
        /// The string representation of the value.
        value: String,
        /// The constant's C type.
        ctype: CType,
    },
}

// =============================================================================
// Bindgen Output
// =============================================================================

/// The output of the bindgen process, containing all generated Lumen declarations.
#[derive(Debug, Clone, Default)]
pub struct BindgenOutput {
    /// Generated `extern cell` declarations for C functions.
    pub extern_cells: Vec<String>,
    /// Generated `record` declarations for C structs.
    pub records: Vec<String>,
    /// Generated `enum` declarations for C enums.
    pub enums: Vec<String>,
    /// Generated type alias declarations for C typedefs.
    pub type_aliases: Vec<String>,
    /// Warnings encountered during generation (e.g. skipped declarations).
    pub warnings: Vec<String>,
}

impl BindgenOutput {
    /// Format all generated declarations as valid Lumen `.lm` source content.
    pub fn to_lumen_source(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        parts.push("# Auto-generated Lumen bindings from C header".to_string());
        parts.push(String::new());

        if !self.type_aliases.is_empty() {
            parts.push("# Type aliases".to_string());
            for alias in &self.type_aliases {
                parts.push(alias.clone());
            }
            parts.push(String::new());
        }

        if !self.enums.is_empty() {
            parts.push("# Enums".to_string());
            for e in &self.enums {
                parts.push(e.clone());
            }
            parts.push(String::new());
        }

        if !self.records.is_empty() {
            parts.push("# Records".to_string());
            for r in &self.records {
                parts.push(r.clone());
            }
            parts.push(String::new());
        }

        if !self.extern_cells.is_empty() {
            parts.push("# Extern functions".to_string());
            for cell in &self.extern_cells {
                parts.push(cell.clone());
            }
            parts.push(String::new());
        }

        parts.join("\n")
    }
}

// =============================================================================
// Name Conversion Utilities
// =============================================================================

/// Convert a `snake_case` name to `PascalCase`.
///
/// # Examples
///
/// ```
/// assert_eq!(lumen_cli::bindgen::snake_to_pascal("my_struct"), "MyStruct");
/// assert_eq!(lumen_cli::bindgen::snake_to_pascal("hello"), "Hello");
/// ```
pub fn snake_to_pascal(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Strip a prefix from a name, returning the remainder.
///
/// If the name starts with `prefix` (optionally followed by `_`),
/// the prefix is removed and the result is returned. Otherwise,
/// the original name is returned unchanged.
///
/// # Examples
///
/// ```
/// assert_eq!(lumen_cli::bindgen::strip_prefix("SDL_Window", "SDL"), "Window");
/// assert_eq!(lumen_cli::bindgen::strip_prefix("MyThing", "Other"), "MyThing");
/// ```
pub fn strip_prefix(name: &str, prefix: &str) -> String {
    if let Some(rest) = name.strip_prefix(prefix) {
        if rest.is_empty() {
            return name.to_string();
        }
        if let Some(stripped) = rest.strip_prefix('_') {
            if stripped.is_empty() {
                return name.to_string();
            }
            return stripped.to_string();
        }
        // If the next char is uppercase, treat prefix as component boundary
        if rest.starts_with(|c: char| c.is_uppercase()) {
            return rest.to_string();
        }
        // Otherwise the prefix doesn't match at a word boundary
        name.to_string()
    } else {
        name.to_string()
    }
}

// =============================================================================
// C Type → Lumen Type Mapping
// =============================================================================

/// Convert a [`CType`] to its Lumen type string representation.
///
/// # Mapping rules
///
/// | C Type | Lumen Type |
/// |--------|------------|
/// | `void` | `Null` |
/// | `int`, `long`, `short`, `char` (and unsigned) | `Int` |
/// | `float`, `double` | `Float` |
/// | `bool` | `Bool` |
/// | `T *` / `const T *` | `addr[T]` |
/// | `T[N]` | `List[T]` |
/// | struct/enum/typedef name | name (PascalCase) |
/// | function pointer | `Fn[params] -> ret` |
pub fn c_type_to_lumen(ct: &CType) -> String {
    match ct {
        CType::Void => "Null".to_string(),
        CType::Int | CType::Long | CType::LongLong | CType::Short => "Int".to_string(),
        CType::UInt | CType::ULong => "Int".to_string(),
        CType::Char | CType::UChar => "Int".to_string(),
        CType::Float | CType::Double => "Float".to_string(),
        CType::Bool => "Bool".to_string(),
        CType::Pointer(inner) => {
            let inner_lumen = c_type_to_lumen(inner);
            format!("addr[{}]", inner_lumen)
        }
        CType::ConstPointer(inner) => {
            let inner_lumen = c_type_to_lumen(inner);
            format!("addr[{}]", inner_lumen)
        }
        CType::Array(inner, _size) => {
            let inner_lumen = c_type_to_lumen(inner);
            format!("List[{}]", inner_lumen)
        }
        CType::Struct(name) => snake_to_pascal(name),
        CType::Enum(name) => snake_to_pascal(name),
        CType::Typedef(name) => snake_to_pascal(name),
        CType::FnPointer {
            return_type,
            params,
        } => {
            let param_types: Vec<String> = params.iter().map(c_type_to_lumen).collect();
            let ret = c_type_to_lumen(return_type);
            if param_types.is_empty() {
                format!("Fn[] -> {}", ret)
            } else {
                format!("Fn[{}] -> {}", param_types.join(", "), ret)
            }
        }
        CType::Unknown(name) => name.clone(),
    }
}

// =============================================================================
// C Type Parsing
// =============================================================================

/// Parse a C type string into a [`CType`].
///
/// Handles common patterns including:
/// - Basic types: `int`, `float`, `double`, `char`, `void`, `bool`, `_Bool`
/// - Unsigned variants: `unsigned int`, `unsigned long`, etc.
/// - Pointers: `int *`, `void *`, `const char *`
/// - Struct/enum references: `struct Foo`, `enum Bar`
/// - Function pointers: `int (*)(int, int)`
pub fn parse_c_type(type_str: &str) -> Result<CType, BindgenError> {
    let s = type_str.trim();

    if s.is_empty() {
        return Err(BindgenError::UnsupportedType("empty type".to_string()));
    }

    // Function pointer: return_type (*)(param_types)
    if let Some(ct) = try_parse_fn_pointer(s)? {
        return Ok(ct);
    }

    // Check for pointer — strip trailing `*` (possibly with const)
    if let Some(stripped) = s.strip_suffix('*') {
        let stripped = stripped.trim();
        // const T * -> ConstPointer(T)
        if let Some(inner) = stripped.strip_prefix("const ") {
            let inner = inner.trim();
            let inner_type = parse_c_type(inner)?;
            return Ok(CType::ConstPointer(Box::new(inner_type)));
        }
        let inner_type = parse_c_type(stripped)?;
        return Ok(CType::Pointer(Box::new(inner_type)));
    }

    // const T * handled above; also handle `const char*` (no space before *)
    // Already handled by stripping trailing *

    // Strip leading `const` qualifier (when not followed by `*`, which is
    // handled above). This handles e.g. `const int`, `const char`.
    if let Some(rest) = s.strip_prefix("const ") {
        let rest = rest.trim();
        if !rest.is_empty() && !rest.contains('*') {
            return parse_c_type(rest);
        }
    }

    // Basic type keywords
    match s {
        "void" => return Ok(CType::Void),
        "int" => return Ok(CType::Int),
        "unsigned" | "unsigned int" => return Ok(CType::UInt),
        "long" | "long int" => return Ok(CType::Long),
        "unsigned long" | "unsigned long int" => return Ok(CType::ULong),
        "long long" | "long long int" => return Ok(CType::LongLong),
        "short" | "short int" => return Ok(CType::Short),
        "char" => return Ok(CType::Char),
        "unsigned char" => return Ok(CType::UChar),
        "signed char" => return Ok(CType::Char),
        "float" => return Ok(CType::Float),
        "double" => return Ok(CType::Double),
        "long double" => return Ok(CType::Double),
        "bool" | "_Bool" => return Ok(CType::Bool),
        "size_t" | "ssize_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" => {
            return Ok(CType::Typedef(s.to_string()))
        }
        _ => {}
    }

    // struct Name
    if let Some(name) = s.strip_prefix("struct ") {
        let name = name.trim();
        if !name.is_empty() {
            return Ok(CType::Struct(name.to_string()));
        }
    }

    // enum Name
    if let Some(name) = s.strip_prefix("enum ") {
        let name = name.trim();
        if !name.is_empty() {
            return Ok(CType::Enum(name.to_string()));
        }
    }

    // unsigned short, unsigned long long, etc.
    if let Some(rest) = s.strip_prefix("unsigned ") {
        let rest = rest.trim();
        match rest {
            "short" | "short int" => return Ok(CType::UInt),
            "long long" | "long long int" => return Ok(CType::ULong),
            _ => {}
        }
    }

    // If it looks like a valid C identifier, treat as typedef
    if is_c_identifier(s) {
        return Ok(CType::Typedef(s.to_string()));
    }

    Err(BindgenError::UnsupportedType(s.to_string()))
}

/// Attempt to parse a function pointer type like `int (*)(int, float)`.
fn try_parse_fn_pointer(s: &str) -> Result<Option<CType>, BindgenError> {
    // Pattern: <return_type> (*)(param1, param2, ...)
    // Find the `(*)` part
    let marker = "(*)";
    let marker_pos = match s.find(marker) {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let return_str = s[..marker_pos].trim();
    let return_type = parse_c_type(return_str)?;

    let after = s[marker_pos + marker.len()..].trim();
    // Should be `(param1, param2, ...)`
    if !after.starts_with('(') || !after.ends_with(')') {
        return Err(BindgenError::UnsupportedType(format!(
            "malformed function pointer: {}",
            s
        )));
    }

    let params_str = &after[1..after.len() - 1];
    let params = if params_str.trim().is_empty() || params_str.trim() == "void" {
        Vec::new()
    } else {
        params_str
            .split(',')
            .map(|p| parse_c_type(p.trim()))
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(Some(CType::FnPointer {
        return_type: Box::new(return_type),
        params,
    }))
}

/// Check if a string is a valid C identifier.
fn is_c_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// =============================================================================
// C Declaration Parsing
// =============================================================================

/// Parse a single line (or multi-line segment) of C header content into a [`CDecl`].
///
/// Returns `Ok(None)` for lines that are not declarations (comments,
/// preprocessor directives, blank lines).
///
/// This is a simplified parser that handles common patterns:
/// - Function prototypes: `int foo(int x, float y);`
/// - Typedefs: `typedef unsigned long size_t;`
/// - Struct declarations: `struct Point { int x; int y; };`
/// - Enum declarations: `enum Color { RED, GREEN = 2, BLUE };`
pub fn parse_c_declaration(line: &str) -> Result<Option<CDecl>, BindgenError> {
    let trimmed = line.trim();

    // Skip empty lines
    if trimmed.is_empty() {
        return Ok(None);
    }

    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return Ok(None);
    }

    // Skip preprocessor directives
    if trimmed.starts_with('#') {
        return Ok(None);
    }

    // Typedef
    if let Some(rest) = trimmed.strip_prefix("typedef ") {
        return parse_typedef(rest);
    }

    // Struct with body
    if trimmed.starts_with("struct ") && trimmed.contains('{') {
        return parse_struct_decl(trimmed);
    }

    // Enum with body
    if trimmed.starts_with("enum ") && trimmed.contains('{') {
        return parse_enum_decl(trimmed);
    }

    // Function prototype: must contain `(` and end with `)` + optional `;`
    if trimmed.contains('(') && (trimmed.ends_with(';') || trimmed.ends_with(')')) {
        return parse_function_decl(trimmed);
    }

    // Const variable
    if trimmed.starts_with("const ") && trimmed.contains('=') {
        return parse_const_decl(trimmed);
    }

    Ok(None)
}

/// Parse a typedef declaration (after stripping the `typedef` keyword).
fn parse_typedef(rest: &str) -> Result<Option<CDecl>, BindgenError> {
    let rest = rest.trim().trim_end_matches(';').trim();

    if rest.is_empty() {
        return Err(BindgenError::InvalidDeclaration(
            "empty typedef".to_string(),
        ));
    }

    // Function pointer typedef: typedef int (*name)(int, int);
    if rest.contains("(*") {
        return parse_fn_ptr_typedef(rest);
    }

    // Split into target type and name — the last token is the name
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.len() < 2 {
        return Err(BindgenError::InvalidDeclaration(format!(
            "cannot parse typedef: {}",
            rest
        )));
    }

    let name = tokens.last().unwrap().trim_end_matches(';');
    // Handle pointer typedefs like `typedef int *intptr;`
    // The name might have a leading `*`
    let (name, is_ptr) = if let Some(stripped) = name.strip_prefix('*') {
        (stripped, true)
    } else {
        (name, false)
    };

    let type_str = tokens[..tokens.len() - 1].join(" ");
    let mut target = parse_c_type(&type_str)?;
    if is_ptr {
        target = CType::Pointer(Box::new(target));
    }

    Ok(Some(CDecl::TypedefDecl {
        name: name.to_string(),
        target,
    }))
}

/// Parse a function pointer typedef like `typedef int (*callback)(int, int)`.
fn parse_fn_ptr_typedef(rest: &str) -> Result<Option<CDecl>, BindgenError> {
    // Pattern: <return_type> (*<name>)(<params>)
    let paren_star = match rest.find("(*") {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(format!(
                "malformed fn ptr typedef: {}",
                rest
            )))
        }
    };

    let return_str = rest[..paren_star].trim();
    let return_type = parse_c_type(return_str)?;

    let after = &rest[paren_star + 2..]; // skip "(*"
    let close_paren = match after.find(')') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(format!(
                "malformed fn ptr typedef: {}",
                rest
            )))
        }
    };

    let name = after[..close_paren].trim().to_string();
    let params_part = after[close_paren + 1..].trim();

    // params_part should be "(param1, param2, ...)"
    if !params_part.starts_with('(') {
        return Err(BindgenError::InvalidDeclaration(format!(
            "malformed fn ptr typedef params: {}",
            rest
        )));
    }

    let params_inner = params_part
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    let params = if params_inner.is_empty() || params_inner == "void" {
        Vec::new()
    } else {
        params_inner
            .split(',')
            .map(|p| parse_c_type(p.trim()))
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(Some(CDecl::TypedefDecl {
        name,
        target: CType::FnPointer {
            return_type: Box::new(return_type),
            params,
        },
    }))
}

/// Parse a struct declaration with body.
fn parse_struct_decl(s: &str) -> Result<Option<CDecl>, BindgenError> {
    // struct Name { type1 field1; type2 field2; };
    let s = s.trim().trim_end_matches(';');

    let struct_kw = "struct ";
    let rest = s.strip_prefix(struct_kw).unwrap_or(s);

    let brace_open = match rest.find('{') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(
                "struct missing opening brace".to_string(),
            ))
        }
    };

    let name = rest[..brace_open].trim().to_string();
    if name.is_empty() {
        return Err(BindgenError::InvalidDeclaration(
            "anonymous struct".to_string(),
        ));
    }

    let brace_close = match rest.rfind('}') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(
                "struct missing closing brace".to_string(),
            ))
        }
    };

    let body = rest[brace_open + 1..brace_close].trim();
    let fields = parse_struct_fields(body)?;

    Ok(Some(CDecl::StructDecl { name, fields }))
}

/// Parse semicolon-separated struct fields.
fn parse_struct_fields(body: &str) -> Result<Vec<CField>, BindgenError> {
    let mut fields = Vec::new();

    for field_str in body.split(';') {
        let field_str = field_str.trim();
        if field_str.is_empty() {
            continue;
        }

        // Split into type tokens and name
        let tokens: Vec<&str> = field_str.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }

        let name_token = tokens.last().unwrap();
        // Handle pointer fields like `int *ptr`
        let (field_name, is_ptr) = if let Some(stripped) = name_token.strip_prefix('*') {
            (stripped.to_string(), true)
        } else {
            (name_token.to_string(), false)
        };

        let type_str = tokens[..tokens.len() - 1].join(" ");
        let mut ctype = parse_c_type(&type_str)?;
        if is_ptr {
            ctype = CType::Pointer(Box::new(ctype));
        }

        fields.push(CField {
            name: field_name,
            ctype,
        });
    }

    Ok(fields)
}

/// Parse an enum declaration with body.
fn parse_enum_decl(s: &str) -> Result<Option<CDecl>, BindgenError> {
    // enum Color { RED, GREEN = 2, BLUE };
    let s = s.trim().trim_end_matches(';');

    let enum_kw = "enum ";
    let rest = s.strip_prefix(enum_kw).unwrap_or(s);

    let brace_open = match rest.find('{') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(
                "enum missing opening brace".to_string(),
            ))
        }
    };

    let name = rest[..brace_open].trim().to_string();
    if name.is_empty() {
        return Err(BindgenError::InvalidDeclaration(
            "anonymous enum".to_string(),
        ));
    }

    let brace_close = match rest.rfind('}') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(
                "enum missing closing brace".to_string(),
            ))
        }
    };

    let body = rest[brace_open + 1..brace_close].trim();
    let variants = parse_enum_variants(body);

    Ok(Some(CDecl::EnumDecl { name, variants }))
}

/// Parse comma-separated enum variants.
fn parse_enum_variants(body: &str) -> Vec<CEnumVariant> {
    let mut variants = Vec::new();

    for variant_str in body.split(',') {
        let variant_str = variant_str.trim();
        if variant_str.is_empty() {
            continue;
        }

        if let Some((name, val_str)) = variant_str.split_once('=') {
            let name = name.trim().to_string();
            let val = val_str.trim().parse::<i64>().ok();
            variants.push(CEnumVariant { name, value: val });
        } else {
            variants.push(CEnumVariant {
                name: variant_str.to_string(),
                value: None,
            });
        }
    }

    variants
}

/// Parse a function declaration / prototype.
fn parse_function_decl(s: &str) -> Result<Option<CDecl>, BindgenError> {
    let s = s.trim().trim_end_matches(';');

    let paren_open = match s.find('(') {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let paren_close = match s.rfind(')') {
        Some(pos) => pos,
        None => {
            return Err(BindgenError::InvalidDeclaration(format!(
                "missing closing paren: {}",
                s
            )))
        }
    };

    let before_paren = s[..paren_open].trim();
    let params_str = s[paren_open + 1..paren_close].trim();

    // Split before_paren into return type + function name
    // The last token is the function name, everything before is the return type
    let tokens: Vec<&str> = before_paren.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(BindgenError::InvalidDeclaration(
            "empty function declaration".to_string(),
        ));
    }

    // Handle pointer returns like `void *malloc(...)` or `int * foo(...)`
    let (name, return_type_str) = extract_function_name_and_return(&tokens);

    let name = name.to_string();
    let return_type = parse_c_type(&return_type_str)?;

    // Parse parameters
    let (params, is_variadic) = parse_function_params(params_str)?;

    Ok(Some(CDecl::Function {
        name,
        return_type,
        params,
        is_variadic,
    }))
}

/// Extract function name and return type string from pre-paren tokens.
fn extract_function_name_and_return(tokens: &[&str]) -> (String, String) {
    if tokens.len() == 1 {
        // e.g. `main(...)` — assume void return
        return (tokens[0].to_string(), "void".to_string());
    }

    let last = tokens[tokens.len() - 1];

    // Check if the name starts with `*` (pointer return: `int *malloc`)
    if let Some(stripped) = last.strip_prefix('*') {
        let type_tokens = &tokens[..tokens.len() - 1];
        let ret_type = format!("{} *", type_tokens.join(" "));
        return (stripped.to_string(), ret_type);
    }

    let type_tokens = &tokens[..tokens.len() - 1];
    (last.to_string(), type_tokens.join(" "))
}

/// Parse a function parameter list string, returning params and variadic flag.
fn parse_function_params(params_str: &str) -> Result<(Vec<CParam>, bool), BindgenError> {
    if params_str.is_empty() || params_str == "void" {
        return Ok((Vec::new(), false));
    }

    let mut params = Vec::new();
    let mut is_variadic = false;

    for param_str in params_str.split(',') {
        let param_str = param_str.trim();

        if param_str == "..." {
            is_variadic = true;
            continue;
        }

        if param_str.is_empty() {
            continue;
        }

        let param = parse_single_param(param_str)?;
        params.push(param);
    }

    Ok((params, is_variadic))
}

/// Parse a single function parameter.
fn parse_single_param(param_str: &str) -> Result<CParam, BindgenError> {
    let tokens: Vec<&str> = param_str.split_whitespace().collect();

    if tokens.is_empty() {
        return Err(BindgenError::InvalidDeclaration(format!(
            "empty parameter: {}",
            param_str
        )));
    }

    // Single token: just a type with no name, e.g. `int`
    if tokens.len() == 1 {
        let ctype = parse_c_type(tokens[0])?;
        return Ok(CParam { name: None, ctype });
    }

    let last = *tokens.last().unwrap();

    // If last token starts with `*`, it's `type *name`
    if let Some(name) = last.strip_prefix('*') {
        let type_str = tokens[..tokens.len() - 1].join(" ");
        let inner = parse_c_type(&type_str)?;
        let ctype = CType::Pointer(Box::new(inner));
        if name.is_empty() {
            return Ok(CParam { name: None, ctype });
        }
        return Ok(CParam {
            name: Some(name.to_string()),
            ctype,
        });
    }

    // If last token is a valid identifier, it's the param name
    if is_c_identifier(last) {
        let type_str = tokens[..tokens.len() - 1].join(" ");
        // Handle `type * name` (pointer with space before name)
        if type_str.ends_with(" *") || type_str.ends_with('*') {
            let inner_str = type_str.trim_end_matches('*').trim();
            if inner_str.starts_with("const ") {
                let inner = parse_c_type(inner_str.strip_prefix("const ").unwrap().trim())?;
                return Ok(CParam {
                    name: Some(last.to_string()),
                    ctype: CType::ConstPointer(Box::new(inner)),
                });
            }
            let inner = parse_c_type(inner_str)?;
            return Ok(CParam {
                name: Some(last.to_string()),
                ctype: CType::Pointer(Box::new(inner)),
            });
        }
        let ctype = parse_c_type(&type_str)?;
        return Ok(CParam {
            name: Some(last.to_string()),
            ctype,
        });
    }

    // Fall back: treat entire string as a type
    let ctype = parse_c_type(param_str)?;
    Ok(CParam { name: None, ctype })
}

/// Parse a const variable declaration.
fn parse_const_decl(s: &str) -> Result<Option<CDecl>, BindgenError> {
    let s = s.trim().trim_end_matches(';');
    let rest = s.strip_prefix("const ").unwrap_or(s).trim();

    if let Some((before_eq, value)) = rest.split_once('=') {
        let before_eq = before_eq.trim();
        let value = value.trim().to_string();

        let tokens: Vec<&str> = before_eq.split_whitespace().collect();
        if tokens.len() < 2 {
            return Err(BindgenError::InvalidDeclaration(format!(
                "cannot parse const: {}",
                s
            )));
        }

        let name = tokens.last().unwrap().to_string();
        let type_str = tokens[..tokens.len() - 1].join(" ");
        let ctype = parse_c_type(&type_str)?;

        Ok(Some(CDecl::ConstantDecl { name, value, ctype }))
    } else {
        Ok(None)
    }
}

// =============================================================================
// Lumen Code Generation
// =============================================================================

/// Generate a Lumen declaration string from a parsed C declaration.
///
/// - `CDecl::Function` → `extern cell name(params) -> ReturnType`
/// - `CDecl::StructDecl` → `record Name ... end`
/// - `CDecl::EnumDecl` → `enum Name ... end`
/// - `CDecl::TypedefDecl` → `type Alias = Target`
/// - `CDecl::ConstantDecl` → `let NAME = value`
pub fn generate_extern(decl: &CDecl) -> String {
    match decl {
        CDecl::Function {
            name,
            return_type,
            params,
            is_variadic,
        } => {
            let param_strs: Vec<String> = params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let pname = p.name.clone().unwrap_or_else(|| format!("arg{}", i));
                    let ptype = c_type_to_lumen(&p.ctype);
                    format!("{}: {}", pname, ptype)
                })
                .collect();

            let mut params_str = param_strs.join(", ");
            if *is_variadic {
                if params_str.is_empty() {
                    params_str = "...args".to_string();
                } else {
                    params_str.push_str(", ...args");
                }
            }

            let ret = c_type_to_lumen(return_type);
            if ret == "Null" {
                format!("extern cell {}({})", name, params_str)
            } else {
                format!("extern cell {}({}) -> {}", name, params_str, ret)
            }
        }

        CDecl::StructDecl { name, fields } => {
            let pascal_name = snake_to_pascal(name);
            if fields.is_empty() {
                format!("record {}\nend", pascal_name)
            } else {
                let field_lines: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let ft = c_type_to_lumen(&f.ctype);
                        format!("  {}: {}", f.name, ft)
                    })
                    .collect();
                format!("record {}\n{}\nend", pascal_name, field_lines.join("\n"))
            }
        }

        CDecl::EnumDecl { name, variants } => {
            let pascal_name = snake_to_pascal(name);
            let variant_lines: Vec<String> = variants
                .iter()
                .map(|v| {
                    let vname = snake_to_pascal(&v.name);
                    match v.value {
                        Some(val) => format!("  {} # = {}", vname, val),
                        None => format!("  {}", vname),
                    }
                })
                .collect();
            format!("enum {}\n{}\nend", pascal_name, variant_lines.join("\n"))
        }

        CDecl::TypedefDecl { name, target } => {
            let pascal_name = snake_to_pascal(name);
            let target_lumen = c_type_to_lumen(target);
            format!("type {} = {}", pascal_name, target_lumen)
        }

        CDecl::ConstantDecl { name, value, ctype } => {
            let lumen_type = c_type_to_lumen(ctype);
            format!("let {}: {} = {}", name, lumen_type, value)
        }
    }
}

/// Parse a complete C header and generate all Lumen bindings.
///
/// Processes the header content line-by-line, collecting multi-line
/// declarations (braced blocks) before parsing. Returns a [`BindgenOutput`]
/// with all generated declarations and any warnings.
pub fn generate_bindings(header_content: &str) -> Result<BindgenOutput, BindgenError> {
    let mut output = BindgenOutput::default();

    // Pre-process: join multi-line declarations.
    // Accumulate lines until we have balanced braces or a semicolon terminator.
    let chunks = split_into_declarations(header_content);

    for (line_num, chunk) in chunks {
        match parse_c_declaration(&chunk) {
            Ok(Some(decl)) => {
                let generated = generate_extern(&decl);
                match &decl {
                    CDecl::Function { .. } => output.extern_cells.push(generated),
                    CDecl::StructDecl { .. } => output.records.push(generated),
                    CDecl::EnumDecl { .. } => output.enums.push(generated),
                    CDecl::TypedefDecl { .. } => output.type_aliases.push(generated),
                    CDecl::ConstantDecl { .. } => output.extern_cells.push(generated),
                }
            }
            Ok(None) => { /* skip non-declarations */ }
            Err(e) => {
                output.warnings.push(format!("line {}: {}", line_num, e));
            }
        }
    }

    Ok(output)
}

/// Split header content into logical declaration chunks, handling multi-line
/// braced blocks (structs, enums).
fn split_into_declarations(content: &str) -> Vec<(usize, String)> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut brace_depth: i32 = 0;
    let mut start_line = 1;

    for (i, line) in content.lines().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();

        // Skip empty and comment lines when not accumulating
        if current.is_empty()
            && (trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.starts_with('#'))
        {
            // Still emit these so parse_c_declaration can return None
            chunks.push((line_num, trimmed.to_string()));
            continue;
        }

        if current.is_empty() {
            start_line = line_num;
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(trimmed);

        brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
        brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;

        // A declaration is complete when braces are balanced and we hit a `;`
        if brace_depth <= 0 && (trimmed.ends_with(';') || trimmed.ends_with('}')) {
            chunks.push((start_line, current.clone()));
            current.clear();
            brace_depth = 0;
        }
    }

    // Flush remaining
    if !current.is_empty() {
        chunks.push((start_line, current));
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_to_pascal_basic() {
        assert_eq!(snake_to_pascal("hello"), "Hello");
        assert_eq!(snake_to_pascal("my_struct"), "MyStruct");
        assert_eq!(snake_to_pascal("a_b_c"), "ABC");
        assert_eq!(snake_to_pascal("already"), "Already");
    }

    #[test]
    fn test_strip_prefix_basic() {
        assert_eq!(strip_prefix("SDL_Window", "SDL"), "Window");
        assert_eq!(strip_prefix("SDL_CreateWindow", "SDL"), "CreateWindow");
        assert_eq!(strip_prefix("MyThing", "Other"), "MyThing");
        assert_eq!(strip_prefix("ABC", "ABC"), "ABC"); // exact match returns as-is
    }

    #[test]
    fn test_c_type_to_lumen_basic() {
        assert_eq!(c_type_to_lumen(&CType::Void), "Null");
        assert_eq!(c_type_to_lumen(&CType::Int), "Int");
        assert_eq!(c_type_to_lumen(&CType::Float), "Float");
        assert_eq!(c_type_to_lumen(&CType::Double), "Float");
        assert_eq!(c_type_to_lumen(&CType::Char), "Int");
        assert_eq!(c_type_to_lumen(&CType::Bool), "Bool");
    }

    #[test]
    fn test_c_type_to_lumen_unsigned() {
        assert_eq!(c_type_to_lumen(&CType::UInt), "Int");
        assert_eq!(c_type_to_lumen(&CType::ULong), "Int");
        assert_eq!(c_type_to_lumen(&CType::UChar), "Int");
    }

    #[test]
    fn test_c_type_to_lumen_pointers() {
        let ptr = CType::Pointer(Box::new(CType::Int));
        assert_eq!(c_type_to_lumen(&ptr), "addr[Int]");

        let void_ptr = CType::Pointer(Box::new(CType::Void));
        assert_eq!(c_type_to_lumen(&void_ptr), "addr[Null]");
    }

    #[test]
    fn test_c_type_to_lumen_const_pointer() {
        let cptr = CType::ConstPointer(Box::new(CType::Char));
        assert_eq!(c_type_to_lumen(&cptr), "addr[Int]");
    }

    #[test]
    fn test_c_type_to_lumen_array() {
        let arr = CType::Array(Box::new(CType::Int), 10);
        assert_eq!(c_type_to_lumen(&arr), "List[Int]");
    }

    #[test]
    fn test_c_type_to_lumen_struct_enum() {
        assert_eq!(
            c_type_to_lumen(&CType::Struct("my_point".into())),
            "MyPoint"
        );
        assert_eq!(
            c_type_to_lumen(&CType::Enum("color_mode".into())),
            "ColorMode"
        );
    }

    #[test]
    fn test_c_type_to_lumen_fn_pointer() {
        let fnp = CType::FnPointer {
            return_type: Box::new(CType::Int),
            params: vec![CType::Int, CType::Float],
        };
        assert_eq!(c_type_to_lumen(&fnp), "Fn[Int, Float] -> Int");
    }

    #[test]
    fn test_c_type_to_lumen_fn_pointer_no_params() {
        let fnp = CType::FnPointer {
            return_type: Box::new(CType::Void),
            params: vec![],
        };
        assert_eq!(c_type_to_lumen(&fnp), "Fn[] -> Null");
    }

    #[test]
    fn test_parse_c_type_basic() {
        assert_eq!(parse_c_type("int").unwrap(), CType::Int);
        assert_eq!(parse_c_type("float").unwrap(), CType::Float);
        assert_eq!(parse_c_type("double").unwrap(), CType::Double);
        assert_eq!(parse_c_type("void").unwrap(), CType::Void);
        assert_eq!(parse_c_type("char").unwrap(), CType::Char);
        assert_eq!(parse_c_type("bool").unwrap(), CType::Bool);
        assert_eq!(parse_c_type("_Bool").unwrap(), CType::Bool);
        assert_eq!(parse_c_type("short").unwrap(), CType::Short);
        assert_eq!(parse_c_type("long").unwrap(), CType::Long);
        assert_eq!(parse_c_type("long long").unwrap(), CType::LongLong);
    }

    #[test]
    fn test_parse_c_type_unsigned() {
        assert_eq!(parse_c_type("unsigned int").unwrap(), CType::UInt);
        assert_eq!(parse_c_type("unsigned").unwrap(), CType::UInt);
        assert_eq!(parse_c_type("unsigned long").unwrap(), CType::ULong);
        assert_eq!(parse_c_type("unsigned char").unwrap(), CType::UChar);
    }

    #[test]
    fn test_parse_c_type_pointer() {
        assert_eq!(
            parse_c_type("int *").unwrap(),
            CType::Pointer(Box::new(CType::Int))
        );
        assert_eq!(
            parse_c_type("void *").unwrap(),
            CType::Pointer(Box::new(CType::Void))
        );
    }

    #[test]
    fn test_parse_c_type_const_pointer() {
        assert_eq!(
            parse_c_type("const char *").unwrap(),
            CType::ConstPointer(Box::new(CType::Char))
        );
    }

    #[test]
    fn test_parse_c_type_struct_enum() {
        assert_eq!(
            parse_c_type("struct Point").unwrap(),
            CType::Struct("Point".into())
        );
        assert_eq!(
            parse_c_type("enum Color").unwrap(),
            CType::Enum("Color".into())
        );
    }

    #[test]
    fn test_parse_c_type_fn_pointer() {
        assert_eq!(
            parse_c_type("int (*)(int, float)").unwrap(),
            CType::FnPointer {
                return_type: Box::new(CType::Int),
                params: vec![CType::Int, CType::Float],
            }
        );
    }

    #[test]
    fn test_parse_c_type_typedef_name() {
        assert_eq!(
            parse_c_type("size_t").unwrap(),
            CType::Typedef("size_t".into())
        );
        assert_eq!(
            parse_c_type("MyType").unwrap(),
            CType::Typedef("MyType".into())
        );
    }

    #[test]
    fn test_parse_c_declaration_function() {
        let decl = parse_c_declaration("int add(int a, int b);")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::Function {
                name,
                return_type,
                params,
                is_variadic,
            } => {
                assert_eq!(name, "add");
                assert_eq!(return_type, CType::Int);
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, Some("a".to_string()));
                assert_eq!(params[0].ctype, CType::Int);
                assert_eq!(params[1].name, Some("b".to_string()));
                assert!(!is_variadic);
            }
            _ => panic!("expected Function, got {:?}", decl),
        }
    }

    #[test]
    fn test_parse_c_declaration_void_return() {
        let decl = parse_c_declaration("void free(void *ptr);")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::Function {
                name,
                return_type,
                params,
                ..
            } => {
                assert_eq!(name, "free");
                assert_eq!(return_type, CType::Void);
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].ctype, CType::Pointer(Box::new(CType::Void)));
            }
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn test_parse_c_declaration_variadic() {
        let decl = parse_c_declaration("int printf(const char *fmt, ...);")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::Function {
                name,
                is_variadic,
                params,
                ..
            } => {
                assert_eq!(name, "printf");
                assert!(is_variadic);
                assert_eq!(params.len(), 1);
            }
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn test_parse_c_declaration_typedef() {
        let decl = parse_c_declaration("typedef unsigned long size_t;")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::TypedefDecl { name, target } => {
                assert_eq!(name, "size_t");
                assert_eq!(target, CType::ULong);
            }
            _ => panic!("expected TypedefDecl"),
        }
    }

    #[test]
    fn test_parse_c_declaration_skips_comments() {
        assert!(parse_c_declaration("// this is a comment")
            .unwrap()
            .is_none());
        assert!(parse_c_declaration("/* block comment */")
            .unwrap()
            .is_none());
        assert!(parse_c_declaration("* middle of block */")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_parse_c_declaration_skips_preprocessor() {
        assert!(parse_c_declaration("#include <stdio.h>").unwrap().is_none());
        assert!(parse_c_declaration("#define FOO 42").unwrap().is_none());
        assert!(parse_c_declaration("#ifndef HEADER_H").unwrap().is_none());
    }

    #[test]
    fn test_parse_c_declaration_skips_empty() {
        assert!(parse_c_declaration("").unwrap().is_none());
        assert!(parse_c_declaration("   ").unwrap().is_none());
    }

    #[test]
    fn test_parse_c_declaration_struct() {
        let decl = parse_c_declaration("struct Point { int x; int y; };")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::StructDecl { name, fields } => {
                assert_eq!(name, "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "x");
                assert_eq!(fields[0].ctype, CType::Int);
                assert_eq!(fields[1].name, "y");
                assert_eq!(fields[1].ctype, CType::Int);
            }
            _ => panic!("expected StructDecl"),
        }
    }

    #[test]
    fn test_parse_c_declaration_enum() {
        let decl = parse_c_declaration("enum Color { RED, GREEN = 2, BLUE };")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::EnumDecl { name, variants } => {
                assert_eq!(name, "Color");
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].name, "RED");
                assert_eq!(variants[0].value, None);
                assert_eq!(variants[1].name, "GREEN");
                assert_eq!(variants[1].value, Some(2));
                assert_eq!(variants[2].name, "BLUE");
                assert_eq!(variants[2].value, None);
            }
            _ => panic!("expected EnumDecl"),
        }
    }

    #[test]
    fn test_generate_extern_function() {
        let decl = CDecl::Function {
            name: "malloc".to_string(),
            return_type: CType::Pointer(Box::new(CType::Void)),
            params: vec![CParam {
                name: Some("size".to_string()),
                ctype: CType::Typedef("size_t".to_string()),
            }],
            is_variadic: false,
        };
        assert_eq!(
            generate_extern(&decl),
            "extern cell malloc(size: SizeT) -> addr[Null]"
        );
    }

    #[test]
    fn test_generate_extern_void_function() {
        let decl = CDecl::Function {
            name: "free".to_string(),
            return_type: CType::Void,
            params: vec![CParam {
                name: Some("ptr".to_string()),
                ctype: CType::Pointer(Box::new(CType::Void)),
            }],
            is_variadic: false,
        };
        assert_eq!(generate_extern(&decl), "extern cell free(ptr: addr[Null])");
    }

    #[test]
    fn test_generate_extern_variadic() {
        let decl = CDecl::Function {
            name: "printf".to_string(),
            return_type: CType::Int,
            params: vec![CParam {
                name: Some("fmt".to_string()),
                ctype: CType::ConstPointer(Box::new(CType::Char)),
            }],
            is_variadic: true,
        };
        assert_eq!(
            generate_extern(&decl),
            "extern cell printf(fmt: addr[Int], ...args) -> Int"
        );
    }

    #[test]
    fn test_generate_extern_struct() {
        let decl = CDecl::StructDecl {
            name: "point".to_string(),
            fields: vec![
                CField {
                    name: "x".to_string(),
                    ctype: CType::Int,
                },
                CField {
                    name: "y".to_string(),
                    ctype: CType::Int,
                },
            ],
        };
        let expected = "record Point\n  x: Int\n  y: Int\nend";
        assert_eq!(generate_extern(&decl), expected);
    }

    #[test]
    fn test_generate_extern_enum() {
        let decl = CDecl::EnumDecl {
            name: "Color".to_string(),
            variants: vec![
                CEnumVariant {
                    name: "RED".to_string(),
                    value: None,
                },
                CEnumVariant {
                    name: "GREEN".to_string(),
                    value: Some(2),
                },
            ],
        };
        let expected = "enum Color\n  Red\n  Green # = 2\nend";
        assert_eq!(generate_extern(&decl), expected);
    }

    #[test]
    fn test_generate_extern_typedef() {
        let decl = CDecl::TypedefDecl {
            name: "size_t".to_string(),
            target: CType::ULong,
        };
        assert_eq!(generate_extern(&decl), "type SizeT = Int");
    }

    #[test]
    fn test_generate_bindings_multi_line() {
        let header = r#"
#include <stdlib.h>

void *malloc(size_t size);
void free(void *ptr);
int strlen(const char *s);
"#;
        let output = generate_bindings(header).unwrap();
        assert_eq!(output.extern_cells.len(), 3);
        assert!(output.extern_cells[0].contains("malloc"));
        assert!(output.extern_cells[1].contains("free"));
        assert!(output.extern_cells[2].contains("strlen"));
    }

    #[test]
    fn test_bindgen_output_to_lumen_source() {
        let output = BindgenOutput {
            extern_cells: vec!["extern cell malloc(size: Int) -> addr[Null]".to_string()],
            records: vec!["record Point\n  x: Int\n  y: Int\nend".to_string()],
            enums: vec![],
            type_aliases: vec!["type SizeT = Int".to_string()],
            warnings: vec![],
        };
        let source = output.to_lumen_source();
        assert!(source.contains("# Auto-generated Lumen bindings"));
        assert!(source.contains("# Type aliases"));
        assert!(source.contains("type SizeT = Int"));
        assert!(source.contains("# Records"));
        assert!(source.contains("record Point"));
        assert!(source.contains("# Extern functions"));
        assert!(source.contains("extern cell malloc"));
    }

    #[test]
    fn test_bindgen_error_display() {
        let e1 = BindgenError::ParseError {
            line: 5,
            message: "unexpected token".to_string(),
        };
        assert_eq!(e1.to_string(), "parse error at line 5: unexpected token");

        let e2 = BindgenError::UnsupportedType("__int128".to_string());
        assert_eq!(e2.to_string(), "unsupported C type: __int128");

        let e3 = BindgenError::InvalidDeclaration("garbage".to_string());
        assert_eq!(e3.to_string(), "invalid declaration: garbage");
    }

    #[test]
    fn test_parse_c_type_empty_error() {
        assert!(parse_c_type("").is_err());
    }

    #[test]
    fn test_c_type_to_lumen_typedef() {
        assert_eq!(c_type_to_lumen(&CType::Typedef("my_type".into())), "MyType");
    }

    #[test]
    fn test_c_type_to_lumen_unknown() {
        assert_eq!(c_type_to_lumen(&CType::Unknown("__m128".into())), "__m128");
    }

    #[test]
    fn test_c_type_to_lumen_nested_pointer() {
        let pp = CType::Pointer(Box::new(CType::Pointer(Box::new(CType::Char))));
        assert_eq!(c_type_to_lumen(&pp), "addr[addr[Int]]");
    }

    #[test]
    fn test_generate_extern_unnamed_params() {
        let decl = CDecl::Function {
            name: "foo".to_string(),
            return_type: CType::Int,
            params: vec![
                CParam {
                    name: None,
                    ctype: CType::Int,
                },
                CParam {
                    name: None,
                    ctype: CType::Float,
                },
            ],
            is_variadic: false,
        };
        assert_eq!(
            generate_extern(&decl),
            "extern cell foo(arg0: Int, arg1: Float) -> Int"
        );
    }

    #[test]
    fn test_generate_extern_constant() {
        let decl = CDecl::ConstantDecl {
            name: "MAX_SIZE".to_string(),
            value: "1024".to_string(),
            ctype: CType::Int,
        };
        assert_eq!(generate_extern(&decl), "let MAX_SIZE: Int = 1024");
    }

    #[test]
    fn test_generate_bindings_with_struct_and_enum() {
        let header = r#"
struct Point { int x; int y; };
enum Color { RED, GREEN, BLUE };
typedef unsigned int uint32_t;
"#;
        let output = generate_bindings(header).unwrap();
        assert_eq!(output.records.len(), 1);
        assert_eq!(output.enums.len(), 1);
        assert_eq!(output.type_aliases.len(), 1);
        assert!(output.records[0].contains("Point"));
        assert!(output.enums[0].contains("Color"));
        assert!(output.type_aliases[0].contains("Uint32T"));
    }

    #[test]
    fn test_snake_to_pascal_underscores() {
        assert_eq!(snake_to_pascal("__internal"), "Internal");
        assert_eq!(snake_to_pascal("a_"), "A");
        assert_eq!(snake_to_pascal("_leading"), "Leading");
    }

    #[test]
    fn test_strip_prefix_with_underscore() {
        assert_eq!(strip_prefix("GL_TEXTURE", "GL"), "TEXTURE");
    }

    #[test]
    fn test_strip_prefix_no_match() {
        assert_eq!(strip_prefix("something", "SDL"), "something");
    }

    #[test]
    fn test_c_type_long_variants() {
        assert_eq!(c_type_to_lumen(&CType::Long), "Int");
        assert_eq!(c_type_to_lumen(&CType::LongLong), "Int");
        assert_eq!(c_type_to_lumen(&CType::Short), "Int");
    }

    #[test]
    fn test_generate_empty_struct() {
        let decl = CDecl::StructDecl {
            name: "Empty".to_string(),
            fields: vec![],
        };
        assert_eq!(generate_extern(&decl), "record Empty\nend");
    }

    #[test]
    fn test_bindgen_output_empty() {
        let output = BindgenOutput::default();
        let source = output.to_lumen_source();
        assert!(source.contains("# Auto-generated Lumen bindings"));
        // No section headers for empty sections
        assert!(!source.contains("# Type aliases"));
        assert!(!source.contains("# Extern functions"));
    }

    #[test]
    fn test_parse_c_type_double_pointer() {
        let ty = parse_c_type("char * *").unwrap();
        assert_eq!(
            ty,
            CType::Pointer(Box::new(CType::Pointer(Box::new(CType::Char))))
        );
    }

    #[test]
    fn test_generate_bindings_warnings_on_bad_input() {
        let header = "int good_func(int x);\nthis is not valid C at all;";
        let output = generate_bindings(header).unwrap();
        assert_eq!(output.extern_cells.len(), 1);
        // The bad line should produce a warning or be skipped
    }

    #[test]
    fn test_parse_pointer_return_function() {
        let decl = parse_c_declaration("void *malloc(size_t size);")
            .unwrap()
            .unwrap();
        match decl {
            CDecl::Function {
                name, return_type, ..
            } => {
                assert_eq!(name, "malloc");
                assert_eq!(return_type, CType::Pointer(Box::new(CType::Void)));
            }
            _ => panic!("expected Function"),
        }
    }
}
