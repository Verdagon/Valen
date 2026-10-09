// Resolving a Rust callee request — the synthesized extern `PrototypeI` the instantiator records at
// an `ExternFunctionCall` — to the rustc `(DefId, args)` the synthetic MIR body reifies.

use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;

use crate::instantiating::ast::ast::PrototypeI;
use crate::instantiating::ast::names::{INameI, IStructTemplateNameI};
use crate::instantiating::ast::templata::ITemplataI;
use crate::instantiating::ast::types::KindIT;
use crate::interner::StrI;
use crate::typing::rust_interop::horizon::tyctxt_oracle::resolve_crate_qualified_path;

use super::rustc_ty::{
  build_generic_args, citizen_def_id_and_args, kind_to_rustc_ty, resolve_local_fn, templata_to_rustc_ty,
};

/// One collected Rust callee request, resolved to the rustc `(DefId, args)` to reify, plus a
/// human-readable log line for the firing log.
pub(super) struct ResolvedRequest<'tcx> {
  pub(super) log: String,
  pub(super) dep: (DefId, ty::GenericArgsRef<'tcx>),
}

/// Resolve one Rust callee request, trying each shape a Rust callee can have in turn: a free function
/// at its crate-qualified path, a method through its receiver's inherent impls, a `deref` through the
/// receiver's `Deref` impl, an associated function through the owner named in the id, and finally a
/// drop through the `__vale_drop` shim. A request that is none of these is a bug, so it panics.
pub(super) fn resolve_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
) -> ResolvedRequest<'tcx> {
  let (fn_name, _, _) = request_name_parts(proto);
  let path = rust_request_path(proto, fn_name);
  let own_arg_tys = rust_request_arg_tys(tcx, rust_crates, proto);

  // A free function: the whole path resolves to an item in the crate graph.
  if let Some(def_id) = resolve_crate_qualified_path(
    tcx,
    proto.id.package_coord.module,
    proto.id.package_coord.packages.as_slice(),
    fn_name,
  ) {
    let args = build_generic_args(tcx, def_id, &own_arg_tys);
    return ResolvedRequest {
      log: format!("{path}{own_arg_tys:?} => {}", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // A method: resolve it through the receiver type (the first parameter).
  if let Some((def_id, args)) = resolve_method_request(tcx, rust_crates, proto, &own_arg_tys) {
    return ResolvedRequest {
      log: format!("{path} => {} (method)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // A slice method (`<[T]>::get`) whose receiver is the synthesized `__slice<T>` citizen. That
  // citizen has no `DefId`, so `resolve_method_request` (which keys on the receiver's ADT) misses it.
  if let Some((def_id, args)) = resolve_slice_method_request(tcx, rust_crates, proto) {
    return ResolvedRequest {
      log: format!("{path} => {} (slice method)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // A synthesized `deref` (from autoderef): its receiver type implements `Deref`, but `deref` is a
  // trait method, not inherent, so the method attempt misses it. Checked after the inherent attempt
  // so a real inherent method named `deref` still wins.
  if let Some((def_id, args)) = resolve_deref_request(tcx, rust_crates, proto, &own_arg_tys) {
    return ResolvedRequest {
      log: format!("{path} => {} (deref)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // An associated function (e.g. `Type::new`): no receiver, but its owner type is named in the id's
  // init path.
  if let Some((def_id, args)) = resolve_assoc_fn_request(tcx, proto, &own_arg_tys) {
    return ResolvedRequest {
      log: format!("{path} => {} (assoc)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // A drop of an imported type: it has no Rust `Drop` to resolve to, so reify the generic
  // `__vale_drop<T>` shim. Checked last so a real method named `drop` still resolves normally.
  if let Some((def_id, args)) = resolve_drop_request(tcx, rust_crates, proto) {
    return ResolvedRequest {
      log: format!("{path} => {} (drop shim)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  panic!("rust interop: could not resolve Rust callee `{path}`")
}

/// Resolve a drop request to `__vale_drop::<T>`, where `T` is the dropped type (the request's first
/// parameter). Recognized by the function name `drop`. `__vale_drop` is a generic shim in the final
/// file that calls `ptr::drop_in_place`.
fn resolve_drop_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (name, _, parameters) = request_name_parts(proto);
  if name.as_str() != "drop" {
    return None;
  }
  let dropped_ty = kind_to_rustc_ty(tcx, rust_crates, parameters.first()?)?;
  let drop_def_id = resolve_local_fn(tcx, "__vale_drop")?;
  Some((drop_def_id, build_generic_args(tcx, drop_def_id, &[dropped_ty])))
}

/// Resolve a method request through its receiver type's inherent impls. The receiver is the request's
/// first parameter; the method's generic args are the receiver's type args followed by the method's
/// own (matching rustc's parent-inclusive generic order).
fn resolve_method_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
  own_arg_tys: &[Ty<'tcx>],
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (method_name, _, parameters) = request_name_parts(proto);
  let (owner_def_id, receiver_arg_tys) = receiver_owner(tcx, rust_crates, parameters.first()?)?;
  let method_def_id = resolve_inherent_method(tcx, owner_def_id, method_name.as_str())?;
  let mut method_arg_tys = receiver_arg_tys;
  method_arg_tys.extend_from_slice(own_arg_tys);
  Some((method_def_id, build_generic_args(tcx, method_def_id, &method_arg_tys)))
}

/// Resolve a `deref` request to the `Deref::deref` fn of the receiver type's shared `Deref` impl — the
/// instantiator mirror of the oracle's `Deref` discovery. Recognized by the method name `deref` on a
/// receiver whose type implements `Deref`.
fn resolve_deref_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
  own_arg_tys: &[Ty<'tcx>],
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (method_name, _, parameters) = request_name_parts(proto);
  if method_name.as_str() != "deref" {
    return None;
  }
  let (owner_def_id, receiver_arg_tys) = receiver_owner(tcx, rust_crates, parameters.first()?)?;
  let deref_did = tcx.lang_items().deref_trait()?;
  let receiver_ty = tcx.type_of(owner_def_id).instantiate_identity();
  let mut deref_fn: Option<DefId> = None;
  tcx.for_each_relevant_impl(deref_did, receiver_ty, |impl_did| {
    if deref_fn.is_some() {
      return;
    }
    let self_adt = tcx
      .impl_trait_ref(impl_did)
      .instantiate_identity()
      .self_ty()
      .ty_adt_def()
      .map(|d| d.did());
    if self_adt != Some(owner_def_id) {
      return;
    }
    for assoc in tcx.associated_items(impl_did).in_definition_order() {
      if assoc.as_tag() == ty::AssocTag::Fn {
        deref_fn = Some(assoc.def_id);
      }
    }
  });
  let deref_fn = deref_fn?;
  let mut method_arg_tys = receiver_arg_tys;
  method_arg_tys.extend_from_slice(own_arg_tys);
  Some((deref_fn, build_generic_args(tcx, deref_fn, &method_arg_tys)))
}

/// Resolve an associated function (e.g. `Domino::new`) through its owner type. The owner is named in
/// the request id's init path (a struct/interface template segment); the function is then found in the
/// owner's inherent impls.
fn resolve_assoc_fn_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  proto: &PrototypeI,
  own_arg_tys: &[Ty<'tcx>],
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (fn_name, _, _) = request_name_parts(proto);
  let owner_human = proto.id.init_steps.iter().rev().find_map(|step| match step {
    INameI::StructTemplate(t) => Some(t.human_name),
    INameI::InterfaceTemplate(t) => Some(t.human_namee),
    _ => None,
  })?;
  let owner_def_id = resolve_crate_qualified_path(
    tcx,
    proto.id.package_coord.module,
    proto.id.package_coord.packages.as_slice(),
    owner_human,
  )?;
  let fn_def_id = resolve_inherent_method(tcx, owner_def_id, fn_name.as_str())?;
  Some((fn_def_id, build_generic_args(tcx, fn_def_id, own_arg_tys)))
}

/// Resolve a slice method (`<[T]>::get`) whose receiver is the synthesized `__slice<T>` citizen. The
/// citizen has no `DefId` (a slice is structural), so the element type comes from its one type
/// argument, and the method is found among the slice's incoherent inherent impls (`impl<T> [T]`). Its
/// args are parent-inclusive `[element, usize]` — the impl's `T` and the `SliceIndex` parameter
/// concretized to `usize`, mirroring the oracle's slice-method signature.
fn resolve_slice_method_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (method_name, _, parameters) = request_name_parts(proto);
  let elem_ty = slice_receiver_element(tcx, rust_crates, parameters.first()?)?;
  let method_def_id = resolve_incoherent_slice_method(tcx, method_name.as_str())?;
  let args = build_generic_args(tcx, method_def_id, &[elem_ty, tcx.types.usize]);
  Some((method_def_id, args))
}

/// The element type of a `&__slice<T>` receiver, or `None` when the receiver isn't the slice citizen.
fn slice_receiver_element<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  kind: &KindIT,
) -> Option<Ty<'tcx>> {
  let KindIT::StructIT(s) = kind.peel_all_references() else {
    return None;
  };
  let INameI::StructName(sn) = s.id.local_name else {
    return None;
  };
  let IStructTemplateNameI::StructTemplate(t) = sn.template else {
    return None;
  };
  if t.human_name.as_str() != "__slice" { // ataflbz-allow: `__slice` is a synthesized-citizen sentinel name, not a real Rust item's identity
    return None;
  }
  templata_to_rustc_ty(tcx, rust_crates, sn.template_args.first()?)
}

/// Find a slice inherent method (`impl<T> [T]`) by name, via the incoherent inherent impls keyed by
/// the slice `SimplifiedType` — a slice has no ADT `DefId` for `inherent_impls`.
fn resolve_incoherent_slice_method(tcx: TyCtxt<'_>, method_name: &str) -> Option<DefId> {
  for impl_def_id in tcx.incoherent_impls(ty::fast_reject::SimplifiedType::Slice).iter() {
    for assoc in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc.as_tag() == ty::AssocTag::Fn && assoc.name().as_str() == method_name {
        return Some(assoc.def_id);
      }
    }
  }
  None
}

/// The owning type of a method receiver: its `DefId` and its own type arguments. Peels any reference
/// wrappers first, so a `&self`/`&mut self` receiver resolves the same as a by-value one. `None` for a
/// receiver that isn't a citizen, which is how an associated function whose first parameter is a
/// primitive (`Delta::seconds(i64)`) falls through to the assoc-fn attempt.
fn receiver_owner<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  kind: &KindIT,
) -> Option<(DefId, Vec<Ty<'tcx>>)> {
  match kind.peel_all_references() {
    KindIT::StructIT(s) => citizen_def_id_and_args(tcx, rust_crates, &s.id),
    KindIT::InterfaceIT(i) => citizen_def_id_and_args(tcx, rust_crates, &i.id),
    _ => None,
  }
}

/// Find an inherent method by name on a type, returning its `DefId`. Mirrors the oracle's
/// `inherent_impls` → `associated_items` walk.
fn resolve_inherent_method(tcx: TyCtxt<'_>, owner_def_id: DefId, method_name: &str) -> Option<DefId> {
  for impl_def_id in tcx.inherent_impls(owner_def_id).iter() {
    for assoc in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc.as_tag() == ty::AssocTag::Fn && assoc.name().as_str() == method_name {
        return Some(assoc.def_id);
      }
    }
  }
  None
}

/// The dotted path of a Rust callee request, for the firing log.
fn rust_request_path(proto: &PrototypeI, human_name: StrI) -> String {
  let mut segments: Vec<&str> = vec![proto.id.package_coord.module.as_str()];
  segments.extend(proto.id.package_coord.packages.as_slice().iter().map(|s| s.as_str()));
  segments.push(human_name.as_str());
  segments.join(".")
}

/// The name, generic type-args, and parameter types of a Rust callee request. The request is the
/// synthesized *extern* prototype recorded at the `ExternFunctionCall` node, so its name is normally
/// `INameI::ExternFunction`; a plain `FunctionNameIX` is accepted too.
fn request_name_parts<'s, 'i>(
  proto: &PrototypeI<'s, 'i>,
) -> (StrI<'s>, &'i [ITemplataI<'s, 'i>], &'i [KindIT<'s, 'i>]) {
  match proto.id.local_name {
    INameI::ExternFunction(e) => (e.human_name, e.template_args, e.parameters),
    INameI::FunctionNameIX(fnx) => (fnx.template.human_name, fnx.template_args, fnx.parameters),
    other => panic!("rust interop: a Rust callee request with a non-function name: {other:?}"),
  }
}

/// The rustc type arguments of a Rust callee request, converted from the Vale template args on its
/// instantiated name. A generic type arg that won't lower to a rustc `Ty` is a real gap — an
/// unprojected or unlowerable type — never a benign skip, so it panics here, naming the callee and the
/// offending arg, rather than far away in the backend on a missing extern.
fn rust_request_arg_tys<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
) -> Vec<Ty<'tcx>> {
  let (name, template_args, _) = request_name_parts(proto);
  template_args
    .iter()
    .map(|t| {
      templata_to_rustc_ty(tcx, rust_crates, t).unwrap_or_else(|| {
        panic!(
          "rust interop: cannot lower generic type argument of Rust callee `{}` to a rustc type: {t:?}",
          name.as_str()
        )
      })
    })
    .collect()
}
