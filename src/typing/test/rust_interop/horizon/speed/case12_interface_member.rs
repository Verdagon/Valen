use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
#[ignore]
fn case12_interface_member() {
  let run = drive_and_run_without_borrow_check("horizon/rust_trait", r#"
import mycrate.Callback;
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
"#);
  assert_eq!(
    run.process_exit,
    Some(200),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
