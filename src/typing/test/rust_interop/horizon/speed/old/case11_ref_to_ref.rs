use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
#[ignore]
fn case11_ref_to_ref() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
func bump<g'>(pp &&Ship in g) {
  i = 0;
  while i < 100 {
    set pp.fuel = __copy_prim(pp.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  s = Ship(0);
  p = &s;
  bump(&p);
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
