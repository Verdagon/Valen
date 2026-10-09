// The `per_instance_mir` provider: rustc's mono collector asks it for the MIR of each Vale item it
// walks — an export's `__vale_<name>` stub, or a projected trait-impl method that Rust calls back into
// — and it drives Vale's instantiator for that item, then answers with a synthetic body that mentions
// every Rust function the instantiated Vale code calls, so rustc queues those for codegen.

use rustc_hir::Safety;
use rustc_index::IndexVec;
use rustc_middle::mir::{
  BasicBlock, BasicBlockData, Body, CastKind, ClearCrossCrate, Const, ConstOperand, CoercionSource,
  Local, LocalDecl, MirSource, Operand, Place, Rvalue, SourceInfo, SourceScopeData, Statement,
  StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Instance, Ty, TyCtxt};
use rustc_span::def_id::DefId;

use crate::instantiating::ast::ast::FunctionExternI;
use crate::instantiating::ast::templata::ITemplataI;
use crate::instantiating::instantiated_humanizer::humanize_id;
use crate::instantiating::instantiator::{
  DenizenBoundToDenizenCallerBoundArgI, InstantiatedOutputsI, InstantiatorI,
};
use crate::typing::ast::ast::PrototypeT;
use crate::typing::compiler::Compiler;
use crate::typing::names::names::{INameT, IdT};
use crate::typing::templata_compiler::get_interface_template;
use crate::typing::types::types::RegionT;
use crate::utils::fx::IndexMap;
use crate::utils::range::CodeLocationS;

use super::bifrost_state::{BifrostState, CallbackReq, OpaqueKindI};
use super::driver_state::horizon_state;
use super::extern_abi::compute_extern_abi;
use super::override_queries::is_vale_codegen_target;
use super::resolve_request::{resolve_request, ResolvedRequest};
use super::rustc_ty::{citizen_or_opaque_to_rustc_ty, opaque_typeid, read_opaque_typeid};

