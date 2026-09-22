use super::util::assert_borrow_error_renders;

const ALIASING_DIAGNOSTIC: &str = r#"At test:0.vale:6:14:
    badpair(&e, &e);
             ^
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#;

fn prelude_program(statement: &str) -> String {
  format!(
    "struct Entity {{ hp int; }}\n\
     func badpair<r', s'>(a &Entity in r, d &Entity in s) mut(r) {{ }}\n\
     exported func main() int {{\n  e = Entity(5);\n{statement}\n  return 0;\n}}\n"
  )
}

#[test]
fn test_violation_in_nested_block_caught() {
  assert_borrow_error_renders(
    &prelude_program("  block {\n    badpair(&e, &e);\n  }"),
    ALIASING_DIAGNOSTIC,
  );
}

#[test]
fn test_violation_in_if_arm_caught() {
  assert_borrow_error_renders(
    &prelude_program("  if (true) {\n    badpair(&e, &e);\n  }"),
    ALIASING_DIAGNOSTIC,
  );
}

#[test]
fn test_violation_in_while_body_caught() {
  assert_borrow_error_renders(
    &prelude_program("  while (false) {\n    badpair(&e, &e);\n  }"),
    ALIASING_DIAGNOSTIC,
  );
}

#[test]
fn test_violation_in_nested_arg_call_caught() {
  assert_borrow_error_renders(
    r#"
struct Entity { hp int; }
func badpairi<r', s'>(a &Entity in r, d &Entity in s) int mut(r) { return 0; }
func outer(x int) int { return x; }
exported func main() int {
  e = Entity(5);
  outer(badpairi(&e, &e));
  return 0;
}
"#,
    r#"At test:0.vale:7:19:
  outer(badpairi(&e, &e));
                  ^
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}

fn value_call_program(statement: &str) -> String {
  format!(
    "struct Entity {{ hp int; }}\n\
     func badpairi<r', s'>(a &Entity in r, d &Entity in s) int mut(r) {{ return 0; }}\n\
     exported func main() int {{\n  e = Entity(5);\n{statement}\n}}\n"
  )
}

#[test]
fn test_violation_in_let_initializer_caught() {
  assert_borrow_error_renders(
    &value_call_program("  y = badpairi(&e, &e);\n  return y;"),
    r#"At test:0.vale:5:17:
  y = badpairi(&e, &e);
                ^
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}

#[test]
fn test_violation_in_return_caught() {
  assert_borrow_error_renders(
    &value_call_program("  return badpairi(&e, &e);"),
    r#"At test:0.vale:5:20:
  return badpairi(&e, &e);
                   ^
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}

#[test]
fn test_violation_in_set_source_caught() {
  assert_borrow_error_renders(
    &value_call_program("  y = 0;\n  set y = badpairi(&e, &e);\n  return y;"),
    r#"At test:0.vale:6:21:
  set y = badpairi(&e, &e);
                    ^
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}

