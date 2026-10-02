use rustc_hir::def::DefKind;
use rustc_hir::def::Res;
use crate::postparsing::ast::ImportS;
use crate::scout_arena::ScoutArena;
use crate::Keywords;
use crate::StrI;
use crate::typing::compiler_error_reporter::CouldNotPostparseReason;
use crate::typing::env::environment::{ImportedItemKind, ResolvedName};
use crate::typing::rust_interop::bifrost::oracle::*;
use crate::typing::typing_interner::TypingInterner;
use crate::utils::code_hierarchy::PackageCoordinate;
use rustc_middle::ty::{Ty, TyCtxt, TyKind};
use rustc_span::def_id::DefId;

pub struct RealRustcOracle<'tcx, 's> {
  tcx: TyCtxt<'tcx>,
  // Every item that we imported, and also all of their methods.
  items: Vec<RustItem<'s>>,
}
impl<'tcx, 's> RealRustcOracle<'tcx, 's> {
  pub fn new(
    tcx: TyCtxt<'tcx>,
    scout_arena: &ScoutArena<'s>,
    keywords: &Keywords<'s>,
    imports: &[&ImportS<'s>],
  ) -> Self {
    let mut items: Vec<RustItem<'s>> = Vec::new();

    for import in imports {
      let def_id =
          find_rust_def_id(tcx, import.module_name, import.package_names, import.importee_name)
              .unwrap(); // error unimplemented
      let def_kind =
          match tcx.def_kind(def_id) {
            DefKind::Fn => ImportedItemKind::Function,
            DefKind::Struct => ImportedItemKind::Type,
            DefKind::Enum => ImportedItemKind::Enum,
            DefKind::Trait => ImportedItemKind::Trait,
            _ => unimplemented!(),
          };
      let name_symbol = tcx.item_name(def_id);
      let name_str = scout_arena.intern_str(name_symbol.as_str());

      let generic_param_names: Vec<StrI<'s>> =
          tcx.generics_of(def_id).own_params.iter()
              .map(|p| scout_arena.intern_str(p.name.as_str()))
              .collect();

      items.push(RustItem {
        name: ResolvedName {
          package_coord: scout_arena.intern_package_coordinate(import.module_name, import.package_names),
          importee_name: name_str,
          kind: def_kind,
        },
        origin: RustItemOrigin::Rustc(def_id),
        container_id: None,
        generic_param_names,
      });
    }

    let mut type_indices: Vec<usize> = Vec::new();
    for (item_index, item) in items.iter().enumerate() {
      match item.name.kind {
        ImportedItemKind::Type | ImportedItemKind::Enum => {
          type_indices.push(item_index);
        }
        _ => {}
      }
    }
    for item_index in type_indices {
      add_inherent_methods(tcx, scout_arena, &mut items, item_index);
      add_synthesized_drop(keywords, &mut items, item_index);
    }

    // TODO: traits, deref

    RealRustcOracle { tcx, items }
  }

  fn translate_ty<'t>(
    &self,
    ty: Ty<'tcx>,
    own_param_names: &[StrI<'s>],
    def_id: DefId,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<TypeR<'s, 't>, CouldNotPostparseReason>
  where
      's: 't,
  {
    match ty.kind() {
      TyKind::Param(_param) => unimplemented!(),
      TyKind::Alias(..) => unimplemented!(),
      TyKind::Adt(adt_def, adt_generic_args) => {
        let adt_def_id = adt_def.did();
        let idx = self
            .items
            .iter()
            .position(|i| match i.name.kind {
              ImportedItemKind::Type | ImportedItemKind::Enum => i.origin == RustItemOrigin::Rustc(adt_def_id),
              _ => false
            })
            .unwrap(); // unimplemented
        let mut args: Vec<TypeR<'s, 't>> = Vec::new();
        for adt_generic_arg in adt_generic_args.types() {
          args.push(self.translate_ty(adt_generic_arg, own_param_names, def_id, interner)?);
        }
        Ok(TypeR::Citizen {
          name: self.items[idx].name.importee_name,
          package: self.items[idx].name.package_coord,
          args: interner.alloc_slice_from_vec(args),
        })
      }
      TyKind::Ref(_, inner, mutability) => {
        let lowered =
            self.translate_ty(*inner, own_param_names, def_id, interner)
                .unwrap(); // unimplemented
        Ok(TypeR::Borrow { inner: interner.alloc(lowered), is_mut: mutability.is_mut() })
      }
      TyKind::Tuple(tys) => {
        if tys.is_empty() {
          Ok(TypeR::Primitive(PrimitiveR::Void))
        } else {
          unimplemented!()
        }
      }
      TyKind::Int(rustc_middle::ty::IntTy::I64) => Ok(TypeR::Primitive(PrimitiveR::Int64)),
      TyKind::Int(rustc_middle::ty::IntTy::I32) => Ok(TypeR::Primitive(PrimitiveR::Int32)),
      _ => unimplemented!(),
    }
  }
}

impl<'tcx, 's, 't> RustOracle<'s, 't> for RealRustcOracle<'tcx, 's>
where
    's: 't,
{
  fn resolve(&self, container: Option<RustItemId>, needle: &ResolvedName<'s>) -> Option<RustItemId> {
    self
        .items
        .iter()
        .position(|item| item.container_id == container && item.name == *needle)
        .map(|idx| RustItemId(idx as u32))
  }

  fn resolve_import(&self, import: &ImportS<'s>) -> Option<ResolvedName<'s>> {
    let def_id =
        find_rust_def_id(self.tcx, import.module_name, import.package_names, import.importee_name)
        .unwrap(); // error unimplemented
    let item =
        self.items.iter().find(|item| item.origin == RustItemOrigin::Rustc(def_id))
            .unwrap(); // error unimplemented. we might want to lazily add it too
    Some(item.name)
  }

  fn fn_sig(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<FuncSignatureR<'s, 't>, CouldNotPostparseReason> {
    let rust_item =
        self.items.get(item.0 as usize)
            .expect("RustItemId out of range");
    let def_id = match rust_item.origin {
      RustItemOrigin::Rustc(def_id) => def_id,
      RustItemOrigin::Slice => unimplemented!(),
      RustItemOrigin::SynthesizedDrop => {
        let owner_id = match rust_item.container_id { Some(v) => v, None => unimplemented!() };
        let owner = &self.items[owner_id.0 as usize];
        let mut receiver_args: Vec<TypeR<'s, 't>> = Vec::new();
        for i in 0..owner.generic_param_names.len() {
          receiver_args.push(TypeR::Generic(i as u32));
        }
        let receiver = TypeR::Citizen {
          name: owner.name.importee_name,
          package: owner.name.package_coord,
          args: interner.alloc_slice_from_vec(receiver_args),
        };
        return Ok(FuncSignatureR {
          generic_param_names: interner.alloc_slice_copy(&rust_item.generic_param_names),
          generic_param_bounds: &[],
          params: interner.alloc_slice_from_vec(vec![receiver]),
          ret: TypeR::Primitive(PrimitiveR::Void),
        });
      }
    };
    // Not entirely sure why we need the binder or identity. Supposedly we're replacing generic
    // param mentions with placeholders.
    let binder = self.tcx.fn_sig(def_id).instantiate_identity();
    let signature = binder.skip_binder();

    // TODO: handle parameters' lifetimes

    let generic_params = &rust_item.generic_param_names;

    let mut params: Vec<TypeR<'s, 't>> = Vec::new();
    for input_type in signature.inputs() {
      params.push(self.translate_ty(*input_type, generic_params, def_id, interner)?);
    }
    let ret = self.translate_ty(signature.output(), generic_params, def_id, interner)?;

    // Handle the `where P: Trait`, bring those into Valen as implements(...)
    let mut bounds: Vec<ImplBoundR<'s, 't>> = Vec::new();
    for (clause, _span) in self.tcx.predicates_of(def_id).predicates {
      let Some(trait_clause) = clause.as_trait_clause() else {
        continue;
      };
      let trait_pred = trait_clause.skip_binder();
      if trait_pred.polarity != rustc_middle::ty::PredicatePolarity::Positive {
        continue;
      }
      let TyKind::Param(param) = trait_pred.self_ty().kind() else {
        continue;
      };
      let Some(sub_generic_index) =
          generic_params.iter().position(|p| p.0 == param.name.as_str()) else {
        continue;
      };
      let trait_did = trait_pred.trait_ref.def_id;
      let Some(idx) =
          self.items.iter().position(|i| i.name.kind == ImportedItemKind::Trait && i.origin == RustItemOrigin::Rustc(trait_did)) else {
        continue;
      };
      // The trait's own type args, if any. Skip args[0] which is the `Self`.
      let mut trait_args: Vec<TypeR<'s, 't>> = Vec::new();
      for thing in trait_pred.trait_ref.args.types().skip(1) {
        trait_args.push(self.translate_ty(thing, generic_params, def_id, interner)?);
      }
      bounds.push(
        ImplBoundR {
          sub_generic_index: sub_generic_index as u32,
          super_trait: TypeR::Citizen {
            name: self.items[idx].name.importee_name,
            package: self.items[idx].name.package_coord,
            args: interner.alloc_slice_from_vec(trait_args),
          },
        })
    }

    Ok(FuncSignatureR {
      generic_param_names: interner.alloc_slice_copy(generic_params),
      generic_param_bounds: interner.alloc_slice_from_vec(bounds),
      params: interner.alloc_slice_from_vec(params),
      ret,
    })
  }

  fn type_generic_params(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> &'t [StrI<'s>] {
    match self.items.get(item.0 as usize) {
      Some(rust_item) => interner.alloc_slice_copy(&rust_item.generic_param_names),
      None => unimplemented!(),
    }
  }

  fn methods(&self, item: RustItemId) -> Vec<(StrI<'s>, RustItemId)> {
    // TODO: lets switch to a loop, or even better, lets add an index for this
    self
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.container_id == Some(item))
        .map(|(idx, i)| (i.name.importee_name, RustItemId(idx as u32)))
        .collect()
  }

  fn deref_target_imports(&self) -> Vec<ResolvedName<'s>> {
    unimplemented!()
  }

  fn rust_spelling(&self, package_coord: &PackageCoordinate<'s>, name: StrI<'s>) -> Option<String> {
    for item in &self.items {
      if item.container_id.is_none() && item.name.kind != ImportedItemKind::Function {
        if *item.name.package_coord == *package_coord && item.name.importee_name == name {
          if let RustItemOrigin::Rustc(def_id) = item.origin {
            return Some(visible_rust_path(self.tcx, def_id));
          }
        }
      }
    }
    None
  }
}

// Get the publicly importable path for a type, because some types are private to their own
// crate but are publicly exported under a different name.
pub(crate) fn visible_rust_path(tcx: TyCtxt<'_>, def_id: DefId) -> String {
  let visible_parents = tcx.visible_parent_map(());
  let mut segments: Vec<String> = Vec::new();
  let mut current = def_id;
  while !current.is_crate_root() {
    segments.push(tcx.item_name(current).to_string());
    current =
        match visible_parents.get(&current) {
          Some(parent) => *parent,
          None => unimplemented!(),
        };
  }
  segments.reverse();
  format!("::{}::{}", tcx.crate_name(current.krate), segments.join("::"))
}

pub(crate) fn find_rust_def_id<'tcx>(
  tcx: TyCtxt<'tcx>,
  crate_name: StrI<'_>,
  modules: &[StrI<'_>],
  item_name: StrI<'_>,
) -> Option<DefId> {
  let crate_num =
      tcx.crates(()).iter().copied().find(|c| tcx.crate_name(*c).as_str() == crate_name.0)
          .unwrap(); // error unimplemented
  let crate_module = crate_num.as_def_id();
  assert!(modules.len() == 0); // unimplemented
  for child in tcx.module_children(crate_module) {
    if child.ident.as_str() == item_name.0 {
      if let Res::Def(_, def_id) = child.res {
        return Some(def_id);
      }
    }
  }
  None
}

fn add_inherent_methods<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  items: &mut Vec<RustItem<'s>>,
  owner_index: usize,
) {
  let owner_def_id = match items[owner_index].origin {
    RustItemOrigin::Rustc(def_id) => def_id,
    RustItemOrigin::SynthesizedDrop => unimplemented!(),
    RustItemOrigin::Slice => unimplemented!(),
  };
  let package_coord = items[owner_index].name.package_coord;
  for impl_def_id in tcx.inherent_impls(owner_def_id).iter() {
    for assoc_item in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc_item.as_tag() == rustc_middle::ty::AssocTag::Fn {
        items.push(RustItem {
          name: ResolvedName {
            package_coord,
            importee_name: scout_arena.intern_str(assoc_item.name().as_str()),
            kind: ImportedItemKind::Function,
          },
          origin: RustItemOrigin::Rustc(assoc_item.def_id),
          container_id: Some(RustItemId(owner_index as u32)),
          generic_param_names: generic_param_names_including_parent(tcx, scout_arena, assoc_item.def_id),
        });
      }
    }
  }
}

// Rustc doesn't define drop functions, they're implicit. We compensate for that gap here.
fn add_synthesized_drop<'s>(
  keywords: &Keywords<'s>,
  items: &mut Vec<RustItem<'s>>,
  owner_index: usize,
) {
  let owner = &items[owner_index];
  let drop_item = RustItem {
    name: ResolvedName {
      package_coord: owner.name.package_coord,
      importee_name: keywords.drop,
      kind: ImportedItemKind::Function,
    },
    origin: RustItemOrigin::SynthesizedDrop,
    container_id: Some(RustItemId(owner_index as u32)),
    generic_param_names: owner.generic_param_names.clone(),
  };
  items.push(drop_item);
}

fn generic_param_names_including_parent<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  def_id: DefId,
) -> Vec<StrI<'s>> {
  let generics = tcx.generics_of(def_id);
  let mut names =
      match generics.parent {
        Some(parent) => generic_param_names_including_parent(tcx, scout_arena, parent),
        None => Vec::new(),
      };
  for generic_param in generics.own_params.iter() {
    names.push(scout_arena.intern_str(generic_param.name.as_str()));
  }
  names
}