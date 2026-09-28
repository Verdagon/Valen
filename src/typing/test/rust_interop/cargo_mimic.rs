use crate::typing::rust_interop::deps_dir_of;
use std::path::Path;
use std::process::Command;

pub fn mimic_compile_dependency_rust_lib(
  temp_dir: &Path,
  valen_project_dir: &Path,
  rustc_location: &str,
  dependency_rust_crate_name: &str,
) {
  let deps_dir = deps_dir_of(valen_project_dir);
  let dependency_rust_lib_dir = temp_dir.join(dependency_rust_crate_name);
  let status = Command::new(rustc_location)
      .arg(dependency_rust_lib_dir.join("src").join("lib.rs"))
      .args(["--crate-type=lib", "--edition=2021"])
      .arg(format!("--crate-name={dependency_rust_crate_name}"))
      .arg(format!("-L{}", deps_dir.display()))
      .arg("--out-dir")
      .arg(&deps_dir)
      .status()
      .expect("couldn't run rustc to build the dependency rlib");
  assert!(status.success(), "Failed to compile dependency lib {}", dependency_rust_crate_name);
}
