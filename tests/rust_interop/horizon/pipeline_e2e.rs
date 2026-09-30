// End-to-end tests for the real `valen build` pipeline: each runs the built `valen` binary — real cargo
// -> the real `valenc-rs` wrapper -> generated workspace -> link — over a project staged in a
// `tempfile::TempDir`, then runs the produced binary. Unlike the typecheck tests (in-process rustc) and
// the drive tests (in-process `drive`), these exercise the actual orchestrator, cargo's process boundary, and
// rustc's incremental cache across rebuilds. Cargo builds `valen` and `valenc-rs` before these run.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use frontend_rust::typing::rust_interop::{stage_workspace, valen_build_dir_of, BuildInputs};
use tempfile::TempDir;

/// The outcome of a real `valen build`: cargo's exit code, the produced binary's path, and what the
/// build printed (for failure messages).
struct BuildRun {
  build_rc: i32,
  exe: PathBuf,
  output: String,
}

/// Run the real `valen build` over a staged project directory, with or without the borrow checker.
/// `<project>/Valen.toml` is the manifest, the build dir is `<project>/target/valen-build`, and the
/// produced binary lands at `<build_dir>/target/debug/<bin_name>`. `valen` never clears rustc's
/// incremental cache, so a project's first build is cold and every later one is warm.
fn valen_build(project: &Path, bin_name: &str, borrow_check: bool) -> BuildRun {
  let mut command = Command::new(env!("CARGO_BIN_EXE_valen"));
  command.arg("build").arg("--manifest-path").arg(project.join("Valen.toml"));
  if !borrow_check {
    command.arg("--no-borrow-check");
  }
  let out = command.output().expect("could not run valen");
  let output = format!(
    "stdout:\n{}\nstderr:\n{}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );
  let exe = valen_build_dir_of(project).join("target").join("debug").join(bin_name);
  BuildRun { build_rc: out.status.code().unwrap_or(-1), exe, output }
}

/// The driven crate's incremental cache directory, two levels under the build dir.
fn incremental_dir(project: &Path) -> PathBuf {
  valen_build_dir_of(project).join("target").join("debug").join("incremental")
}

/// Every object work-product (`*.o`) under `dir`, with its mtime — the artifacts rustc copies from
/// cache (rather than re-codegening) on a warm rebuild. Recursive; empty vec if `dir` is absent.
fn object_work_products(dir: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
  let mut found = Vec::new();
  collect_object_work_products(dir, &mut found);
  found.sort_by(|a, b| a.0.cmp(&b.0));
  found
}

fn collect_object_work_products(dir: &Path, out: &mut Vec<(PathBuf, std::time::SystemTime)>) {
  let entries = match fs::read_dir(dir) {
    Ok(e) => e,
    Err(_) => return,
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
      collect_object_work_products(&path, out);
    } else if path.extension().map(|e| e == "o").unwrap_or(false) {
      let mtime = fs::metadata(&path).and_then(|m| m.modified()).unwrap();
      out.push((path, mtime));
    }
  }
}

/// Run a produced binary and return its exit code.
fn run_binary(exe: &Path) -> i32 {
  let out =
    Command::new(exe).output().unwrap_or_else(|e| panic!("could not run {}: {e}", exe.display()));
  out.status.code().unwrap_or(-1)
}

