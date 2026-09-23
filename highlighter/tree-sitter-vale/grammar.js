/**
 * Vale language grammar for tree-sitter.
 *
 * Vale syntax reference: https://vale.dev/
 * Authoritative token source: FrontendRust/src/lexing/lexer.rs
 *
 * Design notes:
 * - We prioritize highlighting correctness over perfect AST accuracy.
 * - `<` / `>` ambiguity (generics vs comparisons) is resolved via prec.dynamic.
 * - Block `{...}` is used for function bodies, struct bodies, lambdas, and
 *   if/while/foreach bodies. We represent all of these as `block` nodes.
 * - Vale has no `let` keyword: `x = expr;` is a let-binding at statement level.
 */

module.exports = grammar({
  name: 'vale',

  // Used for keyword extraction (keyword conflicts resolved by word boundary).
  word: $ => $.identifier,

  // Always-valid nodes transparently skipped during parsing.
  extras: $ => [
    /\s/,
    $.line_comment,
    $.chevron_comment,
    $.ellipsis,
  ],

  // External scanner handles string interpolation and triple-quoted strings.
  externals: $ => [
    $._string_start,            // "  (single-quoted string open)
    $._multiline_string_start,  // """ (triple-quoted string open)
    $._string_content,          // characters inside a string
    $._string_end,              // " or """ (string close, matches opening)
    $._interp_open,             // { inside a string (not followed by \n)
    $._interp_close,            // } that closes an interpolation
    // `<` that opens template args. The scanner emits this only when the `<` is
    // NOT surrounded by whitespace on both sides; a space-surrounded ` < ` is left
    // to the internal lexer as a comparison operator (the Valen parser's rule).
    $._generic_open,
  ],

  // GLR conflict declarations.
  // Only truly ambiguous conflicts go here; unnecessary ones cause GLR paths
  // that trigger error-recovery which breaks external scanner token handling.
  conflicts: $ => [
    // Foo<T>(args) — generic call vs comparison: < is both generic-open and less-than
    [$._expression, $.generic_args],
    [$.generic_args, $.binary_expression],
    // named_type: region + identifier is ambiguous (region is optional)
    [$.named_type, $.named_type],
    // Destructure pattern vs call expression: Foo(a, b) at statement level
    [$.destructure_pattern, $._expression],
    // named_parameter ambiguous with expression in interpolated strings
    [$.named_parameter, $._expression],
    // function_type: <mut>(...) vs array_type < mut > followed by function_type(...)
    [$.function_type],
    // Lambda: (x) could be parameter_list or parenthesized_expression
    [$.parameter_list, $.parenthesized_expression],
    // Lambda expression ambiguity: &(params){body}
    [$.lambda_expression],
    // method_call vs field_access: expr.name followed by '('
    [$.method_call_expression, $.field_access_expression],
    // `[` opens either an array type (`[]int`) or a sequence destructure (`[a, b]`).
    [$.array_type, $.sequence_destructure_pattern],
    // `&{ … }` is either a borrow of a block or a no-param shorthand lambda.
    [$._expression, $.lambda_expression],
    // `&x'[]T`: the region may attach to the borrow or to the array type.
    [$.array_type],
    // A leading region opens either a region_block (`x'{…}`) or a region-qualified
    // type (`x'int`); with a parameter name in play the reading also involves
    // named_parameter.
    [$.named_type, $._expression],
    [$.named_parameter, $.named_type, $._expression],
    // `self` as a parameter receiver vs `self` used as an expression.
    [$.self_parameter, $.self_expression],
    // `name Type in g`: the `in g` placement can attach to the parameter or its type.
    [$.named_parameter, $.named_type],
    // `a name …`: an infix call vs a following lambda/identifier expression.
    [$.infix_call_expression, $.lambda_expression],
    [$.infix_call_expression, $._expression],
    // An `if` (or block-valued expr) at the tail of a block could be a statement
    // or the block's implicit-return result; let the GLR parser keep both.
    [$._statement, $._expression],
  ],

  rules: {
    // -----------------------------------------------------------------------
    // Top level
    // -----------------------------------------------------------------------

    source_file: $ => repeat($._definition),

    _definition: $ => choice(
      $.import_definition,
      $.function_definition,
      $.struct_definition,
      $.interface_definition,
      $.impl_definition,
      $.export_definition,
    ),

    // -----------------------------------------------------------------------
    // Attributes
    // -----------------------------------------------------------------------

    _attribute: $ => choice(
      $.keyword_attribute,
      $.macro_attribute,
      $.extern_attribute,
    ),

    keyword_attribute: $ => choice(
      'abstract', 'pure', 'unsafe', 'weakable', 'sealed',
      'linear', 'additive', 'exported', 'virtual',
    ),

    // #DeriveStructDrop, #!DeriveAnonymousSubstruct, etc.
    macro_attribute: $ => token(/#!?[A-Z][a-zA-Z0-9]*/),

    // extern or extern("cpp_name")
    extern_attribute: $ => seq(
      'extern',
      optional(seq('(', $.string_literal, ')')),
    ),

    // -----------------------------------------------------------------------
    // Import
    // -----------------------------------------------------------------------

    import_definition: $ => seq(
      'import',
      $._dotted_path,
      ';',
    ),

    _dotted_path: $ => seq(
      $.identifier,
      repeat(seq('.', choice($.identifier, '*'))),
    ),

    // -----------------------------------------------------------------------
    // Export
    // -----------------------------------------------------------------------

    // export Type as Alias;  (e.g. `export []int as MutIntArray;`)
    export_definition: $ => seq(
      'export',
      field('exported', $._type_expression),
      optional(seq('as', field('alias', $.identifier))),
      ';',
    ),

    // -----------------------------------------------------------------------
    // Function definition
    // -----------------------------------------------------------------------

    function_definition: $ => seq(
      repeat($._attribute),
      'func',
      field('name', choice($.identifier, $.operator_name)),
      optional(field('generic_params', $.generic_params)),
      field('parameters', $.parameter_list),
      optional(seq(
        field('return_type', $._type_expression),
        // region placement and variadic marker: `&T in g...`
        optional(seq('in', field('return_region', $.identifier))),
        optional('...'),
      )),
      optional(field('where_clause', $.where_clause)),
      choice(
        field('body', $.block),
        field('body', $.region_block),
        ';',
      ),
    ),

    // Operator function names: func +(a int, b int) int { ... }, func <(...) bool
    operator_name: $ => choice('+', '-', '*', '/', '===', '==', '!=', '<=>', '<=', '>=', '<', '>'),

    parameter_list: $ => seq(
      '(',
      optional(seq(
        $._parameter,
        repeat(seq(',', $._parameter)),
        optional(','),
      )),
      ')',
    ),

    _parameter: $ => choice(
      $.self_parameter,
      $.named_parameter,
    ),

    self_parameter: $ => seq(
      optional('virtual'),
      choice('self', 'this'),
      $._type_expression,
    ),

    named_parameter: $ => seq(
      optional('virtual'),
      field('name', $.identifier),
      optional(field('type', $._type_expression)),
      // region placement: `arr &[]E in g`
      optional(seq('in', field('region', $.identifier))),
    ),

    // -----------------------------------------------------------------------
    // Struct definition
    // -----------------------------------------------------------------------

    // struct Name<generics>? share? (where ...)? (region ...)? ( ; | { members } )
    struct_definition: $ => seq(
      repeat($._attribute),
      'struct',
      field('name', $.identifier),
      optional(field('generic_params', $.generic_params)),
      optional(field('sharedness', $.sharedness)),
      optional(field('where_clause', $.where_clause)),
      optional(field('default_region', $.default_region)),
      choice(
        seq('{', repeat($._struct_member), '}'),
        ';',
      ),
    ),

    _struct_member: $ => choice(
      $.field_definition,
      $.function_definition,
    ),

    // name int; or name! int; (! = mutable field). The name may be an index
    // (`0 T0;`) for tuple-shaped structs.
    field_definition: $ => seq(
      field('name', choice($.identifier, $.integer_literal)),
      optional('!'),
      field('type', $._type_expression),
      ';',
    ),

    // -----------------------------------------------------------------------
    // Interface definition
    // -----------------------------------------------------------------------

    // interface Name<generics>? share? (where ...)? (region ...)? ( ; | { methods } )
    interface_definition: $ => seq(
      repeat($._attribute),
      'interface',
      field('name', $.identifier),
      optional(field('generic_params', $.generic_params)),
      optional(field('sharedness', $.sharedness)),
      optional(field('where_clause', $.where_clause)),
      optional(field('default_region', $.default_region)),
      choice(
        seq('{', repeat($.function_definition), '}'),
        ';',
      ),
    ),

    // -----------------------------------------------------------------------
    // Impl definition
    // -----------------------------------------------------------------------

    impl_definition: $ => seq(
      'impl',
      optional(field('generic_params', $.generic_params)),
      field('interface', $._type_expression),
      'for',
      field('struct', $._type_expression),
      choice(
        seq('{', repeat($.function_definition), '}'),
        ';',
      ),
    ),

    // -----------------------------------------------------------------------
    // Generic params (type parameter declarations)
    // -----------------------------------------------------------------------

    generic_params: $ => seq(
      $._generic_open,
      $._generic_param,
      repeat(seq(',', $._generic_param)),
      optional(','),
      '>',
    ),

    _generic_param: $ => choice(
      $.region_param,
      $.int_param,
      $.type_param,
    ),

    region_param: $ => seq($.region, optional(choice($.mutability, 'rw', 'ro')), optional('Region')),

    int_param: $ => seq('#', $.identifier, optional('Int')),

    type_param: $ => seq($.identifier, optional($._type_constraint)),

    _type_constraint: $ => choice(
      seq('Ref', optional($.ownership), optional($.mutability)),
      'Kind',
      'Region',
      'Int',
      'Prot',
    ),

    // -----------------------------------------------------------------------
    // Generic args (at call/instantiation sites)
    // -----------------------------------------------------------------------

    generic_args: $ => seq(
      $._generic_open,
      $._generic_arg,
      repeat(seq(',', $._generic_arg)),
      optional(','),
      '>',
    ),

    _generic_arg: $ => choice(
      $.region,
      // A mutability/ownership kind as a positional template arg, e.g.
      // `IFunction1<mut, int, bool>`. (The standalone `<mut>`/`<imm>` annotation
      // forms are deprecated and intentionally unsupported.)
      $.mutability,
      $.ownership,
      $.integer_literal,
      seq('#', $.integer_literal),
      $._type_expression,
    ),

    // -----------------------------------------------------------------------
    // Where clause
    // -----------------------------------------------------------------------

    where_clause: $ => seq(
      'where',
      $.where_constraint,
      repeat(seq(',', $.where_constraint)),
    ),

    where_constraint: $ => choice(
      // where func drop(T)void | where func ==(&T, &T)bool | where func(&F, E)void
      seq(
        'func',
        optional(field('name', choice($.identifier, $.operator_name))),
        '(',
        optional(seq(
          $._type_expression,
          repeat(seq(',', $._type_expression)),
          optional(','),
        )),
        ')',
        field('return_type', $._type_expression),
      ),
      // where implements(Sub, Super)
      seq(
        field('name', $.identifier),
        '(',
        optional(seq(
          $._type_expression,
          repeat(seq(',', $._type_expression)),
          optional(','),
        )),
        ')',
      ),
      // where T   |   where T : Super
      seq($._type_expression, optional(seq(':', $._type_expression))),
    ),

    // -----------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------

    _type_expression: $ => choice(
      $.primitive_type,
      $.metatype,
      $.borrow_type,
      $.weak_ref_type,
      $.array_type,
      $.function_type,
      $.named_type,
    ),

    primitive_type: $ => choice(
      'int', 'bool', 'float', 'str', 'void',
      'i8', 'i16', 'i32', 'i64',
      'u8', 'u16', 'u32', 'u64',
      '__Never',
    ),

    metatype: $ => choice(
      'Ref', 'Kind', 'Region', 'Prot', 'RefList',
      'Ownership', 'Variability', 'Mutability', 'Location', 'Refs', 'Int',
    ),

    // &Type, &'r Type, &'r mut Type
    borrow_type: $ => prec(2, seq(
      '&',
      optional($.region),
      optional($.mutability),
      $._type_expression,
    )),

    // &&Type (weak reference)
    weak_ref_type: $ => prec(2, seq('&&', $._type_expression)),

    // []T, [#N]T, #[]T (immutable), with an optional region prefix (`x'[]bool`).
    // A leading `#` marks an immutable array; `[#N]` inside the brackets is size.
    array_type: $ => seq(
      optional('#'),
      optional($.region),
      '[',
      optional(seq('#', choice($.integer_literal, $.identifier))),
      ']',
      $._type_expression,
    ),

    // (int, bool) -> void
    function_type: $ => seq(
      '(',
      optional(seq(
        $._type_expression,
        repeat(seq(',', $._type_expression)),
        optional(','),
      )),
      ')',
      optional(seq('->', $._type_expression)),
    ),

    // Foo, 'r Foo, mut Foo, Foo<T>, Foo in g, etc.
    named_type: $ => seq(
      optional($.region),
      optional($.mutability),
      optional($.ownership),
      field('name', $.identifier),
      optional(field('type_args', $.generic_args)),
      // region placement: `OkType in g`
      optional(seq('in', field('region', $.identifier))),
    ),

    // -----------------------------------------------------------------------
    // Ownership and mutability
    // -----------------------------------------------------------------------

    ownership: $ => choice('own', 'borrow', 'weak', 'share'),
    mutability: $ => choice('mut', 'imm'),
    // Citizen-level sharedness marker: `struct Foo share { ... }`.
    sharedness: $ => 'share',
    // Optional default-region declaration on a citizen: `region 'r`.
    default_region: $ => seq('region', $._type_expression),

    // Region annotation: a name with a trailing apostrophe, e.g. r' we' gen'
    region: $ => /[A-Za-z_]\w*'/,

    // -----------------------------------------------------------------------
    // Block
    // -----------------------------------------------------------------------

    // A block is zero or more statements, then an optional bare result expression
    // (the implicit return, e.g. the `1` in `if c { 1 } else { 2 }`).
    block: $ => seq(
      '{',
      repeat($._statement),
      optional(field('result', $._expression)),
      '}',
    ),

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    _statement: $ => choice(
      $.let_statement,
      $.set_statement,
      $.return_statement,
      $.break_statement,
      $.destruct_statement,
      $.unlet_statement,
      $.while_statement,
      $.foreach_statement,
      // `if` / `block { … }` used as a statement need no trailing ';'.
      $.if_expression,
      $.block_expression,
      $.place_assignment,
      $.expression_statement,
    ),

    // Assignment to a place (field/index), no `set` keyword: `self.hp = 10;`.
    place_assignment: $ => seq(
      field('target', choice($.field_access_expression, $.index_expression)),
      '=',
      field('value', $._expression),
      ';',
    ),

    // pattern type? = expr;  (no 'let' keyword in Vale; the optional type is an
    // explicit annotation, e.g. `x Result<Raza, IShip> = ...;`)
    let_statement: $ => prec(1, seq(
      field('pattern', $._let_pattern),
      optional(field('type', $._binding_type)),
      '=',
      field('value', $._expression),
      ';',
    )),

    // Type in a binding annotation. Excludes function_type: a leading '(' there
    // would collide with a bare call statement `name(args);`.
    _binding_type: $ => choice(
      $.primitive_type,
      $.metatype,
      $.borrow_type,
      $.weak_ref_type,
      $.array_type,
      $.named_type,
    ),

    _let_pattern: $ => prec(1, choice(
      $.identifier,
      $.destructure_pattern,
      $.sequence_destructure_pattern,
    )),

    // [a, b] = expr; — positional destructure.
    sequence_destructure_pattern: $ => seq(
      '[',
      optional(seq(
        $._destructure_element,
        repeat(seq(',', $._destructure_element)),
        optional(','),
      )),
      ']',
    ),

    // set target = expr;
    set_statement: $ => seq(
      'set',
      field('target', $._expression),
      '=',
      field('value', $._expression),
      ';',
    ),

    return_statement: $ => seq('return', optional($._expression), ';'),

    break_statement: $ => seq('break', ';'),

    destruct_statement: $ => seq(
      'destruct',
      $._let_pattern,
      '=',
      $._expression,
      ';',
    ),

    unlet_statement: $ => seq('unlet', $.identifier, ';'),

    // Condition may be parenthesized or bare: `while (x) { }` or `while x < 3 { }`.
    while_statement: $ => seq('while', field('condition', $._expression), field('body', $.block)),

    foreach_statement: $ => seq(
      'foreach',
      field('pattern', $._let_pattern),
      'in',
      field('iterable', $._expression),
      $.block,
    ),

    // expr; — expression used as a statement. The ';' is required; a bare trailing
    // expression is the block's implicit-return result instead (see `block`).
    expression_statement: $ => seq($._expression, ';'),

    // -----------------------------------------------------------------------
    // Destructure pattern
    // -----------------------------------------------------------------------

    destructure_pattern: $ => seq(
      $.identifier,
      '(',
      optional(seq(
        $._destructure_element,
        repeat(seq(',', $._destructure_element)),
        optional(','),
      )),
      ')',
    ),

    _destructure_element: $ => choice(
      $.identifier,
      '_',
      $.destructure_pattern,
      // nested [a, b] and `set x` (rebind existing) elements
      $.sequence_destructure_pattern,
      seq('set', $.identifier),
    ),

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    _expression: $ => choice(
      $.binary_expression,
      $.infix_call_expression,
      $.unary_not_expression,
      $.unary_minus_expression,
      $.if_expression,
      $.foreach_statement,
      $.region_block,
      $.lambda_expression,
      $.call_expression,
      $.method_call_expression,
      $.field_access_expression,
      $.index_expression,
      $.borrow_expression,
      $.weak_borrow_expression,
      $.borrow_mut_expression,
      $.owning_expression,
      $.weak_expression,
      $.array_expression,
      $.array_construct_expression,
      $.block_expression,
      $.operator_call_expression,
      $.set_expression,
      $.as_expression,
      $.string_literal,
      $.multiline_string_literal,
      $.float_literal,
      $.integer_literal,
      $.boolean_literal,
      $.tuple_expression,
      $.parenthesized_expression,
      $.block,
      $.self_expression,
      $.identifier,
    ),

    self_expression: $ => choice('self', 'this'),

    // Infix function call: `3 bork 3` means `bork(3, 3)`. Lower precedence than
    // the built-in binary operators.
    infix_call_expression: $ => prec.left(0, seq(
      field('left', $._expression),
      field('function', $.identifier),
      field('right', $._expression),
    )),

    binary_expression: $ => {
      const ops = [
        [['..'],               6, 'left'],
        [['*', '/'],          5, 'left'],
        [['+', '-'],          4, 'left'],
        [['mod'],             3, 'left'],
        [[token(prec(1, '<=>')),  // prec(1) so <=> beats <= in the tokenizer
          '<=', '>=', '===', '==', '!='], 2, 'left'],
        [['<', '>'],          2, 'left'],
        [['and', 'or'],       1, 'left'],
      ];
      return choice(...ops.flatMap(([operators, p, assoc]) =>
        operators.map(op =>
          (assoc === 'left' ? prec.left : prec.right)(p, seq(
            field('left', $._expression),
            field('operator', op),
            field('right', $._expression),
          ))
        )
      ));
    },

    unary_not_expression: $ => prec(10, seq('not', $._expression)),
    unary_minus_expression: $ => prec(10, seq('-', $._expression)),

    // Condition may be parenthesized or bare: `if (x) { }` or `if x < 3 { }`.
    if_expression: $ => prec.right(seq(
      'if',
      field('condition', $._expression),
      field('then', $.block),
      optional(seq('else', field('else', choice($.block, $.if_expression)))),
    )),

    // Lambda forms:
    //   (params) => { body }
    //   (params) { body }
    //   &(params) => { body }
    //   &(params) { body }
    //   &!(params) { body }
    lambda_expression: $ => {
      // The body is a block or a bare expression; `=>` is optional before a block.
      const body = choice($.block, seq('=>', $._expression));
      return choice(
        // (params) body, optionally &-prefixed / &!-prefixed
        seq(optional(seq('&', optional('!'))), $.parameter_list, body),
        // shorthand with the implicit `_` param and no list: &{ _ * 2 }
        seq('&', optional('!'), $.block),
      );
    },

    // Foo(args), Foo<T>(args)
    call_expression: $ => prec(8, seq(
      field('function', $._expression),
      optional(field('generic_args', $.generic_args)),
      '(',
      optional(field('arguments', $.argument_list)),
      ')',
    )),

    // expr.method(args), expr.method<T>(args)
    method_call_expression: $ => prec(9, seq(
      field('receiver', $._expression),
      '.',
      field('method', $.identifier),
      optional(field('generic_args', $.generic_args)),
      '(',
      optional(field('arguments', $.argument_list)),
      ')',
    )),

    argument_list: $ => seq(
      $._expression,
      repeat(seq(',', $._expression)),
      optional(','),
    ),

    // expr.field, expr.0
    field_access_expression: $ => prec(9, seq(
      field('object', $._expression),
      '.',
      field('field', choice($.identifier, $.integer_literal)),
    )),

    // expr[idx]
    index_expression: $ => prec(9, seq(
      field('object', $._expression),
      '[',
      field('index', $._expression),
      ']',
    )),

    borrow_expression:     $ => prec(7, seq('&',  $._expression)),
    weak_borrow_expression:$ => prec(7, seq('&&', $._expression)),
    borrow_mut_expression: $ => prec(7, seq('&!', $._expression)),
    // ^expr — owning-move / take.
    owning_expression:     $ => prec(7, seq('^',  $._expression)),
    // weak expr — make a weak reference.
    weak_expression:       $ => prec(7, seq('weak', $._expression)),

    // [#](a, b, c) or sized [#5](...) — array literal.
    array_expression: $ => prec(8, seq(
      '[', '#', optional($.integer_literal), ']',
      '(',
      optional(field('elements', $.argument_list)),
      ')',
    )),

    // An array type used as a constructor: `[][]int(0)`, `[#5](&{_ * 42})`.
    array_construct_expression: $ => prec(8, seq(
      field('type', $.array_type),
      '(',
      optional(field('arguments', $.argument_list)),
      ')',
    )),

    // block { ... } and pure block { ... } as an expression.
    block_expression: $ => seq(optional('pure'), 'block', $.block),

    // A region-scoped block: `x'{ ... }`.
    region_block: $ => seq($.region, $.block),

    // Calling an operator like a function: `+(a, b)`.
    operator_call_expression: $ => prec(8, seq(
      field('function', $.operator_name),
      '(',
      optional(field('arguments', $.argument_list)),
      ')',
    )),

    // set as an expression (returns the assigned value): `(set b = expr)`.
    set_expression: $ => prec.right(seq(
      'set',
      field('target', $._expression),
      '=',
      field('value', $._expression),
    )),

    as_expression: $ => prec.left(6, seq($._expression, 'as', $._type_expression)),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    // (a, b, c) — tuple literal (two or more elements distinguishes it from a
    // parenthesized expression).
    tuple_expression: $ => seq(
      '(',
      $._expression,
      ',',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression)),
        optional(','),
      )),
      ')',
    ),

    // -----------------------------------------------------------------------
    // String literals
    // -----------------------------------------------------------------------

    string_literal: $ => seq(
      $._string_start,
      repeat($._string_part),
      $._string_end,
    ),

    multiline_string_literal: $ => seq(
      $._multiline_string_start,
      repeat($._string_part),
      $._string_end,
    ),

    _string_part: $ => choice(
      $._string_content,
      $.string_escape,
      $.string_interpolation,
    ),

    string_interpolation: $ => seq(
      $._interp_open,
      $._expression,
      $._interp_close,
    ),

    string_escape: $ => token(seq(
      '\\',
      choice(/[rtn\\"\/\{\}]/, seq('u', /[0-9A-Fa-f]{4}/)),
    )),

    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------

    float_literal:   $ => /\d+\.\d+/,
    integer_literal: $ => /\d+(?:i8|i16|i32|i64|u8|u16|u32|u64|usize)?/,
    boolean_literal: $ => choice('true', 'false'),

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    line_comment:    $ => token(seq('//', /.*/)),
    chevron_comment: $ => /\u00AB[^\u00BB]*\u00BB/,
    ellipsis:        $ => choice('...', '\u2026'),

    // -----------------------------------------------------------------------
    // Identifiers
    // -----------------------------------------------------------------------

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,
  },
});
