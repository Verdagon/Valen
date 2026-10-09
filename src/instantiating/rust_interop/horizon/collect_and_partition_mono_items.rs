use rustc_hir::attrs::Linkage;
use rustc_middle::mir::mono::{CodegenUnit, MonoItem, MonoItemPartitions, Visibility};
use rustc_middle::ty::{self, TyCtxt};
use std::collections::HashSet;

use super::driver_state::horizon_state;
use super::override_queries::{is_vale_codegen_target, DEFAULT_COLLECT_AND_PARTITION};
use super::rustc_ty::{build_generic_args, resolve_local_fn};

/// Rebuild rustc's CGUs with two interop fixups. (1) Drop Vale's `#[vale::emit_consumer_body]` stub
/// items, so rustc emits no `.o` for a body Vale itself emits under the same mangled name. (2) Force
/// each surviving reified Rust leaf to `External` linkage.
///
/// Why (2): rustc's release partitioner internalizes those leaves — for an `Executable` crate the item
/// is `Hidden`/`can_be_internalized`, and `internalize_symbols` demotes it to `Internal` because its
/// only *apparent* user shares its CGU. But the real user is Vale's out-of-band `vale_cgu` object,
/// which references the leaf as an external symbol, so `Internal` strands it as an undefined symbol at
/// link. Debug builds leave them `External` and link; this reproduces that for release. The leaves are
/// exactly the `FunctionExternI` the provider materialized, so each item's `symbol_name` is matched
/// against their `link_name`s (never `def_path_str`, which ICEs in this non-diagnostic context).
///
/// `leaf_symbols` is read before the re-fires below, so on a warm build — where the re-fires are what
/// fill `function_externs` — no leaf is promoted. That only bites warm *release* builds; bifrost's
/// handoff tracks the same ordering bug.
pub(super) fn lang_collect_and_partition_mono_items<'tcx>(
  tcx: TyCtxt<'tcx>,
  key: (),
) -> MonoItemPartitions<'tcx> {
  let upstream = DEFAULT_COLLECT_AND_PARTITION.get().expect("missing DEFAULT_COLLECT_AND_PARTITION");
  let MonoItemPartitions { codegen_units: upstream_cgus, all_mono_items: reachable, .. } =
    upstream(tcx, key);

  let state = horizon_state("collect_and_partition_mono_items");
  let leaf_symbols: HashSet<String> =
    state.monouts.borrow().function_externs.iter().map(|e| e.link_name.to_string()).collect();

  // Exports first, for warm-rebuild determinism. Re-fire `per_instance_mir` for every Vale EXPORT
  // before the CGU loop below re-fires any callback. An export's provider instantiates its body and
  // drains, which POPULATES `monouts` and `opaque_universe`; a callback's provider READS them and
  // asserts every crossed type is present. On a warm rebuild the upstream partitioner served
  // `items_of_instance` from the incremental cache and never called `per_instance_mir`, so the state
  // starts empty and this function is its sole re-populator — but the CGU order rustc hands us does not
  // guarantee the export precedes the callback. A cold build gets populate-then-read for free, since
  // the collector discovers a callback only after the export's body reifies the caller that reaches
  // it. Seeding from the export list (sorted, for a byte-stable order) restores that invariant, and
  // also covers an export that no Rust code calls and so is in no CGU. The CGU loop's re-fires of these
  // same exports are then cache hits: `per_instance_mir` is never disk-cached and is in-memory-cached,
  // so its provider runs at most once per instance per build.
  //
  // Only single-level reverse callbacks are ordered this way. A callback whose body hands another
  // lambda to a Rust trait would need callback-to-callback ordering; nothing exercises that yet.
  let mut export_names: Vec<String> = state
    .hinputs
    .borrow()
    .as_ref()
    .expect("missing hinputs")
    .function_exports
    .iter()
    .map(|e| e.exported_name.0.to_string())
    .collect();
  export_names.sort();
  for name in export_names {
    if let Some(def_id) = resolve_local_fn(tcx, &format!("__vale_{name}")) {
      let instance = ty::Instance::new_raw(def_id, build_generic_args(tcx, def_id, &[]));
      let _ = tcx.per_instance_mir(instance);
    }
  }

  let mut filtered_cgus: Vec<CodegenUnit<'tcx>> = Vec::with_capacity(upstream_cgus.len());
  for cgu in upstream_cgus.iter() {
    let mut new_cgu = CodegenUnit::new(cgu.name());
    for (&mono_item, &data) in cgu.items() {
      if is_vale_codegen_target(tcx, mono_item.def_id()) {
        // Re-fire `per_instance_mir` for our stub instances. Its provider's side effects (the entry
        // symbol, the instantiated bodies, the callbacks) are the emit's only inputs, but rustc reaches
        // it only through the disk-cached `items_of_instance` — so on a *warm* rebuild the collector
        // never calls it. This query is `eval_always`, so it runs every build, and calling
        // `per_instance_mir` here forces the side effect to fire; on a cold build it is a cache hit.
        // The body is discarded; only the side effect matters here.
        if let MonoItem::Fn(instance) = mono_item {
          let _ = tcx.per_instance_mir(instance);
        }
        continue;
      }
      let mut data = data;
      if leaf_symbols.contains(mono_item.symbol_name(tcx).name) {
        // Undo the release internalize so Vale's out-of-band `vale_cgu` reference resolves at link.
        data.linkage = Linkage::External;
        data.visibility = Visibility::Default;
      }
      new_cgu.items_mut().insert(mono_item, data);
    }
    if cgu.is_primary() {
      new_cgu.make_primary();
    }
    if cgu.is_code_coverage_dead_code_cgu() {
      new_cgu.make_code_coverage_dead_code_cgu();
    }
    // A rebuilt CGU starts with a zero size estimate, and rustc asserts a non-empty CGU has a nonzero
    // one before it sorts CGUs by it.
    new_cgu.compute_size_estimate();
    filtered_cgus.push(new_cgu);
  }

  MonoItemPartitions {
    codegen_units: tcx.arena.alloc_from_iter(filtered_cgus),
    all_mono_items: reachable,
  }
}
