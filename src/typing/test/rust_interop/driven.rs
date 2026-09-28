use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn a_driven_case_resolves_a_builtin_operator() {
  let run = drive_and_run("main", r#"
exported func main() int {
  return 3 + 4;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "did not exit 7 rustc_exit={} process_exit={:?} firings {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

#[test]
fn rustc_driven_bin_links_and_returns_from_add_i64() {
  let run = drive_and_run("main", r#"
import mycrate.add_i64;
exported func main() i64 {
  return add_i64(20i64, 22i64);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "did not exit 42 rustc_exit={} process_exit={:?} firings {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

#[test]
fn constructs_a_zst_imported_struct() {
  let run = drive_and_run("zst_return", r#"
import mycrate.Alpha;
exported func main() int {
  a = Alpha.new();
  return 7;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "did not exit 7 rustc_exit={} process_exit={:?} firings {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
