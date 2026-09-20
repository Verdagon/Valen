use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-14-caller-hoists-across-readonly-callee",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(200),
};

#[test]
fn case14_caller_hoists_across_readonly_callee() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(200),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
