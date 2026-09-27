use rustc_middle::middle::deduced_param_attrs::DeducedParamAttrs;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::LocalDefId;

use super::override_queries::{is_vale_codegen_target, DEFAULT_DEDUCED_PARAM_ATTRS};

// TODO: rename
pub(super) fn lang_deduced_param_attrs<'tcx>(
  tcx: TyCtxt<'tcx>,
  def_id: LocalDefId,
) -> &'tcx [DeducedParamAttrs] {
  if is_vale_codegen_target(tcx, def_id.to_def_id()) {
    // Don't let rustc deduce anything from the function's body, because the function's body is a
    // lie that we replace later.
    return &[];
  } else {
    let default =
        DEFAULT_DEDUCED_PARAM_ATTRS.get().expect("missing DEFAULT_DEDUCED_PARAM_ATTRS");
    default(tcx, def_id)
  }
}
