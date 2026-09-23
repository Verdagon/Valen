use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures_rust_trait",
  name: "speed-12-interface-member",
  vale: r#"
import rust.mycrate.Callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 2;
}
func poll(cb &Callback) int {
  total = 0;
  i = 0;
  while i < 100 {
    set total = total + cb.on_call();
    set i = i + 1;
  }
  return total;
}
exported func main() int {
  c = MyCb();
  return poll(&c);
}
"#,
  expect: Expect::Returns(200),
};

#[test]
#[ignore]
fn case12_interface_member() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(200),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
