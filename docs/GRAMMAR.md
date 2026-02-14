# Lumen Formal Grammar Specification

This document defines the complete formal grammar for the Lumen programming language in Extended Backus-Naur Form (EBNF).

The grammar reflects the actual implementation in `rust/lumen-compiler/src/compiler/parser.rs` as of the current version.

## Notation Conventions

- `rule = ...;` defines a production rule
- `|` separates alternatives
- `[ ... ]` denotes optional elements (zero or one occurrence)
- `{ ... }` denotes repetition (zero or more occurrences)
- `( ... )` groups elements
- `"keyword"` denotes terminal keywords and operators
- `UPPERCASE` denotes token classes (lexical terminals)
- `;` terminates each rule

## 1. Lexical Grammar

### 1.1 Comments and Whitespace

```ebnf
comment = "#" { ANY_CHAR - NEWLINE } NEWLINE ;

whitespace = SPACE | TAB | NEWLINE ;
```

### 1.2 Keywords

```ebnf
keyword = "record" | "enum" | "cell" | "let" | "if" | "else" | "for" | "in"
        | "match" | "return" | "halt" | "end" | "use" | "tool" | "as"
        | "grant" | "expect" | "schema" | "role" | "where" | "and" | "or"
        | "not" | "null" | "result" | "ok" | "err" | "list" | "map"
        | "while" | "loop" | "break" | "continue" | "mut" | "const" | "pub"
        | "import" | "from" | "async" | "await" | "parallel" | "fn"
        | "trait" | "impl" | "type" | "set" | "tuple" | "emit" | "yield"
        | "mod" | "self" | "with" | "try" | "union" | "step" | "comptime"
        | "macro" | "extern" | "then" | "when" | "bool" | "int" | "float"
        | "string" | "bytes" | "json" | "is" ;
```

### 1.3 Identifiers

```ebnf
identifier = ( LETTER | "_" ) { LETTER | DIGIT | "_" } ;

LETTER = "a".."z" | "A".."Z" ;
DIGIT = "0".."9" ;
```

### 1.4 Literals

```ebnf
integer_literal = [ "-" ] DIGIT { DIGIT } ;

float_literal = [ "-" ] DIGIT { DIGIT } "." DIGIT { DIGIT } [ exponent ] ;

exponent = ( "e" | "E" ) [ "+" | "-" ] DIGIT { DIGIT } ;

boolean_literal = "true" | "false" ;

null_literal = "null" ;

string_literal = "\"" { string_char | interpolation } "\""
               | "\"\"\"" { string_char | interpolation | NEWLINE } "\"\"\"" ;

string_char = ESCAPE_SEQUENCE | ( ANY_CHAR - ( "\"" | "\\" | "{" ) ) ;

interpolation = "{" expression "}" ;

ESCAPE_SEQUENCE = "\\" ( "n" | "t" | "r" | "\\" | "\"" | "{" | "0"
                | "x" HEX_DIGIT HEX_DIGIT
                | "u{" HEX_DIGIT { HEX_DIGIT } "}" ) ;

raw_string_literal = "r\"" { ANY_CHAR - "\"" } "\""
                   | "r\"\"\"" { ANY_CHAR } "\"\"\"" ;

bytes_literal = "b\"" { HEX_DIGIT HEX_DIGIT } "\"" ;

HEX_DIGIT = DIGIT | "a".."f" | "A".."F" ;
```

### 1.5 Operators and Delimiters

```ebnf
operator = "+" | "-" | "*" | "/" | "//" | "%" | "**"
         | "==" | "!=" | "<" | "<=" | ">" | ">="
         | "=" | "->" | "=>" | "."
         | "+=" | "-=" | "*=" | "/=" | "//="
         | "%=" | "**=" | "&=" | "|=" | "^="
         | "<<" | ">>"
         | "|>" | "??" | "?." | "?[" | "!" | "?"
         | ".." | "..=" | "..."
         | "++" | "&" | "~" | "^" | "|"
         | "is" | "as" ;

delimiter = "(" | ")" | "[" | "]" | "{" | "}"
          | "," | ":" | ";" | "@" | "#" ;
```

