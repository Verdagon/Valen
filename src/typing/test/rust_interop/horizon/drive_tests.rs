// Tests that call `drive` directly, the way `valenc-rs` does under cargo. Unlike `cases.rs`, these don't
// use a fixture directory: each writes its own Rust crate inline, builds it to an rlib,
// and hands `drive` the `--extern`/`-L` flags cargo would. The `final_file_*` tests read the generated
// final Rust file through `drive`'s `after_typing` hook; the `drive_*` tests link a bin and run it. Each
// test builds everything in its own `TempDir`.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::typing::rust_interop::get_env_rustc_and_sysroot_locations;
use crate::typing::test::rust_interop::drive_helpers::{drive_and_run_binary, drive_prog, Scratch};

// Build one dependency crate to an rlib in `out_dir` with the fork rustc (`COMPANION_RUSTC`), the same
// way the harness builds a fixture's crates.
fn build_dep_rlib(crate_name: &str, source: &Path, out_dir: &Path) {
  let (rustc_location, _sysroot) = get_env_rustc_and_sysroot_locations();
  let status = Command::new(rustc_location)
    .arg(source)
    .args(["--crate-type=lib", "--edition=2021"])
    .arg(format!("--crate-name={crate_name}"))
    .arg(format!("-L{}", out_dir.display()))
    .arg("--out-dir")
    .arg(out_dir)
    .status()
    .expect("could not run rustc to build the dependency rlib");
  assert!(status.success(), "building the dependency rlib for `{crate_name}` failed");
}

// Write `source` as `<name>.rs` in `out_dir`, build it to `lib<name>.rlib` there, and return the
// `--extern`/`-L` flags that let a driven `.valen` import it.
fn build_inline_crate(out_dir: &Path, crate_name: &str, source: &str) -> Vec<String> {
  let rs = out_dir.join(format!("{crate_name}.rs"));
  fs::write(&rs, source).unwrap_or_else(|e| panic!("could not write {}: {e}", rs.display()));
  build_dep_rlib(crate_name, &rs, out_dir);
  let rlib = out_dir.join(format!("lib{crate_name}.rlib"));
  vec![format!("--extern={crate_name}={}", rlib.display()), format!("-L{}", out_dir.display())]
}

// Build, with cargo on the fork toolchain, a `greeter` crate that re-exports `helper::seven` (helper's
// `seven` is the canonical item; greeter names it only via `pub use`). Returns the `deps` dir holding
// both hashed rlibs. `--target-dir` is explicit because this environment sets a shared cargo target dir.
// Hermetic: path dependency only, no network.
fn build_reexport_crate(root: &Path) -> PathBuf {
  let helper = root.join("helper");
  fs::create_dir_all(helper.join("src")).expect("mkdir helper/src");
  fs::write(
    helper.join("Cargo.toml"),
    "[package]\nname = \"helper\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"rlib\"]\n",
  )
  .expect("write helper Cargo.toml");
  fs::write(helper.join("src/lib.rs"), "pub fn seven() -> i32 { 7 }\n").expect("write helper lib.rs");

  let greeter = root.join("greeter");
  fs::create_dir_all(greeter.join("src")).expect("mkdir greeter/src");
  fs::write(
    greeter.join("Cargo.toml"),
    "[package]\nname = \"greeter\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"rlib\"]\n\n[dependencies]\nhelper = { path = \"../helper\" }\n",
  )
  .expect("write greeter Cargo.toml");
  fs::write(greeter.join("src/lib.rs"), "pub use helper::seven;\n").expect("write greeter lib.rs");
  fs::write(greeter.join("rust-toolchain.toml"), "[toolchain]\nchannel = \"rustc-fork\"\n")
    .expect("write rust-toolchain.toml");

  let target_dir = root.join("target");
  let (_rustc_location, sysroot) = get_env_rustc_and_sysroot_locations();
  let build_out = Command::new("cargo")
    .current_dir(&greeter)
    .arg("build")
    .arg("--offline")
    .arg("--target-dir")
    .arg(&target_dir)
    .env("RUSTUP_TOOLCHAIN", "rustc-fork")
    .env("DYLD_LIBRARY_PATH", format!("{sysroot}/lib"))
    .env_remove("RUSTC")
    .output()
    .expect("could not spawn cargo build");
  assert!(
    build_out.status.success(),
    "cargo build (fork) failed:\nstdout:\n{}\nstderr:\n{}",
    String::from_utf8_lossy(&build_out.stdout),
    String::from_utf8_lossy(&build_out.stderr),
  );
  target_dir.join("debug/deps")
}

