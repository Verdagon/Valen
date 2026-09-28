use crate::typing::test::compiler_test_compilation::compiler_test_compilation;
use crate::builtins::builtins::{builtin_source_bundle, empty_v_builtins_stub};
use crate::code_source::{CodeSource, Source};
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::scout_arena::ScoutArena;
use crate::tests::tests::new_test_code_map;
use crate::typing::test::humanize_helper::{assert_humanized_eq, humanize_compile_error};
use crate::typing::typing_interner::TypingInterner;
use bumpalo::Bump;

pub struct GroupFactsView {
  pub group_paths: Vec<String>,
  pub accessed_group_sets: Vec<Vec<u32>>,
}

fn code_source_for<'a, 'ctx>(
  builtins: &[&str],
  code: &str,
  parse_arena: &'ctx ParseArena<'a>,
  parser_keywords: &'ctx Keywords<'a>,
) -> CodeSource<'a>
where
  'a: 'ctx,
{
  if builtins.is_empty() {
    CodeSource::new(vec![new_test_code_map(parse_arena, code)])
  } else {
    CodeSource::new(vec![
      builtin_source_bundle(parse_arena, parser_keywords, builtins),
      new_test_code_map(parse_arena, code),
      Source::Fn(empty_v_builtins_stub),
    ])
  }
}

pub fn assert_borrow_check_gives_error(builtins: &[&str], code: &str, expected: &str) {
  let (parse_bump, scout_bump, typing_bump) = (Bump::new(), Bump::new(), Bump::new());
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code_source = code_source_for(builtins, code, &parse_arena, &parser_keywords);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let err = compile.get_compiler_outputs().err().expect("expected a borrow error, got Ok");
  assert_humanized_eq(&humanize_compile_error(&mut compile, err), expected);
}

pub fn assert_borrow_check_passes(builtins: &[&str], code: &str) {
  let (parse_bump, scout_bump, typing_bump) = (Bump::new(), Bump::new(), Bump::new());
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code_source = code_source_for(builtins, code, &parse_arena, &parser_keywords);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  compile.expect_compiler_outputs();
}

pub fn param_noalias_of(builtins: &[&str], code: &str, function_human_name: &str) -> Vec<bool> {
  let (parse_bump, scout_bump, typing_bump) = (Bump::new(), Bump::new(), Bump::new());
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code_source = code_source_for(builtins, code, &parse_arena, &parser_keywords);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let hinputs = compile.expect_compiler_outputs();
  hinputs.param_noalias(function_human_name).to_vec()
}

pub fn group_facts_of(builtins: &[&str], code: &str, function_human_name: &str) -> GroupFactsView {
  let (parse_bump, scout_bump, typing_bump) = (Bump::new(), Bump::new(), Bump::new());
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code_source = code_source_for(builtins, code, &parse_arena, &parser_keywords);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let hinputs = compile.expect_compiler_outputs();
  let info = hinputs.aliasing_info(function_human_name);
  GroupFactsView {
    group_paths: info.group_paths.iter().map(|g| g.name()).collect(),
    accessed_group_sets: info
      .instruction_loc_to_accessed_groups
      .iter()
      .map(|(_loc, set)| set.to_vec())
      .collect(),
  }
}
