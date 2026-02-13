; Highlights for Lumen

; Keywords
[
  "cell"
  "record"
  "enum"
  "type"
  "effect"
  "handler"
  "agent"
  "memory"
  "machine"
  "pipeline"
  "orchestration"
  "guardrail"
  "eval"
  "pattern"
  "trait"
  "impl"
  "macro"
] @keyword.definition

[
  "if"
  "else"
  "match"
  "for"
  "in"
  "while"
  "loop"
  "break"
  "continue"
  "return"
  "halt"
] @keyword.control

[
  "let"
  "const"
  "mut"
] @keyword.storage

[
  "use"
  "tool"
  "import"
  "from"
  "as"
] @keyword.import

[
  "grant"
  "bind"
  "to"
  "where"
  "when"
] @keyword.directive

[
  "async"
  "await"
  "spawn"
  "parallel"
  "race"
  "vote"
  "select"
] @keyword.async

[
  "try"
  "catch"
  "finally"
  "emit"
] @keyword.exception

[
  "and"
  "or"
  "not"
] @keyword.operator

[
  "fn"
  "end"
] @keyword

; Built-in types
[
  "Int"
  "Float"
  "Bool"
  "String"
  "Bytes"
  "Json"
  "Null"
  "Any"
  "list"
  "map"
  "set"
  "tuple"
  "result"
] @type.builtin

; Function definitions
(cell_declaration
  name: (identifier) @function)

; Function calls
(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (qualified_name) @function.call)

; Method calls
(member_expression
  property: (identifier) @function.method)

; Type references
(type_annotation (simple_type) @type)
(type_annotation (identifier) @type)

(generic_type
  name: (identifier) @type)

(record_declaration
  name: (identifier) @type)

(enum_declaration
  name: (identifier) @type)

(type_alias
  name: (identifier) @type)

(agent_declaration
  name: (identifier) @type)

(process_declaration
  name: (identifier) @type)

; Record and enum variants
(enum_variant
  name: (identifier) @constant)

(variant_pattern
  variant: (identifier) @constant)

; Fields
(record_field
  name: (identifier) @property)

(record_field_value
  name: (identifier) @property)

(field_pattern
  name: (identifier) @property)

(member_expression
  property: (identifier) @property)

; Parameters
(parameter
  name: (identifier) @variable.parameter)

; Variables
(let_statement
  name: (identifier) @variable)

(identifier_pattern) @variable

; Constants
(const_declaration
  name: (identifier) @constant)

; Literals
(integer) @number
(float) @number
(string) @string
(bytes) @string.special
(boolean) @constant.builtin
"null" @constant.builtin

; String interpolation
(string_content) @string
(interpolation "${" @punctuation.special)
(interpolation "}" @punctuation.special)

; Comments
(comment) @comment

; Operators
[
  "="
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "+="
  "-="
  "*="
  "/="
  "**"
  "++"
  "|>"
  "??"
  "?."
  "!"
  "?"
  "..."
  "=>"
  "&"
  "|"
  "^"
  "~"
  ".."
  "..="
  ">>"
] @operator

; Arrows
[
  "->"
  "=>"
] @punctuation.special

; Delimiters
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ":"
  ";"
  "."
] @punctuation.delimiter

; Directives
(directive
  "@" @punctuation.special
  name: (identifier) @attribute)

; Effect rows
(effect_row
  "/" @punctuation.special)

; Special patterns
(wildcard_pattern) @variable.special

; Tool and grant
(tool_declaration
  tool_name: (qualified_name) @namespace)

(grant_declaration
  tool: (identifier) @namespace)

; Machine states
(machine_state
  name: (identifier) @label)

; Pipeline stages
(pipeline_stage
  name: (identifier) @label)

; Qualified names
(qualified_name) @namespace

; Error recovery
(ERROR) @error