// Drive `vale_source` and return the final Rust file `drive` generated for it.
fn final_file_of(out_dir: &Path, vale_source: &str, extra: Vec<String>) -> String {
  let final_file: RefCell<Option<String>> = RefCell::new(None);
  drive_prog(out_dir, vale_source, extra, /*borrow_check=*/ false, |text| {
    *final_file.borrow_mut() = Some(text.to_string())
  })
  .expect("drive should succeed");
  final_file.into_inner().expect("drive should have generated the final Rust file")
}

// The generator emits the load-bearing final-file shape from the typed program: an `extern crate` per
// imported crate, the marker, a `#[vale::emit_consumer_body]` root per exported func, and the bin shim.
#[test]
fn final_file_emits_consumer_body_from_typed_program() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "tiny", r#"
pub fn seven() -> i32 { 7 }
"#);
  let final_file = final_file_of(&out_dir, r#"
import tiny.seven;
exported func main() int { return seven(); }
"#, extra);
  assert!(final_file.contains("extern crate tiny;"), "final file:\n{final_file}");
  // The attr carries a digest of the `.valen` source (so rustc re-collects on a body-only edit —
  // the incremental fix, Part B), so match its prefix rather than the bare attr.
  assert!(final_file.contains("#[vale::emit_consumer_body(digest = \""), "final file:\n{final_file}");
  assert!(final_file.contains("pub fn __vale_main() -> i32"), "final file:\n{final_file}");
  assert!(final_file.contains("__VALE_STUBS_MARKER"), "final file:\n{final_file}");
  assert!(final_file.contains("fn main()"), "final file:\n{final_file}");
}

// A Vale struct that implements an imported Rust trait must be projected into the final file as a real
// Rust type + trait impl, so rustc can monomorphize the generic caller over it and reach the override body
// (which the Valen backend fills under the same mangled symbol). Without this projection the struct's
// type arg is unconvertible, the generic-method leaf never resolves, and the backend aborts on an
// undeclared extern (the NobiliaV `on_tick` crash). The override signature is rendered from the Vale
// one: a `self &Struct` receiver → `&self`, a `&ImportedType` param → `&ImportedType`, void return.
#[test]
fn final_file_projects_a_valen_struct_that_implements_a_rust_trait() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "mycrate", r#"
pub struct Widget {}
impl Widget {
    pub fn poke(&self) {}
}
pub trait Cb {
    fn on_event(&self, w: &Widget);
}
"#);
  let vale = r#"
import mycrate.Widget;
import mycrate.Cb;
struct MyCb { }
impl Cb for MyCb;
func on_event(self &MyCb, w &Widget) {
  w.poke();
}
exported func main() int {
  return 7;
}
"#;
  let final_file = final_file_of(&out_dir, vale, extra);
  // The non-generic struct is the degenerate case of the wrapper-as-field shape: a `__ValeOpaque<HASH>`
  // payload + an empty `PhantomData<()>` carrier, and a non-generic impl naming the trait by crate path.
  assert!(final_file.contains("pub struct MyCb(__ValeOpaque<"), "final file:\n{final_file}");
  assert!(final_file.contains(", ::std::marker::PhantomData<()>);"), "final file:\n{final_file}");
  assert!(final_file.contains("impl ::mycrate::Cb for MyCb {"), "final file:\n{final_file}");
  // The override is emitted inside the impl with a deferred body and the rendered Rust signature, its
  // parameters named by position.
  assert!(
    final_file.contains("fn on_event(&self, _p1: &::mycrate::Widget) {"),
    "final file:\n{final_file}"
  );
  assert!(final_file.contains("unreachable!()"), "final file:\n{final_file}");
}

