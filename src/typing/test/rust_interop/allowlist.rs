use crate::typing::test::rust_interop::drive_helpers::typecheck;

#[test]
fn an_empty_allowlist_makes_nothing_importable() {
  let outcome = typecheck("main", r#"
exported func main() int {
  return add_two_numbers(20, 22);
}
"#, |_| ());
  let failure = outcome.expect_failure();
  assert!(
    failure.detail.contains("Couldn't find a suitable function add_two_numbers"),
    "failure:\n{}", failure.detail);
}

#[test]
fn an_item_not_in_the_allowlist_is_not_importable() {
  let outcome = typecheck("main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return seven();
}
"#, |_| ());
  let failure = outcome.expect_failure();
  assert!(
    failure.detail.contains("Couldn't find a suitable function seven"),
    "failure:\n{}", failure.detail);
}
