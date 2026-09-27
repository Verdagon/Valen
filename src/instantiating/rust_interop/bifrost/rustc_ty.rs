use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;

use crate::instantiating::ast::names::{INameI, IStructTemplateNameI, IdI};
use crate::instantiating::ast::templata::ITemplataI;
use crate::instantiating::ast::types::KindIT;
use crate::interner::StrI;
use crate::typing::rust_interop::find_rust_def_id;

fn templata_to_rustc_ty<'tcx>(tcx: TyCtxt<'tcx>, templata: &ITemplataI) -> Option<Ty<'tcx>> {
  unimplemented!()
}

pub(super) fn kind_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  kind: &KindIT,
) -> Option<Ty<'tcx>> {
  match kind {
    KindIT::StructIT(s) => citizen_or_opaque_to_rustc_ty(tcx, rust_crates, &s.id),
    _ => unimplemented!(),
  }
}

fn citizen_or_opaque_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<Ty<'tcx>> {
  if let Some(ty) = citizen_to_rustc_ty(tcx, rust_crates, id) {
    return Some(ty);
  }
  unimplemented!()
}

pub(super) fn citizen_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<Ty<'tcx>> {
  let (def_id, arg_tys) = citizen_def_id_and_args(tcx, rust_crates, id)?;
  let args = build_generic_args(tcx, def_id, &arg_tys);
  Some(Ty::new_adt(tcx, tcx.adt_def(def_id), args))
}

pub(super) fn citizen_def_id_and_args<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<(DefId, Vec<Ty<'tcx>>)> {
  let (human_name, template_args) = match id.local_name {
    INameI::StructName(sn) => match sn.template {
      IStructTemplateNameI::StructTemplate(t) => (t.human_name, sn.template_args),
      _ => unimplemented!(),
    },
    _ => unimplemented!(),
  };
  let def_id = if rust_crates.contains(&id.package_coord.module) {
    find_rust_def_id(tcx, id.package_coord.module, id.package_coord.packages.as_slice(), human_name)?
  } else {
    unimplemented!()
  };
  let arg_tys: Vec<Ty<'tcx>> =
    template_args.iter().map(|t| templata_to_rustc_ty(tcx, t)).collect::<Option<_>>()?;
  Some((def_id, arg_tys))
}

pub(super) fn build_generic_args<'tcx>(
  tcx: TyCtxt<'tcx>,
  def_id: DefId,
  arg_tys: &[Ty<'tcx>],
) -> ty::GenericArgsRef<'tcx> {
  let mut types = arg_tys.iter().copied();
  ty::GenericArgs::for_item(tcx, def_id, |param, _| match param.kind {
    ty::GenericParamDefKind::Lifetime => unimplemented!(),
    ty::GenericParamDefKind::Type { .. } => types
      .next()
      .unwrap_or_else(|| panic!("too few type args for {def_id:?}"))
      .into(),
    ty::GenericParamDefKind::Const { .. } => {
      unimplemented!()
    }
  })
}
