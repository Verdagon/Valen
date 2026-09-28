use crate::typing::test::rust_interop::drive_helpers::typecheck;

#[test]
fn an_empty_allowlist_makes_nothing_importable() {
  let outcome = typecheck("main", r#"
exported func main() int {
  return add_two_numbers(20, 22);
}
"#, |_| ());
  assert!(outcome.expect_failure().is("CouldntFindFunctionToCallT"));
}

#[test]
fn an_item_not_in_the_allowlist_is_not_importable() {
  let outcome = typecheck("main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return seven();
}
"#, |_| ());
  assert!(outcome.expect_failure().is("CouldntFindFunctionToCallT"));
}
