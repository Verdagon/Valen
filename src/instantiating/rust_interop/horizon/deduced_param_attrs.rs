use rustc_middle::middle::deduced_param_attrs::DeducedParamAttrs;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::LocalDefId;

use super::override_queries::{is_vale_codegen_target, DEFAULT_DEDUCED_PARAM_ATTRS};

/// Claim no deduced param attrs for a Vale stub item: its `unreachable!()` MIR touches no params, so
/// rustc would infer `readonly`/`captures(none)` and stamp them at every call site — a lie against
/// Vale's real body. `&[]` is the conservative safe default. Delegates otherwise. rustc only asks this
/// when optimizing, so debug builds never reach it.
pub(super) fn lang_deduced_param_attrs<'tcx>(
  tcx: TyCtxt<'tcx>,
  def_id: LocalDefId,
) -> &'tcx [DeducedParamAttrs] {
  if is_vale_codegen_target(tcx, def_id.to_def_id()) {
    return &[];
  }
  let default = DEFAULT_DEDUCED_PARAM_ATTRS.get().expect("missing DEFAULT_DEDUCED_PARAM_ATTRS");
  default(tcx, def_id)
}
