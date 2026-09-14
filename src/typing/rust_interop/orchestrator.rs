// The `valen` orchestrator — the cargo-like front for interop builds. `valen build` parses a
// `Valen.toml`, generates a Cargo workspace, and runs `RUSTC_WORKSPACE_WRAPPER=valenc-rs cargo
// +rustc-fork build` over it (design §257-303). cargo then invokes `valenc-rs` once per crate; a
// `.valen` crate drives Valen, a pure-Rust dependency passes through to plain rustc.
//
// `generate_workspace` is the pure dark box (@DBAPIZ): a parsed manifest in, the workspace file set out,
// no cargo and no filesystem. `run_build` (a later slice) is the thin effectful shell — it writes those
// files, copies the project `src/`, and spawns cargo.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// A parsed `Valen.toml`.
#[derive(Debug, Deserialize)]
pub struct Manifest {
  pub project: Project,
  #[serde(default, rename = "rust-dependencies")]
  pub rust_dependencies: BTreeMap<String, DepSpec>,
  #[serde(default, rename = "valen-dependencies")]
  pub valen_dependencies: BTreeMap<String, DepSpec>,
  #[serde(default, rename = "bin")]
  pub bins: Vec<BinTarget>,
}

#[derive(Debug, Deserialize)]
pub struct Project {
  pub name: String,
  pub version: String,
  pub edition: String,
}

/// A dependency's spec: either a crates.io version string or a local `{ path = ... }`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DepSpec {
  Version(String),
  Path { path: String },
}

#[derive(Debug, Deserialize)]
pub struct BinTarget {
  pub name: String,
  /// The crate root — a `.valen` file, relative to the project (e.g. `src/main.valen`).
  pub source: String,
}

/// The cargo files `generate_workspace` produces, as (path relative to the build dir, contents).
/// `run_build` writes these, then copies the project `src/` and runs cargo.
pub struct GeneratedWorkspace {
  pub files: Vec<(PathBuf, String)>,
}

/// Turn a parsed `Valen.toml` into the cargo files that build it — pure, no cargo and no filesystem.
///
/// Interim shape: a single flat package (not the design's multi-project workspace), enough for
/// NobiliaV's one-binary case. cargo's `[[bin]] path` points straight at the `.valen`: cargo accepts a
/// non-`.rs` target path, and `valenc-rs` turns it into the compiled stub. Rust deps render verbatim (a
/// path dep carries an absolute path); adjusting paths to a generated layout and the
/// `valen-dependencies` are the permanent form's job.
pub fn generate_workspace(manifest: &Manifest) -> GeneratedWorkspace {
  let mut cargo = String::new();
  cargo.push_str("[package]\n");
  cargo.push_str(&format!("name = \"{}\"\n", manifest.project.name));
  cargo.push_str(&format!("version = \"{}\"\n", manifest.project.version));
  cargo.push_str("edition = \"2021\"\n\n");
  // Declare our own workspace so cargo never tries to fold the generated package into a parent
  // workspace it happens to be nested under. The build dir (`<project>/target/valen-build`) is often
  // inside the user's real cargo workspace — e.g. a `Valen.toml` beside a crate in a `members` glob —
  // and without this cargo errors that the package "believes it's in a workspace when it's not".
  cargo.push_str("[workspace]\n\n");
  cargo.push_str("[dependencies]\n");
  for (name, spec) in &manifest.rust_dependencies {
    cargo.push_str(&render_dep(name, spec));
  }
  for bin in &manifest.bins {
    cargo.push_str("\n[[bin]]\n");
    cargo.push_str(&format!("name = \"{}\"\n", bin.name));
    cargo.push_str(&format!("path = \"{}\"\n", bin.source));
  }

  let toolchain = "[toolchain]\nchannel = \"rustc-fork\"\n".to_string();

  GeneratedWorkspace {
    files: vec![
      (PathBuf::from("Cargo.toml"), cargo),
      (PathBuf::from("rust-toolchain.toml"), toolchain),
    ],
  }
}

/// Render one `[dependencies]` line: a crates.io version, or a local `{ path = ... }`.
fn render_dep(name: &str, spec: &DepSpec) -> String {
  match spec {
    DepSpec::Version(version) => format!("{name} = \"{version}\"\n"),
    DepSpec::Path { path } => format!("{name} = {{ path = \"{path}\" }}\n"),
  }
}

/// Everything `run_build` needs, gathered by the `valen` bin's `main()` above the boundary (@DBAPIZ).
pub struct BuildInputs {
  /// The project's `Valen.toml`.
  pub manifest_path: PathBuf,
  /// Where to generate the cargo workspace (e.g. `<project>/target/valen-build`).
  pub build_dir: PathBuf,
  /// The `valenc-rs` wrapper binary cargo uses as `RUSTC_WORKSPACE_WRAPPER`.
  pub valenc_rs: PathBuf,
  /// Wipe the driven crate's incremental cache before building. See `run_build` for why this exists:
  /// with it unset, a warm rebuild links an *empty* `vale_cgu` (undefined `__vale_main`) until Parts
  /// A+B of the incremental fix land. Tests drive both values directly; the `valen` bin flips this to
  /// `false` once the fix is in, so real rebuilds reuse rustc's incremental cache.
  pub clear_incremental: bool,
  /// Whether the group borrow checker runs on the driven Valen crates. Handed to `valenc-rs` as
  /// `VALEN_BORROW_CHECK=1|0` on the cargo spawn, set both ways explicitly so a value leaked into the
  /// user's shell cannot override it. `valen build --no-borrow-check` passes `false`, so a user can
  /// see whether interop alone builds and runs a program the checker does not yet accept.
  pub borrow_check: bool,
}

