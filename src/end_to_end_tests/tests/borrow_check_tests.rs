use crate::end_to_end_tests::assert_inline_compile_and_run_without_borrow_check;

#[test]
fn program_symphony_cannot_check_compiles_and_runs_with_borrow_check_disabled() {
  assert_inline_compile_and_run_without_borrow_check(
    r#"
struct Spaceship { fuel int; }
func getFuel(a &Spaceship) int {
  return __copy_prim(a.fuel);
}
exported func main() int {
  ship = Spaceship(42);
  ret = (&ship).getFuel();
  [_] = ^ship;
  return ret;
}
"#,
    42,
  );
}
