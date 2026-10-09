use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
#[ignore]
fn case10_stored_borrow_in_struct() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
struct Holder<g'> { s &Ship in g; }
func bump<g'>(h &Holder<g>) {
  i = 0;
  while i < 100 {
    set h.s.fuel = __copy_prim(h.s.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  s = Ship(0);
  h = Holder(&s);
  bump(&h);
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
