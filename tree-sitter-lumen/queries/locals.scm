; Locals for Lumen - variable scope tracking

; Scopes
(source_file) @local.scope

(cell_declaration) @local.scope
(lambda_expression) @local.scope
(if_statement) @local.scope
(match_statement) @local.scope
(for_statement) @local.scope
(while_statement) @local.scope
(loop_statement) @local.scope
(try_statement) @local.scope

(record_declaration) @local.scope
(enum_declaration) @local.scope
(agent_declaration) @local.scope
(process_declaration) @local.scope
(handler_declaration) @local.scope
(trait_declaration) @local.scope
(impl_declaration) @local.scope

; Definitions
(cell_declaration
  name: (identifier) @local.definition.function)

(parameter
  name: (identifier) @local.definition.parameter)

(let_statement
  name: (identifier) @local.definition.variable)

(for_statement
  variable: (identifier) @local.definition.variable)

(match_arm
  pattern: (identifier_pattern) @local.definition.variable)

(try_statement
  error: (identifier) @local.definition.variable)

(record_declaration
  name: (identifier) @local.definition.type)

(enum_declaration
  name: (identifier) @local.definition.type)

(type_alias
  name: (identifier) @local.definition.type)

(agent_declaration
  name: (identifier) @local.definition.type)

(process_declaration
  name: (identifier) @local.definition.type)

(const_declaration
  name: (identifier) @local.definition.constant)

; References
(identifier) @local.reference