### 1.6 Indentation

```ebnf
INDENT = (* Indentation increase detected by lexer *) ;
DEDENT = (* Indentation decrease detected by lexer *) ;
NEWLINE = (* Line terminator *) ;
```

## 2. Top-Level Structure

```ebnf
program = { directive } { ( declaration | top_level_statement ) } ;

directive = "@" identifier [ value ] NEWLINE ;

declaration = record_declaration
            | enum_declaration
            | cell_declaration
            | agent_declaration
            | process_declaration
            | effect_declaration
            | effect_bind_declaration
            | handler_declaration
            | use_tool_declaration
            | grant_declaration
            | type_alias_declaration
            | trait_declaration
            | impl_declaration
            | import_declaration
            | const_declaration
            | macro_declaration
            | addon_declaration
            | attribute_declaration
            | schema_declaration ;

top_level_statement = statement ;
```

## 3. Type Expressions

```ebnf
type_expr = named_type
          | list_type
          | map_type
          | set_type
          | tuple_type
          | result_type
          | union_type
          | optional_type
          | null_type
          | function_type
          | generic_type ;

named_type = identifier ;

optional_type = type_expr "?" ;   (* T? desugars to T | Null *)

list_type = "list" "[" type_expr "]" ;

map_type = "map" "[" type_expr "," type_expr "]" ;

set_type = "set" "[" type_expr "]" ;

tuple_type = "tuple" "[" [ type_expr { "," type_expr } ] "]"
           | "(" [ type_expr { "," type_expr } ] ")" ;

result_type = "result" "[" type_expr "," type_expr "]" ;

union_type = type_expr "|" type_expr { "|" type_expr } ;

null_type = "Null" | "null" ;

function_type = "fn" "(" [ type_expr { "," type_expr } ] ")"
                [ "->" type_expr ] [ effect_row ] ;

generic_type = identifier "[" type_expr { "," type_expr } "]" ;
```

## 4. Declarations

### 4.1 Record Declaration

```ebnf
record_declaration = [ "pub" ] "record" identifier [ generic_params ] NEWLINE
                     [ INDENT ] { field_definition | attribute | migrate_block } [ DEDENT ]
                     "end" ;

field_definition = identifier ":" type_expr [ "=" expression ]
                   [ "where" expression ] NEWLINE ;

generic_params = "[" generic_param { "," generic_param } "]" ;

generic_param = identifier [ ":" identifier { "+" identifier } ] ;
```

### 4.2 Enum Declaration

```ebnf
enum_declaration = [ "pub" ] "enum" identifier [ generic_params ] NEWLINE
                   [ INDENT ] { enum_variant | enum_method } [ DEDENT ]
                   "end" ;

enum_variant = identifier [ "(" type_expr ")" ] NEWLINE ;

enum_method = cell_declaration ;
```

### 4.3 Cell Declaration

```ebnf
cell_declaration = [ "pub" ] [ "async" ] "cell" identifier [ generic_params ]
                   "(" [ parameter_list ] ")" [ "->" type_expr ] [ effect_row ]
                   ( cell_body | "=" expression ) ;

cell_body = NEWLINE [ INDENT ] { statement } [ DEDENT ] "end" ;

parameter_list = parameter { "," parameter } ;

parameter = [ "..." ] identifier [ ":" type_expr ] [ "=" expression ] ;
            (* "..." prefix marks a variadic parameter that collects remaining args *)

effect_row = "/" ( "{" effect_name { "," effect_name } "}" | effect_name ) ;

effect_name = identifier ;
```

### 4.4 Agent Declaration

```ebnf
agent_declaration = "agent" [ identifier ] NEWLINE
                    [ INDENT ]
                    { cell_declaration | grant_declaration | attribute }
                    [ DEDENT ]
                    "end" ;
```

### 4.5 Process Declaration

