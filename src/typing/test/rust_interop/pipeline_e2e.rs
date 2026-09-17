// Tier-3 end-to-end tests for the real `valen build` pipeline (@DBAPIZ): they drive the library
// dark-box `run_build` — real cargo -> the real `valenc-rs` wrapper -> generated workspace -> link
// -> run — over a project staged in a `tempfile::TempDir` (@TMBFIZ). Unlike the corpus (in-process
// rustc) and `drive_tests.rs` (in-process `run_wrapper`), these exercise the actual orchestrator.
//
// They need the `valenc-rs` binary built (`cargo +rustc-fork build --features rust_interop --bin
// valenc-rs --bin valen`), which the plain interop `--lib` gate does not build. So each test
// SOFT-SKIPS when the binary is absent (mirrors `wasi_skip!`); a dedicated fire-commit command
// builds the bin first, then runs this module.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::typing::rust_interop::orchestrator::{run_build, stage_workspace, BuildInputs};

/// Locate the `valenc-rs` binary beside the test executable (`<target>/debug/valenc-rs`) — the same
/// sibling-in-the-target-dir layout `valen build` itself assumes (`valen/main.rs`). `None` when it
/// hasn't been built (the plain `--lib` gate builds no bins); the tests soft-skip on `None`.
fn valenc_rs_bin() -> Option<PathBuf> {
  let exe = std::env::current_exe().ok()?;
  // <target>/debug/deps/<testbin>  ->  <target>/debug
  let target_debug = exe.parent()?.parent()?;
  let bin = target_debug.join("valenc-rs");
  bin.exists().then_some(bin)
}

/// Soft-skip a test when the `valenc-rs` binary isn't built. Yields the binary path, or returns.
macro_rules! require_valenc_rs {
  () => {
    match valenc_rs_bin() {
      Some(bin) => bin,
      None => {
        eprintln!(
          "pipeline_e2e skip: valenc-rs not built. Run via the e2e fire-commit command (it builds \
           the bins first): cargo +rustc-fork build --features rust_interop --bin valenc-rs --bin \
           valen && cargo +rustc-fork test --lib --features rust_interop pipeline_e2e"
        );
        return;
      }
    }
  };
}

/// The outcome of a real `valen build`: cargo's exit code and the produced binary's path.
struct BuildRun {
  build_rc: i32,
  exe: PathBuf,
}

/// Run the real `valen build` pipeline over a staged project directory (the `run_build` dark-box),
/// choosing whether to wipe the incremental cache first and whether the borrow checker runs.
/// `<project>/Valen.toml` is the manifest, the build dir is `<project>/target/valen-build`, and the
/// produced binary lands at `<build_dir>/target/debug/<bin_name>`.
fn valen_build_ex(
  project: &Path,
  valenc_rs: &Path,
  bin_name: &str,
  clear_incremental: bool,
  borrow_check: bool,
) -> BuildRun {
  let build_dir = project.join("target").join("valen-build");
  let build_rc = run_build(&BuildInputs {
    manifest_path: project.join("Valen.toml"),
    build_dir: build_dir.clone(),
    valenc_rs: valenc_rs.to_path_buf(),
    clear_incremental,
    borrow_check,
  })
  .expect("run_build should not error");
  let exe = build_dir.join("target").join("debug").join(bin_name);
  BuildRun { build_rc, exe }
}

/// A plain `valen build` that clears the incremental cache first (the current `valen` bin's behavior),
/// with the borrow checker on, for the cold-build tests.
fn valen_build(project: &Path, valenc_rs: &Path, bin_name: &str) -> BuildRun {
  valen_build_ex(project, valenc_rs, bin_name, /*clear_incremental=*/ true, /*borrow_check=*/ true)
}

