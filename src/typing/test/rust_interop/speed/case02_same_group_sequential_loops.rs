use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-02-same-group-sequential-loops",
  vale: r#"
import rust.mycrate.do_nothing;
struct Ship { fuel int; }
func bump_both<g'>(a &Ship in g, b &Ship in g) {
  i = 0;
  while i < 100 {
    set a.fuel = __copy_prim(a.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
  j = 0;
  while j < 100 {
    set b.fuel = __copy_prim(b.fuel) + 1;
    do_nothing();
    set j = j + 1;
  }
}
exported func main() int {
  s1 = Ship(0);
  s2 = Ship(0);
  bump_both(&s1, &s2);
  return __copy_prim(s1.fuel) + __copy_prim(s2.fuel);
}
"#,
  expect: Expect::Returns(200),
};

#[test]
fn case02_same_group_sequential_loops() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(200),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
