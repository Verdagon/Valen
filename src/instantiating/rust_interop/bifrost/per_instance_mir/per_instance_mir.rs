use std::collections::HashSet;
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
use crate::instantiating::ast::names::IdI;
use crate::instantiating::instantiated_humanizer::humanize_id;
use crate::instantiating::instantiator::{InstantiatedOutputsI, InstantiatorI};
use crate::utils::range::CodeLocationS;
use crate::instantiating::rust_interop::bifrost::bifrost_state::BifrostState;
use crate::instantiating::rust_interop::bifrost::driver_state::BIFROST_STATE;
use crate::instantiating::rust_interop::bifrost::extern_abi::compute_extern_abi;
use crate::instantiating::rust_interop::bifrost::override_queries::is_vale_codegen_target;
use super::resolve_request::{resolve_request, ResolvedRequest};

// Called by rustc, for rustc to ask us to monomorphize a Valen function.
// We'll monomorphize it and, recursively/deeply, also monomorphize all of the Valen functions
// it calls, and all the Valen functions they call, and so on. We *DON'T* monomorphize any Rust
// functions we call. Instead, we return those requests to rustc, by mentioniong them in the
// generated MIR body.
// TODO: rename
pub(crate) fn lang_per_instance_mir<'tcx>(
  tcx: TyCtxt<'tcx>,
  instance: Instance<'tcx>,
) -> Option<&'tcx Body<'tcx>> {
  let def_id = instance.def_id();
  // Decline monomorphizing any Rust function; rustc will monomorphize those itself.
  if !is_vale_codegen_target(tcx, def_id) {
    return None;
  }

  let state_ptr = BIFROST_STATE.with(|c| c.get());
  assert!(!state_ptr.is_null(), "per_instance_mir fired for a Vale item with no BifrostState armed");
  // SAFETY: the `DriverState` outlives the entire rustc invocation, so safe to access in here.
  let state: &BifrostState = unsafe { &*(state_ptr as *const BifrostState) };

  // Rust is calling into this Valen function, which means it's calling into an `exported` Valen
  // function, which means it starts with the `__vale_` prefix. Strip it off to get the export name.
  // TODO: closures shouldn't do this i think
  let stub_name = tcx.item_name(def_id).to_string();
  let export_name = stub_name.strip_prefix("__vale_").unwrap_or(&stub_name).to_string();

  // Figure out whether this is an exported Valen function (like `exported func main`, which
  // produces a `__vale_main` function), or is it a Valen impl for a Rust trait, for a Rust
  // dependency to call back into a Valen program's function.
  let is_export = {
    let h = state.hinputs.borrow();
    h.as_ref()
      .map_or(false, |h| h.function_exports.iter().any(|e| e.exported_name.0 == export_name))
  };

  // Do the actual recursive monomorphization, collecting the Rust functions that our new instances
  // want to call.
  let rust_deps: Vec<(DefId, ty::GenericArgsRef<'tcx>)> =
      if is_export {
        if stub_name == "__vale_main" {
          *state.entry_symbol.borrow_mut() = Some(tcx.symbol_name(instance).name.to_string());
        }
        let requests = instantiate_and_take_rust_requests(state, tcx, &export_name);
        let log = requests.iter().map(|r| r.log.as_str()).collect::<Vec<_>>().join(", ");
        state.firings.borrow_mut().push(format!("{stub_name} -> [{log}]"));
        requests.iter().map(|r| r.dep).collect()
      } else {
        // TODO: support other exports when Rust calls into Valen libraries.
        // TODO: refactor this area once we have that.
        unimplemented!()
      };

  // Now make the resulting MIR body to hand back to rustc, including mentions of the things we
  // want rustc to instantiate.
  let body = build_mir_body_with_mentions(tcx, instance, &rust_deps);
  Some(tcx.arena.alloc(body))
}


// Instantiates a function and, recursively/deeply, also instantiate all of the Valen functions
// it calls, and all the Valen functions they call, and so on. We *DON'T* instantiate any Rust
// functions we call. Instead, we return those requests to rustc, by mentioniong them in the
// generated MIR body.
pub(super) fn instantiate_and_take_rust_requests<'tcx>(
  bifrost_state: &BifrostState,
  tcx: TyCtxt<'tcx>,
  export_name: &str,
) -> Vec<ResolvedRequest<'tcx>> {
  let hinputs_ref = bifrost_state.hinputs.borrow();
  let hinputs = hinputs_ref.as_ref().expect("missing hinputs");
  let export = hinputs
      .function_exports
      .iter()
      .find(|e| e.exported_name.0 == export_name)
      .unwrap_or_else(|| panic!("per_instance_mir called for `{export_name}`, which isn't a Valen export"));
  let instantiator = InstantiatorI {
    opts: bifrost_state.opts,
    interner: bifrost_state.interner,
    typing_interner: bifrost_state.typing_interner,
    scout_arena: bifrost_state.scout_arena,
    keywords: bifrost_state.keywords,
    rust_crates: bifrost_state.rust_crates,
    hinputs,
  };
  let mut monouts = bifrost_state.monouts.borrow_mut();
  // TODO: get rid of this `before` list, we shouldn't need it.
  let before: HashSet<_> = monouts.rust_instantiation_requests.keys().copied().collect();
  // Trigger the actual instantiation.
  let export_i = instantiator.instantiate_exported_function(&mut monouts, export);
  bifrost_state.function_exports.borrow_mut().push(export_i);
  instantiator.drain_instantiation_queue(&mut monouts);

  // Instantiation gave us a bunch of requests in the form of PrototypeI etc.
  // Now we translate them into rustc DefId + ty::GenericArgsRef<'tcx>.
  let new_reqs: Vec<_> = monouts
      .rust_instantiation_requests
      .iter()
      .filter(|(id, _)| !before.contains(*id)) // TODO: don't do this
      .map(|(_id, proto)| (*proto, resolve_request(tcx, bifrost_state.rust_crates, proto)))
      .collect();

  // Make a bunch of FunctionExternI, which include the actual mangled final symbol, so the backend
  // knows what LLVM symbol to call.
  let code_map = |loc: CodeLocationS| format!("{:?}", loc);
  for (proto, req) in &new_reqs {
    let (def_id, args) = req.dep;
    let instance = ty::Instance::new_raw(def_id, args);
    let symbol = tcx.symbol_name(instance).name;
    let symbol_i: &str = bifrost_state.interner.bump().alloc_str(symbol);
    monouts.function_externs.push(FunctionExternI {
      prototype: *proto,
      num_inherited_generic_parameters: 0, // TODO: do we still need this?
      link_name: symbol_i,
    });
    // TODO: could we include these in the FunctionExternI?
    let abi = compute_extern_abi(tcx, instance);
    bifrost_state.extern_abis.borrow_mut().insert(humanize_id(&code_map, &proto.id, None), abi);
  }

  new_reqs.into_iter().map(|(_, req)| req).collect()
}

// Assemble the MIR body that we'll return to rustc.
fn build_mir_body_with_mentions<'tcx>(
  tcx: TyCtxt<'tcx>,
  instance: Instance<'tcx>,
  rust_deps: &[(DefId, ty::GenericArgsRef<'tcx>)],
) -> Body<'tcx> {
  let def_id = instance.def_id();

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

  // Mention all called Rust functions as ReifyFnPointer instructions, assigned into locals.
  for &(dep_def_id, dep_args) in rust_deps {
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
            // The star of the show, the ReifyFnPointer instruction that mentions the Rust function
            // that Valen is calling.
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
  // Assemble the final body. We don't make a "MIR function" per se, bodies are stored separately
  // from IDs/headers, ECS-esque.
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
  body.set_required_consts(vec![]);
  body.set_mentioned_items(vec![]);
  body
}
