// Rust interop tests that only horizon passes. The ones bifrost passes too are in the parent's
// `cases.rs`.
//
// Each test writes its Vale program inline, then does one of two things:
// - `typecheck*`: typechecks it against a real `TyCtxt` and asserts on the typed AST and the oracle log.
// - `drive_*`: drives it through bifrost's `drive` and runs the produced binary.
//
// Two rules shape what these assert on, both learned the hard way (plan §5):
//
//   - **Prefer the Vale program to carry the assertion.** `pick<int, bool>` returning `A` means a
//     swapped generic index yields `bool` where `int` belongs, and `main() int` stops typechecking.
//     That survives any refactor of how the compiler renders anything. Substring assertions against
//     `Debug` output broke twice in one day and neither break was a behaviour change.
//   - **The oracle log's one remaining job is vacuity** — proving the oracle was consulted at all,
//     which no source program can express, and which caught a compilation that silently built
//     nothing on its first run.

use std::fs::{read_dir, read_to_string};
use std::path::PathBuf;

use crate::collect_only_tnode;
use crate::collect_where_tnode;
use crate::interner::StrI;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::ast::expressions::FunctionCallTE;
use crate::typing::names::names::{
  FunctionNameT, FunctionTemplateNameT, INameT, IStructTemplateNameT, IdT, StructNameT,
  StructTemplateNameT,
};
use crate::typing::templata::templata::{ITemplataT, KindTemplataT};
use crate::typing::types::types::{BoolT, IntT, KindT, StructTT};
use crate::utils::code_hierarchy::PackageCoordinate;
// Tests run with the default borrow checker (symphony) on. The ones still calling a
// `_without_borrow_check` entry point hit a symphony gap in `groupify_function.rs` (traits and
// callbacks, a concrete kind in a lambda's bound); see the handoff.
use crate::typing::test::rust_interop::drive_helpers::{
  drive_and_run, drive_and_run_without_borrow_check, drive_lib, drive_lib_without_borrow_check,
  final_rust_source_without_borrow_check, typecheck, typecheck_without_borrow_check,
  typecheck_without_builtins,
};
use crate::typing::test::traverse::NodeRefT;
#[test]
fn calls_a_rust_free_function() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(20, 22);
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("add_two_numbers"), .. },
                        parameters: [KindT::Int(IntT { bits: 32 }), KindT::Int(IntT { bits: 32 })],
                        ..
                    }), ..
                },
                return_type: KindT::Int(IntT { bits: 32 }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Ensures a program can import a Rust trait: the oracle resolves and offers it, and the import
/// compiles even when unused. This is the first step toward a Vale struct implementing a Rust trait.
#[test]
fn imports_a_rust_trait() {
  let outcome = typecheck_without_borrow_check("horizon/main", r#"
import mycrate.Callback;
exported func main() int {
  return 0;
}
"#, |_| ());

  outcome.expect_compiled();
}

/// Ensures a Vale struct can implement an imported Rust trait: the impl resolves `on_call` against
/// the trait's projected abstract method, and the method is callable through a `&Callback` reference.
/// The program compiles only if the interface carries `on_call` and the impl's edge exists.
#[test]
fn a_struct_implements_a_rust_trait() {
  let outcome = typecheck_without_borrow_check("horizon/main", r#"
import mycrate.Callback;
struct MyCb { }
impl Callback for MyCb;
func on_call(self &MyCb) int {
  return 7;
}
func invoke(cb &Callback) int {
  return cb.on_call();
}
exported func main() int {
  c = MyCb();
  return invoke(&c);
}
"#, |_| ());

  outcome.expect_compiled();
}

/// A lambda handed to an imported Rust trait's anonymous-substruct constructor typechecks: the
/// anon-substruct macro must fire on the imported `Callback` trait, so `Callback({ 7 })` resolves to
/// a synthesized forwarder substruct with no hand-written struct/impl/override, and `on_call`
/// resolves through it. Without the macro firing on imported traits, the constructor `Callback(...)`
/// is never registered and this fails with `CouldntFindFunctionToCallT`.
#[test]
fn a_lambda_typechecks_against_a_rust_trait() {
  let outcome = typecheck_without_borrow_check("horizon/main", r#"
import mycrate.Callback;
exported func main() int {
  c = Callback({ 7 });
  return c.on_call();
}
"#, |_| ());

  outcome.expect_compiled();
}

/// The anon-substruct path for the `on_tick`-shaped trait (two imported-type borrow params): a lambda
/// handed to `Cb((x, y) => { x.touch(); })` typechecks with no hand-written forwarder. This is the real
/// NobiliaV target and the reason the anon macro's bound must carry value-position param types (the
/// Rust citizen borrows `&Alpha`/`&Beta`). Without the value-position param fix it fails to compile
/// (`evaluate_templex: can't substitute` on the bound); without the macro firing, the `Cb(..)`
/// constructor is absent (`CouldntFindFunctionToCallT`).
#[test]
fn a_lambda_typechecks_against_a_two_imported_param_trait() {
  let outcome = typecheck_without_borrow_check("horizon/two_imported_params", r#"
import mycrate.Alpha;
import mycrate.Beta;
import mycrate.Cb;
exported func main() int {
  a = Alpha.new();
  cb = Cb((x, y) => { x.touch(); });
  return a.run_cb(&cb);
}
"#, |_| ());

  outcome.expect_compiled();
}

/// The final-file generator projects the macro-synthesized anon substruct as a real Rust
/// `pub struct + impl` — the piece that cannot come from anything earlier, because the substruct exists
/// only after typing. Feeds the typed program of the `on_tick`-shaped case through the generator and
/// asserts the emitted Rust declares `Cb__anon` (the shared `anon_substruct_rust_name` mangling),
/// implements the imported `Cb` trait by its crate path, and renders the override
/// `go(&self, _p1: &Alpha, _p2: &Beta)` with a deferred body — the exact shape the second rustc pass
/// compiles and Valen's backend fills.
#[test]
fn final_file_projects_the_anon_substruct_for_an_imported_trait() {
  let outcome = final_rust_source_without_borrow_check(
    "horizon/two_imported_params",
    r#"
import mycrate.Alpha;
import mycrate.Beta;
import mycrate.Cb;
exported func main() int {
  a = Alpha.new();
  cb = Cb((x, y) => { x.touch(); });
  return a.run_cb(&cb);
}
"#,
  );
  let final_file = outcome.expect_compiled();
  assert!(final_file.contains("pub struct Cb__anon"), "final file:\n{final_file}");
  assert!(final_file.contains("::mycrate::Cb for Cb__anon"), "final file:\n{final_file}");
  assert!(
    final_file.contains("fn go(&self, _p1: &::mycrate::Alpha, _p2: &::mycrate::Beta)"),
    "final file:\n{final_file}"
  );
  assert!(final_file.contains("__ValeOpaque<"), "final file:\n{final_file}");
  assert!(final_file.contains("unreachable!()"), "final file:\n{final_file}");
}

/// The final file renders `&mut` for a **`&mut`-signature** trait's auto-generated anon substruct.
/// `on_tick`'s receiver and its `w` param are `&mut`, `input` is a shared `&` negative control —
/// NobiliaV's `on_tick(&mut self, w: &mut NobiliaWindow, input: &FrameInput)` shape. Mutability lives
/// on the abstract method's `mut(g)` effect clause, not the `KindT`, so this only passes when the
/// generator reads those effects off `CompilerOutputs`. Positional param names (`_p1`/`_p2`) because
/// the typed AST carries no source names.
#[test]
fn final_file_projects_mut_borrows_for_an_imported_trait() {
  let outcome = final_rust_source_without_borrow_check(
    "horizon/mut_callback",
    r#"
import mycrate.Window;
import mycrate.Frame;
import mycrate.MainLoop;
exported func main() int {
  w = Window.new();
  cb = MainLoop((win, inp) => { });
  return w.run(&cb);
}
"#,
  );
  let final_file = outcome.expect_compiled();
  assert!(final_file.contains("pub struct MainLoop__anon"), "final file:\n{final_file}");
  assert!(final_file.contains("::mycrate::MainLoop for MainLoop__anon"), "final file:\n{final_file}");
  assert!(
    final_file.contains("fn on_tick(&mut self, _p1: &mut ::mycrate::Window, _p2: &::mycrate::Frame)"),
    "final file:\n{final_file}"
  );
}

/// Ensures an `impl` of a Rust trait that provides no override for the trait's method is rejected —
/// the projected abstract `on_call` is enforced, so an impl missing it fails to compile. This guards
/// that the trait's method projection is real: without it, `impl Callback for MyCb` would compile
/// vacuously.
#[test]
fn a_trait_impl_missing_its_override_is_rejected() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Callback;
struct MyCb { }
impl Callback for MyCb;
func invoke(cb &Callback) int {
  return cb.on_call();
}
exported func main() int {
  c = MyCb();
  return invoke(&c);
}
"#, |_| ());

  assert!(
    outcome.expect_failure().is("CouldntFindOverrideT"),
    "an impl missing its trait-method override was accepted"
  );
}

/// Milestone (reverse direction): rustc's collector monomorphizes a generic Rust fn with a Valen
/// struct as its type argument (`run_callback::<MyCb>`) and, walking its body, discovers the Valen
/// trait-impl callback `<MyCb as Callback>::on_call`. Collector-driven (no run): asserts rustc drove
/// `__vale_main`, requested `run_callback`, discovered `on_call`, resolved everything, and codegen'd.
#[test]
fn rustc_discovers_a_valen_trait_impl_callback() {
  let run = drive_lib_without_borrow_check("horizon/rust_trait", r#"
import mycrate.Callback;
import mycrate.run_callback;
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
  let firings = &run.firings;
  assert!(
    firings.iter().any(|f| f.contains("__vale_main")),
    "per_instance_mir never fired on __vale_main; firings: {firings:?}"
  );
  // `run_callback::<MyCb>` must actually resolve — MyCb, a local Valen struct, converted to a rustc
  // type argument and the generic monomorphization reified — not decline as unconvertible.
  assert!(
    !firings.iter().any(|f| f.contains("ARGS-UNCONVERTIBLE") || f.contains("UNRESOLVED")),
    "run_callback::<MyCb> did not resolve (unconvertible/unresolved); firings: {firings:?}"
  );
  assert!(
    firings.iter().any(|f| f.contains("run_callback") && f.contains("=>") && !f.contains("UNCONVERTIBLE")),
    "run_callback::<MyCb> was not reified for rustc; firings: {firings:?}"
  );
  assert_eq!(
    run.rustc_exit, 0,
    "rustc did not complete codegen (exit {}); firings: {firings:?}",
    run.rustc_exit
  );
}

/// Milestone (reverse direction, tier 2): **Rust owns the call**, and a Valen method runs because Rust
/// called it. `run_callback::<MyCb>(&mmlcb)` is rustc's own generic fn; its `c.on_call()` dispatches
/// statically to `<MyCb as Callback>::on_call`, whose body Valen emits under rustc's mangled symbol
/// (single-symbol). The linked bin runs and returns 7 — `rustc_discovers_...` above only proved rustc
/// *reached* `on_call`; this proves the Valen body actually runs when Rust invokes it.
#[test]
fn rust_calls_back_a_valen_callback_returns_seven() {
  let run = drive_and_run_without_borrow_check("horizon/rust_trait", r#"
import mycrate.Callback;
import mycrate.run_callback;
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
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven callback bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 6 (reverse direction): a scalar argument crosses Rust->Valen. `run_adder::<MyAdder>(&a, 35)`
/// hands the i32 `35` inbound to Valen's `add`, which returns it; the linked bin exits 35. The
/// `&self`-only callback above proved static dispatch; this proves an inbound *value* arrives intact.
#[test]
fn a_valen_callback_takes_a_scalar_arg() {
  let run = drive_and_run_without_borrow_check("horizon/rust_callback_scalar", r#"
import mycrate.Adder;
import mycrate.run_adder;
struct MyAdder { }
impl Adder for MyAdder;
func add(self &MyAdder, n int) int {
  return n;
}
exported func main() int {
  a = MyAdder();
  return run_adder(&a, 35);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(35),
    "the driven scalar-arg callback bin did not exit 35 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 7 (reverse direction): a Rust borrow crosses inbound and the callback calls back out to Rust.
/// `run_ticker::<MyTicker>()` makes a `Counter` and hands `&Counter` to Valen's `on_tick`, which
/// returns `w.peek()` — an outbound Rust call on the received borrow. The linked bin exits 5.
#[test]
fn a_valen_callback_receives_a_rust_borrow() {
  let run = drive_and_run_without_borrow_check("horizon/rust_callback_borrow", r#"
import mycrate.Counter;
import mycrate.Ticker;
import mycrate.run_ticker;
struct MyTicker { }
impl Ticker for MyTicker;
func on_tick(self &MyTicker, w &Counter) int {
  return w.peek();
}
exported func main() int {
  t = MyTicker();
  return run_ticker(&t);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(5),
    "the driven borrow-arg callback bin did not exit 5 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 8 (reverse direction): a Rust struct crosses inbound **by value**. `run_summer::<MySummer>()`
/// makes a `Small { a: 3, b: 6 }` and hands it inbound by value to Valen's `on_sum`, which returns
/// `s.sum()` (3 + 6). The linked bin exits 9 — a small aggregate reassembled from its two registers.
#[test]
fn a_valen_callback_receives_a_rust_struct_by_value() {
  let run = drive_and_run_without_borrow_check("horizon/rust_callback_byval", r#"
import mycrate.Small;
import mycrate.Summer;
import mycrate.run_summer;
struct MySummer { }
impl Summer for MySummer;
func on_sum(self &MySummer, s Small) int {
  return s.sum();
}
exported func main() int {
  m = MySummer();
  return run_summer(&m);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(9),
    "the driven byval-struct callback bin did not exit 9 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 8c (forward direction): a `Pair` **return** — Vale calls `Small2.new(3,6)` (a Rust assoc fn
/// returning `{i32,i32}` by value), binds it, and reads `s.sum()`. The struct returns in two registers
/// and is reassembled Vale-side. Exits 9.
#[test]
fn vale_receives_a_rust_pair_return() {
  let run = drive_and_run("horizon/pair_forward", r#"
import mycrate.Small2;
exported func main() int {
  s = Small2.new(3, 6);
  return s.sum();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(9),
    "the driven pair-return bin did not exit 9 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 8b (forward direction): a `Pair` **argument** — Vale passes a small `{i32,i32}` struct by
/// value into a Rust free function (`add_small(s)`). The struct crosses outbound in two registers.
/// Exits 9.
#[test]
fn vale_passes_a_rust_pair_arg() {
  let run = drive_and_run("horizon/pair_forward", r#"
import mycrate.Small2;
import mycrate.add_small;
exported func main() int {
  s = Small2.new(3, 6);
  return add_small(^s);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(9),
    "the driven pair-arg bin did not exit 9 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 8d (reverse direction): a callback **returns** a Rust struct by value. Valen's `make` returns
/// `Small.new(3,6)` and Rust's `run_maker::<MyMaker>()` reads `c.make().sum()`. The struct crosses
/// Valen -> Rust in two registers (an inbound Pair return). Exits 9.
#[test]
fn a_valen_callback_returns_a_rust_struct_by_value() {
  let run = drive_and_run_without_borrow_check("horizon/rust_callback_retpair", r#"
import mycrate.Small;
import mycrate.Maker;
import mycrate.run_maker;
struct MyMaker { }
impl Maker for MyMaker;
func make(self &MyMaker) Small {
  return Small.new(3, 6);
}
exported func main() int {
  m = MyMaker();
  return run_maker(&m);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(9),
    "the driven retpair callback bin did not exit 9 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Slice 9 (the capstone): **Rust owns a loop** calling the Valen callback N times. `main_loop::<MyCb>`
/// loops `i = 0..5` calling `c.on_tick(i)` (which returns `i`) and sums the returns; the linked bin
/// exits 10 (0 + 1 + 2 + 3 + 4). Proves the callback survives repeated re-entry with a fresh scalar
/// each iteration — the NobiliaV `main_loop` shape where Rust drives the loop and calls Valen per frame.
#[test]
fn rust_owns_a_loop_calling_the_callback() {
  let run = drive_and_run_without_borrow_check("horizon/rust_main_loop", r#"
import mycrate.Looper;
import mycrate.main_loop;
struct MyCb { }
impl Looper for MyCb;
func on_tick(self &MyCb, i int) int {
  return i;
}
exported func main() int {
  cb = MyCb();
  return main_loop(&cb);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(10),
    "the driven main-loop bin did not exit 10 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// An imported trait whose method takes TWO imported-type borrow params and returns
/// void, invoked through a generic *method* caller (`a.run_cb::<MyCb>(&cb)`).
#[test]
fn a_trait_method_with_two_imported_params() {
  let run = drive_and_run_without_borrow_check("horizon/two_imported_params", r#"
import mycrate.Alpha;
import mycrate.Beta;
import mycrate.Cb;
struct MyCb { }
impl Cb for MyCb;
func go(self &MyCb, x &Alpha, y &Beta) {
  x.touch();
}
exported func main() int {
  a = Alpha.new();
  cb = MyCb();
  return a.run_cb(&cb);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven two-imported-params callback bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// A callback receives a zero-sized imported struct by value (`on(self, z Zst, n
/// int)`), an `Ignore` inbound arg that crosses with no C param. The inbound wrapper must synthesize
/// an empty Vale value for `z` while not messing up the following scalar `n`.
#[test]
fn a_valen_callback_receives_a_zst_by_value() {
  let run = drive_and_run_without_borrow_check("horizon/zst_callback_arg", r#"
import mycrate.Zst;
import mycrate.Cb;
import mycrate.run_cb;
struct MyCb { }
impl Cb for MyCb;
func on(self &MyCb, z Zst, n int) int { return n; }
exported func main() int {
  cb = MyCb();
  return run_cb(&cb);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven zst-callback-arg bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// A callback returns a zero-sized imported struct by value (`make(self) Zst`), an
/// `Ignore` return that crosses nothing. The inbound wrapper's LLVM return type is void, so it must
/// return void and let the ZST value evaporate.
#[test]
fn a_valen_callback_returns_a_zst() {
  let run = drive_and_run_without_borrow_check("horizon/zst_callback_return", r#"
import mycrate.Zst;
import mycrate.Maker;
import mycrate.run_maker;
struct MyMaker { }
impl Maker for MyMaker;
func make(self &MyMaker) Zst { return Zst.new(); }
exported func main() int {
  m = MyMaker();
  return run_maker(&m);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(8),
    "the driven zst-callback-return bin did not exit 8 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

#[test]
fn calls_a_zero_arg_rust_function() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.seven;
exported func main() int {
  return seven();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("seven"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

#[test]
fn passes_and_returns_a_bool() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.is_positive;
import mycrate.to_int;
exported func main() int {
  return to_int(is_positive(5));
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("is_positive"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("to_int"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A Rust citizen in argument position of a free function — a different lowering path from return
/// position, and a different discovery path from a method.
#[test]
fn takes_a_rust_type_as_a_parameter() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.value_of_counter;
import mycrate.Counter;
exported func main() int {
  return value_of_counter(make_counter());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("value_of_counter"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Counter"), ..
                                    }), ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The same citizen identity on both sides of one signature.
#[test]
fn takes_and_returns_a_rust_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.bump;
import mycrate.value_of_counter;
import mycrate.Counter;
exported func main() int {
  return value_of_counter(bump(make_counter()));
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("bump"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Counter"), ..
                                    }), ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Counter"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The mirror canary for the generic index mapping. Together with
/// `reads_a_generic_signature_structurally`, no single wrong mapping satisfies both.
#[test]
fn binds_the_second_generic_parameter() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.pick_second;
import mycrate.seven;
exported func main() int {
  return pick_second<bool, int>(true, seven());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("pick_second"), .. }, ..
                    }), ..
                },
                return_type: KindT::Int(IntT { bits: 32 }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A floor, not a canary: `id<T>` passes under any index mapping, so it says only that
/// substitution happens at all.
#[test]
fn instantiates_a_generic_at_one_parameter() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.id;
exported func main() int {
  return id<int>(9);
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("id"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A generic function whose parameter is the generic type applied to its own parameter.
///
/// Isolates the backward inference that a generic type's `drop` also needs, away from drop.
#[test]
fn calls_a_generic_function_taking_a_generic_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_holder;
import mycrate.holder_ignore;
import mycrate.Holder;
exported func main() int {
  return holder_ignore<int>(make_holder());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("holder_ignore"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Holder"), ..
                                    }),
                                    template_args: [ITemplataT::Kind(KindTemplataT {
                                        kind: KindT::Int(IntT { bits: 32 }),
                                    })], ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A Rust citizen as a **generic argument**, rather than as a parameter or return type.
#[test]
fn instantiates_a_generic_at_a_rust_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.id;
import mycrate.make_counter;
import mycrate.value_of_counter;
import mycrate.Counter;
exported func main() int {
  return value_of_counter(id<Counter>(make_counter()));
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("id"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Counter"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An associated function with no receiver — the `Vec::new` shape — is an ordinary declaration
/// that happens to take no parameters.
#[test]
fn calls_an_associated_function_with_no_receiver() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Counter;
import mycrate.value_of_counter;
exported func main() int {
  return value_of_counter(Counter.new());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("new"), .. },
                        parameters: [], ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Counter"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An associated function whose impl **fixes** one of the type's parameters (`impl<T> Boxed<T, Fixed>`,
/// the `Vec::new` shape), called with the generic on the method: `Boxed.new<int>()`. `new` ranges over
/// one generic, so naming one type argument does not trip the resolver's container-vs-function rune
/// subtraction (the `1 - 2` underflow the over-specified `Boxed<int, Fixed>.new()` form would hit).
#[test]
fn calls_an_assoc_fn_with_a_fixed_impl_param_method_generic() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Boxed;
import mycrate.Fixed;
import mycrate.boxed_ignore;
exported func main() int {
  return boxed_ignore<int>(Boxed.new<int>());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("new"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The same fixed-impl-param associated function, called with the generic on the type:
/// `Boxed<int>.new()` — the `Vec<int>.with_capacity()` form.
#[test]
fn calls_an_assoc_fn_with_a_fixed_impl_param_type_generic() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Boxed;
import mycrate.Fixed;
import mycrate.boxed_ignore;
exported func main() int {
  return boxed_ignore<int>(Boxed<int>.new());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("new"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A Rust `usize` imported as the Vale `usize` primitive (`Vec::len`'s shape): `some_size() -> usize`
/// produces one, `consume_usize(usize) -> i32` takes it. `usize` used to decline as `UnsignedInteger`.
#[test]
fn imports_usize_as_a_primitive() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.some_size;
import mycrate.consume_usize;
exported func main() int {
  return consume_usize(some_size());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("some_size"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("consume_usize"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A Rust enum imports as an opaque sealed interface, and its inherent method resolves — the opaque
/// tier's payoff (a method without variants, the `Option::unwrap` shape).
#[test]
fn calls_a_method_on_an_imported_enum() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Shade;
import mycrate.make_shade;
exported func main() int {
  return (make_shade()).level();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("level"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An imported enum bound to a local and never consumed gets a scope-end drop — an interface's drop,
/// synthesized like a struct's.
#[test]
fn an_imported_enum_bound_to_a_local_gets_a_scope_end_drop() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Shade;
import mycrate.make_shade;
exported func main() int {
  s = make_shade();
  return 4;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The capstone: real `std::vec::Vec` + `std::alloc::Global` from the actual `alloc` crate,
/// `Vec.new<int>()` bound to a local with a scope-end drop, typechecked against live rustc.
#[test]
fn imports_real_vec_and_constructs_it() {
  let outcome = typecheck("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
exported func main() int {
  v = Vec.new<int>();
  return 0;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("alloc"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("new"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Real `Vec` `&mut self` method: `v.push(42)`.
#[test]
fn calls_push_on_a_real_vec() {
  let outcome = typecheck("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
exported func main() int {
  v = Vec.new<int>();
  v.push(42);
  return 0;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("alloc"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("push"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Real `Vec` `&self` method returning `usize`: `v.len()`.
#[test]
fn calls_len_on_a_real_vec() {
  let outcome = typecheck("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
import mycrate.consume_usize;
exported func main() int {
  v = Vec.new<int>();
  return consume_usize(v.len());
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("alloc"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("len"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The capstone: real `Vec::pop() -> Option<int>` then `Option::unwrap() -> int` — a struct method
/// returning a real `std` enum, whose inherent method hands back the element.
#[test]
fn calls_pop_then_unwrap_on_a_real_vec() {
  let outcome = typecheck("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
exported func main() int {
  v = Vec.new<int>();
  return (v.pop()).unwrap();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("alloc"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("pop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("core"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("unwrap"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A struct wrapping a `HashMap`, used through methods: build a `Domino`, add a `Glyph` via a `&mut self`
/// method, read one back via a `&self` method returning a **borrow** (`&Glyph`) bound to a local, then
/// read the glyph's field through an accessor. The borrow-return bound to a local (`d_ref`) is the new
/// mechanic — earlier cases proved borrow *receivers*, never a borrow *return* of a citizen held in a
/// local. `location` returning `int32` is main's observable (the stored glyph's location, 7).
#[test]
fn a_struct_wrapping_a_hashmap_is_used_through_methods() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  d_ref = d.get_glyph(7);
  return d_ref.location();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("add_glyph"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get_glyph"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("location"), .. }, ..
                    }), ..
                },
                return_type: KindT::Int(IntT { bits: 32 }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An imported `fn nudge(a: &mut Counter, b: &Counter)` becomes `func nudge<g0', g1'>(a &Counter in g0,
/// b &Counter in g1) mut(g0)`; calling `nudge(&s, &s)` aliases `s` into the mutated group `g0` and the
/// disjoint group `g1`, so the borrow checker must reject it.
// Ignored until symphony has a disjoint-group aliasing check: only the experimental checker reports
// `AliasingIntoDisjointMutGroups`, and with no checker the program compiles.
#[test]
#[ignore]
fn a_mut_borrow_aliasing_a_shared_borrow_of_one_local_is_rejected() {
  // Mirroring Rust `&mut` into a `mut(g)` group makes a callee's disjoint-group assumption checkable:
  // the same local into a mutated group and a distinct group is an aliasing violation.
  // The borrow checker is off: symphony has no disjoint-group aliasing check yet (only the experimental
  // checker reports `AliasingIntoDisjointMutGroups`).
  let outcome = typecheck_without_borrow_check("horizon/main", r#"
import mycrate.Counter;
import mycrate.nudge;
exported func main() int {
  s = Counter.new();
  return nudge(&s, &s);
}
"#, |_| ());
  assert!(outcome.expect_failure().is("BorrowCheckError"));
}

/// The disjoint counterpart compiles: distinct locals into `nudge`'s mutated and shared groups do not
/// alias, so the group mirroring must not reject them.
#[test]
fn a_mut_borrow_and_a_shared_borrow_of_distinct_locals_compiles() {
  // Guards against over-rejection: emitting groups must flag only aliasing calls, not every two-borrow one.
  // No builtins, so the borrow checker can stay on: the program uses no Valen operators.
  typecheck_without_builtins("horizon/main", r#"
import mycrate.Counter;
import mycrate.nudge;
exported func main() int {
  s = Counter.new();
  t = Counter.new();
  return nudge(&s, &t);
}
"#, |_| ())
    .expect_compiled();
}

/// A Rust signature sharing one lifetime across two parameters is declined, not imported with a guess.
/// Faithfully mirroring it needs lifetime decoding Vale doesn't do yet, so calling it is a compile error.
#[test]
fn calling_a_shared_parameter_lifetime_import_is_a_compile_error() {
  // Shared-across-parameters lifetimes are rejected rather than assumed disjoint (what per-parameter
  // groups would assume) — the one case where we refuse to guess until real decoding lands.
  let outcome = typecheck("horizon/main", r#"
import mycrate.Counter;
import mycrate.tie;
exported func main() int {
  s = Counter.new();
  t = Counter.new();
  return tie(&s, &t);
}
"#, |_| ());
  assert!(outcome.expect_failure().is("CouldNotPostparseFunction"));
}

/// Milestone M (free-function case): rustc's mono collector drives *our monomorphizer* end to end.
/// Compiling the stub to completion with the `per_instance_mir` override fires our provider on the
/// `__vale_main` root; the provider seeds that export, drains the instantiator, collects the Rust
/// functions `main` transitively calls, resolves each to a rustc `DefId`, and hands rustc a
/// `ReifyFnPointer` body naming them (which is what queues them for codegen). Reaching here proves
/// the whole loop: rustc drives us, the drive finds the Rust leaf `add_two_numbers`, and it resolves
/// back to a real rustc item (the firing records `<path> => <resolved def path>`).
#[test]
fn rustc_collector_drives_our_monomorphizer() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(20, 22);
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("__vale_main")),
    "per_instance_mir never fired on __vale_main; firings: {firings:?}"
  );
  assert!(
    firings.iter().any(|f| f.contains("add_two_numbers[] =>")),
    "the drive did not collect + resolve a Rust request for add_two_numbers; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve to a DefId; firings: {firings:?}"
  );
}

/// Milestone M, generic callee: the same driven loop, but `main` calls a *generic* Rust function
/// `id<int>(9)`. The request now carries a type argument, so the provider must convert the Vale
/// templata `int` to the rustc `Ty` `i32` and build the callee's `GenericArgs` before reifying
/// `id::<i32>`. Proves the templata → rustc-`Ty` bridge for a primitive type argument.
#[test]
fn rustc_collector_drives_a_generic_rust_callee() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.id;
exported func main() int {
  return id<int>(9);
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("mycrate.id[i32] =>")),
    "id<int> did not convert its type arg to i32 and resolve; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "the generic id<int> request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, generic callee at a Rust type: `main` calls `id<Counter>(make_counter())`, so the
/// generic function's type argument is itself an imported Rust type rather than a primitive. The
/// provider must lower the Vale `Counter` kind to the rustc `Adt` type and build `id::<Counter>`'s
/// args. Proves the templata → rustc-`Ty` bridge for a Rust-backed (non-generic) type argument.
#[test]
fn rustc_collector_drives_a_generic_at_a_rust_type() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.id;
import mycrate.make_counter;
import mycrate.value_of_counter;
import mycrate.Counter;
exported func main() int {
  return value_of_counter(id<Counter>(make_counter()));
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("mycrate.id[") && f.contains("Counter")),
    "id<Counter> did not convert its Rust-type arg and resolve; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, scope-end drop: `s = make_shade(); return 4;` binds an imported enum to a local and
/// never consumes it, so it takes a synthesized scope-end drop. An imported type has no Rust `Drop`
/// to resolve to, so the provider maps the drop to a generic `__vale_drop<T>` shim (arch §15.7).
// Ignored at the architect's request, to revisit once the rest of horizon works: the backend aborts
// the whole test binary on it (`coerceExternReturn`, `externs.cpp:70`, has no `DirectInt` arm for an
// enum returned by value). The old suite never emitted for this test.
#[test]
#[ignore]
fn rustc_collector_drives_a_scope_end_drop() {
  let firings = drive_lib_without_borrow_check("horizon/main", r#"
import mycrate.Shade;
import mycrate.make_shade;
exported func main() int {
  s = make_shade();
  return 4;
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("drop => __vale_drop") && f.contains("(drop shim)")),
    "the scope-end drop did not map to the __vale_drop shim; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, generic associated function: `b = Boxed<int>.new()`. `Boxed<T, Fixed>::new` is a
/// generic assoc fn whose impl pins the second type param to `Fixed`, so the callee's args must be
/// reconstructed from the owner's type args plus the impl-pinned param. The bound-and-dropped local
/// also drops the generic `Boxed<int, Fixed>`.
#[test]
fn rustc_collector_drives_a_generic_assoc_function() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.Boxed;
import mycrate.Fixed;
exported func main() int {
  b = Boxed<int>.new();
  return 5;
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("new =>") && f.contains("(assoc)")),
    "Boxed<int>::new did not resolve as a generic assoc fn; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a generic assoc fn request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, real `std` `Vec`: `v = Vec.new<int>()`. `Vec::new` is in `impl<T> Vec<T, Global>`,
/// and `Global` is a *default* type param, so the dropped `Vec<int, Global>` carries an arg Vale never
/// names — the provider must fill the defaulted allocator param when it cannot from the Vale args.
#[test]
fn rustc_collector_drives_real_vec_new() {
  let firings = drive_lib("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
exported func main() int {
  v = Vec.new<int>();
  return 0;
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("vec.new =>") && f.contains("(assoc)")),
    "Vec::new did not resolve as a generic assoc fn; firings: {firings:?}"
  );
  assert!(
    firings.iter().any(|f| f.contains("__vale_drop")),
    "the Vec<int, Global> drop did not map to the shim; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a real-Vec request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, the composed domino case: opaque struct wrapping a `HashMap`, driven through
/// `Domino.new()` / `Glyph.new()` (associated functions), `&mut self` add, `&self` borrow-return get,
/// a field accessor, and scope-end drops. The end-to-end target of the driven path.
#[test]
fn rustc_collector_drives_the_domino_case() {
  let run = drive_lib("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  d_ref = d.get_glyph(7);
  return d_ref.location();
}
"#);
  let firings = &run.firings;
  // Every callee shape composed in one program resolves: associated functions, methods (incl.
  // `&mut self`/`&self`), a field accessor, and the scope-end drop shim.
  assert!(
    firings.iter().any(|f| f.contains("(assoc)"))
      && firings.iter().any(|f| f.contains("(method)"))
      && firings.iter().any(|f| f.contains("(drop shim)")),
    "the domino case did not exercise assoc/method/drop resolution; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request in the domino case failed to resolve; firings: {firings:?}"
  );
  // rustc drove all the way through codegen without erroring on any reified leaf. This is the
  // stronger "the frontend half is real" signal: every `(DefId, GenericArgs)` we handed back was
  // valid enough for rustc to monomorphize and codegen. (It does not run the program — the real
  // Vale bodies await the backend's `fill_extra_modules`.)
  assert_eq!(
    run.rustc_exit, 0,
    "rustc did not complete codegen on the domino case (exit {}); firings: {firings:?}",
    run.rustc_exit
  );
}

/// The `fill_extra_modules` codegen hook fires, and it lowers the Vale program and emits its bodies
/// into a module it hands to rustc: `consumer_fill_modules` packages the instantiator's outputs,
/// lowers them through `populate_metal_cache`, mints a `ModuleLlvm` and hands its `(context, module)`
/// to the C++ backend, whose `LLVMVerifyModule` runs on the Vale IR in that module. rc 0 = emitted
/// and verified. A lib crate, so not linked or run.
#[test]
fn rustc_codegen_emits_vale_bodies_into_borrowed_module() {
  let run = drive_lib("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(20, 22);
}
"#);
  assert!(
    run.firings.iter().any(|f| f.contains("consumer_fill_modules emitted rc=0")),
    "the backend did not emit + verify Vale IR into rustc's borrowed module; firings: {:?}",
    run.firings
  );
  assert_eq!(
    run.rustc_exit, 0,
    "rustc did not complete codegen after the borrowed emit (exit {}); firings: {:?}",
    run.rustc_exit, run.firings
  );
}

/// Stage 3 (tier 2): the whole round trip. Drive rustc to a linked bin, emit the Vale bodies into it,
/// run the executable, and assert it exits with what `main` returns. `seven()` (`return seven();`) is
/// the simplest real Rust call — zero args, scalar `i32`, so Rust ABI == C ABI. This is the first case
/// that observes a *value*, not just that emission verified.
#[test]
fn rustc_driven_bin_links_and_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.seven;
exported func main() int {
  return seven();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Stage 3, the final goal: a real **two-argument** Rust free function. `add_two_numbers(20, 22)`
/// passes two scalar `i32`s across the boundary to rustc's own `add_two_numbers`, linked and run,
/// asserting the process exits 42. This is the canonical driven case (the Stage-1/2 tests emit it),
/// now taken all the way to a running binary — a Vale program calling real Rust, end to end.
#[test]
fn rustc_driven_bin_links_and_returns_from_add_two_numbers() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(20, 22);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

// Exercises structs with i64 in them, across the boundary.
#[test]
fn rustc_driven_bin_i64_struct_method_returns_42() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Delta;
exported func main() i64 {
  d = Delta.seconds(42i64);
  return d.num_seconds();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven i64-struct bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

// Exercises calling a Rust function with `#[track_caller]`, see @TCHAPZ.
#[test]
fn rustc_driven_bin_track_caller_returns_42() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.tracked_sum;
exported func main() int {
  return tracked_sum(20, 22);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven track_caller bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// The goal (tier 2): the full domino case, `d = Domino.new(); d.add_glyph(Glyph.new(7)); d_ref =
/// d.get_glyph(7); return d_ref.location();`, linked and run, returns 7. It protects every ABI mode at
/// once, each sourced from rustc (`tcx.layout_of` for sizes, `tcx.fn_abi_of_instance` for conventions):
/// - `Indirect`: the 48-byte `Domino` returned via `sret`, with the aarch64 `sret` attribute.
/// - `DirectPtr`: the `&mut self`/`&self` receivers and the `&Glyph` return, as pointers.
/// - `DirectInt`: `Glyph` and `i32` in registers.
/// - `Ignore`: the scope-end `drop_in_place` of `d`, its owned value spilled to a pointer.
#[test]
fn rustc_driven_bin_domino_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  d_ref = d.get_glyph(7);
  return d_ref.location();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven domino bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Passes a large struct BY VALUE into a Rust free function: `domino_size(d)` moves a 48-byte `Domino`,
/// which rustc classifies `PassMode::Indirect`, so it must cross as LLVM `byval` (a pointer to a
/// caller-owned copy, ownership moved to the callee). One glyph is inserted before the move, so it
/// returns 1. This is the argument mirror of the sret return the domino case already exercises.
#[test]
fn rustc_driven_bin_domino_byval_arg_returns_one() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
import mycrate.domino_size;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  return domino_size(^d);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(1),
    "the driven byval-arg bin did not exit 1 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Pins the byval attribute index when the byval argument sits BEHIND an sret return.
/// `add_and_return(^d, 7)` moves a `Domino` in by value and returns a `Domino` by value (sret), so the
/// byval argument is physical parameter 1 (the sret out-pointer is 0); a byval attribute placed by
/// logical argument index would land on the sret pointer. Returns 7.
#[test]
fn rustc_driven_bin_domino_byval_arg_with_sret_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
import mycrate.add_and_return;
exported func main() int {
  d = Domino.new();
  d2 = add_and_return(^d, 7);
  d_ref = d2.get_glyph(7);
  return d_ref.location();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven byval-arg-with-sret bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// A Rust function returns an 8-byte struct by value — rustc `PassMode::Cast` crossing as a single
/// `i64` (count 1). Vale reassembles the `Small8` from the `i64` and reads field `a` (=6). PieceId's
/// return shape.
#[test]
fn rustc_driven_bin_small8_cast_return_returns_six() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Small8;
import mycrate.make_small;
exported func main() int {
  s = make_small(6, 1, 2);
  return s.small_a();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(6),
    "the driven small8 cast-return bin did not exit 6 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// A Vale program passes an 8-byte struct by value into a Rust function — rustc `PassMode::Cast` as a
/// single `i64`, alongside a scalar arg. `small_plus(^s, 4)` returns `s.a + 4` = 10. The Cast argument
/// direction (`pack_id`'s shape).
#[test]
fn rustc_driven_bin_small8_cast_arg_returns_ten() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.Small8;
import mycrate.make_small;
import mycrate.small_plus;
exported func main() int {
  s = make_small(6, 1, 2);
  return small_plus(^s, 4);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(10),
    "the driven small8 cast-arg bin did not exit 10 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Ladder rung 1 (tier 2): the first case that runs a non-scalar aggregate across the boundary,
/// `(make_counter()).get()`, which returns and consumes `Counter{i32}` by value, returns 7. Two things
/// must hold: the struct-layout map sizes `translateType(Counter)` to a real `[1 x i32]`, and the
/// extern-abi map crosses `Counter` as `DirectInt(32)`, reinterpreting the value and its register integer.
#[test]
fn rustc_driven_bin_method_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return (make_counter()).get();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Ladder rung 2 (tier 2): `c = make_counter(); return c.peek();` → 7, linked and run. Adds two ABI
/// modes over rung 1: the `&self` borrow receiver crosses as a real pointer (`DirectPtr`, a pointer-
/// scalar layout, not reinterpreted as an integer), and the scope-end drop of `c` has a unit return
/// (`Ignore`). Sizing of `Counter` is shared with rung 1.
#[test]
fn rustc_driven_bin_borrow_self_method_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  c = make_counter();
  return c.peek();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Interop autoderef, end to end: `s = make_sheath(); return s.read();` → 7, linked and run. `read`
/// lives on `Core` (`Sheath`'s `Deref::Target`), so the call resolves only through a callsite rewrite
/// to `read(deref(s))`, and BOTH leaves must materialize and run: `deref` (resolved via the `Deref`
/// impl) crossing `&Sheath -> &Core`, then `read` crossing `&Core -> i32`.
#[test]
fn rustc_driven_bin_deref_reached_method_returns_seven() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.make_sheath;
import mycrate.Sheath;
exported func main() int {
  s = make_sheath();
  return s.read();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(7),
    "the driven deref-reached-method bin did not exit 7 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Milestone M, borrow-receiver method: `c = make_counter(); return c.peek();`. `peek(&self)` takes a
/// borrow receiver, so the request's first parameter is a borrow-wrapped `Counter` rather than a bare
/// one; the provider must peel the reference to find the owning type. `c` also takes a scope-end drop.
#[test]
fn rustc_collector_drives_a_borrow_receiver_method() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  c = make_counter();
  return c.peek();
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("peek =>") && f.contains("(method)")),
    "peek(&self) did not resolve through its borrow receiver; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// Autoderef (single-step, shared, interop-scoped): `read` is NOT an inherent method of `Sheath` — it
/// lives on `Core`, `Sheath`'s `Deref::Target`. So `s.read()` resolves only if the importer follows
/// `Sheath`'s shared `Deref<Target=Core>` and the callsite rewrites the receiver to `deref(s)`. Isolated
/// from the real-`Vec::get` blockers (sized named target, scalar return, no indexing).
#[test]
fn rustc_collector_drives_a_deref_reached_method() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.make_sheath;
import mycrate.Sheath;
exported func main() int {
  s = make_sheath();
  return s.read();
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("read =>") && f.contains("(method)")),
    "read() did not resolve through Sheath's Deref target; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, method callee: `main` calls a Rust method `(make_counter()).get()`. A method is not
/// a crate-qualified free function — `get` lives in `Counter`'s inherent impl — so the provider must
/// resolve it through the receiver type rather than a module path. By-value `get` consumes its
/// receiver, so there is no scope-end drop; this isolates method resolution from drop synthesis.
#[test]
fn rustc_collector_drives_a_method_callee() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return (make_counter()).get();
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("get =>") && f.contains("(method)")),
    "the method call get() did not resolve through the receiver type; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// Milestone M, multi-parameter generic: `main` calls `pick<int, bool>(...)`, so the callee has two
/// type args and the provider must fill them in declaration order (`A = i32`, `B = bool`). `pick` is
/// the ordering canary — a swap would produce `[bool, i32]` and fail here.
#[test]
fn rustc_collector_drives_a_multi_param_generic() {
  let firings = drive_lib("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.pick;
exported func main() int {
  return pick<int, bool>(add_two_numbers(10, 5), true);
}
"#).firings;
  assert!(
    firings.iter().any(|f| f.contains("mycrate.pick[i32, bool] =>")),
    "pick<int, bool> did not convert both type args in order; firings: {firings:?}"
  );
  assert!(
    !firings.iter().any(|f| f.contains("UNRESOLVED")),
    "a Rust request failed to resolve; firings: {firings:?}"
  );
}

/// A two-parameter generic value from an associated function, bound to a local and dropped at scope
/// end — the real `let v = Vec<int, Global>.new();` shape. The generated drop names no type argument, so
/// `T` must come from the value.
#[test]
fn a_generic_assoc_result_bound_to_a_local_gets_a_scope_end_drop() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Boxed;
import mycrate.Fixed;
exported func main() int {
  b = Boxed<int>.new();
  return 5;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A method on a generic type whose signature names the type's own parameter (`into_value(self) -> T`).
/// `T` is inherited from `impl<T> Holder<T>`, not declared by the method — the case that used to decline
/// as `InheritedParameter` before the oracle reported parent-inclusive generic params for methods.
#[test]
fn calls_a_method_naming_the_types_generic() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Holder;
import mycrate.make_holder;
exported func main() int {
  return (make_holder()).into_value();
}
"#, |_| ());
  outcome.expect_compiled();
}

/// Method discovery is a list, not a lucky single — and it is lazy per method.
#[test]
fn calls_two_methods_on_one_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  x = (make_counter()).get();
  return (make_counter()).doubled();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("doubled"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A method carrying its own type parameter, on top of the container's.
#[test]
fn calls_a_generic_method() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return (make_counter()).or_else<int>(19);
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("or_else"), .. }, ..
                    }), ..
                },
                return_type: KindT::Int(IntT { bits: 32 }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Excess type arguments do not resolve — three named against `pick<A, B>`'s two slots.
#[test]
fn wrong_generic_arity_does_not_resolve() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.pick;
exported func main() int {
  return pick<int, bool, int>(3, true);
}
"#, |_| ());

  assert!(outcome.expect_failure().is("CouldntFindFunctionToCallT"));
}

/// A Vale function and a Rust function sharing a name do **not** collide — candidate collection is
/// plural, so the outcome is a designed error rather than the panic a *type*-name collision gives.
#[test]
fn a_vale_function_and_a_rust_function_with_the_same_name() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(1, 2);
}
func add_two_numbers(a int, b int) int {
  return 99;
}
"#, |_| ());

  assert!(outcome.expect_failure().is("CouldntNarrowDownCandidates"));
}

/// A generic Rust function is read **structurally** — parameters intact, not collapsed to one
/// instantiation. This is the thing the previous design could not express at all, and the reason
/// the arc pivoted.
#[test]
fn reads_a_generic_signature_structurally() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.pick;
exported func main() int {
  return pick<int, bool>(add_two_numbers(10, 5), true);
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("pick"), .. }, ..
                    }), ..
                },
                return_type: KindT::Int(IntT { bits: 32 }),
            }, ..
        }) => Some(())
    );
  });

  // The strong half of this assertion is that the program compiled at all: it calls
  // `pick<int, bool>` and returns the result from `main() int`, so binding `A` to the wrong slot
  // yields `bool` where `int` belongs and fails to resolve. `id<T>(x: T) -> T` would pass under
  // either mapping and prove nothing.
  outcome.expect_compiled();
}

/// A Rust type reaches Vale by inference from a signature — never by name — and its method lives in
/// the type's outer environment, resolved via the receiver when `v.get()` desugars to `get(v)`.
#[test]
fn calls_a_method_on_a_rust_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return (make_counter()).get();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_counter"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Counter"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Counter"), ..
                                    }), ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A `&self` (borrow-receiver) method called on a local. A local read is a `BorrowRef`, so this only
/// resolves if a borrow receiver matches `&self` — the shape every real `Vec::len`/`push` takes.
#[test]
fn calls_a_borrow_self_method_on_a_local() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  c = make_counter();
  return c.peek();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("peek"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A generic Rust value bound to a local and never consumed needs a scope-end drop.
#[test]
fn a_generic_rust_type_gets_a_scope_end_drop() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_holder;
import mycrate.Holder;
exported func main() int {
  h = make_holder();
  return 17;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Hand-written Vale naming a Rust type in a parameter and calling a method on it.
#[test]
fn vale_source_calls_a_method_on_a_named_rust_parameter() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return value_of(make_counter());
}
func value_of(c Counter) int {
  return (^c).get();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("stub"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("value_of"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Two types' methods coexist, each resolving to its own receiver.
///
/// `Counter::get` and `Gauge::get` share a name deliberately. Each lives in its own type's outer env,
/// so the risk is the importer pairing a method with the wrong receiver, which surfaces as a
/// resolution failure rather than a wrong answer.
#[test]
fn calls_methods_on_two_different_rust_types() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_counter;
import mycrate.Counter;
import mycrate.make_gauge;
import mycrate.Gauge;
exported func main() int {
  x = (make_counter()).get();
  return (make_gauge()).get();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Counter"), ..
                                    }), ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Gauge"), ..
                                    }), ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Two Rust types imported in one compilation — the importer is a loop, not a single-item path.
#[test]
fn imports_two_rust_types_at_once() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Counter;
import mycrate.make_counter;
import mycrate.value_of_counter;
import mycrate.Gauge;
import mycrate.make_gauge;
import mycrate.gauge_reading;
exported func main() int {
  x = value_of_counter(make_counter());
  return gauge_reading(make_gauge());
}
"#, |_| ());

  outcome.expect_compiled();
}

/// A Rust citizen produced by one call and consumed by another, with a third in between.
///
/// A lowering that minted a fresh kind per signature would typecheck each call in isolation and
/// fail only here, where the same type has to be recognised across a call boundary twice.
#[test]
fn a_rust_type_flows_through_two_calls() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Counter;
import mycrate.make_counter;
import mycrate.bump;
import mycrate.value_of_counter;
exported func main() int {
  return value_of_counter(bump(make_counter()));
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("bump"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An item in a nested module, named by a dotted path — the shape `Vec` needs.
///
/// A crate-root-only walk cannot see it at all, so a one-level walk fails this test. Most tests in
/// this file import from a crate root, which is the degenerate path.
#[test]
fn imports_an_item_from_a_nested_module() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.instruments.depth_reading;
exported func main() int {
  return depth_reading();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_reading"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A type in a nested module, plus its method — a different `DefKind` and therefore a different arm.
#[test]
fn imports_a_type_from_a_nested_module() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.instruments.Sonar;
import mycrate.instruments.make_sonar;
exported func main() int {
  return (make_sonar()).depth_of();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_of"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// An item reached through a re-exported name — the shape `std::vec::Vec` actually has.
#[test]
fn imports_through_a_re_exported_item() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.readouts.depth_reading;
exported func main() int {
  return depth_reading();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_reading"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Descending **through** a re-exported module, rather than landing on a re-exported item.
#[test]
fn imports_through_a_re_exported_module() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.gear.instruments.Sonar;
import mycrate.gear.instruments.make_sonar;
exported func main() int {
  return (make_sonar()).depth_of();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_of"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A re-export whose target lives in another crate, reached by a path through the re-exporting one.
#[test]
fn imports_through_a_cross_crate_re_exported_item() {
  let outcome = typecheck("horizon/two_crates", r#"
import othercrate.vendored.make_gadget;
import othercrate.vendored.Gadget;
exported func main() int {
  return (make_gadget()).gadget_value();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("gadget_value"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// Descending through a re-exported **module** whose target is in another crate — `std::vec`'s form.
#[test]
fn imports_through_a_cross_crate_re_exported_module() {
  let outcome = typecheck("horizon/two_crates", r#"
import othercrate.toolkit.tools.make_spanner;
import othercrate.toolkit.tools.Spanner;
exported func main() int {
  return (make_spanner()).spanner_size();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("spanner_size"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// **Everything at once** — the composition case.
///
/// Every other case is narrow so failures localize. This one exists for the question narrowness
/// cannot answer: whether the mechanisms coexist. Interference is its own failure class — a shared
/// name resolving to the wrong item, an import-order dependency, a drop that only works when it is
/// the only drop — and no narrow case can see it.
///
/// The assertions are on the **callee list**, not on the return value alone: a program this size
/// could return 31 while silently having resolved half its calls to the wrong thing.
#[test]
fn a_program_using_everything_at_once() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.seven;
import mycrate.Counter;
import mycrate.make_counter;
import mycrate.value_of_counter;
import mycrate.bump;
import mycrate.Gauge;
import mycrate.make_gauge;
import mycrate.pick;
import mycrate.id;
import mycrate.Holder;
import mycrate.make_holder;
import mycrate.make_bool_holder;
import mycrate.holder_ignore;
import mycrate.bool_holder_flag;
import mycrate.instruments.depth_reading;
import mycrate.instruments.Sonar;
import mycrate.readouts.make_sonar;
import mycrate.first;
import mycrate.unsigned_count;
import mycrate.half_of;
exported func main() int {
  held_counter = make_counter();
  held_gauge = make_gauge();
  held_sonar = make_sonar();
  held_holder = make_holder();

  from_zero_arg = seven();
  from_free_fn = add_two_numbers(20, 22);
  from_generic_fn = pick<int, bool>(add_two_numbers(10, 5), true);
  from_generic_at_citizen = id<Counter>(make_counter());

  from_second_type = (make_gauge()).get();
  from_second_method = (make_counter()).doubled();
  from_generic_method = (make_counter()).or_else<int>(19);
  from_chained_calls = value_of_counter(bump(Counter.new()));

  from_int_holder = holder_ignore<int>(make_holder());
  from_bool_holder = bool_holder_flag(make_bool_holder());

  from_nested_type = (make_sonar()).depth_of();
  from_vale_fn = vale_counter_value(make_counter());

  return depth_reading();
}
func vale_counter_value(c Counter) int {
  return (^c).get();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    assert!(!collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("add_two_numbers"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("seven"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    assert!(!collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_counter"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("doubled"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("or_else"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("new"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("bump"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("pick"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("id"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("holder_ignore"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("bool_holder_flag"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_reading"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("depth_of"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    assert!(!collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_sonar"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    assert!(collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("first"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    assert!(collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("unsigned_count"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    assert!(collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("half_of"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("stub"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("vale_counter_value"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Holder"), ..
                                    }),
                                    template_args: [ITemplataT::Kind(KindTemplataT {
                                        kind: KindT::Int(IntT { bits: 32 }),
                                    })], ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. },
                        parameters: [KindT::Struct(StructTT {
                            id: IdT {
                                package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                                local_name: INameT::Struct(StructNameT {
                                    template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                        human_name: StrI("Gauge"), ..
                                    }),
                                    template_args: [], ..
                                }), ..
                            }, ..
                        })], ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// A signature Vale cannot represent costs nothing when it is imported but never called: with lazy
/// synthesis its signature is never even queried.
///
/// `first<I: Iterator>(i: I) -> I::Item` returns `<I as Iterator>::Item`, and normalizing that
/// requires the `I: Iterator` predicate to find the impl. No predicates are read at all, so this is
/// not merely an unbounded parameter but an un-normalizable alias — it would decline if forced. Here
/// it is offered but uncalled, so it is never forced.
#[test]
fn declines_an_unrepresentable_signature() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.first;
exported func main() int {
  return add_two_numbers(1, 4);
}
"#, |_| ());

  // An uncalled unrepresentable import must not disturb the rest of the import.
  outcome.expect_compiled();
}

/// The same unrepresentable type in **argument** position, offered but uncalled: still never queried.
#[test]
fn declines_an_unrepresentable_parameter() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.take_first;
exported func main() int {
  return add_two_numbers(2, 4);
}
"#, |_| ());

  // An uncalled unrepresentable import must not disturb the rest of the import.
  outcome.expect_compiled();
}

/// An unsigned integer would decline if forced — its signature is `u32`-shaped, and `IntT` carries a
/// width but no signedness, so importing it would hand back a plausible `i32`. Offered but uncalled,
/// it is never forced, so it is never queried.
#[test]
fn declines_an_unsigned_integer() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.unsigned_count;
exported func main() int {
  return add_two_numbers(3, 4);
}
"#, |_| ());

  // An uncalled unrepresentable import must not disturb the rest of the import.
  outcome.expect_compiled();
}

/// The decline path, actually forced: a called Rust function whose signature Vale cannot represent
/// (an unsigned-int return) surfaces as a `CouldNotPostparseFunction` compile error, not a panic.
#[test]
fn calling_a_declined_signature_is_a_compile_error() {
  // A forced decline must be a clean diagnostic naming the item, not a `vfail` panic mid-resolution.
  let outcome = typecheck("horizon/main", r#"
import mycrate.unsigned_count;
exported func main() int {
  unsigned_count();
  return 0;
}
"#, |_| ());
  assert!(outcome.expect_failure().is("CouldNotPostparseFunction"));
}

/// A float would decline if forced — `FloatT` has no width, so `f32` and `f64` would intern
/// identically. Offered but uncalled, it is never forced.
#[test]
fn declines_a_float() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.half_of;
exported func main() int {
  return add_two_numbers(4, 4);
}
"#, |_| ());

  // An uncalled unrepresentable import must not disturb the rest of the import.
  outcome.expect_compiled();
}

/// @RTMEIZ from the side that is easy to miss: reaching a type through another item's signature does
/// not import it. `takes_hidden` is allowed, `Hidden` is not, so `takes_hidden` would decline if
/// forced — but offered and uncalled, it is never forced, so it is never queried.
#[test]
fn declines_a_signature_naming_an_unimported_type() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.takes_hidden;
exported func main() int {
  return add_two_numbers(4, 5);
}
"#, |_| ());

  // An uncalled unrepresentable import must not disturb the rest of the import.
  outcome.expect_compiled();
}

/// A generic Rust type imports **with its arguments intact**.
///
/// `Holder<i32>` and `Holder<bool>` must be two distinct Vale kinds. Until 2026-07-26 they were
/// not: both interned as a bare `Holder` with `template_args: []`, so Vale gave the same answer
/// for different types — the worst of the three possible behaviours, which is why this case
/// existed asserting the defect before it asserted the fix.
///
/// Two things had to change together. `TyCtxtOracle::type_kind` now reads the ADT's
/// `GenericArgsRef` instead of dropping it — but that alone changes nothing, because a synthesized
/// declaration does not carry the lowered kind. It names the type through rules, so the
/// declaration also had to stop emitting a bare `LookupSR` and start emitting `LookupSR` (bind the
/// template) + `CallSR` (apply the arguments), which needs the type registered as a real
/// `IEnvEntryT::Struct` rather than a finished `ITemplataT::Kind`.
#[test]
fn a_generic_rust_type_carries_its_arguments() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.make_holder;
import mycrate.make_bool_holder;
import mycrate.holder_value;
import mycrate.bool_holder_flag;
import mycrate.Holder;
exported func main() int {
  a = holder_value(make_holder());
  b = bool_holder_flag(make_bool_holder());
  return 13;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_holder"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Holder"), ..
                            }),
                            template_args: [ITemplataT::Kind(KindTemplataT {
                                kind: KindT::Int(IntT { bits: 32 }),
                            })], ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_bool_holder"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Holder"), ..
                            }),
                            template_args: [ITemplataT::Kind(KindTemplataT {
                                kind: KindT::Bool(BoolT),
                            })], ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
  });

  // Asserting the arguments rather than merely that the two differ: "they differ" would also be
  // satisfied by two wrong-but-distinct answers, and the defect this replaced was precisely two
  // instantiations rendering the same.
  outcome.expect_compiled();
}

/// A stale allowlist entry is inert. An `import` list outlives the crate versions it was written
/// against, so a name that stops existing must not take the compilation down.
#[test]
fn an_allowlist_entry_the_crate_does_not_export_is_ignored() {
  // Importing an item the crate does not export now fails the compile (`UnresolvableRustImport`)
  // rather than being silently ignored.
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.no_such_item_exists_anywhere;
exported func main() int {
  return add_two_numbers(2, 8);
}
"#, |_| ());
  assert!(outcome.expect_failure().is("UnresolvableRustImport"));
}

/// A crate's module children include its own `extern crate std`. Without the `DefKind` filter, a
/// name match would hand back a module where a function or type was asked for.
#[test]
fn a_module_named_in_the_allowlist_is_filtered_by_defkind() {
  // A module (not a fn/struct) fails the `DefKind` filter, so the import resolves to nothing and the
  // compile fails (`UnresolvableRustImport`) rather than the entry being silently ignored.
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
import mycrate.std;
exported func main() int {
  return add_two_numbers(4, 8);
}
"#, |_| ());
  assert!(outcome.expect_failure().is("UnresolvableRustImport"));
}

/// A Rust callee competes on `params_match` like any other candidate.
#[test]
fn wrong_argument_types_do_not_resolve() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.add_two_numbers;
exported func main() int {
  return add_two_numbers(true, 4);
}
"#, |_| ());

  assert!(outcome.expect_failure().is("CouldntFindFunctionToCallT"));
}

/// Two crates' items reach Vale in one compilation, and stay two types.
///
/// The distinct-short-name half of the two-crate fixture, so this exercises multiplicity without
/// also posing the collision below. Each item's `package_coord` comes from its own `tcx.def_path`,
/// so the two land in different packages and therefore in different top-level stores.
#[test]
fn imports_from_two_crates() {
  let outcome = typecheck("horizon/two_crates", r#"
import mycrate.Gadget;
import mycrate.make_gadget;
import othercrate.Doohickey;
import othercrate.make_doohickey;
exported func main() int {
  d = (make_doohickey()).doohickey_value();
  return (make_gadget()).gadget_value();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_gadget"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("othercrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_doohickey"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("gadget_value"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("othercrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("doohickey_value"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

/// The distinctness half — and the only shape that can observe it.
///
/// The case above proves both `Widget`s import. It cannot prove they stayed *distinct*: a
/// conflated pair satisfies every call in that program, because each call is consistent within its
/// own crate. Crossing them is what tells them apart, and it does so by **failing** — `widget_value`
/// takes `mycrate`'s `Widget` and is handed `othercrate`'s.
///
/// So a regression that merged the two types makes this program start *compiling*. That is an
/// unusual direction for a test in this file and worth stating plainly, since "it passes" here means
/// "the compiler rejected the program."
#[test]
fn a_type_from_one_crate_does_not_satisfy_the_others_parameter() {
  let outcome = typecheck("horizon/two_crates", r#"
import mycrate.Widget;
import othercrate.Widget;
import mycrate.make_widget;
import mycrate.widget_value;
import othercrate.make_other_widget;
exported func main() int {
  return widget_value(make_other_widget());
}
"#, |_| ());

  assert!(outcome.expect_failure().is("CouldntFindFunctionToCallT"));
}

/// **@ATAFLBZ fence: nothing in horizon may take a Rust item's identity from its human name.**
///
/// The hazard is that Rust has no uniqueness rule for short names — `new`, `len`, `Error`, `Box`
/// recur across crates — and `tcx.crates(())` hands us every loaded crate. A `DefId` chosen by
/// string match eventually drives a mangled symbol, so the failure surfaces as a link error against
/// a plausible-looking name, far from the mistake.
///
/// Three sites once decided this way; two were deleted with the per-call-site oracle and the third
/// now derives each item's `package_coord` from `tcx.def_path`. **The fence is not for those three
/// — it is for the next one**, which is why Harmonious pushed for it after their own version of
/// this bug: *"the value is not the site that was fixed, it is the next one."*
///
/// A grep rather than an AST walk, deliberately: the pattern is a *comparison against a name
/// field*, which is one line and reads the same in any shape. Add an allow-marker comment on the
/// line if a match is genuinely about **selection** (which items an allowlist admits) rather than
/// **identity** — the allowlist is name-shaped by its own semantics, and that is fine.
#[test]
fn no_rust_item_identity_comes_from_a_human_name() {
  const ALLOW: &str = "ataflbz-allow";
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

  let mut offenders: Vec<String> = Vec::new();
  let mut walk = vec![
    root.join("src/typing/rust_interop/horizon"),
    root.join("src/instantiating/rust_interop/horizon"),
  ];
  while let Some(path) = walk.pop() {
    for entry in read_dir(&path).expect("could not read a horizon dir") {
      let entry = entry.expect("could not read dir entry").path();
      if entry.is_dir() {
        walk.push(entry);
        continue;
      }
      // Fixture crates are Rust *input*, not compiler source.
      if entry.extension().is_none_or(|e| e != "rs") || entry.to_string_lossy().contains("fixtures")
      {
        continue;
      }
      let source = read_to_string(&entry).expect("could not read source");
      for (number, line) in source.lines().enumerate() {
        if line.contains(ALLOW) {
          continue;
        }
        let compares_a_name = (line.contains("human_name") || line.contains(".ident"))
          && (line.contains("==") || line.contains("!=") || line.contains(".contains("));
        if compares_a_name {
          offenders.push(format!(
            "{}:{}: {}",
            entry.file_name().expect("a file has a name").to_string_lossy(),
            number + 1,
            line.trim()
          ));
        }
      }
    }
  }

  assert!(
    offenders.is_empty(),
    "these lines take a Rust item's identity from a human name (@ATAFLBZ). Key on `DefId` or \
         on the `tcx.def_path`-derived package coordinate instead; if the comparison is about \
         which items the allowlist *admits* rather than which item something *is*, add a \
         `{ALLOW}` comment on the line:\n  {}",
    offenders.join("\n  ")
  );
}


/// Ensures the borrow checker rejects a use-after-churn across the Rust boundary: a `&Glyph` from
/// a Rust `&self` method (`get_glyph`) is used after a Rust `&mut self` method (`add_glyph`)
/// churns its owner.
#[test]
fn use_after_churn_through_a_rust_borrow_return_is_rejected() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  d_ref = d.get_glyph(7);
  d.add_glyph(Glyph.new(8));
  return d_ref.location();
}
"#, |_| ());

  assert!(
    outcome.expect_failure().is("BorrowCheckError"),
    "a use-after-churn across the Rust interop boundary was accepted"
  );
}

/// RED (until the importer follows `Deref<Target=[T]>`): a real `std::vec::Vec` element accessor
/// should be importable, so `v.get(0)` should resolve and the program should compile. It does not
/// today — `get` is a slice method reached through `Deref<[T]>`, and the importer discovers only a
/// type's inherent methods (`new`/`push`/`pop`/`len`), so the call fails with
/// `CouldntFindFunctionToCallT`. This is the first of three blockers to a real-`Vec` element
/// use-after-churn; the use-after-churn R test itself uses the Domino wrapper (inherent
/// `get_glyph -> &Glyph`).
// Ignored until the Deref method-discovery work lands (the next rust-interop task). `get` is unreachable
// until the importer follows `Deref<Target=[T]>` and lowers the slice
// `[T]` / `usize`, so the program cannot compile yet.
#[test]
#[ignore]
fn a_real_vec_element_accessor_is_importable() {
  let outcome = typecheck_without_borrow_check("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
exported func main() int {
  v = Vec.new<int>();
  v.push(7);
  e = (v.get(0)).unwrap();
  return 0;
}
"#, |_| ());

  outcome.expect_compiled();
}

/// The negative control for the case above: the same fixture and churn method, but the `&Glyph`
/// borrow is taken *after* the last churn, so it is valid and the program must compile. It guards the
/// eventual churn rule against over-rejection — once the importer carries the group facts, this must
/// stay compiling.
#[test]
fn rust_borrow_return_taken_after_last_churn_is_clean() {
  let outcome = typecheck("horizon/main", r#"
import mycrate.Domino;
import mycrate.Glyph;
exported func main() int {
  d = Domino.new();
  d.add_glyph(Glyph.new(7));
  d.add_glyph(Glyph.new(8));
  d_ref = d.get_glyph(7);
  return d_ref.location();
}
"#, |_| ());

  outcome.expect_compiled();
}

/// Tier-2 tracer for the `layout_of` override: a data-carrying Vale struct (`Ship { fuel int }`) goes
/// by value into `id<T>` and back, and the program returns its field → 42. rustc sizes the crossing
/// `__ValeOpaque<typeid>` from Vale's real members via the override; before it, rustc's zero-sized view
/// made the argument `PassMode::Ignore`, so nothing crossed and the field read garbage.
#[test]
fn rustc_driven_bin_vale_struct_round_trips_by_value_returns_42() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.id;
struct Ship { fuel int; }
exported func main() int {
  s = id(Ship(42));
  return s.fuel;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven vale-struct-round-trip bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Tier-2: a Vale struct lives by value inside a real Rust `Vec` and is read back through `at<T>`'s
/// borrow → 42. Rides Slice 1's `layout_of` override for the element stride; nothing else is new here
/// unless the generic borrow-return import (`&Vec<T>` in, `&T` out) turns out to be a gap of its own.
#[test]
fn rustc_driven_bin_vec_of_vale_structs_read_through_borrow_returns_42() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Ship { fuel int; }
exported func main() int {
  v = Vec.new<Ship>();
  v.push(Ship(42));
  return at(&v, 0i64).fuel;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven vec-of-vale-structs bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}

/// Tier-2 guard for the benchmark shape: a field write through one alias of a `Vec<Ship>` element is
/// read back through the other alias → 42. Vale keeps both borrows live and emits the store/load at
/// its own field offset behind Vec's buffer; Rust's `at<T>` only hands the pointers out.
#[test]
fn rustc_driven_bin_aliased_vec_element_write_returns_42() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Ship { fuel int; }
exported func main() int {
  v = Vec.new<Ship>();
  v.push(Ship(1));
  ref_a = &v;
  ref_b = &v;
  a = at(ref_a, 0i64);
  b = at(ref_b, 0i64);
  set a.fuel = 42;
  do_nothing();
  return b.fuel;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(42),
    "the driven aliased-vec-element-write bin did not exit 42 (rustc_exit={}, process_exit={:?}); firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