// A Vale override that mirrors a Rust `&mut` trait method must project `&mut` into the final file, or
// rustc rejects the impl with E0053 ("types differ in mutability") — the NobiliaV `on_tick(&mut self, w:
// &mut NobiliaWindow, ...)` wall. Mutability lives in the override's `mut(g)` effect clause plus each
// borrow's `in g` region, never on the `&T` type, so the emitter reads the effect clause and each
// parameter's (and the receiver's) region: a borrow whose region is marked `mut` renders `&mut`. `input`
// has no `mut` region and is the negative control that stays a shared `&FrameInput`. (The non-mut
// `on_event` test above is the regression guard that a shared-only override still renders
// `&self`/`&Widget`.)
#[test]
fn final_file_projects_mut_borrows_as_mut_references() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(
    &out_dir,
    "mycrate",
    "pub struct Widget {}\n\
     impl Widget {\n\
     \x20   pub fn poke(&self) {}\n\
     }\n\
     pub struct FrameInput {}\n\
     pub trait Cb {\n\
     \x20   fn on_tick(&mut self, w: &mut Widget, input: &FrameInput);\n\
     }\n",
  );
  let vale = r#"
import mycrate.Widget;
import mycrate.FrameInput;
import mycrate.Cb;
struct MyCb { }
impl Cb for MyCb;
func on_tick<r', s'>(self &MyCb in s, w &Widget in r, input &FrameInput) mut(r) mut(s) {
  w.poke();
}
exported func main() int {
  return 7;
}
"#;
  let final_file = final_file_of(&out_dir, vale, extra);
  // `&mut self` (region s is mut), `&mut Widget` (region r is mut), and a shared `&FrameInput` (no mut
  // region) — all three in one signature, so the mut-set read and the shared negative control are proven
  // together.
  assert!(
    final_file.contains("fn on_tick(&mut self, _p1: &mut ::mycrate::Widget, _p2: &::mycrate::FrameInput) {"),
    "final file:\n{final_file}"
  );
}

// A generic, data-carrying forwarder (`MyCb<F>` holding a functor, implementing an imported trait) is the
// shape that lets a lambda be handed to a rust callback. It projects as the design's opaque wrapper-as-field
// shape (arch §10): `pub struct MyCb<F>(__ValeOpaque<HASH>, PhantomData<(F)>)` + `impl<F> Cb for MyCb<F>`,
// with the `__ValeOpaque<const T: u64>` wrapper predeclared once at the file root. rustc keeps MyCb's own
// DefId (so the impl resolves) but never sees its fields; Vale owns the layout via a `layout_of` override.
// Mirrors Sky's `__ToylangOpaque` shape (toylangc/src/stub_gen.rs).
#[test]
fn final_file_projects_a_generic_forwarder_as_opaque_wrapper() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "mycrate", r#"
pub struct Widget {}
impl Widget {
    pub fn poke(&self) {}
}
pub trait Cb {
    fn on_event(&self, w: &Widget);
}
"#);
  let vale = r#"
import mycrate.Widget;
import mycrate.Cb;
struct MyCb<F> where func drop(F)void, func __call(&F, &Widget)void { f F; }
impl<F> Cb for MyCb<F>;
func on_event<F>(self &MyCb<F>, w &Widget) {
  (&self.f)(w);
}
exported func main() int {
  return 7;
}
"#;
  let final_file = final_file_of(&out_dir, vale, extra);
  // The universal opaque wrapper is predeclared once at the file root, as the 3-marker-field struct:
  // a real `UnsafeCell<()>` for `!Freeze` (so rustc emits no `readonly` on a `&__ValeOpaque<..>` param;
  // a `PhantomData` of it would stay `Freeze`), `PhantomData<*mut ()>` for `!Send + !Sync`,
  // `PhantomPinned` for `!Unpin`; fields at all so rustc's debuginfo walker has something to visit.
  // Pinned as the exact text because each field is load-bearing and the composition is easy to
  // "simplify" back into a hole.
  assert!(
    final_file.contains(
      "pub struct __ValeOpaque<const T: u64>(::core::cell::UnsafeCell<()>, \
       ::std::marker::PhantomData<*mut ()>, ::std::marker::PhantomPinned);"
    ),
    "final file:\n{final_file}"
  );
  // The forwarder is the 2-field wrapper-as-field shape, generic over its one parameter (rendered
  // positionally as `T0`), with a PhantomData carrier so the declared generic is "used" (E0392). The
  // typeid hash is elided (it is stability-fenced separately).
  assert!(final_file.contains("pub struct MyCb<T0>(__ValeOpaque<"), "final file:\n{final_file}");
  assert!(final_file.contains(">, ::std::marker::PhantomData<(T0)>);"), "final file:\n{final_file}");
  // The impl mirrors the struct's generics: `impl<T0> ::mycrate::Cb for MyCb<T0>`.
  assert!(final_file.contains("impl<T0> ::mycrate::Cb for MyCb<T0> {"), "final file:\n{final_file}");
  assert!(
    final_file.contains("fn on_event(&self, _p1: &::mycrate::Widget) {"),
    "final file:\n{final_file}"
  );
}

