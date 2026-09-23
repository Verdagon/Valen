use super::util::assert_param_noalias;

#[test]
fn sole_borrow_param_is_noalias() {
  assert_param_noalias(
    r#"
struct Ship { fuel int; }
func peek<g'>(s &Ship in g) { }
exported func main() int {
  ship = Ship(1);
  peek(&ship);
  return 0;
}
"#,
    "peek",
    &[true],
  );
}

// Parameters in distinct groups are each the sole reference into their own group, so both are
// `noalias` — even read-only, since a caller aliasing them is harmless without mutation.
#[test]
fn distinct_group_params_are_both_noalias() {
  assert_param_noalias(
    r#"
struct Ship { fuel int; }
func duo<r', s'>(a &Ship in r, b &Ship in s) { }
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  duo(&s1, &s2);
  return 0;
}
"#,
    "duo",
    &[true, true],
  );
}

#[test]
fn anonymous_group_params_are_both_noalias() {
  assert_param_noalias(
    r#"
struct Ship { fuel int; }
func anon(a &Ship, b &Ship) { }
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  anon(&s1, &s2);
  return 0;
}
"#,
    "anon",
    &[true, true],
  );
}

#[test]
fn non_borrow_param_is_not_noalias() {
  assert_param_noalias(
    r#"
struct Ship { fuel int; }
func mix<g'>(a &Ship in g, b int) { }
exported func main() int {
  ship = Ship(1);
  mix(&ship, 7);
  return 0;
}
"#,
    "mix",
    &[true, false],
  );
}