/// Answer `Some(synthetic_body)` for a Vale item and `None` for everything else, so the collector
/// falls through to rustc's own `instance_mir`.
pub(super) fn lang_per_instance_mir<'tcx>(
  tcx: TyCtxt<'tcx>,
  instance: Instance<'tcx>,
) -> Option<&'tcx Body<'tcx>> {
  let def_id = instance.def_id();
  if !is_vale_codegen_target(tcx, def_id) {
    return None;
  }
  let state = horizon_state("per_instance_mir");

  // The stub root `__vale_<name>` names the Vale export `<name>`.
  let stub_name = tcx.item_name(def_id).to_string();
  let export_name = stub_name.strip_prefix("__vale_").unwrap_or(&stub_name).to_string();

  // Is this fired item a Vale *export* (`__vale_main`, or a library export), or a trait-impl
  // *callback* that rustc's collector reached because Rust calls it directly? An export is named in
  // `hinputs.function_exports`; a callback (e.g. `on_call`) is not, so it takes the branch that
  // instantiates its body and records an inbound wrapper.
  let is_export = {
    let h = state.hinputs.borrow();
    h.as_ref()
      .map_or(false, |h| h.function_exports.iter().any(|e| e.exported_name.0 == export_name))
  };

  // Each resolved request's `(DefId, args)` becomes a `ReifyFnPointer` cast in the body, which is what
  // puts that Rust `Instance` into the collector's queue.
  let requests = if is_export {
    if stub_name == "__vale_main" {
      // The backend emits `__vale_main`'s body under rustc's own mangled name for this stub instance,
      // so the final file's `fn main` (which calls the Rust name `__vale_main`) links to Vale's body.
      *state.entry_symbol.borrow_mut() = Some(tcx.symbol_name(instance).name.to_string());
    }
    let requests = instantiate_export(state, tcx, &export_name);
    let log = requests.iter().map(|r| r.log.as_str()).collect::<Vec<_>>().join(", ");
    state.firings.borrow_mut().push(format!("{stub_name} -> [{log}]"));
    requests
  } else {
    // A Rust→Vale callback: instantiate its body, record the wrapper the backend emits under the
    // rustc-mangled symbol, and reify any Rust leaves the callback body itself calls (e.g. an
    // outbound `w.get()`) so rustc's collector queues them — just like an export body's leaves.
    let requests = instantiate_callback(state, tcx, instance, &stub_name);
    let log = requests.iter().map(|r| r.log.as_str()).collect::<Vec<_>>().join(", ");
    state.firings.borrow_mut().push(format!("{stub_name} -> [callback: {log}]"));
    requests
  };
  let rust_deps: Vec<(DefId, ty::GenericArgsRef<'tcx>)> = requests.iter().map(|r| r.dep).collect();

  let body = build_mir_body_with_mentions(tcx, instance, &rust_deps);
  Some(tcx.arena.alloc(body))
}

/// Drive the instantiator for the one exported function named `export_name`: seed it, drain the queue
/// (instantiating Vale functions, recording Rust callees in `rust_instantiation_requests`), and return
/// the Rust requests this drain added.
fn instantiate_export<'tcx>(
  state: &BifrostState,
  tcx: TyCtxt<'tcx>,
  export_name: &str,
) -> Vec<ResolvedRequest<'tcx>> {
  let hinputs_ref = state.hinputs.borrow();
  let hinputs = hinputs_ref.as_ref().expect("missing hinputs");
  let export = hinputs
    .function_exports
    .iter()
    .find(|e| e.exported_name.0 == export_name)
    .unwrap_or_else(|| panic!("per_instance_mir called for `{export_name}`, which isn't a Valen export"));
  let instantiator = InstantiatorI {
    opts: state.opts,
    interner: state.interner,
    typing_interner: state.typing_interner,
    scout_arena: state.scout_arena,
    keywords: state.keywords,
    rust_crates: state.rust_crates,
    hinputs,
  };
  let mut monouts = state.monouts.borrow_mut();
  let requests_before = monouts.rust_instantiation_requests.len();
  // Retain the export the collector walked: the emit finalizes `monouts` with exactly the exports
  // demand reached (rustc memoizes per instance, so once each).
  let export_i = instantiator.instantiate_exported_function(&mut monouts, export);
  state.function_exports.borrow_mut().push(export_i);
  instantiator.drain_instantiation_queue(&mut monouts);
  register_instantiated_kinds(state, &monouts);
  resolve_new_requests(state, tcx, &mut monouts, requests_before)
}

/// Bring `opaque_universe` up to date with everything a drain just instantiated. Called right after
/// each drain and before any rustc query on the new leaves, so by the time `fn_abi_of_instance` asks
/// the layout of a `__ValeOpaque<typeid>` argument, that typeid is answerable. A kind already
/// registered (by an earlier drain) is left where it is.
fn register_instantiated_kinds<'s, 't, 'i>(
  state: &BifrostState<'s, '_, 't, 'i>,
  monouts: &InstantiatedOutputsI<'s, 't, 'i>,
) {
  let mut universe = state.opaque_universe.borrow_mut();
  for (id, def) in monouts.structs.iter() {
    universe.entry(opaque_typeid(id)).or_insert(OpaqueKindI::Struct(def));
  }
  for id in monouts.interfaces_without_methods.keys() {
    universe.entry(opaque_typeid(id)).or_insert(OpaqueKindI::Interface(*id));
  }
}

