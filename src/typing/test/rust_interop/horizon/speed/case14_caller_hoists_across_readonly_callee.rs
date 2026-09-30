use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case14_caller_hoists_across_readonly_callee() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
struct Ship { fuel int; }
func peek(s &Ship) int {
  return __copy_prim(s.fuel);
}
func total<g'>(s &Ship in g) int {
  sum = 0;
  i = 0;
  while i < 100 {
    set sum = sum + __copy_prim(s.fuel) + peek(s);
    do_nothing();
    set i = i + 1;
  }
  return sum;
}
exported func main() int {
  s = Ship(1);
  return total(&s);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(200),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
