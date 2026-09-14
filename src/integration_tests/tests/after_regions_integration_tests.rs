#![allow(unused_imports, dead_code, unused_variables, unreachable_code)]
use crate::collect_where_tnode;
use crate::typing::test::traverse::NodeRefT;
use crate::integration_tests::tests::run_compilation::test;
use crate::integration_tests::tests::run_compilation::test_without_borrow_check;
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::scout_arena::ScoutArena;
use crate::tests::tests::load_expected;
use crate::typing::typing_interner::TypingInterner;
use crate::testvm::von::IVonData;
use crate::testvm::von::VonInt;
pub struct AfterRegionsIntegrationTests;

#[test]
fn test_returning_empty_seq() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
export () as Tup0;
exported func main() () {
  return ();
}
",
    );
    compile.run_primitive_args(Vec::new()).unwrap();
}

#[test]
fn imm_tuple_access() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let source = load_expected("programs/tuples/immtupleaccess.vale");
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        source.as_str(),
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 42 }) => {}
        other => panic!("Expected VonInt(42), got {:?}", other),
    }
}

#[test]
fn impl_bounded_generic_is_merely_called_with_a_concrete_type() {
    // VCOORD: clean up this comment
    // Minimal repro of the impl-bound onion regression: a generic fn with a
    // `where implements(T, IShip)` bound is merely CALLED with a concrete type —
    // no method is dispatched through the bound. This should just return 7, but
    // the instantiator panics on an impl-bound count mismatch (1 resolved at the
    // call site vs 0 recorded on the callee template) because the define-side
    // impl-bound harvest in resolve_conclusions_for_define is commented out.
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
sealed interface IShip { }
struct Raza { }
impl IShip for Raza;

func needsShip<T>(x &T) int
where implements(T, IShip) {
  return 7;
}

exported func main() int {
  return needsShip(&Raza());
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 7 }) => {}
        other => panic!("Expected VonInt(7), got {:?}", other),
    }
}

#[test]
fn interface_method_call_on_impl_bounded_generic_dispatches_through_interface() {
    // VCOORD: clean up this comment
    // The scenario: genericGetFuel<T> takes &T with a `where implements(T, IShip)` bound
    // and calls x.getFuel() in its body. The user expects this to find IShip's abstract
    // getFuel, then dispatch virtually to Raza's override at runtime.
    //
    // Why this isn't automatic: Vale's interface abstract methods don't sit at the package
    // level. They live inside the interface's own outer env, reachable only via
    // coutputs.getOuterEnvForType(getInterfaceTemplate(IShip)). For a *concrete* &IShip
    // receiver, OverloadResolver.getParamEnvironments mechanically returns IShip's outer
    // env (because the receiver's type names IShip directly). For a *placeholder* &T
    // receiver, the type doesn't name IShip — IShip is one indirection away, declared via
    // the where-clause as an IsaTemplataT(T, IShip) entry in genericGetFuel's near-env.
    // Without something following that indirection, the lookup of getFuel finds only the
    // free function `getFuel(self &Raza)` (which type-mismatches T) and never reaches
    // IShip's outer env where the abstract method lives. Pre-fix, this produced
    // "No ancestors satisfy call" and the program failed to type-check.
    //
    // What we changed: OverloadResolver.getCandidateBanners now also calls
    // getPlaceholderImplBoundEnvs alongside getParamEnvironments. For each placeholder-
    // typed param, it looks up ambient impl bounds keyed by the placeholder's imprecise
    // name (ImplSubCitizenImpreciseNameS, populated automatically when addRunedDataToNearEnv
    // writes the IsaTemplataT into the near-env), pulls each IsaTemplataT, and adds each
    // super-interface's outer env to the candidate search. With that, the abstract
    // getFuel(virtual self &IShip) becomes a candidate; the inner per-call-site solve
    // verifies T isa IShip via the same IsaTemplataT (through ImplCompiler.isParent); the
    // call resolves; the instantiator monomorphizes genericGetFuel<Raza>; and the backend
    // dispatches getFuel virtually through Raza's vtable, returning 42.
    //
    // The fix is principle-aligned with @BDPFWDZ (By Default Pull From Where Declared):
    // IShip's methods stay in IShip's outer env where they were declared; the resolver
    // walks (via the where-clause's IsaTemplataT link) to find them; nothing is copied
    // into the calling function's near-env. See
    // docs/arcana/ByDefaultPullFromWhereDeclared-BDPFWDZ.md for the broader principle.
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        // TSUGAR: self.fuel is &int
        r"
sealed interface IShip {
  func getFuel(virtual self &IShip) int;
}
struct Raza { fuel int; }
impl IShip for Raza;
func getFuel(self &Raza) int { return __copy_prim(&self.fuel); }

func genericGetFuel<T>(x &T) int
where implements(T, IShip) {
  return x.getFuel();
}

exported func main() int {
  return genericGetFuel(&Raza(42));
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 42 }) => {}
        other => panic!("Expected VonInt(42), got {:?}", other),
    }
}

// Ensures an impl-bounded method call runs correctly end-to-end: `cb.bork()` where `cb: &C` and
// `where implements(C, Bork)`, called with a concrete `C`, returns 7 in the TestVM. (exp-4's repro.)
#[test]
fn impl_bounded_generic_method_call_forwarder_testvm() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
#!DeriveInterfaceDrop
sealed interface Bork { func bork(virtual self &Bork) int; }

#!DeriveStructDrop
struct BorkForwarder<Lam> where func drop(Lam)void, func __call(&Lam)int { lam Lam; }

impl<Lam> Bork for BorkForwarder<Lam>;

func bork<Lam>(self &BorkForwarder<Lam>) int { return (&self.lam)(); }

func run<C>(cb &C) int where implements(C, Bork) { return cb.bork(); }