fn write_rust_dep_crate(project: &Path, crate_name: &str, lib_rs: &str) {
  let krate = project.join(crate_name);
  fs::create_dir_all(krate.join("src")).unwrap();
  fs::write(
    krate.join("Cargo.toml"),
    format!(
      "[package]\nname = \"{crate_name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
       [lib]\ncrate-type = [\"rlib\"]\n"
    ),
  )
  .unwrap();
  fs::write(krate.join("src/lib.rs"), lib_rs).unwrap();
}

fn write_valen_project(project: &Path, project_name: &str, dep_crate: &str, main_valen: &str) {
  fs::create_dir_all(project.join("src")).unwrap();
  fs::write(
    project.join("Valen.toml"),
    format!(
      "[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
       [rust-dependencies]\n{dep_crate} = {{ path = \"../../{dep_crate}\" }}\n\n\
       [[bin]]\nname = \"main\"\nsource = \"src/main.valen\"\n"
    ),
  )
  .unwrap();
  fs::write(project.join("src/main.valen"), main_valen).unwrap();
}

/// Cold-build and run the binary, apply `edit` to mutate a source file, then warm-rebuild (reusing
/// rustc's incremental cache) and run again — returning `(cold_exit, warm_exit)`. The produced binary
/// path is reused across builds, so the cold run is captured *before* the warm build overwrites it.
/// Asserts both builds succeed (rc 0).
fn cold_edit_warm_exits(
  project: &Path,
  bin_name: &str,
  borrow_check: bool,
  edit: impl FnOnce(&Path),
) -> (i32, i32) {
  let cold = valen_build(project, bin_name, borrow_check);
  assert_eq!(cold.build_rc, 0, "cold build should succeed\n{}", cold.output);
  let cold_exit = run_binary(&cold.exe);
  edit(project);
  let warm = valen_build(project, bin_name, borrow_check);
  assert_eq!(warm.build_rc, 0, "warm rebuild after the edit should link\n{}", warm.output);
  let warm_exit = run_binary(&warm.exe);
  (cold_exit, warm_exit)
}

// The borrow checker is on by default through the real pipeline: a Vale function churning a
// parameter's group through an imported `&mut self` method without declaring `mut(g)` fails the
// build. Paired with the test below, this proves the switch crosses the cargo process boundary,
// which an in-process `drive` test cannot see.
//
// Ignored until symphony can reject it: symphony accepts the program, since it has no check for an
// undeclared churn of a parameter's group (the old suite got this rejection from the experimental
// checker). Its in-process twin is `drive_rejects_an_undeclared_churn_of_an_imported_mut_method`.
#[test]
#[ignore]
fn valen_build_fails_an_undeclared_churn_with_the_borrow_checker_on() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "counter", r#"
pub struct Counter { pub n: i32 }
impl Counter {
    pub fn new() -> Counter { Counter { n: 6 } }
    pub fn bump(&mut self) { self.n += 1; }
    pub fn get(&self) -> i32 { self.n }
}
"#);
  write_valen_project(project.path(), "churner", "counter", r#"
import counter.Counter;
func bump_it<g'>(c &Counter in g) {
  c.bump();
}
exported func main() int {
  c = Counter.new();
  bump_it(&c);
  return c.get();
}
"#);
  let run = valen_build(project.path(), "main", /*borrow_check=*/ true);
  assert_ne!(run.build_rc, 0, "the borrow checker should fail the build\n{}", run.output);
  assert!(run.output.contains("BorrowCheckError"), "the build should fail in the borrow checker\n{}", run.output);
}

// `valen build --no-borrow-check`: the same program builds, links and runs → 7, so rust interop can
// be exercised end to end on a program the borrow checker is not yet happy with.
#[test]
fn valen_build_without_the_borrow_checker_runs_an_undeclared_churn_to_seven() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "counter", r#"
pub struct Counter { pub n: i32 }
impl Counter {
    pub fn new() -> Counter { Counter { n: 6 } }
    pub fn bump(&mut self) { self.n += 1; }
    pub fn get(&self) -> i32 { self.n }
}
"#);
  write_valen_project(project.path(), "churner", "counter", r#"
import counter.Counter;
func bump_it<g'>(c &Counter in g) {
  c.bump();
}
exported func main() int {
  c = Counter.new();
  bump_it(&c);
  return c.get();
}
"#);
  let run = valen_build(project.path(), "main", /*borrow_check=*/ false);
  assert_eq!(run.build_rc, 0, "valen build --no-borrow-check should succeed\n{}", run.output);
  assert_eq!(run_binary(&run.exe), 7);
}

// A Vale binary that imports and calls a real Rust free function builds, links, and runs to its
// return value — through the actual `valen build` pipeline (cargo + the `valenc-rs` wrapper).
#[test]
fn builds_and_runs_a_binary_calling_a_rust_fn() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "tiny", r#"
pub fn seven() -> i32 { 7 }
"#);
  write_valen_project(project.path(), "tracer", "tiny", r#"
import tiny.seven;
exported func main() int { return seven(); }
"#);
  let run = valen_build(project.path(), "main", /*borrow_check=*/ true);
  assert_eq!(run.build_rc, 0, "valen build should succeed\n{}", run.output);
  assert_eq!(run_binary(&run.exe), 7);
}

// A warm rebuild — the same project built twice, reusing the incremental cache — still links, runs to
// the same value, and actually *reuses* the cache (the perf payoff). Without the partitioner re-firing
// `per_instance_mir` for cached items, the warm build emits an empty `vale_cgu` and the link fails with
// an undefined `__vale_main`.
#[test]
fn warm_rebuild_no_edit_runs_seven_and_reuses_cache() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "tiny", r#"
pub fn seven() -> i32 { 7 }
"#);
  write_valen_project(project.path(), "tracer", "tiny", r#"
import tiny.seven;
exported func main() int { return seven(); }
"#);

  // Cold build — establishes the incremental cache.
  let cold = valen_build(project.path(), "main", /*borrow_check=*/ true);
  assert_eq!(cold.build_rc, 0, "cold valen build should succeed\n{}", cold.output);
  assert_eq!(run_binary(&cold.exe), 7);

  let incr = incremental_dir(project.path());
  let before = object_work_products(&incr);
  assert!(!before.is_empty(), "cold build should leave object work-products in the incremental cache");

  // Warm rebuild — the bug's trigger. (`stage_workspace` re-copies the source each build, bumping its
  // mtime, so cargo re-invokes rustc over the reused incremental cache.)
  let warm = valen_build(project.path(), "main", /*borrow_check=*/ true);
  assert_eq!(warm.build_rc, 0, "warm rebuild should link (the partitioner re-fires per_instance_mir)\n{}", warm.output);
  assert_eq!(run_binary(&warm.exe), 7, "warm rebuild should run to the same value");

  // Reuse, not regeneration: every object work-product the cold build wrote is still present with an
  // unchanged mtime — rustc copied it from cache rather than re-codegening it.
  assert!(incr.exists(), "the incremental cache should persist across a warm rebuild");
  for (path, mtime) in &before {
    let now = fs::metadata(path)
      .and_then(|m| m.modified())
      .unwrap_or_else(|e| panic!("work-product {} vanished after warm rebuild: {e}", path.display()));
    assert_eq!(&now, mtime, "work-product {} was regenerated, not reused", path.display());
  }
}

// A warm rebuild after editing the Vale body to call a *different already-imported* Rust function
// must run the new value. The imports are unchanged, so the importer file is byte-identical — rustc's
// fingerprint doesn't move on its own, and without the source digest it reuses a stale mono partition
// that never codegens the swapped-in leaf (undefined symbol at link, or a stale 7).
#[test]
fn warm_rebuild_after_forward_call_swap() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "tiny", r#"
pub fn seven() -> i32 { 7 }
pub fn eleven() -> i32 { 11 }
"#);
  write_valen_project(project.path(), "swaptracer", "tiny", r#"
import tiny.seven;
import tiny.eleven;
exported func main() int { return seven(); }
"#);
  let (cold_exit, warm_exit) =
    cold_edit_warm_exits(project.path(), "main", /*borrow_check=*/ true, |p| {
      fs::write(p.join("src/main.valen"), r#"
import tiny.seven;
import tiny.eleven;
exported func main() int { return eleven(); }
"#).unwrap();
    });
  assert_eq!(cold_exit, 7, "cold build calls seven");
  assert_eq!(warm_exit, 11, "warm rebuild runs the swapped-in eleven (the digest re-collected it)");
}

/// The checked-in `driver-check` project beside this file.
fn driver_check_template() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rust_interop/horizon/driver-check")
}

/// Recursively copy a checked-in project template into `to`, skipping any `target` dir (build
/// output). Staging into a fresh tempdir keeps the run isolated — the checked-in project is never
/// mutated by the build.
fn copy_tree_skipping_target(from: &Path, to: &Path) {
  fs::create_dir_all(to).unwrap();
  for entry in fs::read_dir(from).unwrap().flatten() {
    if entry.file_name() == "target" {
      continue;
    }
    let src = entry.path();
    let dst = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy_tree_skipping_target(&src, &dst);
    } else {
      fs::copy(&src, &dst).unwrap();
    }
  }
}

