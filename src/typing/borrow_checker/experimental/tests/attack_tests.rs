use super::util::assert_compiles_clean;

// Slice 22 (capstone): `attack`'s own body mutates both borrows' members (no structural op), and
// `main` calls it with both distinct and aliasing arguments. The whole program borrow-checks clean
// end-to-end — member writes are not call violations, and common-group aliasing is safe.
#[test]
fn test_full_attack_program_is_safe() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  set a.hp = 1;
  set d.hp = 2;
}
exported func main() int {
  e = Entity(5);
  e2 = Entity(6);
  attack(&e, &e2);
  attack(&e, &e);
  return 0;
}
"#);
}