// The tracer: a `.valen` binary that imports a caller-supplied std-only rlib links and runs → 7. `drive`
// generates the importer file, substitutes it for the `.valen`, drives rustc to a linked bin, and (via
// the helper) the bin is run to check the forwarded exit code.
#[test]
fn drive_links_a_valen_binary_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  // The rlib the program imports (a bare std-only crate built the canonical way, as Pearl's cargo would).
  let extra = build_inline_crate(&out_dir, "tiny", r#"
pub fn seven() -> i32 { 7 }
"#);
  let exit = drive_and_run_binary(&out_dir, r#"
import tiny.seven;
exported func main() int { return seven(); }
"#, extra, true);
  assert_eq!(exit, 7);
}

// Pearl's real scenario through `drive`: a Valen program imports an item the named crate only
// *re-exports* (`greeter` does `pub use helper::seven`), the canonical crate (`helper`) being a separate
// dependency. Linked with the explicit `--extern greeter=<rlib>` cargo hands `valenc-rs` + `-L
// dependency=<deps>`. Proves the canonical (`helper`) symbol resolves from `-L dependency` alone — i.e.
// one `--extern`, not three.
//
// Ignored until the crate list is split: `seven`'s coordinate is its defining crate, `helper`, which is
// not an `--extern` name, so `Compiler::in_rust_crate` says no and typing finds no function for it. See
// the handoff's "one crate list is doing two jobs" item.
#[test]
#[ignore]
fn drive_links_a_cargo_crate_through_a_pub_use_re_export() {
  let build = TempDir::new().expect("could not create build dir");
  let deps_dir = build_reexport_crate(build.path());
  let greeter_rlib = find_rlib(&deps_dir, "greeter");

  let scratch = Scratch::new();
  let exit = drive_and_run_binary(
    &scratch.out_dir(),
    r#"
import greeter.seven;
exported func main() int { return seven(); }
"#,
    vec![
      format!("--extern=greeter={}", greeter_rlib.display()),
      format!("-Ldependency={}", deps_dir.display()),
    ],
    /*borrow_check=*/ false,
  );
  assert_eq!(exit, 7);
}

// Find cargo's content-hashed `lib<name>-<hash>.rlib` in a deps dir — the explicit path cargo hands
// `valenc-rs` as `--extern <name>=<path>`.
fn find_rlib(deps_dir: &Path, name: &str) -> PathBuf {
  let prefix = format!("lib{name}-");
  fs::read_dir(deps_dir)
    .expect("could not read the deps dir")
    .flatten()
    .map(|entry| entry.path())
    .find(|path| {
      path
        .file_name()
        .and_then(|f| f.to_str())
        .is_some_and(|f| f.starts_with(&prefix) && f.ends_with(".rlib"))
    })
    .unwrap_or_else(|| panic!("no lib{name}-*.rlib in {}", deps_dir.display()))
}

// The combined reverse + forward `&mut` shape, end to end through `drive` — the exact two walls
// NobiliaV's driver hits the instant the reverse projection compiles, in one program:
//   - REVERSE: a Vale struct `impl`s a Rust trait whose method takes `&mut self` + a `&mut Window` (an
//     opaque imported struct) inbound; the override calls `w.push()`, a `&mut self` window method.
//   - FORWARD: `main` hands its own Vale struct to a Rust `fn run<C>(&mut self, cb: &mut C)` by exclusive
//     ref, and calls that `&mut self` method on an owned opaque `Window` local.
// The generated final file must render `&mut self`/`&mut Window` or rustc rejects the impl with E0053;
// then the whole thing links and runs → 7. Proves both `&mut` directions cross the boundary together, not
// just the reverse projection in isolation.
#[test]
fn drive_links_a_reverse_and_forward_mut_callback_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  // The Rust facade: an opaque `Window` with a `&mut self` method (`push`) and a generic `&mut self`
  // caller (`run`) that owns a `Frame` and calls the callback's `&mut self` `on_tick` with `&mut self`
  // (the window) inbound — NobiliaV's `main_loop`/`on_tick` shape after its interior-mutability removal.
  let extra = build_inline_crate(
    &out_dir,
    "noblike",
    "pub struct Window { pub ticks: i32 }\n\
     pub struct Frame {}\n\
     pub trait MainLoop {\n\
     \x20   fn on_tick(&mut self, w: &mut Window, input: &Frame);\n\
     }\n\
     impl Window {\n\
     \x20   pub fn new() -> Window { Window { ticks: 0 } }\n\
     \x20   pub fn push(&mut self) { self.ticks += 1; }\n\
     \x20   pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {\n\
     \x20       let frame = Frame {};\n\
     \x20       cb.on_tick(self, &frame);\n\
     \x20       7\n\
     \x20   }\n\
     }\n",
  );
  let vale = r#"
import noblike.Window;
import noblike.Frame;
import noblike.MainLoop;
struct MyCb { }
impl MainLoop for MyCb;
func on_tick<r', s'>(self &MyCb in s, w &Window in r, input &Frame) mut(r) mut(s) {
  w.push();
}
exported func main() int {
  w = Window.new();
  mmlcb = MyCb();
  return w.run(&mmlcb);
}
"#;
  let exit = drive_and_run_binary(&out_dir, vale, extra, /*borrow_check=*/ false);
  assert_eq!(exit, 7);
}

