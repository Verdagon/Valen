use super::util::{assert_borrow_error_renders, assert_compiles_clean};

#[test]
fn test_only_the_unsafe_call_among_many_is_flagged() {
  assert_borrow_error_renders(
    r#"
struct Entity { hp int; }
func badpair<r', s'>(a &Entity in r, d &Entity in s) mut(r) { }
func safe(x int) int { return x; }
exported func main() int {
  e = Entity(5);
  safe(1);
  badpair(&e, &e);
  safe(2);
  return 0;
}
"#,
    r#"At test:0.vale:8:12:
  badpair(&e, &e);
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}

#[test]
fn test_mixed_group_and_plain_params_no_false_positive() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func mixed<r'>(a &Entity in r, b int) mut(r) { }
exported func main() int {
  e = Entity(5);
  mixed(&e, 7);
  return 0;
}
"#);
}

#[test]
fn test_same_callee_safe_and_unsafe_sites() {
  assert_borrow_error_renders(
    r#"
struct Entity { hp int; }
func badpair<r', s'>(a &Entity in r, d &Entity in s) mut(r) { }
exported func main() int {
  e = Entity(5);
  e2 = Entity(6);
  badpair(&e, &e2);
  badpair(&e, &e);
  return 0;
}
"#,
    r#"At test:0.vale:8:12:
  badpair(&e, &e);
Arguments 0 and 1 both borrow into e, but their parameters are in disjoint mutated groups r and s, which the callee may treat as non-aliasing.
"#,
  );
}