/// Resolve each Rust callee recorded since `requests_before` (the request map's length before the
/// drain; it is an `IndexMap`, so new entries come after), materialize its `FunctionExternI` with
/// rustc's real mangled symbol — the one place that symbol is known — and stash its boundary ABI.
/// Returns the resolved requests so the caller can reify each in its synthetic body. Shared by the
/// export and callback paths: a callback body can call out to Rust exactly as an export body does.
fn resolve_new_requests<'s, 't, 'i, 'tcx>(
  state: &BifrostState<'s, '_, 't, 'i>,
  tcx: TyCtxt<'tcx>,
  monouts: &mut InstantiatedOutputsI<'s, 't, 'i>,
  requests_before: usize,
) -> Vec<ResolvedRequest<'tcx>> {
  let new_reqs: Vec<_> = monouts
    .rust_instantiation_requests
    .values()
    .skip(requests_before)
    .map(|proto| (*proto, resolve_request(tcx, state.rust_crates, proto)))
    .collect();

  let code_map = |loc: CodeLocationS| format!("{:?}", loc);
  for (proto, req) in &new_reqs {
    let (def_id, args) = req.dep;
    let instance = ty::Instance::new_raw(def_id, args);
    let symbol = tcx.symbol_name(instance).name;
    let symbol_i: &str = state.interner.bump().alloc_str(symbol);
    // The extern is born here, complete, with rustc's real symbol — the sole registration point for a
    // Rust extern (the instantiator only records the request).
    monouts.function_externs.push(FunctionExternI {
      prototype: *proto,
      num_inherited_generic_parameters: 0,
      link_name: symbol_i,
    });
    // The leaf's boundary ABI, keyed by the same humanized prototype name the metal lowerer uses.
    let abi = compute_extern_abi(tcx, instance);
    state.extern_abis.borrow_mut().insert(humanize_id(&code_map, &proto.id, None), abi);
  }

  new_reqs.into_iter().map(|(_, req)| req).collect()
}

