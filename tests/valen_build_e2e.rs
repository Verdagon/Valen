// These test invoking the whole orchestrator from the outside as a subprocess, as opposed to the
// rest of the tests which invoke things through the dark box API.

#![cfg(feature = "rust_interop")]
#![feature(rustc_private)]

use std::fs::{create_dir_all, write};
use std::path::{Path, PathBuf};
use std::process::Command;

use frontend_rust::typing::rust_interop::copy_dir_recursive;
use frontend_rust::typing::rust_interop::valen_build_dir_of;
use tempfile::TempDir;

fn stage_dependency_crate(project_dir: &Path, crate_name: &str) {
  let fixture_dir =
      PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/typing/test/bifrost/fixtures").join(crate_name);
  let crate_dir = project_dir.join(crate_name);
  copy_dir_recursive(&fixture_dir, &crate_dir).expect("could not copy the dependency crate fixture");
  write(
    crate_dir.join("Cargo.toml"),
    format!(
      "[package]\nname = \"{crate_name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
       [lib]\ncrate-type = [\"rlib\"]\n"),
  )
  .expect("could not write the dependency crate's Cargo.toml");
}

fn stage_valen_project(project_dir: &Path, project_name: &str, dependency_crate_name: &str, main_valen: &str) {
  create_dir_all(project_dir.join("src")).expect("could not create the project src dir");
  write(
    project_dir.join("Valen.toml"),
    format!(
      "[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
       [rust-dependencies]\n{dependency_crate_name} = {{ path = \"../../{dependency_crate_name}\" }}\n\n\
       [[bin]]\nname = \"{project_name}\"\nsource = \"src/main.valen\"\n"),
  )
  .expect("could not write Valen.toml");
  write(project_dir.join("src").join("main.valen"), main_valen).expect("could not write main.valen");
}

#[test]
fn valen_build_runs_the_slot_alias_program_to_73() {
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  let project_dir = temp_dir.path().join("slot_project");
  stage_dependency_crate(&project_dir, "simple_dep_rust_lib");
  stage_valen_project(
    &project_dir,
    "slot_project",
    "simple_dep_rust_lib",
    r#"
import simple_dep_rust_lib.Slot;
exported func main() i64 {
  slot = Slot.new();
  ref_a = &slot;
  ref_b = &slot;
  ref_a.mutate(42i64);
  ref_b.mutate(73i64);
  return ref_a.get();
}
"#);

  let build_output = Command::new(env!("CARGO_BIN_EXE_valen"))
      .arg("build")
      .arg("--manifest-path")
      .arg(project_dir.join("Valen.toml"))
      .arg("--no-borrow-check")
      .output()
      .expect("could not run valen");
  assert!(
    build_output.status.success(),
    "valen build failed with {:?}\nstdout:\n{}\nstderr:\n{}",
    build_output.status.code(),
    String::from_utf8_lossy(&build_output.stdout),
    String::from_utf8_lossy(&build_output.stderr));

  let exe = valen_build_dir_of(&project_dir).join("target").join("debug").join("slot_project");
  let run_output = Command::new(&exe).output().expect("could not run the built binary");
  assert_eq!(
    run_output.status.code(),
    Some(73),
    "both aliases mutate one Slot, so the second mutate (73) is what get() reads back");
}

#[test]
fn valen_build_runs_a_rust_free_function_call_to_7() {
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  let project_dir = temp_dir.path().join("free_fn_project");
  stage_dependency_crate(&project_dir, "simple_dep_rust_lib");
  stage_valen_project(
    &project_dir,
    "free_fn_project",
    "simple_dep_rust_lib",
    r#"
import simple_dep_rust_lib.add_i64;
exported func main() i64 {
  return add_i64(3i64, 4i64);
}
"#);

  let build_output = Command::new(env!("CARGO_BIN_EXE_valen"))
      .arg("build")
      .arg("--manifest-path")
      .arg(project_dir.join("Valen.toml"))
      .arg("--no-borrow-check")
      .output()
      .expect("could not run valen");
  assert!(
    build_output.status.success(),
    "valen build failed with {:?}\nstdout:\n{}\nstderr:\n{}",
    build_output.status.code(),
    String::from_utf8_lossy(&build_output.stdout),
    String::from_utf8_lossy(&build_output.stderr));

  let exe = valen_build_dir_of(&project_dir).join("target").join("debug").join("free_fn_project");
  let run_output = Command::new(&exe).output().expect("could not run the built binary");
  assert_eq!(run_output.status.code(), Some(7), "add_i64(3, 4) is 7");
}

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
      .arg("--no-borrow-check")
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