exported func main() int {
  f = BorkForwarder({ 7 });
  z = run(&f);
  [_] = ^f;
  return ^z;
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 7 }) => {}
        other => panic!("Expected VonInt(7), got {:?}", other),
    }
}

// The impl-bounded method call `cb.bork()` inside a generic must be typed as a BoundFunctionCall
// (receiver kept un-upcast, devirtualizable at monomorphization), never a FunctionCall-to-abstract
// with an UpcastGeneric receiver. exp-4 repro.
#[test]
fn impl_bounded_method_call_emits_bound_function_call() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
#!DeriveInterfaceDrop
sealed interface Bork { func bork(virtual self &Bork) int; }

#!DeriveStructDrop
struct BorkForwarder<Lam> where func drop(Lam)void, func __call(&Lam)int { lam Lam; }

impl<Lam> Bork for BorkForwarder<Lam>;

func bork<Lam>(self &BorkForwarder<Lam>) int { return (&self.lam)(); }

func run<C>(cb &C) int where implements(C, Bork) { return cb.bork(); }

exported func main() int {
  f = BorkForwarder({ 7 });
  z = run(&f);
  [_] = ^f;
  return ^z;
}
",
    );
    let coutputs = compile.expect_compiler_outputs();
    let run = coutputs.lookup_function_by_str("run");
    let bound_calls: Vec<_> = collect_where_tnode!(
        NodeRefT::FunctionDefinition(run),
        NodeRefT::BoundFunctionCall(b) => Some(b)
    );
    let generic_upcasts: Vec<_> = collect_where_tnode!(
        NodeRefT::FunctionDefinition(run),
        NodeRefT::UpcastGeneric(u) => Some(u)
    );
    assert!(
        !bound_calls.is_empty(),
        "expected >=1 BoundFunctionCall in `run`, got {}",
        bound_calls.len()
    );
    assert_eq!(
        generic_upcasts.len(),
        0,
        "expected 0 UpcastGeneric in `run` (receiver is kept in the BoundFunctionCall), got {}",
        generic_upcasts.len()
    );
}

// After monomorphization with a concrete receiver, the impl-bounded method call must devirtualize:
// the instantiated `run` body must contain no Upcast (no fat-ptr construction) — a direct static
// call to the concrete override instead. (The TestVM result — 7 — is covered by
// impl_bounded_generic_method_call_forwarder_testvm.)
#[test]
fn impl_bounded_method_call_instantiates_to_static_override() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
#!DeriveInterfaceDrop
sealed interface Bork { func bork(virtual self &Bork) int; }

#!DeriveStructDrop
struct BorkForwarder<Lam> where func drop(Lam)void, func __call(&Lam)int { lam Lam; }

impl<Lam> Bork for BorkForwarder<Lam>;

func bork<Lam>(self &BorkForwarder<Lam>) int { return (&self.lam)(); }

func run<C>(cb &C) int where implements(C, Bork) { return cb.bork(); }

exported func main() int {
  f = BorkForwarder({ 7 });
  z = run(&f);
  [_] = ^f;
  return ^z;
}
",
    );
    let monouts = compile.get_monouts();
    let run_body = monouts
        .functions
        .iter()
        .find(|f| format!("{:?}", f.header.id).contains("\"run\""))
        .map(|f| format!("{:?}", f.body))
        .expect("monomorphized `run` not found");
    assert!(
        !run_body.contains("Upcast"),
        "instantiated `run` should have no Upcast (devirtualized to a static override call), body:\n{}",
        run_body
    );
}

// The contrast: a real, user-written concrete->interface upcast stays an UpcastInterface, never
// becomes an UpcastGeneric.
#[test]
fn direct_interface_upcast_stays_interface() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
sealed interface IShip {}
struct Raza {}
impl IShip for Raza;
func launch(s &IShip) { }
exported func main() {
  launch(&Raza());
}
",
    );
    let coutputs = compile.expect_compiler_outputs();
    let main = coutputs.lookup_function_by_str("main");
    let interface_upcasts: Vec<_> = collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::UpcastInterface(u) => Some(u)
    );
    let generic_upcasts: Vec<_> = collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::UpcastGeneric(u) => Some(u)
    );
    assert!(
        !interface_upcasts.is_empty(),
        "expected >=1 UpcastInterface for launch(&Raza()), got {}",
        interface_upcasts.len()
    );
    assert_eq!(
        generic_upcasts.len(),
        0,
        "expected 0 UpcastGeneric for a direct upcast, got {}",
        generic_upcasts.len()
    );
}

#[test]
fn call_array_without_element_type() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
exported func main() int {
  a = Array(3, &{13 + _});
  sum = 0;
  drop_into(^a, &(e) => { set sum = sum + e; });
  return sum;
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 42 }) => {}
        other => panic!("Expected VonInt(42), got {:?}", other),
    }
}

#[test]
fn make_array_without_type() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        // TSUGAR: a.3 is &int
        r"
exported func main() int {
  a = [](10, &{^_});
  return __copy_prim(&a.3);
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 3 }) => {}
        other => panic!("Expected VonInt(3), got {:?}", other),
    }
}

#[ignore = "blocked on migrate builtin (__vbi_panic); re-enable when borrow-group (R1) lands"]
#[test]
fn borrowing_to_array() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        // TSUGAR: l.toArray()[1] is &int
        r"
import list.*;

func toArray<E>(list &List<E>) []&E {
  return []&E(list.len(), &{ list.get(_) });
}

exported func main() int {
  l = List<int>();
  add(&l, 5);
  add(&l, 9);
  add(&l, 7);
  return __copy_prim(&l.toArray()[1]);
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 9 }) => {}
        other => panic!("Expected VonInt(9), got {:?}", other),
    }
}