```ebnf
process_declaration = process_kind [ identifier ] NEWLINE
                      [ INDENT ] { process_element } [ DEDENT ]
                      "end" ;

process_kind = "pipeline" | "orchestration" | "machine" | "memory"
             | "guardrail" | "eval" | "pattern" ;

process_element = cell_declaration
                | grant_declaration
                | pipeline_stages
                | machine_initial
                | machine_state
                | attribute ;

pipeline_stages = "stages" ":" NEWLINE
                  [ INDENT ] { "->" identifier NEWLINE } [ DEDENT ]
                  [ "end" ] ;

machine_initial = "initial" ":" identifier NEWLINE ;

machine_state = "state" identifier [ "(" parameter_list ")" ] NEWLINE
                [ INDENT ]
                { state_property }
                [ DEDENT ]
                [ "end" ] ;

state_property = "terminal" ":" boolean_literal NEWLINE
               | "guard" ":" expression NEWLINE
               | "transition" identifier [ "(" expression_list ")" ] NEWLINE
               | state_handler ;

state_handler = ( "on_enter" | "on_event" | "on_timeout" ) NEWLINE
                [ INDENT ] { statement | "transition" identifier [ "(" expression_list ")" ] } [ DEDENT ]
                [ "end" ] ;
```

### 4.6 Effect and Handler Declarations

```ebnf
effect_declaration = "effect" [ identifier ] NEWLINE
                     [ INDENT ] { cell_declaration } [ DEDENT ]
                     "end" ;

effect_bind_declaration = "bind" "effect" dotted_identifier "to" dotted_identifier NEWLINE ;

handler_declaration = "handler" [ identifier ] NEWLINE
                      [ INDENT ] { handler_operation } [ DEDENT ]
                      "end" ;

handler_operation = "handle" dotted_identifier "(" [ parameter_list ] ")"
                    [ "->" type_expr ] cell_body ;

dotted_identifier = identifier { "." identifier } ;
```

### 4.7 Tool and Grant Declarations

```ebnf
use_tool_declaration = "use" "tool" dotted_identifier "as" identifier
                       [ "from" string_literal ] NEWLINE ;

grant_declaration = "grant" identifier { grant_constraint } NEWLINE ;

grant_constraint = identifier ( string_literal | integer_literal | identifier ) ;
```

### 4.8 Type, Trait, and Impl Declarations

```ebnf
type_alias_declaration = [ "pub" ] "type" identifier [ generic_params ] "=" type_expr
                         [ "where" expression ] NEWLINE ;

trait_declaration = [ "pub" ] "trait" identifier [ ":" trait_bounds ] NEWLINE
                    [ INDENT ] { cell_declaration } [ DEDENT ]
                    "end" ;

trait_bounds = identifier { ( "," | "+" ) identifier } ;

impl_declaration = "impl" [ generic_params ] identifier "for" type_spec NEWLINE
                   [ INDENT ] { cell_declaration } [ DEDENT ]
                   "end" ;

type_spec = identifier [ "[" type_expr { "," type_expr } "]" ] ;
```

### 4.9 Import Declaration

```ebnf
import_declaration = [ "pub" ] "import" dotted_identifier ":" import_list NEWLINE ;

import_list = "*" | import_name { "," import_name } ;

import_name = identifier [ "as" identifier ] ;
```

### 4.10 Const and Macro Declarations

```ebnf
const_declaration = "const" identifier [ ":" type_expr ] "=" expression NEWLINE ;

macro_declaration = "macro" [ identifier ] [ "!" ] [ "(" { identifier } ")" ] NEWLINE
                    { ANY } "end" ;
```

### 4.11 Addon Declarations

```ebnf
addon_declaration = identifier [ identifier ] { ANY } ( NEWLINE | "end" ) ;

attribute_declaration = "@" [ identifier ] [ "(" { ANY } ")" ]
                        [ NEWLINE [ INDENT ] { ANY } [ DEDENT ] [ "end" ] ] ;

schema_declaration = "schema" NEWLINE { ANY } "end" ;

migrate_block = "migrate" NEWLINE { ANY } "end" ;
```