// The reverse-callback pipeline: the vendored `driver-check` project (NobiliaV's frame-loop callback,
// handed as a lambda to the imported trait and called back by a generic Rust method caller) builds,
// links, AND runs end to end through the real `valen build`. `nobiliav`'s bodies are deterministic —
// `main_loop` runs a bounded frame loop (hard-capped, no wall clock / windowing) that dispatches into the
// callback each frame until it asks to exit at frame 30 — so the honest assertion is run-to-exit-7, not
// just link.
#[test]
fn automates_driver_check_reverse_callback() {
  let project = TempDir::new().unwrap();
  copy_tree_skipping_target(&driver_check_template(), project.path());
  // `--no-borrow-check`: `on_tick(&mut self, w: &mut NobiliaWindow, …)` and the lambda churns the window
  // (`rotate_camera`/`request_exit`, `&mut self`). The auto-generated anon substruct renders `&mut`, but
  // churn enforcement through the `where func` bound is unbuilt, so the borrow checker is stepped around —
  // NobiliaV's real `valen build --no-borrow-check` shape.
  let run = valen_build(project.path(), "driver_check", /*borrow_check=*/ false);
  assert_eq!(run.build_rc, 0, "driver-check should build + link through valen build --no-borrow-check\n{}", run.output);
  assert!(run.exe.exists(), "the driver_check binary should be produced");
  assert_eq!(run_binary(&run.exe), 7, "driver-check should run the frame loop and exit 7");
}

