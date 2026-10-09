
use super::util::assert_borrow_check_passes;
use crate::typing::test::borrow_checker::same_group_aliasing_tests::program;

#[test]
fn test_churn_fresh_local_group_is_accepted() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], &program(r#"
func attack<r'>(a &Vec<Entity> in r, t &Vec<Entity> in r) {
  nv = Vec<Entity>(Array<Entity>(0));
  e = &a.data[0];
  grow(&nv);
  observe(e);
}
exported func main() int {
  v = Vec<Entity>(Array<Entity>(3));
  attack(&v, &v);
  return 0;
}
"#));
}
