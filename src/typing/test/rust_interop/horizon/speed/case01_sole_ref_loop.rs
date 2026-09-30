use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case01_sole_ref_loop() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
func bump(s &Ship) {
  i = 0;
  while i < 100 {
    set s.fuel = __copy_prim(s.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  s = Ship(0);
  bump(&s);
  return __copy_prim(s.fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
