use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case09_owned_heap_member() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
struct Fleet { flagship Ship; }
func bump(f &Fleet) {
  i = 0;
  while i < 100 {
    set f.flagship.fuel = __copy_prim(f.flagship.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  f = Fleet(Ship(0));
  bump(&f);
  return __copy_prim(f.flagship.fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
