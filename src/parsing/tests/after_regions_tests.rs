use crate::keywords::Keywords;
use crate::lexing::errors::ParseError;
use crate::parse_arena::ParseArena;
use crate::parsing::tests::utils::{compile_block_contents, compile_statement};
use bumpalo::Bump;

#[test]
fn forgetting_set_when_changing() {
  // This test does not pass yet, use #[ignore].
  let parse_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let keywords = Keywords::new_for_parse(&parse_arena);
  let error = compile_statement(&parse_arena, &keywords, "ship.x = 4;").unwrap_err();
  match error {
    ParseError::ForgotSetKeyword(_) => {}
    other => panic!("Expected ForgotSetKeyword, got {:?}", other),
  }
}

// A trailing `mut` after a type is a parameter-only placeholder; in a let it is still junk after
// the type, which the let path reports as the "forgot set" case.
#[test]
fn let_with_trailing_mut_still_errors() {
  let parse_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let keywords = Keywords::new_for_parse(&parse_arena);
  let error = compile_statement(&parse_arena, &keywords, "x &Win mut = 5;").unwrap_err();
  match error {
    ParseError::ForgotSetKeyword(_) => {}
    other => panic!("Expected ForgotSetKeyword, got {:?}", other),
  }
}
