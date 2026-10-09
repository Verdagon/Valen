use rustc_middle::middle::deduced_param_attrs::DeducedParamAttrs;
use rustc_middle::mir::mono::MonoItemPartitions;
use rustc_middle::ty::layout::{LayoutError, TyAndLayout};
use rustc_middle::ty::{PseudoCanonicalInput, Ty, TyCtxt};
use rustc_middle::util::Providers;
use rustc_span::def_id::{DefId, LocalDefId};
use rustc_span::Symbol;
use std::sync::OnceLock;

use super::collect_and_partition_mono_items::lang_collect_and_partition_mono_items;
use super::deduced_param_attrs::lang_deduced_param_attrs;
use super::layout_of::lang_layout_of;
use super::per_instance_mir::lang_per_instance_mir;

// rustc's default providers, saved before we override them so the overrides can delegate for
// non-Vale items. Set once per process in `vale_override_queries`.
pub(super) static DEFAULT_COLLECT_AND_PARTITION: OnceLock<
  for<'tcx> fn(TyCtxt<'tcx>, ()) -> MonoItemPartitions<'tcx>,
> = OnceLock::new();
pub(super) static DEFAULT_DEDUCED_PARAM_ATTRS: OnceLock<
  for<'tcx> fn(TyCtxt<'tcx>, LocalDefId) -> &'tcx [DeducedParamAttrs],
> = OnceLock::new();
pub(super) static DEFAULT_LAYOUT_OF: OnceLock<
  for<'tcx> fn(
    TyCtxt<'tcx>,
    PseudoCanonicalInput<'tcx, Ty<'tcx>>,
  ) -> Result<TyAndLayout<'tcx>, &'tcx LayoutError<'tcx>>,
> = OnceLock::new();

/// The `override_queries` hook: a bare `fn` (rustc query providers cannot capture state), installed
/// from bifrost's callbacks' `config()`.
///
/// - `per_instance_mir` → horizon's provider: drives Vale's instantiator for an export, or for a
///   Rust→Vale callback.
/// - `collect_and_partition_mono_items` → strips Vale's `#[vale::emit_consumer_body]` stub bodies from
///   rustc's codegen (Vale emits the real bodies under the same mangled names), force-promotes each
///   reified Rust leaf to `External` linkage, and re-fires `per_instance_mir` on warm builds.
/// - `deduced_param_attrs` → `&[]` for those same items, so rustc infers no `readonly`/`captures(none)`
///   from the `unreachable!()` body, which would be silent UB against Vale's real body.
/// - `layout_of` → Vale's real size and alignment for a `__ValeOpaque<typeid>` naming an instantiated
///   Vale struct, so a Vale struct crossing by value is neither zero-sized nor decomposed.
pub fn vale_override_queries(_session: &rustc_session::Session, providers: &mut Providers) {
  providers.queries.per_instance_mir = lang_per_instance_mir;
  let _ = DEFAULT_COLLECT_AND_PARTITION.set(providers.queries.collect_and_partition_mono_items);
  let _ = DEFAULT_DEDUCED_PARAM_ATTRS.set(providers.queries.deduced_param_attrs);
  let _ = DEFAULT_LAYOUT_OF.set(providers.queries.layout_of);
  providers.queries.collect_and_partition_mono_items = lang_collect_and_partition_mono_items;
  providers.queries.deduced_param_attrs = lang_deduced_param_attrs;
  providers.queries.layout_of = lang_layout_of;
}

/// Is this item one whose body Vale supplies? Gate: it carries `#[vale::emit_consumer_body]`, which
/// the final file puts on every export stub and every projected callback method.
pub(super) fn is_vale_codegen_target(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
  tcx.has_attrs_with_path(
    def_id,
    &[Symbol::intern("vale"), Symbol::intern("emit_consumer_body")],
  )
}