// A warm rebuild of the AUTO-GENERATED reverse-callback path (the driver-check's
// `MainLoopCallback((win,inp) => {...})` lambda, no hand-written forwarder) must not panic and must run
// to the same value. A warm rebuild starts `monouts` empty and the partitioner is its sole re-populator —
// re-firing `per_instance_mir` in rustc's CGU order. If the callback's `per_instance_mir` (which reads
// `monouts`) re-fires before the export's (which populates it), the callback finds nothing to read.
//
// That order is baked into the CGU layout at COLD-build time and then reused by every warm rebuild of the
// same incremental cache — so an order bug shows up PER-LINEAGE (some cold builds produce a "bad"
// callback-before-export layout), not per warm rebuild. Looping warm rebuilds of ONE cache therefore
// can't catch it (a good lineage passes forever, a bad one fails on its first warm build). So this test
// loops many FRESH lineages (a new TempDir → fresh cold build → one warm rebuild each), sampling enough
// cold-build layouts that a live bug reds reliably. Same `--no-borrow-check` shape as
// `automates_driver_check_reverse_callback`.
#[test]
fn warm_rebuild_of_auto_generated_forwarder_runs_seven() {
  for iteration in 0..10 {
    // Fresh lineage: a new TempDir gets its own cold-build CGU layout.
    let project = TempDir::new().unwrap();
    copy_tree_skipping_target(&driver_check_template(), project.path());

    // Cold build — establishes the incremental cache. Cold is order-safe: the collector reaches the
    // callback only after instantiating the export, so `monouts` is populated in causal order.
    let cold = valen_build(project.path(), "driver_check", /*borrow_check=*/ false);
    assert_eq!(cold.build_rc, 0, "cold driver-check build #{iteration} should succeed\n{}", cold.output);
    assert_eq!(run_binary(&cold.exe), 7, "cold driver-check #{iteration} should exit 7");

    // Warm rebuild — if this lineage's cached layout puts the callback's CGU before the export's, the
    // partitioner re-fires the reader before the writer.
    let warm = valen_build(project.path(), "driver_check", /*borrow_check=*/ false);
    assert_eq!(warm.build_rc, 0, "warm rebuild (lineage #{iteration}) should link\n{}", warm.output);
    assert_eq!(
      run_binary(&warm.exe),
      7,
      "warm rebuild (lineage #{iteration}) should run the frame loop and exit 7"
    );
  }
}