## 5. Statements

```ebnf
statement = let_statement
          | if_statement
          | for_statement
          | while_statement
          | loop_statement
          | match_statement
          | return_statement
          | halt_statement
          | break_statement
          | continue_statement
          | emit_statement
          | assignment_statement
          | compound_assignment_statement
          | expression_statement ;

let_statement = "let" [ "mut" ] ( identifier | destructure_pattern )
                [ ":" type_expr ] "=" expression NEWLINE ;

destructure_pattern = tuple_destructure | list_destructure | record_destructure ;

if_statement = "if" [ "let" pattern "=" ] expression
               ( "then" expression [ "else" ( if_statement | expression ) ]
               | NEWLINE statement_block
                 [ "else" ( if_statement | NEWLINE statement_block ) ]
                 "end" ) ;

for_statement = "for" [ "@" identifier ] ( identifier | tuple_destructure ) "in" expression
                [ "if" expression ]
                NEWLINE statement_block "end" ;
                (* "@label" enables labeled loop; "if expr" filters iterations *)

while_statement = "while" [ "@" identifier ]
                  ( [ "let" pattern "=" ] expression )
                  NEWLINE statement_block "end" ;

loop_statement = "loop" [ "@" identifier ] NEWLINE statement_block "end" ;

match_statement = "match" expression NEWLINE
                  [ INDENT ] { match_arm } [ DEDENT ]
                  "end" ;
                  (* enum matches are checked for exhaustiveness *)

match_arm = pattern { "|" pattern } [ "if" expression ] "->"
            ( statement | NEWLINE [ INDENT ] { statement } [ DEDENT ] ) ;

return_statement = "return" expression NEWLINE ;

halt_statement = "halt" "(" expression ")" NEWLINE ;

break_statement = "break" [ "@" identifier | expression ] NEWLINE ;

continue_statement = "continue" [ "@" identifier ] NEWLINE ;

emit_statement = "emit" expression NEWLINE ;

assignment_statement = assignment_target "=" expression NEWLINE ;

assignment_target = identifier { "." identifier } ;

compound_assignment_statement = assignment_target compound_op expression NEWLINE ;

compound_op = "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "**="
            | "&=" | "|=" | "^=" ;

expression_statement = expression NEWLINE ;

statement_block = [ INDENT ] { statement } [ DEDENT ] ;
```

## 6. Patterns

```ebnf
pattern = literal_pattern
        | variant_pattern
        | wildcard_pattern
        | identifier_pattern
        | tuple_destructure
        | list_destructure
        | record_destructure
        | type_check_pattern
        | guard_pattern
        | or_pattern ;

literal_pattern = integer_literal | float_literal | string_literal | boolean_literal ;

variant_pattern = ( "ok" | "err" | identifier ) [ "(" [ identifier ] ")" ] ;

wildcard_pattern = "_" ;

identifier_pattern = identifier ;

tuple_destructure = "(" [ pattern { "," pattern } ] ")" ;

list_destructure = "[" [ pattern { "," pattern } [ "," "..." [ identifier ] ] ] "]" ;

record_destructure = identifier "(" [ field_pattern { "," field_pattern } [ "," ".." ] ] ")" ;

field_pattern = identifier [ ":" pattern ] ;

type_check_pattern = identifier ":" type_expr ;

guard_pattern = pattern "if" expression ;

or_pattern = pattern "|" pattern { "|" pattern } ;
```

## 7. Expressions

### 7.1 Expression Precedence (Lowest to Highest)