/// Catch a Rust→Vale callback the collector reached: a Vale trait-impl override (`item_name`, e.g.
/// `on_call`) that *Rust* calls directly, not a Vale export. Instantiate its body into `monouts` (so
/// the backend emits it as an internal Vale function), compute its inbound ABI (how Rust hands the
/// receiver/args in), and record the wrapper the backend must emit under the method's rustc-mangled
/// symbol. The override is resolved *statically*: Rust dispatched to the concrete impl, so no vtable
/// or abstract-method dispatcher is instantiated.
///
/// Scope — single-level reverse callbacks only. This reads the typeid universe before it writes to
/// it. On a warm rebuild the exports-first phase in `lang_collect_and_partition_mono_items` guarantees
/// every *export* has populated it before any callback reads it; callback-to-callback order (a
/// callback whose body hands another lambda to a Rust trait) is not covered.
fn instantiate_callback<'tcx>(
  state: &BifrostState,
  tcx: TyCtxt<'tcx>,
  instance: Instance<'tcx>,
  item_name: &str,
) -> Vec<ResolvedRequest<'tcx>> {
  let hinputs_ref = state.hinputs.borrow();
  let hinputs = hinputs_ref.as_ref().expect("missing hinputs");
  let code_map = |loc: CodeLocationS| format!("{:?}", loc);

  // The concrete `Self` type rustc monomorphized this override at — e.g. `MyCb<__ValeOpaque<HASH>>`
  // for a lambda forwarder, or a plain `MyCb` for a non-generic callback (the degenerate case).
  let impl_def_id = tcx
    .impl_of_assoc(instance.def_id())
    .expect("a trait-impl override method must live in an impl block");
  let self_ty = tcx.type_of(impl_def_id).instantiate(tcx, instance.args);

  // Identify the concrete Vale impl this callback instance stands for, against the typeid universe
  // the earlier drains registered, keyed by the same content hash the outbound lowering stamps.
  let (matched_impl_t, matched_impl_i) = {
    let monouts = state.monouts.borrow();
    // Fail loud (not silent-wrong) if the instance carries an opaque type Vale never instantiated.
    let universe = state.opaque_universe.borrow();
    for arg in self_ty.walk() {
      if let Some(arg_ty) = arg.as_type() {
        if let Some(tid) = read_opaque_typeid(tcx, arg_ty) {
          assert!(
            universe.contains_key(&tid),
            "rust interop: callback {item_name:?} carries opaque typeid {tid} for a Valen type not in \
             the instantiated universe (a monomorphization Vale never produced)"
          );
        }
      }
    }
    // Match the callback's concrete `Self` against a recorded impl by projecting each candidate's
    // sub-citizen forward through the same converter the outbound path uses and comparing rustc types.
    // Exactly one impl may match; an ambiguous match fails loud rather than dispatching arbitrarily.
    let mut matched = None;
    for impls in monouts.interface_to_impls.values() {
      for (impl_t, impl_i) in impls.iter() {
        let sub_id = monouts
          .impls
          .get(impl_i)
          .expect("an impl in interface_to_impls must be in impls")
          .0
          .id();
        if citizen_or_opaque_to_rustc_ty(tcx, state.rust_crates, &sub_id) == Some(self_ty) {
          assert!(
            matched.is_none(),
            "rust interop: callback {item_name:?} Self matches more than one recorded Valen impl \
             (ambiguous reverse-callback dispatch)"
          );
          matched = Some((*impl_t, *impl_i));
        }
      }
    }
    match matched {
      Some(pair) => pair,
      None => panic!("rust interop: callback {item_name:?} matches no recorded Valen impl for {self_ty:?}"),
    }
  };

  // The typed abstract method header this override implements: reach it through the impl's edge and
  // the interface's blueprint (typing owns the abstract method set and order).
  let impl_template = Compiler::get_impl_template(state.typing_interner, matched_impl_t);
  // `interface_template_to_sub_citizen_to_edge` iterates in no guaranteed order, so a `.find()` that
  // took the first of several matches would be order-dependent. Collect and assert exactly one match.
  let matching_edges: Vec<_> = hinputs
    .interface_template_to_sub_citizen_to_edge
    .values()
    .flat_map(|m| m.values().copied())
    .filter(|edge| Compiler::get_impl_template(state.typing_interner, edge.edge_id) == impl_template)
    .collect();
  assert_eq!(
    matching_edges.len(),
    1,
    "rust interop: expected exactly one edge for the matched impl, found {}",
    matching_edges.len()
  );
  let edge = matching_edges[0];
  let interface_template_id = get_interface_template(state.typing_interner, edge.super_interface);
  let blueprint = hinputs
    .interface_template_to_edge_blueprints
    .get(&interface_template_id)
    .expect("no edge blueprint for the matched impl's interface");
  // Selection: one interface's abstract methods have distinct names, and the interface is already
  // pinned by the matched impl's edge.
  let abstract_proto_t: PrototypeT = blueprint
    .super_family_root_headers
    .iter()
    .map(|(p, _)| *p)
    .find(|p| matches!(&p.id.local_name, INameT::Function(n) if n.template.human_name.as_str() == item_name)) // ataflbz-allow: selection
    .expect("no abstract method by that name in the interface blueprint");

  let instantiator = InstantiatorI {
    opts: state.opts,
    interner: state.interner,
    typing_interner: state.typing_interner,
    scout_arena: state.scout_arena,
    keywords: state.keywords,
    rust_crates: state.rust_crates,
    hinputs,
  };
  let mut monouts = state.monouts.borrow_mut();
  let requests_before = monouts.rust_instantiation_requests.len();

  let empty_bound = DenizenBoundToDenizenCallerBoundArgI {
    func_id_to_bound_arg_prototype: IndexMap::default(),
    bound_param_impl_id_to_bound_arg_impl_id: IndexMap::default(),
  };
  let empty_subs: IndexMap<IdT, ITemplataI> = IndexMap::default();
  // Resolve the concrete override statically, the same seam a devirtualized `where implements` call
  // uses (`BoundFunctionCall` in the instantiator). This yields the plain concrete override prototype
  // directly, so the backend never builds an interface fat pointer. Non-generic callbacks flow through
  // here unchanged as the degenerate case.
  let (concrete_impl_id_t, impl_bound_args) = *monouts
    .instantiated_impl_to_typed_impl_and_bounds
    .get(&matched_impl_i)
    .expect("matched impl not recorded in instantiated_impl_to_typed_impl_and_bounds");
  let abstract_bound_args = instantiator.translate_bound_args_for_callee(
    &mut monouts,
    &abstract_proto_t.id,
    &empty_bound,
    &empty_subs,
    &RegionT::Default,
    hinputs.get_instantiation_bound_args(abstract_proto_t.id),
  );
  let override_proto = instantiator.resolve_override_prototype(
    &mut monouts,
    &concrete_impl_id_t,
    &matched_impl_i,
    &abstract_proto_t,
    &abstract_bound_args,
    impl_bound_args,
  );
  instantiator.drain_instantiation_queue(&mut monouts);
  register_instantiated_kinds(state, &monouts);

  // The instantiated body's metal name is `humanize_id(override_proto.id)` (the metal lowerer keys it
  // that way). That one string is both the extern-ABI key the wrapper's signature looks up and the
  // `vale_name` the wrapper forwards to.
  let vale_name = humanize_id(&code_map, &override_proto.id, None);
  let symbol = tcx.symbol_name(instance).name.to_string();
  let abi = compute_extern_abi(tcx, instance);
  state.extern_abis.borrow_mut().insert(vale_name.clone(), abi);
  state.callbacks.borrow_mut().push(CallbackReq { symbol, vale_name });

  // Reify any Rust leaves the callback body itself calls (e.g. `w.get()`), so rustc's collector
  // queues and codegens them — the same treatment an export body's leaves get.
  resolve_new_requests(state, tcx, &mut monouts, requests_before)
}

