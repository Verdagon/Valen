use rustc_middle::ty::layout::{LayoutError, TyAndLayout};
use rustc_middle::ty::{PseudoCanonicalInput, Ty, TyCtxt};

use super::override_queries::DEFAULT_LAYOUT_OF;
use super::vale_opaque::read_opaque_typeid;

// TODO: rename
pub(super) fn lang_layout_of<'tcx>(
  tcx: TyCtxt<'tcx>,
  query: PseudoCanonicalInput<'tcx, Ty<'tcx>>,
) -> Result<TyAndLayout<'tcx>, &'tcx LayoutError<'tcx>> {
  let default = DEFAULT_LAYOUT_OF.get().expect("missing DEFAULT_LAYOUT_OF");
  let ty = query.value;
  match read_opaque_typeid(tcx, ty) {
    None => default(tcx, query),
    Some(_) => unimplemented!(),
  }
}
