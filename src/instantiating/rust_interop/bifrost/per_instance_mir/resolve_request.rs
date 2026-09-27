use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::CRATE_DEF_ID;
use rustc_span::def_id::DefId;

use crate::instantiating::ast::ast::PrototypeI;
use crate::instantiating::ast::names::INameI;
use crate::instantiating::ast::templata::ITemplataI;
use crate::instantiating::ast::types::KindIT;
use crate::interner::StrI;
use crate::typing::rust_interop::find_rust_def_id;

use crate::instantiating::rust_interop::bifrost::rustc_ty::{build_generic_args, citizen_def_id_and_args, kind_to_rustc_ty};

// In rustc terms, this is a Rust function that a Valen function (or one of its callees) wants to
// call.
// TODO: consider just making an Instance instead.
pub(super) struct ResolvedRequest<'tcx> {
  pub(super) log: String, // TODO: still needed?
  pub(super) dep: (DefId, ty::GenericArgsRef<'tcx>),
}

// Translates a PrototypeI into a rustc `(DefId, args)`.
pub(super) fn resolve_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
) -> ResolvedRequest<'tcx> {
  let (fn_name, _, _) = request_name_parts(proto);
  let path = rust_request_path(proto, fn_name);
  let own_arg_tys = rust_request_arg_tys(proto);

  // free function
  if let Some(def_id) = find_rust_def_id(
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

  // Method
  if let Some((def_id, args)) = resolve_method_request(tcx, rust_crates, proto, &own_arg_tys) {
    return ResolvedRequest {
      log: format!("{path} => {} (method)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // Associated function (e.g. `Type::new`)
  if let Some((def_id, args)) = resolve_assoc_fn_request(tcx, proto, &own_arg_tys) {
    return ResolvedRequest {
      log: format!("{path} => {} (assoc)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  // drop function. Use the `__vale_drop<T>` shim.
  if let Some((def_id, args)) = resolve_drop_request(tcx, rust_crates, proto) {
    return ResolvedRequest {
      log: format!("{path} => {} (drop shim)", tcx.def_path_str(def_id)),
      dep: (def_id, args),
    };
  }

  panic!("unknown Rust callee request `{path}`")
}

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

fn resolve_local_fn(tcx: TyCtxt<'_>, name: &str) -> Option<DefId> {
  for child in tcx.module_children_local(CRATE_DEF_ID) {
    if let Res::Def(DefKind::Fn, def_id) = child.res {
      if child.ident.name.as_str() == name {
        return Some(def_id);
      }
    }
  }
  None
}

fn resolve_method_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  rust_crates: &[StrI],
  proto: &PrototypeI,
  own_arg_tys: &[Ty<'tcx>],
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (method_name, _, parameters) = request_name_parts(proto);
  let (owner_def_id, receiver_arg_tys) =
      match parameters.first()?.peel_all_references() {
        KindIT::StructIT(s) => citizen_def_id_and_args(tcx, rust_crates, &s.id)?,
        _ => {
          // If it's not a struct, it can't be a method.
          // TODO: support interface methods
          return None;
        }
      };
  let method_def_id = resolve_inherent_method(tcx, owner_def_id, method_name.as_str())?;
  let mut method_arg_tys = receiver_arg_tys;
  method_arg_tys.extend_from_slice(own_arg_tys);
  Some((method_def_id, build_generic_args(tcx, method_def_id, &method_arg_tys)))
}

fn resolve_assoc_fn_request<'tcx>(
  tcx: TyCtxt<'tcx>,
  proto: &PrototypeI,
  own_arg_tys: &[Ty<'tcx>],
) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
  let (fn_name, _, _) = request_name_parts(proto);
  let fn_name = fn_name.as_str();
  let owner_human = proto.id.init_steps.iter().rev().find_map(|step| match step {
    INameI::StructTemplate(t) => Some(t.human_name),
    INameI::InterfaceTemplate(t) => Some(t.human_namee),
    _ => None,
  })?;
  let owner_def_id = find_rust_def_id(
    tcx,
    proto.id.package_coord.module,
    proto.id.package_coord.packages.as_slice(),
    owner_human,
  )?;
  let fn_def_id = resolve_inherent_method(tcx, owner_def_id, fn_name)?;
  Some((fn_def_id, build_generic_args(tcx, fn_def_id, own_arg_tys)))
}

fn resolve_inherent_method(tcx: TyCtxt<'_>, owner_def_id: DefId, method_name: &str) -> Option<DefId> {
  for impl_def_id in tcx.inherent_impls(owner_def_id).iter() {
    for assoc in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc.as_tag() == ty::AssocTag::Fn && assoc.name().to_string() == method_name {
        return Some(assoc.def_id);
      }
    }
  }
  None
}

fn rust_request_path(proto: &PrototypeI, human_name: StrI) -> String {
  let mut segments: Vec<&str> = vec![proto.id.package_coord.module.as_str()];
  segments.extend(proto.id.package_coord.packages.as_slice().iter().map(|s| s.as_str()));
  segments.push(human_name.as_str());
  segments.join(".")
}

fn request_name_parts<'s, 'i>(
  proto: &PrototypeI<'s, 'i>,
) -> (StrI<'s>, &'i [ITemplataI<'s, 'i>], &'i [KindIT<'s, 'i>]) {
  match proto.id.local_name {
    INameI::ExternFunction(e) => (e.human_name, e.template_args, e.parameters),
    _ => unimplemented!(),
  }
}

fn rust_request_arg_tys<'tcx>(proto: &PrototypeI) -> Vec<Ty<'tcx>> {
  let (_, template_args, _) = request_name_parts(proto);
  if !template_args.is_empty() {
    unimplemented!()
  }
  Vec::new()
}
