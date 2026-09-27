use rustc_codegen_llvm::ModuleLlvm;
use rustc_codegen_ssa::ModuleCodegen;
use rustc_middle::ty::{self, TyCtxt};
use std::collections::HashMap;

use crate::backend_ffi::backend_inputs::{BackendInputs, BackendMode, InteropInputs};
use crate::backend_ffi::metal_cache::MetalCache;
use crate::backend_ffi::metal_lowerer::{populate_metal_cache, StructLayout};
use crate::backend_ffi::{compile, BackendCompileOptions};
use crate::instantiating::instantiated_humanizer::humanize_id;
use crate::instantiating::instantiator::InstantiatorI;
use crate::utils::code_hierarchy::FileCoordinateMap;
use crate::utils::range::CodeLocationS;

use super::bifrost_state::BifrostState;
use super::driver_state::BIFROST_STATE;
use super::rustc_ty::citizen_to_rustc_ty;

// Our `fill_extra_modules` handler. rustc calls this to ask us if we want to add any LLVM modules
// into its optimization/linking pipeline.
pub fn consumer_fill_modules<'tcx>(tcx: TyCtxt<'tcx>) -> Vec<ModuleCodegen<ModuleLlvm>> {
  let state_ptr = BIFROST_STATE.with(|c| c.get());
  assert!(!state_ptr.is_null(), "missing BIFROST_STATE");
  // SAFETY: BIFROST_STATE outlives the entire rustc instance, so safe to access during rustc
  // callbacks.
  let state: &BifrostState = unsafe { &*(state_ptr as *const BifrostState) };

  let (rc, module) = emit_vale_into_fresh_module(state, tcx);
  state.firings.borrow_mut().push(format!("consumer_fill_modules emitted rc={rc}"));
  assert_eq!(rc, 0, "backend_compile_program_into returned {rc}");
  vec![module]
}

// Called by rustc, invokes the Valen backend.
fn emit_vale_into_fresh_module<'tcx>(
  state: &BifrostState,
  tcx: TyCtxt<'tcx>,
) -> (i32, ModuleCodegen<ModuleLlvm>) {
  let hinputs_ref = state.hinputs.borrow();
  let hinputs = hinputs_ref.as_ref().expect("missing hinputs");
  // Makes an InstantiatorI instance, but instantiation already happened.
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
  let function_exports: Vec<_> = state.function_exports.borrow_mut().drain(..).collect();
  let hinputs_i =
    instantiator.assemble_hinputs(&mut *monouts, Vec::new(), function_exports, Vec::new());

  let typing_env = ty::TypingEnv::fully_monomorphized();

  // Calculate all struct layouts
  let mut struct_layouts: HashMap<String, StructLayout> = HashMap::new();
  for s in hinputs_i.structs.iter() {
    let id = &s.instantiated_citizen.id;
    let ty =
        match citizen_to_rustc_ty(tcx, state.rust_crates, id) {
          Some(t) => t,
          None => continue,
        };
    let layout =
        match tcx.layout_of(typing_env.as_query_input(ty)) {
          Ok(l) => l,
          Err(_) => unimplemented!(),
        };
    struct_layouts.insert(
      humanize_id(&|loc: CodeLocationS| format!("{:?}", loc), id, None),
      StructLayout { size: layout.size.bytes(), align: layout.align.abi.bytes() },
    );
  }

  // Create the MetalCache and send the program into C++
  let extern_abis = state.extern_abis.borrow();
  let cache = MetalCache::new();
  let empty_code_map: FileCoordinateMap<String> = FileCoordinateMap::new();
  let program =
      populate_metal_cache(&cache, &hinputs_i, &empty_code_map, &struct_layouts, &extern_abis);

  // Make a fresh LLVM module and have the backend codegen all Valen things into it
  let name = "vale_cgu";
  let mut module = ModuleLlvm::new(tcx, name);
  let llcx = module.llcx_raw_mut();
  let llmod = module.llmod_raw();
  let opts = BackendCompileOptions { verify: true, ..BackendCompileOptions::default() };
  let entry_symbol = state.entry_symbol.borrow();
  let rc = compile(BackendInputs {
    cache: &cache,
    program: &program,
    options: opts,
    mode: BackendMode::Interop(InteropInputs {
      context: llcx,
      module: llmod,
      entry_symbol: entry_symbol.as_deref(),
      callbacks: Vec::new(),
    }),
    absolute_source_paths: vec![],
  });
  (rc, ModuleCodegen::new_regular(name, module))
}