// Reverse path: a warm rebuild after editing the *callback body* (which Rust calls back into) must run
// the new value. Exercises re-instantiating the override body from fresh `hinputs`, and the digest on
// the override's `#[vale::emit_consumer_body]` attr invalidating across a body-only edit.
#[test]
fn warm_rebuild_after_callback_body_edit() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "reverse", r#"
pub trait Callback { fn on_call(&self) -> i32; }
pub fn run_callback<C: Callback>(c: &C) -> i32 { c.on_call() }
"#);
  write_valen_project(project.path(), "rev", "reverse", r#"
import reverse.Callback;
import reverse.run_callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 7;
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
"#);
  let (cold_exit, warm_exit) =
    cold_edit_warm_exits(project.path(), "main", /*borrow_check=*/ false, |p| {
      fs::write(p.join("src/main.valen"), r#"
import reverse.Callback;
import reverse.run_callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 11;
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
"#).unwrap();
    });
  assert_eq!(cold_exit, 7, "cold build: callback returns seven");
  assert_eq!(warm_exit, 11, "warm rebuild: callback returns the edited eleven");
}

// Nested path (Valen -> Rust -> Valen -> Rust): a warm rebuild after editing the callback body to call
// a *different already-imported* Rust leaf must run the new value. Both `helper_a`/`helper_b` are
// imported, so the importer file is byte-identical — only the source digest forces rustc to re-collect
// the swapped-in outbound leaf (`helper_b`) that the callback body reaches through the generic caller.
#[test]
fn warm_rebuild_after_callback_outbound_swap() {
  let project = TempDir::new().unwrap();
  write_rust_dep_crate(project.path(), "reverse", r#"
pub trait Callback { fn on_call(&self) -> i32; }
pub fn run_callback<C: Callback>(c: &C) -> i32 { c.on_call() }
pub fn helper_a() -> i32 { 3 }
pub fn helper_b() -> i32 { 5 }
"#);
  write_valen_project(project.path(), "nested", "reverse", r#"
import reverse.Callback;
import reverse.run_callback;
import reverse.helper_a;
import reverse.helper_b;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return helper_a();
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
"#);
  let (cold_exit, warm_exit) =
    cold_edit_warm_exits(project.path(), "main", /*borrow_check=*/ false, |p| {
      fs::write(p.join("src/main.valen"), r#"
import reverse.Callback;
import reverse.run_callback;
import reverse.helper_a;
import reverse.helper_b;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return helper_b();
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
"#).unwrap();
    });
  assert_eq!(cold_exit, 3, "cold build: callback calls helper_a");
  assert_eq!(warm_exit, 5, "warm rebuild: callback calls the swapped-in helper_b");
}

/// The fork rustc, found the way `valen` finds it (`rustup which`).
fn fork_rustc() -> PathBuf {
  let out = Command::new("rustup")
    .args(["which", "--toolchain", "rustc-fork", "rustc"])
    .output()
    .expect("could not run rustup to find the rustc-fork toolchain");
  assert!(out.status.success(), "rustup could not find the rustc-fork toolchain");
  PathBuf::from(String::from_utf8(out.stdout).expect("rustup printed a non-utf8 path").trim())
}

// `valen build` has no release mode, so this stages the workspace with the library's `stage_workspace`
// and runs `cargo build --release` itself, with the same environment `valen build` hands cargo: the
// `valenc-rs` wrapper, the fork rustc as `COMPANION_RUSTC`, and the borrow checker off (the driver-check
// churn is stepped around, matching the debug pipeline build).
fn valen_build_release(project: &Path, bin_name: &str) -> (i32, PathBuf) {
  let build_dir = valen_build_dir_of(project);
  let valenc_rs = PathBuf::from(env!("CARGO_BIN_EXE_valenc-rs"));
  stage_workspace(&BuildInputs {
    manifest_path: project.join("Valen.toml"),
    build_dir: build_dir.clone(),
    valenc_rs: valenc_rs.clone(),
    clear_incremental: false,
    borrow_check: false,
  })
  .expect("stage_workspace should succeed");
  let status = Command::new("cargo")
    .current_dir(&build_dir)
    .args(["build", "--release"])
    .env("RUSTC_WORKSPACE_WRAPPER", &valenc_rs)
    .env("COMPANION_RUSTC", fork_rustc())
    .env("VALEN_BORROW_CHECK", "off")
    .env_remove("RUSTC")
    .status()
    .expect("could not spawn cargo");
  let exe = build_dir.join("target").join("release").join(bin_name);
  (status.code().unwrap_or(1), exe)
}

// A `--release` build of the vendored `driver-check` project must link and produce a binary, the same as
// the debug build does. `driver-check`'s `on_tick` is `&mut self` and the lambda churns the window, so
// the borrow checker is off (churn enforcement is unbuilt); the honest assertion is a clean build + link.
#[test]
fn release_build_of_driver_check_links() {
  let project = TempDir::new().unwrap();
  copy_tree_skipping_target(&driver_check_template(), project.path());
  let (build_rc, exe) = valen_build_release(project.path(), "driver_check");
  assert_eq!(build_rc, 0, "release build of driver-check should link");
  assert!(exe.exists(), "the release driver_check binary should be produced");
}