/// Read the `Valen.toml`, generate the cargo workspace, write it under `build_dir`, and copy the
/// project's `src/` in. This is the effectful half of a build up to — but not including — the cargo
/// spawn, so a test can assert the staged tree without needing the `valenc-rs` binary or a toolchain.
pub fn stage_workspace(inputs: &BuildInputs) -> Result<(), String> {
  let manifest_text = fs::read_to_string(&inputs.manifest_path)
    .map_err(|e| format!("could not read {}: {e}", inputs.manifest_path.display()))?;
  let manifest: Manifest = toml::from_str(&manifest_text)
    .map_err(|e| format!("could not parse {}: {e}", inputs.manifest_path.display()))?;

  let workspace = generate_workspace(&manifest);
  for (rel, contents) in &workspace.files {
    let dest = inputs.build_dir.join(rel);
    if let Some(parent) = dest.parent() {
      fs::create_dir_all(parent)
        .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    fs::write(&dest, contents).map_err(|e| format!("could not write {}: {e}", dest.display()))?;
  }

  // Copy the project's `src/` into the generated package so cargo's `[[bin]] path = "src/….valen"`
  // resolves; the `.valen` is the crate root, which `valenc-rs` turns into the compiled stub.
  let project_dir = inputs
    .manifest_path
    .parent()
    .ok_or_else(|| format!("{} has no parent dir", inputs.manifest_path.display()))?;
  copy_dir_recursive(&project_dir.join("src"), &inputs.build_dir.join("src"))
    .map_err(|e| format!("could not copy the project src/ into the build dir: {e}"))?;
  Ok(())
}

/// Stage the workspace, then run `cargo build` over it with `valenc-rs` installed as
/// `RUSTC_WORKSPACE_WRAPPER`, returning cargo's exit code. The generated `rust-toolchain.toml` selects
/// the fork, and `valenc-rs`'s baked rpath finds `librustc_driver`, so no toolchain/dylib env is set
/// here; a leaked `RUSTC` is cleared so it cannot override the wrapper. The one env this does set is
/// `VALEN_BORROW_CHECK` (`inputs.borrow_check` as `1`/`0`, explicitly both ways), which `valenc-rs`'s
/// `main()` reads. cargo invokes `valenc-rs` once per crate — a `.valen` crate drives Valen, a
/// pure-Rust dependency passes through.
///
/// When `inputs.clear_incremental` is set, the driven crate's incremental cache is wiped first — the
/// interim workaround for the warm-rebuild bug. Valen re-emits its module (`vale_cgu`, carrying the
/// entry `__vale_main` + each callback wrapper) on *every* build via `fill_extra_modules`, but that
/// emit's inputs are populated only as a *side effect* of the `per_instance_mir` query writing
/// `DriverState`. rustc reaches `per_instance_mir` only from `collect_items_of_instance` under the
/// `items_of_instance` query (`cache_on_disk_if{true}`), so a warm build serves that from disk, never
/// calls `per_instance_mir`, and the emit comes out fresh but *empty* → undefined `__vale_main` at
/// link. It is **not** a stale object. Clearing forces a fresh full collect so `per_instance_mir`
/// fires. Disabling incremental entirely (`CARGO_INCREMENTAL=0`) is *not* a substitute: its merged-CGU
/// layout DCEs the reified Rust leaves the Valen body calls (e.g. `main_loop::<MyCb>`) — a different
/// failure. Dep rlibs live under `deps/` and are fingerprinted separately, so they skip either way —
/// only the driven bin recompiles (~2s; `stage_workspace` re-copies the source, bumping its mtime).
/// The real no-fork fix (re-fire `per_instance_mir` from the `eval_always` partition override + bake a
/// `.valen`-source digest into the stub) lets callers pass `clear_incremental = false` for fast,
/// correct rebuilds.
pub fn run_build(inputs: &BuildInputs) -> Result<i32, String> {
  stage_workspace(inputs)?;
  if inputs.clear_incremental {
    let incremental = inputs.build_dir.join("target").join("debug").join("incremental");
    if incremental.exists() {
      fs::remove_dir_all(&incremental).map_err(|e| {
        format!("could not clear the incremental cache at {}: {e}", incremental.display())
      })?;
    }
  }
  let status = Command::new("cargo")
    .current_dir(&inputs.build_dir)
    .arg("build")
    .env("RUSTC_WORKSPACE_WRAPPER", &inputs.valenc_rs)
    .env("VALEN_BORROW_CHECK", if inputs.borrow_check { "1" } else { "0" })
    .env_remove("RUSTC")
    .status()
    .map_err(|e| format!("could not spawn cargo: {e}"))?;
  Ok(status.code().unwrap_or(1))
}

/// Recursively copy `from` into `to` (files and subdirs), used to plant the project `src/` in the
/// generated package.
fn copy_dir_recursive(from: &Path, to: &Path) -> io::Result<()> {
  fs::create_dir_all(to)?;
  for entry in fs::read_dir(from)? {
    let entry = entry?;
    let source = entry.path();
    let dest = to.join(entry.file_name());
    if entry.file_type()?.is_dir() {
      copy_dir_recursive(&source, &dest)?;
    } else {
      fs::copy(&source, &dest)?;
    }
  }
  Ok(())
}