```ebnf
expression = pipe_forward_expr ;

pipe_forward_expr = null_coalesce_expr { "|>" null_coalesce_expr } ;

null_coalesce_expr = or_expr { "??" or_expr } ;

or_expr = and_expr { "or" and_expr } ;

and_expr = comparison_expr { "and" comparison_expr } ;

comparison_expr = concat_expr { comparison_op concat_expr } ;

comparison_op = "==" | "!=" | "<" | "<=" | ">" | ">="
              | "in" | "is" | "as"
              | "&" | "^" | "<<" | ">>" ;

concat_expr = range_expr { "++" range_expr } ;

range_expr = additive_expr [ ( ".." | "..=" ) [ additive_expr ] [ "step" additive_expr ] ] ;

additive_expr = multiplicative_expr { ( "+" | "-" ) multiplicative_expr } ;

multiplicative_expr = power_expr { ( "*" | "/" | "//" | "%" ) power_expr } ;

power_expr = unary_expr [ "**" power_expr ] ;

unary_expr = ( "-" | "not" | "~" | "!" | "..." ) unary_expr
           | postfix_expr ;

postfix_expr = primary_expr { postfix_op } ;

postfix_op = "." identifier
           | "?." identifier
           | "[" expression "]"
           | "?[" expression "]"
           | "(" [ call_args ] ")"
           | "?"
           | "!" ;

call_args = call_arg { "," call_arg } ;

call_arg = identifier ":" expression      (* named argument *)
         | "role" identifier ":" expression (* role block argument *)
         | expression ;                     (* positional argument *)
```

### 7.2 Primary Expressions

```ebnf
primary_expr = literal
             | identifier
             | list_literal
             | map_literal
             | tuple_literal
             | set_literal
             | record_literal
             | lambda_expr
             | if_expr
             | match_expr
             | block_expr
             | comprehension
             | role_block
             | expect_schema
             | "(" expression ")"
             | "self"
             | "await" expression ;

literal = integer_literal
        | float_literal
        | string_literal
        | raw_string_literal
        | bytes_literal
        | boolean_literal
        | null_literal ;

list_literal = "[" [ expression { "," expression } ] "]" ;

map_literal = "{" [ map_entry { "," map_entry } ] "}" ;

map_entry = expression ":" expression ;

tuple_literal = "(" expression "," expression { "," expression } ")" ;

set_literal = "set" "[" [ expression { "," expression } ] "]" ;

record_literal = identifier "(" [ field_init { "," field_init } ] ")" ;

field_init = identifier ":" expression ;

lambda_expr = "fn" "(" [ parameter_list ] ")" [ "->" type_expr ]
              ( "=>" expression | NEWLINE statement_block "end" ) ;

if_expr = "if" expression "then" expression "else" expression ;

match_expr = "match" expression NEWLINE
             [ INDENT ] { match_arm } [ DEDENT ]
             "end" ;

block_expr = NEWLINE [ INDENT ] { statement } [ DEDENT ] "end" ;

comprehension = "[" expression "for" identifier "in" expression
                [ "if" expression ] "]" ;

role_block = "role" identifier ":" expression "end" ;

expect_schema = "expect" "schema" identifier ;
```

## 8. Operator Precedence Table

| Precedence | Operators | Associativity | Description |
|------------|-----------|---------------|-------------|
| 1 (lowest) | `\|>` | Left | Pipe forward |
| 2 | `??` | Left | Null coalescing |
| 3 | `or` | Left | Logical OR |
| 4 | `and` | Left | Logical AND |
| 5 | `==` `!=` `<` `<=` `>` `>=` `in` `is` `as` `&` `^` `<<` `>>` | Left | Comparison, membership, type test/cast, bitwise |
| 6 | `++` | Left | String/list concatenation |
| 7 | `..` `..=` | Left | Range operators |
| 8 | `+` `-` | Left | Addition, subtraction |
| 9 | `*` `/` `//` `%` | Left | Multiplication, division, floor division, modulo |
| 10 | `**` | Right | Exponentiation |
| 11 | `-` `not` `~` `!` `...` | Right (prefix) | Unary operators |
| 12 (highest) | `.` `?.` `[]` `?[]` `()` `?` `!` | Left (postfix) | Member access, calls, postfix |

## 9. Implementation Notes

### 9.1 Indentation-Sensitivity

The lexer produces `INDENT`, `DEDENT`, and `NEWLINE` tokens based on whitespace:
- Indentation increases emit `INDENT`
- Indentation decreases emit `DEDENT`
- Line breaks emit `NEWLINE`
- Inside bracketed expressions (`()`, `[]`, `{}`), whitespace tokens are skipped

