
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::typing::rust_interop::bifrost::dir_structure::{
  copy_dir_recursive, incremental_dir_of, src_dir_of,
};

#[derive(Debug, Deserialize)]
pub struct Manifest {
  pub project: Project,
  #[serde(default, rename = "rust-dependencies")]
  pub rust_dependencies: BTreeMap<String, DependencySource>,
  #[serde(default, rename = "valen-dependencies")]
  pub valen_dependencies: BTreeMap<String, DependencySource>,
  #[serde(default, rename = "bin")]
  pub bins: Vec<BinTarget>,
}

#[derive(Debug, Deserialize)]
pub struct Project {
  pub name: String,
  pub version: String,
  pub edition: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DependencySource {
  Version(String),
  Path { path: String },
}

#[derive(Debug, Deserialize)]
pub struct BinTarget {
  pub name: String,
  pub source: String,
}

fn generate_toolchain_toml() -> String {
  "[toolchain]\nchannel = \"rustc-fork\"\n".to_string()
}

fn generate_cargo_toml(manifest: &Manifest) -> String {
  let mut cargo_toml_contents = String::new();
  cargo_toml_contents.push_str("[package]\n");
  cargo_toml_contents.push_str(&format!("name = \"{}\"\n", manifest.project.name));
  cargo_toml_contents.push_str(&format!("version = \"{}\"\n", manifest.project.version));
  cargo_toml_contents.push_str("edition = \"2021\"\n\n");
  cargo_toml_contents.push_str("[workspace]\n\n");
  cargo_toml_contents.push_str("[dependencies]\n");
  for (name, spec) in &manifest.rust_dependencies {
    match spec {
      DependencySource::Version(version) => {
        cargo_toml_contents.push_str(&format!("{name} = \"{version}\"\n"))
      }
      DependencySource::Path { path } => {
        cargo_toml_contents.push_str(&format!("{name} = {{ path = \"{path}\" }}\n"))
      }
    }
  }
  for bin in &manifest.bins {
    cargo_toml_contents.push_str("\n[[bin]]\n");
    cargo_toml_contents.push_str(&format!("name = \"{}\"\n", bin.name));
    cargo_toml_contents.push_str(&format!("path = \"{}\"\n", bin.source));
  }
  cargo_toml_contents
}

pub struct BuildInputs {
  pub manifest_path: PathBuf,
  pub build_dir: PathBuf,
  pub valenc_rs: PathBuf,
  pub clear_incremental: bool,
  pub borrow_check: bool,
}

pub fn stage_workspace(inputs: &BuildInputs) -> Result<(), String> {
  let manifest_text = fs::read_to_string(&inputs.manifest_path)
    .map_err(|e| format!("could not read {}: {e}", inputs.manifest_path.display()))?;
  let manifest: Manifest = toml::from_str(&manifest_text)
    .map_err(|e| format!("could not parse {}: {e}", inputs.manifest_path.display()))?;

  fs::create_dir_all(&inputs.build_dir)
    .map_err(|e| format!("could not create {}: {e}", inputs.build_dir.display()))?;
  let cargo_toml_path = inputs.build_dir.join("Cargo.toml");
  fs::write(&cargo_toml_path, generate_cargo_toml(&manifest))
    .map_err(|e| format!("could not write {}: {e}", cargo_toml_path.display()))?;
  if bundle_bin_dir(&inputs.valenc_rs)?.is_none() {
    let toolchain_toml_path = inputs.build_dir.join("rust-toolchain.toml");
    fs::write(&toolchain_toml_path, generate_toolchain_toml())
      .map_err(|e| format!("could not write {}: {e}", toolchain_toml_path.display()))?;
  }

  let project_dir = inputs
    .manifest_path
    .parent()
    .ok_or_else(|| format!("{} has no parent dir", inputs.manifest_path.display()))?;
  copy_dir_recursive(&src_dir_of(project_dir), &src_dir_of(&inputs.build_dir))
    .map_err(|e| format!("could not copy the project src/ into the build dir: {e}"))?;
  Ok(())
}

pub fn run_build(inputs: &BuildInputs) -> Result<i32, String> {
  stage_workspace(inputs)?;
  if inputs.clear_incremental {
    let incremental = incremental_dir_of(&inputs.build_dir);
    if incremental.exists() {
      fs::remove_dir_all(&incremental).map_err(|e| {
        format!("could not clear the incremental cache at {}: {e}", incremental.display())
      })?;
    }
  }
  let (fork_rustc, cargo_cmd, bundle) =
      match bundle_bin_dir(&inputs.valenc_rs)? {
        Some(bin_dir) => {
          let rustc = bin_dir.join("rustc");
          let cargo = bin_dir.join("cargo");
          if !rustc.is_file() {
            return Err(format!("valen bundle at {} is missing its rustc", bin_dir.display()));
          }
          if !cargo.is_file() {
            return Err(format!("valen bundle at {} is missing its cargo", bin_dir.display()));
          }
          (rustc, cargo, true)
        }
        None => (fork_rustc_path()?, PathBuf::from("cargo"), false),
      };
  let mut command = Command::new(&cargo_cmd);
  command
    .current_dir(&inputs.build_dir)
    .arg("build")
    .env("RUSTC_WORKSPACE_WRAPPER", &inputs.valenc_rs)
    .env("COMPANION_RUSTC", &fork_rustc)
    .env("VALEN_BORROW_CHECK", if inputs.borrow_check { "on" } else { "off" });
  if bundle {
    command.env("RUSTC", &fork_rustc);
  } else {
    command.env_remove("RUSTC");
  }
  let status = command.status().map_err(|e| format!("could not spawn cargo: {e}"))?;
  match status.code() {
    Some(code) => Ok(code),
    None => Err(format!("cargo was terminated by signal {:?}", status.signal())),
  }
}

fn fork_rustc_path() -> Result<PathBuf, String> {
  let output = Command::new("rustup")
    .args(["which", "--toolchain", "rustc-fork", "rustc"])
    .output()
    .map_err(|e| format!("could not run rustup to find the rustc-fork toolchain: {e}"))?;
  if !output.status.success() {
    return Err(format!(
      "rustup could not find the rustc-fork toolchain: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    ));
  }
  let path = String::from_utf8(output.stdout)
    .map_err(|e| format!("rustup printed a non-utf8 rustc path: {e}"))?;
  Ok(PathBuf::from(path.trim()))
}

fn bundle_bin_dir(valenc_rs: &Path) -> Result<Option<PathBuf>, String> {
  let bin_dir = valenc_rs
    .parent()
    .ok_or_else(|| format!("could not determine the directory containing {}", valenc_rs.display()))?;
  if bin_dir.join("valen-bundle.marker").is_file() {
    Ok(Some(bin_dir.to_path_buf()))
  } else {
    Ok(None)
  }
}