/// The driven crate's incremental cache directory, two levels under the build dir.
fn incremental_dir(project: &Path) -> PathBuf {
  project.join("target").join("valen-build").join("target").join("debug").join("incremental")
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

/// Stage the tracer project into `project`: a minimal real Rust dep crate `tiny` exporting `seven()`,
/// a `Valen.toml` depending on it, and a `main.valen` that imports and calls it. The path dep renders
/// verbatim into the generated `<project>/target/valen-build/Cargo.toml` (two levels down), so `tiny`
/// beside the project is `../../tiny` from there.
fn write_forward_fixture(project: &Path) {
  let tiny = project.join("tiny");
  fs::create_dir_all(tiny.join("src")).unwrap();
  fs::write(
    tiny.join("Cargo.toml"),
    "[package]\nname = \"tiny\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
     [lib]\ncrate-type = [\"rlib\"]\n",
  )
  .unwrap();
  fs::write(tiny.join("src/lib.rs"), "pub fn seven() -> i32 { 7 }\n").unwrap();

  fs::create_dir_all(project.join("src")).unwrap();
  fs::write(
    project.join("Valen.toml"),
    "[project]\nname = \"tracer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
     [rust-dependencies]\ntiny = { path = \"../../tiny\" }\n\n\
     [[bin]]\nname = \"main\"\nsource = \"src/main.valen\"\n",
  )
  .unwrap();
  fs::write(
    project.join("src/main.valen"),
    "import rust.tiny.seven; exported func main() int { return seven(); }\n",
  )
  .unwrap();
}

/// Stage a forward project whose Rust dep exports *two* functions (`seven` → 7, `eleven` → 11), with
/// the `.valen` importing **both** but calling only `seven`. Editing the body to call `eleven` leaves
/// the imports — and therefore the generated stub (`pub use` lines + `unreachable!()` root) —
/// byte-identical, so only Part B's source digest can force rustc to re-collect the swapped-in leaf.
fn write_swappable_forward_fixture(project: &Path) {
  let tiny = project.join("tiny");
  fs::create_dir_all(tiny.join("src")).unwrap();
  fs::write(
    tiny.join("Cargo.toml"),
    "[package]\nname = \"tiny\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
     [lib]\ncrate-type = [\"rlib\"]\n",
  )
  .unwrap();
  fs::write(tiny.join("src/lib.rs"), "pub fn seven() -> i32 { 7 }\npub fn eleven() -> i32 { 11 }\n")
    .unwrap();

  fs::create_dir_all(project.join("src")).unwrap();
  fs::write(
    project.join("Valen.toml"),
    "[project]\nname = \"swaptracer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
     [rust-dependencies]\ntiny = { path = \"../../tiny\" }\n\n\
     [[bin]]\nname = \"main\"\nsource = \"src/main.valen\"\n",
  )
  .unwrap();
  fs::write(project.join("src/main.valen"), SWAP_FIXTURE_CALLS_SEVEN).unwrap();
}

const SWAP_FIXTURE_CALLS_SEVEN: &str =
  "import rust.tiny.seven; import rust.tiny.eleven; exported func main() int { return seven(); }\n";
const SWAP_FIXTURE_CALLS_ELEVEN: &str =
  "import rust.tiny.seven; import rust.tiny.eleven; exported func main() int { return eleven(); }\n";

/// Cold-build (clearing the cache) and run the binary, apply `edit` to mutate a source file, then
/// warm-rebuild (no clear, reusing rustc's incremental cache) and run again — returning
/// `(cold_exit, warm_exit)`. The produced binary path is reused across builds, so the cold run is
/// captured *before* the warm build overwrites it. Asserts both builds succeed (rc 0).
fn cold_edit_warm_exits(
  project: &Path,
  valenc_rs: &Path,
  bin_name: &str,
  edit: impl FnOnce(&Path),
) -> (i32, i32) {
  let cold =
    valen_build_ex(project, valenc_rs, bin_name, /*clear_incremental=*/ true, /*borrow_check=*/ true);
  assert_eq!(cold.build_rc, 0, "cold build should succeed");
  let cold_exit = run_binary(&cold.exe);
  edit(project);
  let warm =
    valen_build_ex(project, valenc_rs, bin_name, /*clear_incremental=*/ false, /*borrow_check=*/ true);
  assert_eq!(warm.build_rc, 0, "warm rebuild after the edit should link");
  let warm_exit = run_binary(&warm.exe);
  (cold_exit, warm_exit)
}

/// Stage a project whose Vale function churns one of its *parameters'* groups — calling the imported
/// `&mut self` `bump` on a `&Counter in g` parameter — without declaring `mut(g)`: a `counter` dep
/// crate (an opaque struct with a `&mut self` method), a `Valen.toml` depending on it, and the
/// `main.valen`. Same two-levels-down path-dep layout as `write_forward_fixture`.
fn write_undeclared_churn_fixture(project: &Path) {
  let counter = project.join("counter");
  fs::create_dir_all(counter.join("src")).unwrap();
  fs::write(
    counter.join("Cargo.toml"),
    "[package]\nname = \"counter\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
     [lib]\ncrate-type = [\"rlib\"]\n",
  )
  .unwrap();
  fs::write(
    counter.join("src/lib.rs"),
    "pub struct Counter { pub n: i32 }\n\
     impl Counter {\n\
     \x20   pub fn new() -> Counter { Counter { n: 6 } }\n\
     \x20   pub fn bump(&mut self) { self.n += 1; }\n\
     \x20   pub fn get(&self) -> i32 { self.n }\n\
     }\n",
  )
  .unwrap();

  fs::create_dir_all(project.join("src")).unwrap();
  fs::write(
    project.join("Valen.toml"),
    "[project]\nname = \"churner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
     [rust-dependencies]\ncounter = { path = \"../../counter\" }\n\n\
     [[bin]]\nname = \"main\"\nsource = \"src/main.valen\"\n",
  )
  .unwrap();
  fs::write(
    project.join("src/main.valen"),
    r#"
import rust.counter.Counter;
func bump_it<g'>(c &Counter in g) {
  c.bump();
}
exported func main() int {
  c = Counter.new();
  bump_it(&c);
  return c.get();
}
"#,
  )
  .unwrap();
}

// The borrow checker is on by default through the real pipeline: a Vale function churning a
// parameter's group through an imported `&mut self` method without declaring `mut(g)` fails the
// build. Paired with the test below, this proves the switch crosses the cargo process boundary,
// which an in-process `run_wrapper` test cannot see.
#[test]
fn valen_build_fails_an_undeclared_churn_with_the_borrow_checker_on() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_undeclared_churn_fixture(project.path());
  let run = valen_build_ex(
    project.path(),
    &valenc_rs,
    "main",
    /*clear_incremental=*/ true,
    /*borrow_check=*/ true,
  );
  assert_ne!(run.build_rc, 0, "the borrow checker should fail the build");
}

// `valen build --no-borrow-check`: the same program builds, links and runs → 7, so rust interop can
// be exercised end to end on a program the borrow checker is not yet happy with.
#[test]
fn valen_build_without_the_borrow_checker_runs_an_undeclared_churn_to_seven() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_undeclared_churn_fixture(project.path());
  let run = valen_build_ex(
    project.path(),
    &valenc_rs,
    "main",
    /*clear_incremental=*/ true,
    /*borrow_check=*/ false,
  );
  assert_eq!(run.build_rc, 0, "valen build --no-borrow-check should succeed");
  assert_eq!(run_binary(&run.exe), 7);
}

// A Vale binary that imports and calls a real Rust free function builds, links, and runs to its
// return value — through the actual `valen build` pipeline (cargo + the `valenc-rs` wrapper).
#[test]
fn builds_and_runs_a_binary_calling_a_rust_fn() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_forward_fixture(project.path());
  let run = valen_build(project.path(), &valenc_rs, "main");
  assert_eq!(run.build_rc, 0, "valen build should succeed");
  assert_eq!(run_binary(&run.exe), 7);
}

// A warm rebuild — the same project built twice without wiping the incremental cache between builds —
// still links, runs to the same value, and actually *reuses* the cache (the perf payoff). This is the
// case the cache-clear workaround masks: without Part A the warm build emits an empty `vale_cgu` and
// the link fails with an undefined `__vale_main`.
#[test]
fn warm_rebuild_no_edit_runs_seven_and_reuses_cache() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_forward_fixture(project.path());

  // Cold build — establishes the incremental cache (the clear is a no-op, nothing cached yet).
  let cold = valen_build_ex(
    project.path(),
    &valenc_rs,
    "main",
    /*clear_incremental=*/ true,
    /*borrow_check=*/ true,
  );
  assert_eq!(cold.build_rc, 0, "cold valen build should succeed");
  assert_eq!(run_binary(&cold.exe), 7);

  let incr = incremental_dir(project.path());
  let before = object_work_products(&incr);
  assert!(!before.is_empty(), "cold build should leave object work-products in the incremental cache");

  // Warm rebuild WITHOUT clearing — the bug's trigger. (`stage_workspace` re-copies the source each
  // build, bumping its mtime, so cargo re-invokes rustc over the reused incremental cache.)
  let warm = valen_build_ex(
    project.path(),
    &valenc_rs,
    "main",
    /*clear_incremental=*/ false,
    /*borrow_check=*/ true,
  );
  assert_eq!(warm.build_rc, 0, "warm rebuild should link (Part A re-fires per_instance_mir)");
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
// must run the new value. The imports are unchanged, so the generated stub is byte-identical — rustc's
// stub-crate fingerprint doesn't move on its own, and without Part B's source digest it reuses a stale
// mono partition that never codegens the swapped-in leaf (undefined symbol at link, or a stale 7).
#[test]
fn warm_rebuild_after_forward_call_swap() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_swappable_forward_fixture(project.path());
  let (cold_exit, warm_exit) = cold_edit_warm_exits(project.path(), &valenc_rs, "main", |p| {
    fs::write(p.join("src/main.valen"), SWAP_FIXTURE_CALLS_ELEVEN).unwrap();
  });
  assert_eq!(cold_exit, 7, "cold build calls seven");
  assert_eq!(warm_exit, 11, "warm rebuild runs the swapped-in eleven (Part B re-collected it)");
}

/// Recursively copy a checked-in project template into `to`, skipping any `target` dir (build
/// output). Staging into a fresh tempdir keeps the run isolated (@TMBFIZ) — the checked-in project
/// is never mutated by the build.
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
// called back by a generic Rust caller) builds, links, AND runs end to end through the real
// `valen build`. `nobiliav`'s headless bodies are real and deterministic — `main_loop` runs a bounded
// frame loop (hard-capped, no wall clock / windowing) that dispatches into the callback each frame until
// it asks to exit at frame 30 — so the honest assertion is run-to-exit-7, not just link.
//
// This test's `driver_check.valen` currently uses a HAND-WRITTEN forwarder struct; the endeavor's goal
// is to swap it to the auto-generated form `MainLoopCallback((win,inp) => {...})` (the ready-to-apply
// patch is staged in docs/handoffs/rust-interop-handoff.md, blocked on the Guardian AFEOX `.valen`
// allowlist). Either form returns 7, so this assertion holds across the swap; after the swap it becomes
// the orchestrator-parity proof of the auto-generated forwarder.
#[test]
fn automates_driver_check_reverse_callback() {
  let valenc_rs = require_valenc_rs!();
  let template =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/typing/test/rust_interop/driver-check");
  let project = TempDir::new().unwrap();
  copy_tree_skipping_target(&template, project.path());
  // `--no-borrow-check`: `on_tick(&mut self, w: &mut NobiliaWindow, …)` and the lambda churns the window
  // (`rotate_camera`/`request_exit`, `&mut self`). The auto-generated anon substruct renders `&mut`, but
  // churn enforcement through the `where func` bound is unbuilt, so the borrow checker is stepped around —
  // NobiliaV's real `valen build --no-borrow-check` shape.
  let run =
    valen_build_ex(project.path(), &valenc_rs, "driver_check", /*clear_incremental=*/ true, /*borrow_check=*/ false);
  assert_eq!(run.build_rc, 0, "driver-check should build + link through valen build --no-borrow-check");
  assert!(run.exe.exists(), "the driver_check binary should be produced");
  assert_eq!(run_binary(&run.exe), 7, "driver-check should run the frame loop and exit 7");
}

// A warm rebuild of the AUTO-GENERATED reverse-callback path (the driver-check's
// `MainLoopCallback((win,inp) => {...})` lambda, no hand-written forwarder) must not panic and must run
// to the same value. The clean build is green, but a warm rebuild starts `monouts` empty and fix A
// (`lang_collect_and_partition_mono_items`) is its sole re-populator — re-firing `per_instance_mir` in
// rustc's CGU order. When the callback's `per_instance_mir` (which READS `monouts` to build the typeid
// universe, in `collect_callback`) re-fires before the export's (which POPULATES `monouts`), the universe
// is empty and the universe-presence assert panics.
//
// That order is baked into the CGU layout at COLD-build time and then reused by every warm rebuild of the
// same incremental cache — so the nondeterminism is PER-LINEAGE (~50% of cold builds produce a "bad"
// callback-before-export layout), not per warm rebuild. Looping warm rebuilds of ONE cache therefore
// can't catch it (a good lineage passes forever, a bad one panics on its first warm build). So this test
// loops many FRESH lineages (a new TempDir → fresh cold build → one warm rebuild each), sampling enough
// cold-build layouts that a live bug reds reliably (P(all pass | broken) ≈ 0.5^N). `cold_edit_warm_exits`
// and the forward warm-rebuild tests only cover hand-written/forward paths, so this is the auto-gen
// path's guard. Same `--no-borrow-check` shape as `automates_driver_check_reverse_callback`.
#[test]
fn warm_rebuild_of_auto_generated_forwarder_runs_seven() {
  let valenc_rs = require_valenc_rs!();
  let template =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/typing/test/rust_interop/driver-check");

  for iteration in 0..10 {
    // Fresh lineage: a new TempDir gets its own cold-build CGU layout (the nondeterministic variable).
    let project = TempDir::new().unwrap();
    copy_tree_skipping_target(&template, project.path());

    // Cold build — establishes the incremental cache (the clear is a no-op, nothing cached yet). Cold
    // never panics: the collector reaches the callback only after instantiating the export, so `monouts`
    // is populated in causal order regardless of layout.
    let cold = valen_build_ex(
      project.path(),
      &valenc_rs,
      "driver_check",
      /*clear_incremental=*/ true,
      /*borrow_check=*/ false,
    );
    assert_eq!(cold.build_rc, 0, "cold driver-check build #{iteration} should succeed");
    assert_eq!(run_binary(&cold.exe), 7, "cold driver-check #{iteration} should exit 7");

    // Warm rebuild WITHOUT clearing — the bug's trigger. If this lineage's cached layout puts the
    // callback's CGU before the export's, fix A re-fires the reader before the writer and it panics.
    let warm = valen_build_ex(
      project.path(),
      &valenc_rs,
      "driver_check",
      /*clear_incremental=*/ false,
      /*borrow_check=*/ false,
    );
    assert_eq!(
      warm.build_rc, 0,
      "warm rebuild (lineage #{iteration}) should link (no universe-presence panic)"
    );
    assert_eq!(
      run_binary(&warm.exe),
      7,
      "warm rebuild (lineage #{iteration}) should run the frame loop and exit 7"
    );
  }
}

/// Write the runnable reverse-callback Rust dep crate `reverse` into `<project>/reverse`: a `Callback`
/// trait, a generic `run_callback<C: Callback>` that dispatches into the Valen override, and two leaf
/// helpers (`helper_a` → 3, `helper_b` → 5) the callback body can call back out to. Unlike the vendored
/// `driver-check` (whose `nobiliav` bodies are `unimplemented!()` — link-only), this crate runs.
fn write_reverse_crate(project: &Path) {
  let krate = project.join("reverse");
  fs::create_dir_all(krate.join("src")).unwrap();
  fs::write(
    krate.join("Cargo.toml"),
    "[package]\nname = \"reverse\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
     [lib]\ncrate-type = [\"rlib\"]\n",
  )
  .unwrap();
  fs::write(
    krate.join("src/lib.rs"),
    "pub trait Callback { fn on_call(&self) -> i32; }\n\
     pub fn run_callback<C: Callback>(c: &C) -> i32 { c.on_call() }\n\
     pub fn helper_a() -> i32 { 3 }\n\
     pub fn helper_b() -> i32 { 5 }\n",
  )
  .unwrap();
}

/// Write the `Valen.toml` (depending on the sibling `reverse` crate) and `main.valen` for a reverse
/// project: `main` constructs a Vale `MyCb` implementing the Rust `Callback` trait and hands it to the
/// generic Rust `run_callback`, which calls back into `MyCb::on_call`.
fn write_reverse_project(project: &Path, name: &str, main_valen: &str) {
  write_reverse_crate(project);
  fs::create_dir_all(project.join("src")).unwrap();
  fs::write(
    project.join("Valen.toml"),
    format!(
      "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
       [rust-dependencies]\nreverse = {{ path = \"../../reverse\" }}\n\n\
       [[bin]]\nname = \"main\"\nsource = \"src/main.valen\"\n"
    ),
  )
  .unwrap();
  fs::write(project.join("src/main.valen"), main_valen).unwrap();
}

const CALLBACK_RETURNS_SEVEN: &str = "\
import rust.reverse.Callback;
import rust.reverse.run_callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 7;
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
";
const CALLBACK_RETURNS_ELEVEN: &str = "\
import rust.reverse.Callback;
import rust.reverse.run_callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 11;
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
";

// Reverse path: a warm rebuild after editing the *callback body* (which Rust calls back into) must run
// the new value. Exercises Part A re-instantiating the override body from fresh `hinputs` and Part B's
// digest on the override's `#[vale::emit_consumer_body]` attr invalidating across a body-only edit.
#[test]
fn warm_rebuild_after_callback_body_edit() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_reverse_project(project.path(), "rev", CALLBACK_RETURNS_SEVEN);
  let (cold_exit, warm_exit) = cold_edit_warm_exits(project.path(), &valenc_rs, "main", |p| {
    fs::write(p.join("src/main.valen"), CALLBACK_RETURNS_ELEVEN).unwrap();
  });
  assert_eq!(cold_exit, 7, "cold build: callback returns seven");
  assert_eq!(warm_exit, 11, "warm rebuild: callback returns the edited eleven");
}

const NESTED_CALLS_HELPER_A: &str = "\
import rust.reverse.Callback;
import rust.reverse.run_callback;
import rust.reverse.helper_a;
import rust.reverse.helper_b;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return helper_a();
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
";
const NESTED_CALLS_HELPER_B: &str = "\
import rust.reverse.Callback;
import rust.reverse.run_callback;
import rust.reverse.helper_a;
import rust.reverse.helper_b;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return helper_b();
}
exported func main() int {
  mmlcb = MyCb();
  return run_callback(&mmlcb);
}
";

// Nested path (Valen -> Rust -> Valen -> Rust): a warm rebuild after editing the callback body to call
// a *different already-imported* Rust leaf must run the new value. Both `helper_a`/`helper_b` are
// imported, so the stub is byte-identical — only Part B's source digest forces rustc to re-collect the
// swapped-in outbound leaf (`helper_b`) that the callback body reaches through the generic caller.
#[test]
fn warm_rebuild_after_callback_outbound_swap() {
  let valenc_rs = require_valenc_rs!();
  let project = TempDir::new().unwrap();
  write_reverse_project(project.path(), "nested", NESTED_CALLS_HELPER_A);
  let (cold_exit, warm_exit) = cold_edit_warm_exits(project.path(), &valenc_rs, "main", |p| {
    fs::write(p.join("src/main.valen"), NESTED_CALLS_HELPER_B).unwrap();
  });
  assert_eq!(cold_exit, 3, "cold build: callback calls helper_a");
  assert_eq!(warm_exit, 5, "warm rebuild: callback calls the swapped-in helper_b");
}

// Merged in from the release-mode link fix (main). A `--release` `valen build` stages the workspace
// then builds release directly (not through `run_build`), so `clear_incremental` is immaterial here —
// `stage_workspace` does not read it. `VALEN_BORROW_CHECK=0` is set on the cargo spawn (the release path
// does not go through `run_build`, which is what normally sets it) so the driver-check churn is stepped
// around, matching the debug pipeline build.
fn valen_build_release(project: &Path, valenc_rs: &Path, bin_name: &str) -> (i32, PathBuf) {
  let build_dir = project.join("target").join("valen-build");
  stage_workspace(&BuildInputs {
    manifest_path: project.join("Valen.toml"),
    build_dir: build_dir.clone(),
    valenc_rs: valenc_rs.to_path_buf(),
    clear_incremental: false,
    borrow_check: false,
  })
  .expect("stage_workspace should succeed");
  let status = Command::new("cargo")
    .current_dir(&build_dir)
    .args(["build", "--release"])
    .env("RUSTC_WORKSPACE_WRAPPER", valenc_rs)
    .env("VALEN_BORROW_CHECK", "0")
    .env_remove("RUSTC")
    .status()
    .expect("could not spawn cargo");
  let exe = build_dir.join("target").join("release").join(bin_name);
  (status.code().unwrap_or(1), exe)
}

// A `--release` `valen build --no-borrow-check` of the vendored `driver-check` project must link and
// produce a binary, the same as the debug build does. `driver-check`'s `on_tick` is `&mut self` and the
// lambda churns the window, so the borrow checker is off (churn enforcement is unbuilt); the honest
// assertion is a clean build + link.
#[test]
fn release_build_of_driver_check_links() {
  let valenc_rs = require_valenc_rs!();
  let template =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/typing/test/rust_interop/driver-check");
  let project = TempDir::new().unwrap();
  copy_tree_skipping_target(&template, project.path());
  let (build_rc, exe) = valen_build_release(project.path(), &valenc_rs, "driver_check");
  assert_eq!(build_rc, 0, "release valen build of driver-check should link");
  assert!(exe.exists(), "the release driver_check binary should be produced");
}
