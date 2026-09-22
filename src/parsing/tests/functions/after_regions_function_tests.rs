use crate::keywords::Keywords;
use crate::lexing::errors::ParseError;
use crate::parse_arena::ParseArena;
use crate::parsing::tests::utils::compile_denizen;
use bumpalo::Bump;

#[test]
// V: unignore this
#[ignore]
fn func_with_func_bound_with_missing_where() {
  let parse_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let keywords = Keywords::new_for_parse(&parse_arena);
  let err =
    compile_denizen(&parse_arena, &keywords, "func sum<T>() func moo(&T)void {3}").unwrap_err();
  match err {
    ParseError::FuncBoundWithoutWhere(_) => {}
    other => panic!("Expected FuncBoundWithoutWhere, got {:?}", other),
  }
}
