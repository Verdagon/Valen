// The binary-running tests that only horizon passes.

mod pipeline_e2e;

use std::fs::{create_dir_all, write};
use std::process::Command;

use frontend_rust::typing::rust_interop::valen_build_dir_of;
use tempfile::TempDir;

// The README's "Running a Valen Program with Rust Libraries" example, verbatim, against the real
// `chrono` from crates.io, so running it needs network access. `chrono.TimeDelta` is re-exported at
// the crate root from `chrono::time_delta`, which is what horizon's oracle follows and bifrost's
// doesn't yet.
#[test]
fn test_basic_chrono_rust_interop() {
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  let project_dir = temp_dir.path().join("test_project");
  create_dir_all(project_dir.join("src")).expect("could not create the project src dir");
  write(
    project_dir.join("Valen.toml"),
    r#"[project]
name = "test_project"
version = "0.1.0"
edition = "2021"

[rust-dependencies]
chrono = "0.4"

[[bin]]
name = "main"
source = "src/main.valen"
"#,
  )
  .expect("could not write Valen.toml");
  write(
    project_dir.join("src").join("main.valen"),
    r#"
import chrono.TimeDelta;

exported func main() i64 {
  d = TimeDelta.seconds(42i64);
  return d.num_seconds();
}
"#,
  )
  .expect("could not write main.valen");

  let build_output = Command::new(env!("CARGO_BIN_EXE_valen"))
      .arg("build")
      .arg("--manifest-path")
      .arg(project_dir.join("Valen.toml"))
      .output()
      .expect("could not run valen");
  assert!(
    build_output.status.success(),
    "valen build failed with {:?}\nstdout:\n{}\nstderr:\n{}",
    build_output.status.code(),
    String::from_utf8_lossy(&build_output.stdout),
    String::from_utf8_lossy(&build_output.stderr));

  let exe = valen_build_dir_of(&project_dir).join("target").join("debug").join("main");
  let run_output = Command::new(&exe).output().expect("could not run the built binary");
  assert_eq!(run_output.status.code(), Some(42), "TimeDelta.seconds(42).num_seconds() is 42");
}
