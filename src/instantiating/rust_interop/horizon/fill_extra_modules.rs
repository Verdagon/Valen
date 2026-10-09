use rustc_codegen_llvm::ModuleLlvm;
use rustc_codegen_ssa::ModuleCodegen;
use rustc_middle::ty::{self, TyCtxt};
use std::collections::HashMap;

use crate::backend_ffi::backend_inputs::{BackendInputs, BackendMode, Callback, InteropInputs};
use crate::backend_ffi::metal_cache::MetalCache;
use crate::backend_ffi::metal_lowerer::{populate_metal_cache, StructLayout};
use crate::backend_ffi::{compile, BackendCompileOptions};
use crate::instantiating::instantiated_humanizer::humanize_id;
use crate::instantiating::instantiator::InstantiatorI;
use crate::utils::code_hierarchy::FileCoordinateMap;
use crate::utils::range::CodeLocationS;

use super::bifrost_state::BifrostState;
use super::driver_state::horizon_state;
use super::rustc_ty::citizen_to_rustc_ty;

/// The `fill_extra_modules` hook handler. rustc calls it once per `codegen_crate`, synchronously,
/// before async codegen starts — by which point every `per_instance_mir` call has run, so the
/// instantiator state is complete. It lowers the Vale program, emits its bodies into a fresh module,
/// and returns that module for rustc to optimize and link like any other CGU.
pub fn consumer_fill_modules<'tcx>(tcx: TyCtxt<'tcx>) -> Vec<ModuleCodegen<ModuleLlvm>> {
  let state = horizon_state("fill_extra_modules");
  let (rc, module) = emit_vale_into_fresh_module(state, tcx);
  state.firings.borrow_mut().push(format!("consumer_fill_modules emitted rc={rc}"));
  // A nonzero rc means the C++ backend rejected its own emission (e.g. LLVMVerifyModule failed on the
  // Vale IR in the module). Fail loudly rather than let rustc link a silently-broken module.
  assert_eq!(rc, 0, "backend_compile_program_into returned {rc}");
  vec![module]
}

/// Package the instantiated Vale program and emit its bodies into a fresh module. No monomorphization
/// happens here: `per_instance_mir` already instantiated exactly what rustc demanded and retained the
/// exports it walked, so `assemble_hinputs` only packages that `monouts`.
fn emit_vale_into_fresh_module<'tcx>(
  state: &BifrostState,
  tcx: TyCtxt<'tcx>,
) -> (i32, ModuleCodegen<ModuleLlvm>) {
  let hinputs_ref = state.hinputs.borrow();
  let hinputs = hinputs_ref.as_ref().expect("missing hinputs");
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

  // Ask rustc for the size and alignment of each struct that has a rustc type, keyed by the humanized
  // name of its instantiated id. That name is the one the metal lowerer gives the struct kind, so
  // `Unsafe::defineStruct` finds the layout by `structKind->fullName->name`. A struct with no rustc
  // type (a Vale struct that crosses only as an opaque blob, if at all) is sized by the backend from
  // its members as usual.
  let typing_env = ty::TypingEnv::fully_monomorphized();
  let code_map = |loc: CodeLocationS| format!("{:?}", loc);
  let mut struct_layouts: HashMap<String, StructLayout> = HashMap::new();
  for s in hinputs_i.structs.iter() {
    let id = &s.instantiated_citizen.id;
    let Some(ty) = citizen_to_rustc_ty(tcx, state.rust_crates, id) else {
      continue;
    };
    let layout = tcx
      .layout_of(typing_env.as_query_input(ty))
      .unwrap_or_else(|e| panic!("rust interop: rustc could not lay out {ty:?}: {e:?}"));
    struct_layouts.insert(
      humanize_id(&code_map, id, None),
      StructLayout { size: layout.size.bytes(), align: layout.align.abi.bytes() },
    );
  }

  let extern_abis = state.extern_abis.borrow();
  let cache = MetalCache::new();
  // The interop path has no source code map on hand, so lower with an empty one (source ranges resolve
  // to a no-op location, which the backend discards anyway).
  let empty_code_map: FileCoordinateMap<String> = FileCoordinateMap::new();
  let program =
    populate_metal_cache(&cache, &hinputs_i, &empty_code_map, &struct_layouts, &extern_abis);

  // Mint one fresh module (a fresh LLVMContext + LLVMModule + TargetMachine) the way rustc mints its
  // own per-CGU modules, and take its raw handles for the C++ backend. rustc owns the module from then
  // on and disposes it after codegen.
  let name = "vale_cgu";
  let mut module = ModuleLlvm::new(tcx, name);
  let llcx = module.llcx_raw_mut();
  let llmod = module.llmod_raw();
  let opts = BackendCompileOptions { verify: true, ..BackendCompileOptions::default() };
  let entry_symbol = state.entry_symbol.borrow();
  // The Rust→Vale callbacks, sorted by their rustc symbol (which encodes self/args/trait/method) so
  // the emitted module is byte-stable regardless of the collector's walk order.
  let callback_reqs = state.callbacks.borrow();
  let mut callbacks: Vec<Callback> = callback_reqs
    .iter()
    .map(|c| Callback { symbol: c.symbol.as_str(), vale_name: c.vale_name.as_str() })
    .collect();
  callbacks.sort_by(|a, b| a.symbol.cmp(b.symbol));
  let rc = compile(BackendInputs {
    cache: &cache,
    program: &program,
    options: opts,
    mode: BackendMode::Interop(InteropInputs {
      context: llcx,
      module: llmod,
      entry_symbol: entry_symbol.as_deref(),
      callbacks,
    }),
    absolute_source_paths: vec![],
  });
  (rc, ModuleCodegen::new_regular(name, module))
}
