use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-04b-two-refs-one-loop-same-group",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(150),
};

#[test]
fn case04b_two_refs_one_loop_same_group() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
