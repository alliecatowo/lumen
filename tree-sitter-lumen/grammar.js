/**
 * Tree-sitter grammar for Lumen
 *
 * Lumen is a statically typed programming language for AI-native systems.
 * Source files are markdown (.lm.md) with fenced Lumen code blocks.
 */

module.exports = grammar({
  name: 'lumen',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.primary_type, $.call_expression],
    [$.type_annotation, $.binary_expression],
    [$.record_pattern, $.record_expression],
    [$.pattern, $.expression],
    [$.type_annotation, $.expression],
    [$.record_field_value, $.expression],
    [$.let_statement, $.let_destructure_statement],
  ],

  rules: {
    source_file: $ => seq(
      repeat($.directive),
      repeat($._declaration)
    ),

    // Directives (@strict true, @deterministic true, etc.)
    directive: $ => seq(
      '@',
      field('name', $.identifier),
      optional(field('value', choice(
        $.boolean,
        $.integer,
        $.string
      )))
    ),

    // Top-level declarations
    _declaration: $ => choice(
      $.markdown_block,
      $.cell_declaration,
      $.extern_declaration,
      $.record_declaration,
      $.enum_declaration,
      $.type_alias,
      $.effect_declaration,
      $.handler_declaration,
      $.grant_declaration,
      $.tool_declaration,
      $.bind_declaration,
      $.process_declaration,
      $.agent_declaration,
      $.import_declaration,
      $.const_declaration,
      $.trait_declaration,
      $.impl_declaration,
      $.macro_declaration,
    ),

    // Markdown block: fenced code blocks treated as documentation comments
    markdown_block: $ => token(prec(10, seq(
      '```',
      /[^\n]*/,
      '\n',
      repeat(choice(
        /[^`\n]+/,
        /`[^`]/,
        /``[^`]/,
        '\n'
      )),
      '\n',
      /\s*/,
      '```'
    ))),

    // Cell (function) declaration
    cell_declaration: $ => seq(
      optional('async'),
      'cell',
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      optional(seq('->', field('return_type', $.type_annotation))),
      optional(field('effects', $.effect_row)),
      repeat($._statement),
      'end'
    ),

    parameter_list: $ => seq(
      '(',
      optional(sep1($.parameter, ',')),
      ')'
    ),

    parameter: $ => seq(
      optional('...'),
      field('name', $.identifier),
      optional(seq(':', field('type', $.type_annotation))),
      optional(seq('=', field('default', $.expression)))
    ),

    effect_row: $ => seq(
      '/',
      '{',
      sep1($.identifier, ','),
      '}'
    ),

    // Record declaration
    record_declaration: $ => seq(
      'record',
      field('name', $.identifier),
      optional(field('type_params', $.type_parameters)),
      repeat($.record_field),
      'end'
    ),

    record_field: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type_annotation),
      optional(seq('=', field('default', $.expression))),
      optional(field('constraint', $.where_clause))
    ),

    where_clause: $ => seq(
      'where',
      field('condition', $.expression)
    ),

    // Enum declaration
    enum_declaration: $ => seq(
      'enum',
      field('name', $.identifier),
      optional(field('type_params', $.type_parameters)),
      repeat($.enum_variant),
      'end'
    ),

    enum_variant: $ => seq(
      field('name', $.identifier),
      optional(seq('(', sep1($.type_annotation, ','), ')'))
    ),

    // Type alias
    type_alias: $ => seq(
      'type',
      field('name', $.identifier),
      optional(field('type_params', $.type_parameters)),
      '=',
      field('type', $.type_annotation)
    ),

    type_parameters: $ => seq(
      '[',
      sep1($.identifier, ','),
      ']'
    ),

    // Effect declaration
    effect_declaration: $ => seq(
      'effect',
      field('name', $.identifier),
      repeat($.cell_declaration),
      'end'
    ),

    // Handler declaration
    handler_declaration: $ => seq(
      'handler',
      field('name', $.identifier),
      repeat($.handle_clause),
      'end'
    ),

    handle_clause: $ => seq(
      'handle',
      field('effect', seq($.identifier, '.', $.identifier)),
      field('parameters', $.parameter_list),
      optional(seq('->', field('return_type', $.type_annotation))),
      repeat($._statement),
      'end'
    ),

    // Tool and grant declarations
    tool_declaration: $ => seq(
      'use',
      'tool',
      field('tool_name', $.qualified_name),
      optional(seq('as', field('alias', $.identifier)))
    ),

    grant_declaration: $ => seq(
      'grant',
      field('tool', $.identifier),
      repeat1($.grant_constraint)
    ),

    grant_constraint: $ => seq(
      field('key', $.identifier),
      field('value', choice($.string, $.integer, $.float))
    ),

    bind_declaration: $ => seq(
      'bind',
      'effect',
      field('effect', $.qualified_name),
      'to',
      field('tool', $.identifier)
    ),

    // Process declarations (memory, machine, pipeline, etc.)
    process_declaration: $ => seq(
      field('kind', choice(
        'memory',
        'machine',
        'pipeline',
        'orchestration',
        'guardrail',
        'eval',
        'pattern'
      )),
      field('name', $.identifier),
      repeat(choice(
        $.machine_state,
        $.machine_transition,
        $.pipeline_stage,
        $.cell_declaration,
        $.grant_declaration
      )),
      'end'
    ),

    machine_state: $ => seq(
      'state',
      field('name', $.identifier),
      optional(seq(
        '(',
        sep1($.typed_param, ','),
        ')'
      )),
      optional(seq('guard:', field('guard', $.expression)))
    ),

    typed_param: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type_annotation)
    ),

    machine_transition: $ => seq(
      'transition',
      field('from', $.identifier),
      '->',
      field('to', $.identifier),
      optional(seq(
        '(',
        sep1($.expression, ','),
        ')'
      ))
    ),

    pipeline_stage: $ => seq(
      'stage',
      field('name', $.identifier),
      optional(seq(':', field('type', $.type_annotation)))
    ),

    // Agent declaration
    agent_declaration: $ => seq(
      'agent',
      field('name', $.identifier),
      repeat(choice(
        $.cell_declaration,
        $.grant_declaration
      )),
      'end'
    ),

    // Import declaration: import module.path: Name1, Name2 or import module.path: *
    import_declaration: $ => seq(
      'import',
      field('module', choice($.qualified_name, $.identifier)),
      ':',
      choice(
        '*',
        sep1($.import_item, ',')
      )
    ),

    import_item: $ => seq(
      field('name', $.identifier),
      optional(seq('as', field('alias', $.identifier)))
    ),

    // Const declaration
    const_declaration: $ => seq(
      'const',
      field('name', $.identifier),
      optional(seq(':', field('type', $.type_annotation))),
      '=',
      field('value', $.expression)
    ),

    // Trait declaration
    trait_declaration: $ => seq(
      'trait',
      field('name', $.identifier),
      optional(field('type_params', $.type_parameters)),
      repeat($.cell_declaration),
      'end'
    ),

    // Impl declaration
    impl_declaration: $ => seq(
      'impl',
      field('trait', $.identifier),
      'for',
      field('type', $.type_annotation),
      repeat($.cell_declaration),
      'end'
    ),

    // Extern cell declaration (FFI)
    extern_declaration: $ => seq(
      'extern',
      'cell',
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      optional(seq('->', field('return_type', $.type_annotation))),
      optional(field('effects', $.effect_row))
    ),

    // Macro declaration
    macro_declaration: $ => seq(
      'macro',
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      repeat($._statement),
      'end'
    ),

    // Statements
    _statement: $ => choice(
      $.let_statement,
      $.let_destructure_statement,
      $.assignment_statement,
      $.compound_assignment,
      $.if_statement,
      $.match_statement,
      $.for_statement,
      $.while_statement,
      $.loop_statement,
      $.return_statement,
      $.break_statement,
      $.continue_statement,
      $.halt_statement,
      $.emit_statement,
      $.try_prefix_statement,
      $.defer_statement,
      $.yield_statement,
      $.expression_statement,
    ),

    let_statement: $ => seq(
      'let',
      optional('mut'),
      field('name', $.identifier),
      optional(seq(':', field('type', $.type_annotation))),
      optional(seq('=', field('value', $.expression)))
    ),

    // Destructuring let: let (a, b) = ..., let [x, y] = ..., let { a, b } = ...
    let_destructure_statement: $ => seq(
      'let',
      optional('mut'),
      field('pattern', choice(
        $.tuple_pattern,
        $.list_pattern,
        $.record_destructure_pattern,
        $.variant_pattern,
      )),
      '=',
      field('value', $.expression)
    ),

    // Record destructuring pattern with braces: { a, b }
    record_destructure_pattern: $ => seq(
      '{',
      sep1($.identifier, ','),
      '}'
    ),

    assignment_statement: $ => seq(
      field('target', choice(
        $.identifier,
        $.member_expression,
        $.index_expression
      )),
      '=',
      field('value', $.expression)
    ),

    // Compound assignments: +=, -=, *=, /=, //=, %=, **=, &=, |=, ^=
    compound_assignment: $ => seq(
      field('target', choice(
        $.identifier,
        $.member_expression,
        $.index_expression
      )),
      field('operator', choice(
        '+=', '-=', '*=', '/=', '//=', '%=', '**=', '&=', '|=', '^='
      )),
      field('value', $.expression)
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $.expression),
      repeat($._statement),
      optional(seq(
        'else',
        repeat($._statement)
      )),
      'end'
    ),

    match_statement: $ => seq(
      'match',
      field('value', choice(
        $.expression,
        seq('(', sep1($.expression, ','), ')')
      )),
      repeat1($.match_arm),
      'end'
    ),

    match_arm: $ => seq(
      field('pattern', $.pattern),
      optional(seq('if', field('guard', $.expression))),
      '->',
      choice(
        seq(repeat1($._statement), optional('end')),
        $.expression
      )
    ),

    // For loop with optional label and filter
    for_statement: $ => seq(
      'for',
      optional(field('label', $.label)),
      field('variable', choice(
        $.identifier,
        $.tuple_pattern,
      )),
      'in',
      field('iterable', $.expression),
      optional(seq('if', field('filter', $.expression))),
      repeat($._statement),
      'end'
    ),

    // While loop with optional label
    while_statement: $ => seq(
      'while',
      optional(field('label', $.label)),
      field('condition', $.expression),
      repeat($._statement),
      'end'
    ),

    // Loop with optional label
    loop_statement: $ => seq(
      'loop',
      optional(field('label', $.label)),
      repeat($._statement),
      'end'
    ),

    // Label: @name
    label: $ => seq('@', $.identifier),

    return_statement: $ => seq(
      'return',
      optional(field('value', $.expression))
    ),

    // Break with optional label and value
    break_statement: $ => seq(
      'break',
      optional(field('label', $.label)),
      optional(field('value', $.expression))
    ),

    // Continue with optional label
    continue_statement: $ => seq(
      'continue',
      optional(field('label', $.label))
    ),

    halt_statement: $ => seq(
      'halt',
      optional(field('value', $.expression))
    ),

    emit_statement: $ => seq(
      'emit',
      field('value', $.expression)
    ),

    // try is expression-level only in Lumen (try expr), not a block statement.
    // The try_expression rule handles postfix `?` on expressions.
    // This rule is kept as a statement form for `try expr` prefix usage.
    try_prefix_statement: $ => seq(
      'try',
      field('expression', $.expression)
    ),

    // Defer block: defer ... end
    defer_statement: $ => seq(
      'defer',
      repeat($._statement),
      'end'
    ),

    // Yield statement: yield expr (for generator cells)
    yield_statement: $ => seq(
      'yield',
      field('value', $.expression)
    ),

    expression_statement: $ => $.expression,

    // Patterns
    pattern: $ => choice(
      $.literal_pattern,
      $.identifier_pattern,
      $.variant_pattern,
      $.wildcard_pattern,
      $.list_pattern,
      $.tuple_pattern,
      $.record_pattern,
      $.type_pattern,
      $.or_pattern,
      $.range_pattern,
    ),

    literal_pattern: $ => choice(
      $.integer,
      $.float,
      $.string,
      $.boolean,
      'null'
    ),

    identifier_pattern: $ => $.identifier,

    variant_pattern: $ => seq(
      field('variant', $.identifier),
      optional(seq(
        '(',
        sep1($.pattern, ','),
        ')'
      ))
    ),

    wildcard_pattern: $ => '_',

    list_pattern: $ => seq(
      '[',
      optional(seq(
        sep1($.pattern, ','),
        optional(seq(',', '...', optional(field('rest', $.identifier))))
      )),
      ']'
    ),

    tuple_pattern: $ => seq(
      '(',
      sep1($.pattern, ','),
      ')'
    ),

    record_pattern: $ => seq(
      field('type', $.identifier),
      '(',
      sep1($.field_pattern, ','),
      ')'
    ),

    field_pattern: $ => seq(
      field('name', $.identifier),
      optional(seq(':', field('pattern', $.pattern)))
    ),

    type_pattern: $ => seq(
      field('pattern', choice($.identifier, $.wildcard_pattern)),
      ':',
      field('type', $.type_annotation)
    ),

    or_pattern: $ => seq(
      field('left', $.pattern),
      '|',
      field('right', $.pattern)
    ),

    // Range pattern: 1..10 or 1..=10
    range_pattern: $ => seq(
      field('start', choice($.integer, $.float)),
      field('operator', choice('..', '..=')),
      field('end', choice($.integer, $.float))
    ),

    // Expressions
    expression: $ => choice(
      $.literal,
      $.identifier,
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.member_expression,
      $.index_expression,
      $.null_safe_index_expression,
      $.list_expression,
      $.map_expression,
      $.set_expression,
      $.tuple_expression,
      $.record_expression,
      $.lambda_expression,
      $.pipe_expression,
      $.compose_expression,
      $.when_expression,
      $.comptime_expression,
      $.null_coalesce_expression,
      $.null_safe_expression,
      $.try_expression,
      $.null_assert_expression,
      $.is_expression,
      $.as_expression,
      $.await_expression,
      $.spawn_expression,
      $.parenthesized_expression,
      $.if_expression,
      $.match_expression,
      $.string_interpolation,
      $.range_expression,
    ),

    literal: $ => choice(
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.bytes,
      'null'
    ),

    binary_expression: $ => {
      const table = [
        [13, choice('*', '/', '//', '%')],
        [12, choice('+', '-', '++')],
        [11, choice('<<', '>>')],
        [10, choice('<', '<=', '>', '>=')],
        [9, choice('==', '!=')],
        [8, '&'],
        [7, '^'],
        [6, '|'],
        [5, 'in'],
        [4, 'and'],
        [3, 'or'],
      ];

      return choice(...table.map(([precedence, operator]) =>
        prec.left(precedence, seq(
          field('left', $.expression),
          field('operator', operator),
          field('right', $.expression)
        ))
      ));
    },

    unary_expression: $ => prec(14, seq(
      field('operator', choice('not', '-', '~')),
      field('operand', $.expression)
    )),

    call_expression: $ => prec(16, seq(
      field('function', choice(
        $.identifier,
        $.member_expression,
        $.qualified_name
      )),
      field('arguments', $.argument_list)
    )),

    argument_list: $ => seq(
      '(',
      optional(sep1(choice($.expression, $.named_argument), ',')),
      ')'
    ),

    named_argument: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $.expression)
    ),

    member_expression: $ => prec(17, seq(
      field('object', $.expression),
      '.',
      field('property', choice($.identifier, $.integer))
    )),

    index_expression: $ => prec(17, seq(
      field('object', $.expression),
      '[',
      field('index', $.expression),
      ']'
    )),

    // Null-safe index: expr?[index]
    null_safe_index_expression: $ => prec(17, seq(
      field('object', $.expression),
      '?[',
      field('index', $.expression),
      ']'
    )),

    list_expression: $ => seq(
      '[',
      optional(sep1($.expression, ',')),
      ']'
    ),

    map_expression: $ => seq(
      '{',
      optional(sep1($.map_entry, ',')),
      '}'
    ),

    map_entry: $ => seq(
      field('key', choice($.string, $.identifier)),
      ':',
      field('value', $.expression)
    ),

    // Set expression: {1, 2, 3} or set[1, 2, 3]
    set_expression: $ => choice(
      seq(
        'set',
        '[',
        optional(sep1($.expression, ',')),
        ']'
      ),
      seq(
        'set',
        '{',
        optional(sep1($.expression, ',')),
        '}'
      )
    ),

    tuple_expression: $ => seq(
      '(',
      $.expression,
      ',',
      sep1($.expression, ','),
      optional(','),
      ')'
    ),

    record_expression: $ => seq(
      field('type', $.identifier),
      '(',
      sep1($.record_field_value, ','),
      ')'
    ),

    // Record field value: name: expr or shorthand: just name
    record_field_value: $ => choice(
      seq(
        field('name', $.identifier),
        ':',
        field('value', $.expression)
      ),
      field('name', $.identifier)
    ),

    lambda_expression: $ => seq(
      'fn',
      field('parameters', $.parameter_list),
      choice(
        seq(
          optional(seq('->', field('return_type', $.type_annotation))),
          '=>',
          field('body', $.expression)
        ),
        seq(
          optional(seq('->', field('return_type', $.type_annotation))),
          repeat($._statement),
          'end'
        )
      )
    ),

    pipe_expression: $ => prec.left(1, seq(
      field('value', $.expression),
      '|>',
      field('function', $.expression)
    )),

    // Function composition: f ~> g creates a new composed function
    compose_expression: $ => prec.left(0, seq(
      field('left', $.expression),
      '~>',
      field('right', $.expression)
    )),

    null_coalesce_expression: $ => prec.left(2, seq(
      field('value', $.expression),
      '??',
      field('default', $.expression)
    )),

    null_safe_expression: $ => prec(17, seq(
      field('object', $.expression),
      '?.',
      field('property', $.identifier)
    )),

    try_expression: $ => prec(18, seq(
      field('expression', $.expression),
      '?'
    )),

    // Null assert: expr!
    null_assert_expression: $ => prec(18, seq(
      field('expression', $.expression),
      '!'
    )),

    // Type test: expr is Type
    is_expression: $ => prec.left(10, seq(
      field('expression', $.expression),
      'is',
      field('type', $.type_annotation)
    )),

    // Type cast: expr as Type
    as_expression: $ => prec.left(10, seq(
      field('expression', $.expression),
      'as',
      field('type', $.type_annotation)
    )),

    // Range expression: start..end or start..=end
    range_expression: $ => prec.left(2, seq(
      field('start', $.expression),
      field('operator', choice('..', '..=')),
      field('end', $.expression)
    )),

    await_expression: $ => seq(
      'await',
      choice(
        field('expression', $.expression),
        $.await_block
      )
    ),

    await_block: $ => seq(
      field('kind', choice('parallel', 'race', 'vote', 'select')),
      choice(
        seq(
          'for',
          field('variable', $.identifier),
          'in',
          field('iterable', $.expression),
          repeat($._statement),
          'end'
        ),
        seq(
          repeat($._statement),
          'end'
        )
      )
    ),

    spawn_expression: $ => seq(
      'spawn',
      '(',
      field('expression', $.expression),
      ')'
    ),

    parenthesized_expression: $ => seq(
      '(',
      $.expression,
      ')'
    ),

    if_expression: $ => seq(
      'if',
      field('condition', $.expression),
      field('consequence', $.expression),
      optional(seq('else', field('alternative', $.expression)))
    ),

    match_expression: $ => seq(
      'match',
      field('value', $.expression),
      repeat1($.match_arm),
      'end'
    ),

    // When expression: multi-branch conditional
    when_expression: $ => seq(
      'when',
      repeat1($.when_arm),
      'end'
    ),

    when_arm: $ => seq(
      field('condition', choice($.expression, '_')),
      '->',
      field('value', $.expression)
    ),

    // Comptime expression: compile-time evaluation
    comptime_expression: $ => seq(
      'comptime',
      repeat($._statement),
      'end'
    ),

    // String interpolation uses {expr} (not ${expr})
    string_interpolation: $ => seq(
      '"',
      repeat(choice(
        $.string_content,
        $.interpolation
      )),
      '"'
    ),

    string_content: $ => token(prec(-1, /[^"{\\]+/)),

    interpolation: $ => seq(
      '{',
      $.expression,
      '}'
    ),

    // Type annotations
    type_annotation: $ => choice(
      $.primary_type,
      $.union_type,
      $.function_type,
      $.optional_type,
    ),

    primary_type: $ => choice(
      $.simple_type,
      $.generic_type,
      $.qualified_type,
    ),

    simple_type: $ => choice(
      'Int',
      'Float',
      'Bool',
      'String',
      'Bytes',
      'Json',
      'Null',
      'Any',
      $.identifier,
    ),

    generic_type: $ => seq(
      field('name', choice(
        'list',
        'map',
        'set',
        'tuple',
        'result',
        $.identifier
      )),
      '[',
      sep1($.type_annotation, ','),
      ']'
    ),

    qualified_type: $ => seq(
      field('module', $.identifier),
      '.',
      field('name', $.identifier)
    ),

    union_type: $ => prec.left(1, seq(
      field('left', $.type_annotation),
      '|',
      field('right', $.type_annotation)
    )),

    // Optional type sugar: T? desugars to T | Null
    optional_type: $ => prec(2, seq(
      field('type', $.primary_type),
      '?'
    )),

    function_type: $ => seq(
      'fn',
      '(',
      optional(sep1($.type_annotation, ',')),
      ')',
      '->',
      field('return', $.type_annotation)
    ),

    // Qualified names (for tool names, imports, etc.)
    qualified_name: $ => seq(
      $.identifier,
      repeat1(seq('.', $.identifier))
    ),

    // Primitives
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    integer: $ => token(choice(
      /[0-9][0-9_]*/,
      /0x[0-9a-fA-F][0-9a-fA-F_]*/,
      /0b[01][01_]*/,
      /0o[0-7][0-7_]*/
    )),

    float: $ => token(choice(
      /[0-9][0-9_]*\.[0-9][0-9_]*/,
      /[0-9][0-9_]*\.[0-9][0-9_]*[eE][+-]?[0-9]+/,
      /[0-9][0-9_]*[eE][+-]?[0-9]+/
    )),

    string: $ => token(choice(
      seq('"', repeat(choice(/[^"\\]/, /\\./)), '"'),
      seq("'", repeat(choice(/[^'\\]/, /\\./)), "'"),
      seq('r"', repeat(/[^"]/), '"'),
      seq("r'", repeat(/[^']/), "'")
    )),

    boolean: $ => choice('true', 'false'),

    bytes: $ => token(seq(
      'b"',
      repeat(/[0-9a-fA-F]{2}/),
      '"'
    )),

    // Lumen comments use # (not //)
    comment: $ => token(seq('#', /.*/)),
  }
});

function sep1(rule, separator) {
  return seq(rule, repeat(seq(separator, rule)));
}
