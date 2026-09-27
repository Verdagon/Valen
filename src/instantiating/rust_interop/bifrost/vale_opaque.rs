use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::CRATE_DEF_ID;
use rustc_span::def_id::DefId;

// Given a Ty that is e.g. __ValeOpaque<1337>, return the 1337.
pub(super) fn read_opaque_typeid<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<u64> {
  let ty::TyKind::Adt(adt_def, args) = ty.kind() else {
    return None;
  };
  let opaque_def_id = resolve_local_type(tcx, "__ValeOpaque")?;
  if adt_def.did() != opaque_def_id {
    return None;
  }
  args.const_at(0).try_to_leaf().map(|scalar| scalar.to_u64())
}

// Look up a type in the current crate, with the given name.
fn resolve_local_type(tcx: TyCtxt<'_>, name: &str) -> Option<DefId> {
  for child in tcx.module_children_local(CRATE_DEF_ID) {
    if let Res::Def(DefKind::Struct, def_id) = child.res {
      if child.ident.name.as_str() == name {
        return Some(def_id);
      }
    }
  }
  None
}
