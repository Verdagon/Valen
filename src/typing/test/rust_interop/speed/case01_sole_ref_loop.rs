use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-01-sole-ref-loop",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(100),
};

#[test]
fn case01_sole_ref_loop() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
