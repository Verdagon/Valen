use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-04a-two-refs-one-loop-disjoint-groups",
  vale: r#"
import rust.mycrate.do_nothing;
struct Ship { fuel int; }
func bump_both<g', h'>(a &Ship in g, b &Ship in h) {
  i = 0;
  while i < 50 {
    set a.fuel = __copy_prim(a.fuel) + 1;
    set b.fuel = __copy_prim(b.fuel) + 2;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  s1 = Ship(0);
  s2 = Ship(0);
  bump_both(&s1, &s2);
  return __copy_prim(s1.fuel) + __copy_prim(s2.fuel);
}
"#,
  expect: Expect::Returns(150),
};

#[test]
fn case04a_two_refs_one_loop_disjoint_groups() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
