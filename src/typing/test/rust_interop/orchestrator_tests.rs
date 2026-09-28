use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use crate::typing::rust_interop::{stage_workspace, BuildInputs};

#[test]
fn stage_workspace_writes_the_cargo_workspace() {
  const VALEN_SOURCE: &str = "import tiny.seven; exported func main() int { return seven(); }";

  let root = TempDir::new().expect("could not create root dir");
  let project = root.path().join("prog");
  fs::create_dir_all(project.join("src")).expect("mkdir project/src");
  fs::write(
    project.join("Valen.toml"),
    "[project]\nname = \"prog\"\nversion = \"0.1.0\"\nedition = \"experimental\"\n\n[rust-dependencies]\ntiny = { path = \"/abs/tiny\" }\n\n[[bin]]\nname = \"prog\"\nsource = \"src/main.valen\"\n",
  )
  .expect("write Valen.toml");
  fs::write(project.join("src/main.valen"), VALEN_SOURCE).expect("write src/main.valen");

  let build_dir = root.path().join("build");
  stage_workspace(&BuildInputs {
    manifest_path: project.join("Valen.toml"),
    build_dir: build_dir.clone(),
    valenc_rs: PathBuf::from("/unused/valenc-rs"),
    clear_incremental: false,
    borrow_check: true,
  })
  .expect("stage_workspace should succeed");

  let cargo = fs::read_to_string(build_dir.join("Cargo.toml")).expect("Cargo.toml written");
  assert!(cargo.contains("name = \"prog\""), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("version = \"0.1.0\""), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("edition = \"2021\""), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("[workspace]"), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("tiny = { path = \"/abs/tiny\" }"), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("[[bin]]"), "Cargo.toml:\n{cargo}");
  assert!(cargo.contains("path = \"src/main.valen\""), "Cargo.toml:\n{cargo}");

  let toolchain =
    fs::read_to_string(build_dir.join("rust-toolchain.toml")).expect("toolchain written");
  assert!(toolchain.contains("channel = \"rustc-fork\""), "toolchain:\n{toolchain}");

  let copied = fs::read_to_string(build_dir.join("src/main.valen")).expect("src copied");
  assert_eq!(copied, VALEN_SOURCE);
}