// Outbound arg-lowering in isolation: a generic forwarder `MyCb<F>` (holding a lambda functor, impl-ing an
// imported trait so it's projected) is handed to a Rust generic fn `touch<C>` that takes it by borrow and
// *never calls the trait method*. So this exercises only the outbound leaf `touch::<MyCb<Lambda>>` — its
// type arg `MyCb<Lambda>` must lower, with the internal lambda functor rewritten to `__ValeOpaque<typeid>`
// (arch §10.7 Case 2) — with no inbound callback.
#[test]
fn drive_lowers_a_forwarder_arg_to_a_noncallback_rust_fn() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub trait MainLoop {
    fn on_tick(&self, w: &Window);
}
pub fn touch<C: MainLoop>(_cb: &C) {}
"#);
  let vale = r#"
import noblike.Window;
import noblike.MainLoop;
import noblike.touch;
struct MyCb<F> where func drop(F)void, func __call(&F, &Window)void { f F; }
impl<F> MainLoop for MyCb<F>;
func on_tick<F>(self &MyCb<F>, w &Window) {
  (&self.f)(w);
}
exported func main() int {
  cb = MyCb((w2) => { });
  touch(&cb);
  return 7;
}
"#;
  let exit = drive_and_run_binary(&out_dir, vale, extra, /*borrow_check=*/ false);
  assert_eq!(exit, 7);
}

// The stateless-lambda forwarder, end to end through `drive`. A generic forwarder `MyCb<F>` holds a
// *stateless* lambda functor and implements an imported `&self` trait; `main` hands `MyCb((w)=>{...})` to
// a generic Rust `fn run<C>(&self, cb: &C)`. The functor is a ZST, so the opaque-ZST projection is
// correct here.
#[test]
fn drive_links_a_stateless_lambda_forwarder_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub trait MainLoop {
    fn on_tick(&self, w: &Window);
}
impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn poke(&self) {}
    pub fn run<C: MainLoop>(&self, cb: &C) -> i32 {
        cb.on_tick(self);
        7
    }
}
"#);
  let vale = r#"
import noblike.Window;
import noblike.MainLoop;
struct MyCb<F> where func drop(F)void, func __call(&F, &Window)void { f F; }
impl<F> MainLoop for MyCb<F>;
func on_tick<F>(self &MyCb<F>, w &Window) {
  (&self.f)(w);
}
exported func main() int {
  w = Window.new();
  cb = MyCb((w2) => { w2.poke(); });
  return w.run(&cb);
}
"#;
  let exit = drive_and_run_binary(&out_dir, vale, extra, /*borrow_check=*/ false);
  assert_eq!(exit, 7);
}