### 9.2 String Interpolation

String literals support interpolation with `{expression}` syntax:
```lumen
let name = "World"
let greeting = "Hello, {name}!"  # interpolation
```

Triple-quoted strings preserve formatting and support interpolation:
```lumen
let message = """
    Multi-line
    {greeting}
"""
```

### 9.3 Reserved Words

The following identifiers are reserved keywords and cannot be used as variable names:
- All keywords listed in section 1.2
- `result` (reserved for result type)

Type keywords (`int`, `float`, `bool`, `string`, `bytes`, `json`) are context-sensitive and can appear as identifiers in expression contexts.

### 9.4 Lambda Syntax

Lambdas use `fn(params) => expr` or `fn(params) ... end` syntax, NOT `|x| ...` pipe syntax.

```lumen
let add = fn(a: Int, b: Int) => a + b
let complex = fn(x: Int) -> Int
  return x * 2
end
```

### 9.5 Effect Rows

Effect rows appear after the return type in cell signatures:
```lumen
cell fetch_data() -> String / {http, trace}
  return HttpGet(url: "https://example.com")
end
```

### 9.6 Pratt Expression Parsing

The parser uses Pratt parsing (precedence climbing) for expressions. The `parse_expr(min_bp: u8)` function enforces operator precedence through binding power values.

### 9.7 Error Recovery

The parser includes error recovery via synchronization points:
- `synchronize()` — skips to next top-level declaration boundary
- `synchronize_stmt()` — skips to next statement boundary within a block
- Allows parsing to continue after errors to collect multiple diagnostics

### 9.8 Desugaring

The parser desugars certain constructs:
- `if let pattern = expr` → `match expr { pattern => ..., _ => ... }`
- `while let pattern = expr` → `loop { match expr { pattern => ..., _ => break } }`
- `if cond then a else b` → inline if expression

### 9.9 Top-Level Statements

Statements at the top level (outside any cell) are wrapped into a synthetic `main` or `__script_main` cell.

### 9.10 Parser Entry Point

```rust
pub fn parse_program(&mut self, directives: Vec<Directive>) -> Result<Program, ParseError>
```

The parser consumes a token stream from the lexer and produces an AST `Program` node containing directives and items (declarations/statements).

## 10. Cross-References

For implementation details, see:
- **Lexer**: `/rust/lumen-compiler/src/compiler/lexer.rs`
- **Parser**: `/rust/lumen-compiler/src/compiler/parser.rs`
- **AST**: `/rust/lumen-compiler/src/compiler/ast.rs`
- **Tokens**: `/rust/lumen-compiler/src/compiler/tokens.rs`
- **Specification**: `/SPEC.md`

## 11. Grammar Coverage

This grammar covers:
- All declaration types (records, enums, cells, agents, processes, effects, handlers, etc.)
- All statement types (let, if, for, while, loop, match, return, halt, break, continue, emit, assignment, compound assignment)
- All expression types (literals, operators, calls, lambdas, comprehensions, is/as, etc.)
- All pattern types (literals, variants, destructuring, guards, or-patterns)
- Type expressions (named, list, map, set, tuple, result, union, optional `T?`, function, generic)
- Operator precedence and associativity (including `//`, `<<`, `>>`, `is`, `as`)
- Compound assignment operators (`+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`)
- Labeled loops (`@label`) with targeted `break`/`continue`
- For-loop filters (`for x in items if cond`)
- Null-safe indexing (`?[]`)
- Variadic parameters (`...param`)
- Match exhaustiveness checking for enum types
- Indentation-based scoping
- String interpolation
- Effect rows
- Generic parameters
- Import/export system

## 12. Known Discrepancies with SPEC.md

The grammar accurately reflects the parser implementation. Any differences from SPEC.md are noted here:
- *(None found in current review)*

This grammar is maintained alongside the parser implementation and should be updated when the parser changes.
