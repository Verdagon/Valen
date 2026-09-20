use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-09-owned-heap-member",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(100),
};

#[test]
fn case09_owned_heap_member() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