// The AUTO-GENERATED forwarder, end to end. Same as the stateless-lambda test above, but with NO
// hand-written `struct MyCb / impl MainLoop / func on_tick` — the callsite writes
// `MainLoop((w2) => { w2.poke(); })` and the compiler synthesizes the anon substruct, projects its
// `pub struct MainLoop__anon<F> + impl` through the final file, and drives it to exit 7. This is the whole
// endeavor's goal (NobiliaV writes the lambda, not the forwarder). The anon substruct exists only
// post-typing, which is why it lives in the final file.
#[test]
fn drive_links_an_auto_generated_forwarder_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub trait MainLoop {
    fn on_tick(&self, w: &Window);
}
impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn poke(&self) {}
    pub fn run<C: MainLoop>(&self, cb: &C) -> i32 {
        cb.on_tick(self);
        7
    }
}
"#);
  let vale = r#"
import noblike.Window;
import noblike.MainLoop;
exported func main() int {
  w = Window.new();
  cb = MainLoop((w2) => { w2.poke(); });
  return w.run(&cb);
}
"#;
  let exit = drive_and_run_binary(&out_dir, vale, extra, /*borrow_check=*/ false);
  assert_eq!(exit, 7);
}

// With the borrow checker off, the churning forwarder compiles, links, and runs: the closure's `push`
// reaches the real window through the forwarder (exit 7 = one tick + 6). This is the shape NobiliaV
// can build today with `valen build --no-borrow-check`.
#[test]
fn drive_with_borrow_check_off_links_a_churning_lambda_forwarder_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub struct Frame {}
pub trait MainLoop {
    fn on_tick(&mut self, w: &mut Window, input: &Frame);
}
impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn push(&mut self) { self.ticks += 1; }
    pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {
        let frame = Frame {};
        cb.on_tick(self, &frame);
        self.ticks + 6
    }
}
"#);
  let exit = drive_and_run_binary(&out_dir, r#"
import noblike.Window;
import noblike.Frame;
import noblike.MainLoop;
struct MyCb<F> where func drop(F)void, func __call(&F, &Window mut, &Frame)void { f F; }
impl<F> MainLoop for MyCb<F>;
func on_tick<F, r', s'>(self &MyCb<F> in s, w &Window in r, input &Frame) mut(r) mut(s) {
  (&self.f)(w, input);
}
exported func main() int {
  w = Window.new();
  cb = MyCb((w2, input) => { w2.push(); });
  return w.run(&cb);
}
"#, extra, false);
  assert_eq!(exit, 7);
}

// The AUTO-GENERATED anon substruct for a `&mut`-signature trait runs → 7 with the borrow checker off —
// NobiliaV's real shape with NO hand-written `MyCb<F>` forwarder. `MainLoop((w2, input) => { w2.push();
// })` is handed straight to a `&mut self` / `&mut Window` trait, and the closure churns the window. The
// final file renders `&mut` so rustc accepts the impl; the borrow checker is off to step around the
// unbuilt churn enforcement, exactly as the hand-written churning forwarder above does. `run`'s
// `ticks + 6` return means one `push` → 7, so a 7 proves the churn reached the real window.
#[test]
fn drive_links_an_auto_generated_mut_forwarder_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub struct Frame {}
pub trait MainLoop {
    fn on_tick(&mut self, w: &mut Window, input: &Frame);
}
impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn push(&mut self) { self.ticks += 1; }
    pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {
        let frame = Frame {};
        cb.on_tick(self, &frame);
        self.ticks + 6
    }
}
"#);
  let vale = r#"
import noblike.Window;
import noblike.Frame;
import noblike.MainLoop;
exported func main() int {
  w = Window.new();
  cb = MainLoop((w2, input) => { w2.push(); });
  return w.run(&cb);
}
"#;
  let exit = drive_and_run_binary(&out_dir, vale, extra, /*borrow_check=*/ false);
  assert_eq!(exit, 7);
}

