// Lowering Vale instantiated kinds to rustc `Ty`s, for a Rust callee's generic arguments, a leaf's
// receiver, a drop shim's argument, and struct layouts.
//
// A Vale citizen crosses to Rust one of two ways:
//  - A Rust-backed citizen (its package's module is a Rust crate), or a Vale citizen the final file
//    *projects* into the compiled crate (a struct implementing a Rust trait, or an imported trait's
//    anonymous substruct), lowers to its own `Adt`.
//  - Any other Vale type — a lambda closure, a struct that isn't projected — has no nameable Rust
//    identity, so it crosses as the opaque blob `__ValeOpaque<typeid>`: rustc monomorphizes over it
//    without ever inspecting it, and the `layout_of` override tells rustc its real size.

use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::CRATE_DEF_ID;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;

use crate::instantiating::ast::names::{IInterfaceTemplateNameI, INameI, IStructTemplateNameI, IdI};
use crate::instantiating::ast::templata::ITemplataI;
use crate::instantiating::ast::types::KindIT;
use crate::instantiating::instantiated_humanizer::humanize_id;
use crate::interner::StrI;
use crate::typing::rust_interop::horizon::tyctxt_oracle::resolve_crate_qualified_path;
use crate::typing::rust_interop::horizon::typeid::anon_substruct_rust_name;
use crate::typing::rust_interop::typeid;
use crate::utils::range::CodeLocationS;

/// Convert one Vale type templata to a rustc `Ty`. Only type (`Kind`) templatas participate in a
/// callee's generic args; other templata kinds (function/impl bounds, integer/bool *values*) return
/// `None`.
pub(super) fn templata_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  templata: &ITemplataI,
) -> Option<Ty<'tcx>> {
  match templata {
    ITemplataI::Kind(k) => kind_to_rustc_ty(tcx, rust_crates, &k.kind),
    _ => None,
  }
}

/// Convert a Vale instantiated kind to a rustc `Ty`. Primitives map to their rustc counterparts and
/// citizens go through `citizen_or_opaque_to_rustc_ty`; anything else (arrays, strings, floats, Vale
/// references) has no rustc form yet and returns `None`.
pub(super) fn kind_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  kind: &KindIT,
) -> Option<Ty<'tcx>> {
  match kind {
    KindIT::IntIT(i) => match i.bits {
      32 => Some(tcx.types.i32),
      64 => Some(tcx.types.i64),
      _ => None,
    },
    KindIT::BoolIT(_) => Some(tcx.types.bool),
    KindIT::USizeIT(_) => Some(tcx.types.usize),
    KindIT::StructIT(s) => citizen_or_opaque_to_rustc_ty(tcx, rust_crates, &s.id),
    KindIT::InterfaceIT(i) => citizen_or_opaque_to_rustc_ty(tcx, rust_crates, &i.id),
    _ => None,
  }
}

/// Lower a citizen kind, choosing between its own `Adt` and the opaque `__ValeOpaque<typeid>`. A
/// Rust-backed citizen that fails to resolve is a genuine dependency-resolution failure, not an opaque
/// type, so it stays `None` (and fails loudly at the call site rather than being masked).
pub(super) fn citizen_or_opaque_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<Ty<'tcx>> {
  if let Some(ty) = citizen_to_rustc_ty(tcx, rust_crates, id) {
    return Some(ty);
  }
  if rust_crates.contains(&id.package_coord.module) {
    return None;
  }
  opaque_ty_for_typeid(tcx, opaque_typeid(id))
}

/// `__ValeOpaque<tid>` as a rustc type, or `None` when the compiled crate declares no `__ValeOpaque`.
/// The type every Vale-internal type crosses as, given its typeid.
pub(crate) fn opaque_ty_for_typeid<'tcx>(tcx: TyCtxt<'tcx>, tid: u64) -> Option<Ty<'tcx>> {
  let opaque_def_id = resolve_local_type(tcx, "__ValeOpaque")?;
  Some(Ty::new_adt(tcx, tcx.adt_def(opaque_def_id), build_opaque_args(tcx, opaque_def_id, tid)))
}

/// The one definition of the typeid a Vale kind crosses under: the content hash of its humanized
/// instantiated id. Stamped by `opaque_ty_for_typeid` on the way out, and the key `opaque_universe` is
/// registered under, so the inbound decode (`read_opaque_typeid` → universe) recovers the same kind.
pub(super) fn opaque_typeid(id: &IdI) -> u64 {
  let code_map = |loc: CodeLocationS| format!("{:?}", loc);
  typeid(&humanize_id(&code_map, id, None))
}

/// The `GenericArgs` for `__ValeOpaque<HASH>`: the typeid interned as its single `const T: u64` argument.
fn build_opaque_args<'tcx>(
  tcx: TyCtxt<'tcx>,
  opaque_def_id: DefId,
  tid: u64,
) -> ty::GenericArgsRef<'tcx> {
  let typeid_const =
    ty::Const::from_bits(tcx, tid as u128, ty::TypingEnv::fully_monomorphized(), tcx.types.u64);
  ty::GenericArgs::for_item(tcx, opaque_def_id, |param, _| match param.kind {
    ty::GenericParamDefKind::Const { .. } => typeid_const.into(),
    ty::GenericParamDefKind::Lifetime => tcx.lifetimes.re_erased.into(),
    ty::GenericParamDefKind::Type { .. } => {
      panic!("__ValeOpaque must have only its const param, got a type param")
    }
  })
}

/// Recover the content-addressed typeid from an `__ValeOpaque<HASH>` type — the inverse of
/// `opaque_ty_for_typeid`. rustc hands a callback `Instance` whose `Self` type carries
/// `__ValeOpaque<HASH>` for each Vale-internal type arg (e.g. a lambda functor), and the HASH is
/// `typeid(humanize_id(kind.id))`, so this reader plus the typeid→kind universe recovers which Vale
/// kind the opaque blob stands for. `None` for any other type.
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

