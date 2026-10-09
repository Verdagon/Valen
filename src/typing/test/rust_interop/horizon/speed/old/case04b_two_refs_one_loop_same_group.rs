use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case04b_two_refs_one_loop_same_group() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
func bump_both<g'>(a &Ship in g, b &Ship in g) {
  i = 0;
  while i < 50 {
    set a.fuel = __copy_prim(a.fuel) + 1;
    set b.fuel = __copy_prim(b.fuel) + 2;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  s = Ship(0);
  bump_both(&s, &s);
  return __copy_prim(s.fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
