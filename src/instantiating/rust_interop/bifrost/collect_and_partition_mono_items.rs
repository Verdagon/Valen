use rustc_hir::attrs::Linkage;
use rustc_middle::mir::mono::{CodegenUnit, MonoItem, MonoItemPartitions, Visibility};
use rustc_middle::ty::TyCtxt;
use std::collections::HashSet;

use super::bifrost_state::BifrostState;
use super::driver_state::BIFROST_STATE;
use super::override_queries::{is_vale_codegen_target, DEFAULT_COLLECT_AND_PARTITION};

// Does two things to rustc's CGUs:
//  1. Drop all the Valen functions, so rustc doesn't try to generate .o bodies for them.
//  2. Put `External` linkage on any Rust function that Valen calls.
// It also re-fires per_instance_mir on every Valen function, in case rustc skipped it for warm
// builds.
pub(super) fn lang_collect_and_partition_mono_items<'tcx>(
  tcx: TyCtxt<'tcx>,
  key: (),
) -> MonoItemPartitions<'tcx> {
  let upstream = DEFAULT_COLLECT_AND_PARTITION
    .get()
    .expect("missing DEFAULT_COLLECT_AND_PARTITION");
  let MonoItemPartitions { codegen_units: upstream_cgus, all_mono_items: reachable, .. } =
    upstream(tcx, key);

  let state_ptr = BIFROST_STATE.with(|c| c.get());
  if state_ptr.is_null() {
    panic!("collect_and_partition_mono_items fired with no BifrostState armed")
  }
  // SAFETY: `DriverState` outlives the entire rustc invocation, so this is safe to access.
  let state: &BifrostState = unsafe { &*(state_ptr as *const BifrostState) };
  // TODO: possible bug, we read this before the per_instance_mir re-call, so this might not have
  // the up-to-date things yet.
  let leaf_symbols: HashSet<String> =
      state.monouts.borrow().function_externs.iter().map(|e| e.link_name.to_string()).collect();

  let mut filtered_cgus: Vec<CodegenUnit<'tcx>> = Vec::with_capacity(upstream_cgus.len());
  for cgu in upstream_cgus.iter() {
    let mut new_cgu = CodegenUnit::new(cgu.name());
    for (&mono_item, &data) in cgu.items() {
      if is_vale_codegen_target(tcx, mono_item.def_id()) {
        // On incremental builds, rustc might reuse its memoized items_of_instance results, which
        // means items_of_instance isn't calling our per_instance_mir.
        // Because of that, per_instance_mir might not have fired on incremental builds.
        // Just in case, re-call it here.
        // It's safe to re-call it because it's memoized by rustc.
        if let MonoItem::Fn(instance) = mono_item {
          let _ = tcx.per_instance_mir(instance);
          // Discard the body, the only thing that matters is per_instance_mir's side effects, which
          // generate the instantiated info that we later use for codegen.
        }
      } else {
        // If we get here, we're handling a Rust function.

        let mut data = data;
        if leaf_symbols.contains(mono_item.symbol_name(tcx).name) {
          // If we get here, it's a Rust function that we called from Valen.
          // Make it external linkage so that we can call them from other LLVM modules.
          data.linkage = Linkage::External;
          data.visibility = Visibility::Default;
        }
        new_cgu.items_mut().insert(mono_item, data);
      }
    }
    if cgu.is_primary() {
      new_cgu.make_primary();
    }
    if cgu.is_code_coverage_dead_code_cgu() {
      new_cgu.make_code_coverage_dead_code_cgu();
    }
    // TODO: not sure why we need this
    new_cgu.compute_size_estimate();
    filtered_cgus.push(new_cgu);
  }

  MonoItemPartitions {
    codegen_units: tcx.arena.alloc_from_iter(filtered_cgus),
    all_mono_items: reachable,
  }
}
