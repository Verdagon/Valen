use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case03_same_group_straight_line() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
func do_things<g'>(a &Ship in g, b &Ship in g) int {
  set b.fuel = 1;
  set a.fuel = 2;
  do_nothing();
  set a.fuel = 3;
  x = __copy_prim(a.fuel);
  do_nothing();
  y = __copy_prim(a.fuel);
  return x + y + __copy_prim(b.fuel);
}
exported func main() int {
  s1 = Ship(0);
  s2 = Ship(0);
  return do_things(&s1, &s2);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