/// Build the synthetic MIR body rustc gets for a Vale item: one `ReifyFnPointer` cast per Rust leaf
/// (which is what puts each Rust `Instance` into the collector's queue) followed by `unreachable`. The
/// body never executes — the backend emits the real body under the same rustc-mangled symbol.
fn build_mir_body_with_mentions<'tcx>(
  tcx: TyCtxt<'tcx>,
  instance: Instance<'tcx>,
  rust_deps: &[(DefId, ty::GenericArgsRef<'tcx>)],
) -> Body<'tcx> {
  let def_id = instance.def_id();

  // Shape the locals from the host item's signature: _0 return, _1.._n args.
  let sig = tcx.fn_sig(def_id).instantiate(tcx, instance.args);
  let sig = tcx.normalize_erasing_late_bound_regions(ty::TypingEnv::fully_monomorphized(), sig);

  let span = tcx.def_span(def_id);
  let source_info = SourceInfo::outermost(span);

  let mut local_decls: IndexVec<Local, LocalDecl<'tcx>> = IndexVec::new();
  local_decls.push(LocalDecl::new(sig.output(), span)); // _0: return
  for &input_ty in sig.inputs() {
    local_decls.push(LocalDecl::new(input_ty, span));
  }

  let mut blocks: IndexVec<BasicBlock, BasicBlockData<'tcx>> = IndexVec::new();
  let mut stmts = Vec::new();

  for &(dep_def_id, dep_args) in rust_deps {
    // Each Rust leaf becomes `_k = <dep as fn(...)> as fn(...)` — a ReifyFnPointer cast of the
    // zero-sized FnDef const. The collector queues the FnDef's Instance; the value is never used.
    let fn_def_ty = Ty::new_fn_def(tcx, dep_def_id, dep_args);
    let fn_sig = tcx.fn_sig(dep_def_id).instantiate(tcx, dep_args);
    let fn_ptr_ty = Ty::new_fn_ptr(tcx, fn_sig);
    let fn_ptr_local = local_decls.push(LocalDecl::new(fn_ptr_ty, span));
    stmts.push(Statement::new(
      source_info,
      StatementKind::Assign(Box::new((
        Place::from(fn_ptr_local),
        Rvalue::Cast(
          CastKind::PointerCoercion(
            PointerCoercion::ReifyFnPointer(Safety::Safe),
            CoercionSource::Implicit,
          ),
          Operand::Constant(Box::new(ConstOperand {
            span,
            user_ty: None,
            const_: Const::zero_sized(fn_def_ty),
          })),
          fn_ptr_ty,
        ),
      ))),
    ));
  }
  blocks.push(BasicBlockData::new_stmts(
    stmts,
    Some(Terminator { source_info, kind: TerminatorKind::Unreachable }),
    false,
  ));
  let source_scopes = IndexVec::from_elem_n(
    SourceScopeData {
      span,
      parent_scope: None,
      inlined: None,
      inlined_parent_scope: None,
      local_data: ClearCrossCrate::Clear,
    },
    1,
  );
  let mut body = Body::new(
    MirSource::item(def_id),
    blocks,
    source_scopes,
    local_decls,
    IndexVec::new(),
    sig.inputs().len(),
    vec![],
    span,
    None,
    None,
  );
  // The collector reads these; our body has neither.
  body.set_required_consts(vec![]);
  body.set_mentioned_items(vec![]);
  body
}
