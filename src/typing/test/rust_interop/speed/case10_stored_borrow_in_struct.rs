use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-10-stored-borrow-in-struct",
  vale: r#"
import rust.mycrate.do_nothing;
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
"#,
  expect: Expect::Returns(100),
};

#[test]
#[ignore]
fn case10_stored_borrow_in_struct() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
