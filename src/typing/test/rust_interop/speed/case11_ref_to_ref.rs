use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-11-ref-to-ref",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(100),
};

#[test]
#[ignore]
fn case11_ref_to_ref() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