// The same program with the borrow checker on is rejected: the closure churns its window param and
// nothing declares that (the `mut` placeholder on the bound is not yet read by the checker). Pins the
// wall `--no-borrow-check` exists to step around.
//
// Ignored until symphony can check it: it panics in `groupify_function.rs` ("Expected placeholder,
// unexpected templata") on the concrete `&Window` in the forwarder's `__call` bound.
#[test]
#[ignore]
fn drive_with_borrow_check_on_rejects_the_churning_lambda_forwarder() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "noblike", r#"
pub struct Window { pub ticks: i32 }
pub struct Frame {}
pub trait MainLoop {
    fn on_tick(&mut self, w: &mut Window, input: &Frame);
}
impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn push(&mut self) { self.ticks += 1; }
    pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {
        let frame = Frame {};
        cb.on_tick(self, &frame);
        self.ticks + 6
    }
}
"#);
  match drive_prog(&out_dir, r#"
import noblike.Window;
import noblike.Frame;
import noblike.MainLoop;
struct MyCb<F> where func drop(F)void, func __call(&F, &Window mut, &Frame)void { f F; }
impl<F> MainLoop for MyCb<F>;
func on_tick<F, r', s'>(self &MyCb<F> in s, w &Window in r, input &Frame) mut(r) mut(s) {
  (&self.f)(w, input);
}
exported func main() int {
  w = Window.new();
  cb = MyCb((w2, input) => { w2.push(); });
  return w.run(&cb);
}
"#, extra, true, |_| {}) {
    Err(err) => assert!(err.contains("BorrowCheckError"), "err:\n{err}"),
    Ok(result) => panic!(
      "the borrow checker should reject the churning closure, but rustc exited {} with firings {:?}",
      result.rustc_exit, result.firings
    ),
  }
}

// With the borrow checker on, the undeclared churn is rejected by the producer gate: the importer
// gives `bump` a `mut` effect on its receiver's group, and `bump_it` declares none for `g`. The
// interop twin of producer_gate_tests' `test_undeclared_param_churn_rejected`; `drive` surfaces the
// typing failure as an `Err`, so nothing is emitted.
//
// Ignored until symphony can reject it: symphony accepts the program, since it has no check for an
// undeclared churn of a parameter's group (the old suite got this rejection from the experimental
// checker).
#[test]
#[ignore]
fn drive_rejects_an_undeclared_churn_of_an_imported_mut_method() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "counter", r#"
pub struct Counter { pub n: i32 }
impl Counter {
    pub fn new() -> Counter { Counter { n: 6 } }
    pub fn bump(&mut self) { self.n += 1; }
    pub fn get(&self) -> i32 { self.n }
}
"#);
  match drive_prog(&out_dir, r#"
import counter.Counter;
func bump_it<g'>(c &Counter in g) {
  c.bump();
}
exported func main() int {
  c = Counter.new();
  bump_it(&c);
  return c.get();
}
"#, extra, true, |_| {}) {
    Err(err) => assert!(err.contains("BorrowCheckError"), "err:\n{err}"),
    Ok(result) => panic!(
      "the borrow checker should reject the undeclared churn, but rustc exited {} with firings {:?}",
      result.rustc_exit, result.firings
    ),
  }
}

// The switch NobiliaV needs: with the borrow checker off, the same program compiles, links and runs,
// so rust interop can be exercised on a program the checker is not yet happy with. The only
// difference from the test above is the flag.
#[test]
fn drive_with_borrow_check_off_runs_an_undeclared_churn_to_exit_seven() {
  let scratch = Scratch::new();
  let out_dir = scratch.out_dir();
  let extra = build_inline_crate(&out_dir, "counter", r#"
pub struct Counter { pub n: i32 }
impl Counter {
    pub fn new() -> Counter { Counter { n: 6 } }
    pub fn bump(&mut self) { self.n += 1; }
    pub fn get(&self) -> i32 { self.n }
}
"#);
  let exit = drive_and_run_binary(&out_dir, r#"
import counter.Counter;
func bump_it<g'>(c &Counter in g) {
  c.bump();
}
exported func main() int {
  c = Counter.new();
  bump_it(&c);
  return c.get();
}
"#, extra, false);
  assert_eq!(exit, 7);
}