/// Lower a citizen (struct or enum/trait interface) kind to its rustc `Adt` `Ty`: resolve the
/// `DefId`, convert the citizen's own type arguments (recursively, so `Holder<int>` lowers through
/// this same path), and build the `Adt`.
pub(super) fn citizen_to_rustc_ty<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<Ty<'tcx>> {
  let (def_id, arg_tys) = citizen_def_id_and_args(tcx, rust_crates, id)?;
  let args = build_generic_args(tcx, def_id, &arg_tys);
  Some(Ty::new_adt(tcx, tcx.adt_def(def_id), args))
}

/// A citizen's rustc `DefId` and its converted type arguments, from its instantiated id.
///
/// A Rust-backed citizen lives in a loaded dependency crate: its package coordinate holds the crate
/// and module path, and its name the item. A Vale citizen the final file projects — a struct that
/// implements a Rust trait, or an imported trait's anonymous substruct — lives in the crate being
/// compiled, which `resolve_crate_qualified_path` (dependency crates only) can't see, so it resolves
/// locally by name. `None` for any other Vale citizen, which then crosses as an opaque blob.
pub(super) fn citizen_def_id_and_args<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  id: &IdI,
) -> Option<(DefId, Vec<Ty<'tcx>>)> {
  let (human_name, template_args): (String, _) = match id.local_name {
    INameI::StructName(sn) => match sn.template {
      IStructTemplateNameI::StructTemplate(t) => (t.human_name.as_str().to_string(), sn.template_args),
      _ => return None,
    },
    INameI::InterfaceName(inm) => {
      let IInterfaceTemplateNameI::InterfaceTemplate(t) = inm.template;
      (t.human_namee.as_str().to_string(), inm.template_args)
    }
    // The anonymous substruct auto-generated for an imported trait — a lambda handed to
    // `SomeTrait((..) => {..})`. Its Vale name (`<interface>.anonymous`) is not a Rust identifier, so
    // it crosses under the mangled `<interface>__anon` the final file also emits, resolved locally like
    // a hand-written forwarder struct rather than as an opaque blob — so only the functor it wraps is
    // opaque, not the whole substruct.
    INameI::AnonymousSubstruct(asn) => {
      let IInterfaceTemplateNameI::InterfaceTemplate(t) = asn.template.interface;
      (anon_substruct_rust_name(t.human_namee.as_str()), asn.template_args)
    }
    _ => return None,
  };
  let def_id = if rust_crates.contains(&id.package_coord.module) {
    resolve_crate_qualified_path(
      tcx,
      id.package_coord.module,
      id.package_coord.packages.as_slice(),
      StrI(human_name.as_str()),
    )?
  } else {
    resolve_local_type(tcx, &human_name)?
  };
  let arg_tys: Vec<Ty<'tcx>> = template_args
    .iter()
    .map(|t| templata_to_rustc_ty(tcx, rust_crates, t))
    .collect::<Option<_>>()?;
  Some((def_id, arg_tys))
}

/// The full rustc `GenericArgs` for a Rust item, filling type slots from `arg_tys` (already
/// converted, in declaration order) and lifetime slots with `re_erased` (borrowck ran on the Rust side;
/// lifetimes are irrelevant post-borrowck). A non-generic item has no slots, so `arg_tys` is empty and
/// the callback never fires. Panics on a const-generic slot or a type-slot shortfall — both are "not
/// supported yet" rather than something to guess.
pub(super) fn build_generic_args<'tcx>(
  tcx: TyCtxt<'tcx>,
  def_id: DefId,
  arg_tys: &[Ty<'tcx>],
) -> ty::GenericArgsRef<'tcx> {
  let mut types = arg_tys.iter().copied();
  ty::GenericArgs::for_item(tcx, def_id, |param, _| match param.kind {
    ty::GenericParamDefKind::Lifetime => tcx.lifetimes.re_erased.into(),
    ty::GenericParamDefKind::Type { .. } => types
      .next()
      .unwrap_or_else(|| panic!("too few type args for Rust item {def_id:?}"))
      .into(),
    ty::GenericParamDefKind::Const { .. } => {
      panic!("const-generic Rust item args not supported: {def_id:?}")
    }
  })
}

/// A free function defined in the crate being compiled (the final file), by name. Used for the
/// `__vale_drop` shim and the `__vale_<export>` stubs, which live there rather than in a dependency.
pub(super) fn resolve_local_fn(tcx: TyCtxt<'_>, name: &str) -> Option<DefId> {
  for child in tcx.module_children_local(CRATE_DEF_ID) {
    if let Res::Def(DefKind::Fn, def_id) = child.res {
      // Selection: `name` is one the final file itself emitted (`__vale_drop`, `__vale_<export>`).
      if child.ident.name.as_str() == name { // ataflbz-allow: selection
        return Some(def_id);
      }
    }
  }
  None
}

/// A struct defined in the crate being compiled (the final file), by name: `__ValeOpaque`, or a Vale
/// type the final file projects. The type analog of `resolve_local_fn`.
pub(super) fn resolve_local_type(tcx: TyCtxt<'_>, name: &str) -> Option<DefId> {
  for child in tcx.module_children_local(CRATE_DEF_ID) {
    if let Res::Def(DefKind::Struct, def_id) = child.res {
      // Selection: `name` is one the final file itself emitted (`__ValeOpaque`, a projected type).
      if child.ident.name.as_str() == name { // ataflbz-allow: selection
        return Some(def_id);
      }
    }
  }
  None
}
