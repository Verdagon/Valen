// Horizon's real oracle: answers from a live `TyCtxt`.
//
// This is the full-featured counterpart of bifrost's `RealRustcOracle`, and it implements the same
// `RustOracle` trait (bifrost's `oracle.rs`), so the importer and everything above it see the same
// `TypeR` / `FuncSignatureR` vocabulary whichever implementation the `horizon` feature selects.
// Where `RealRustcOracle` stops at `unimplemented!()`, this one carries the old implementation's
// support for generics, traits, enums, nested modules, re-exports and single-step `Deref`.
//
// `TyCtxt<'tcx>` is `Copy`, but `'tcx` is tied to arenas owned by `run_compiler`'s stack frame:
// this oracle cannot outlive the callback that built it, and must never be stashed in a static or
// in `HinputsT`.

use rustc_hir::def::{DefKind, Res};
use rustc_middle::ty::{Ty, TyCtxt, TyKind};
use rustc_span::def_id::DefId;

use crate::interner::StrI;
use crate::keywords::Keywords;
use crate::postparsing::ast::ImportS;
use crate::scout_arena::ScoutArena;
use crate::typing::compiler_error_reporter::CouldNotPostparseReason;
use crate::typing::env::environment::{ImportedItemKind, ResolvedName};
use crate::typing::rust_interop::bifrost::oracle::{
  FuncSignatureR, ImplBoundR, PrimitiveR, RustItemId, RustItemOrigin, RustOracle, TypeR,
};
use crate::typing::rust_interop::visible_rust_path;
use crate::typing::typing_interner::TypingInterner;
use crate::utils::code_hierarchy::PackageCoordinate;

/// An item's own generic parameter names, in declaration order.
///
/// `own_params` rather than the full list: a method on `impl<T> Foo<T>` sees the impl's
/// parameters at the low indices of its parent-inclusive list, and Vale's declaration names only
/// what the item itself declares. Resolving by name later means that offset never has to be
/// computed — and names are safe to key on because Rust forbids an item from shadowing a generic
/// parameter name declared by its parent impl (E0403).
fn own_generic_param_names<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  def_id: DefId,
) -> Vec<StrI<'s>> {
  tcx
    .generics_of(def_id)
    .own_params
    .iter()
    .map(|p| scout_arena.intern_str(p.name.as_str()))
    .collect()
}

/// The parent-inclusive generic parameter names for an item: the parent's params first (at the low
/// indices), then the item's own — the exact order rustc numbers `ty::Param`s and `GenericArgs::for_item`
/// fills them.
///
/// A method on a generic type references its **impl's** parameters in its signature — `Vec::push`'s
/// `value: T` and `Vec::new`'s return `Vec<T, Global>` both name the impl's `T` — but `generics_of(method)`
/// reports those under `.parent`, leaving the method's *own* params empty. Lowering that signature against
/// only the own params rejects the inherited `T` as an `InheritedParameter`, so a method needs this full
/// list. Which impl matters: `Vec::new` sits in `impl<T> Vec<T>` (parent params `[T]`, `Global` concrete),
/// while `Vec::push` sits in `impl<T, A> Vec<T, A>` (parent params `[T, A]`) — the parent walk picks up
/// each. A free function or a top-level type has no parent, so this reduces to `own_generic_param_names`.
fn parent_inclusive_generic_param_names<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  def_id: DefId,
) -> Vec<StrI<'s>> {
  let generics = tcx.generics_of(def_id);
  let mut names = match generics.parent {
    Some(parent) => parent_inclusive_generic_param_names(tcx, scout_arena, parent),
    None => Vec::new(),
  };
  names.extend(generics.own_params.iter().map(|p| scout_arena.intern_str(p.name.as_str())));
  names
}

/// The Vale package coordinate for a Rust item: its crate as the module, then the module path it
/// sits under as the packages.
///
/// **Asked of rustc rather than reconstructed.** `tcx.def_path` is the *definition* path and is
/// unique by construction, so two crates each exporting a `Widget` land in different packages and
/// can never intern to one Vale id. The alternative — one coordinate handed to the constructor and
/// stamped on every item — is exactly what made them indistinguishable, and it is the shape
/// @ATAFLBZ warns about: identity from a short name rather than from the thing itself.
///
/// The **last** named segment is the item's own name and belongs in `local_name`, not in the
/// coordinate; everything before it is module nesting. Segments carrying no name — an `impl`
/// block, for one — contribute nothing to a source-level path and are skipped.
///
/// One consequence to know: this is the *definition* path, so an item imported as `std.vec.Vec`
/// lands at `alloc.[vec]`, because `std::vec` is a re-export of `alloc::vec`. That is right for
/// identity and wrong for a diagnostic, which should echo the path the user wrote.
fn package_coord_for<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  def_id: DefId,
) -> &'s PackageCoordinate<'s> {
  let named: Vec<String> = tcx
    .def_path(def_id)
    .data
    .iter()
    .filter_map(|segment| segment.data.get_opt_name())
    .map(|name| name.to_string())
    .collect();

  let crate_name = scout_arena.intern_str(tcx.crate_name(def_id.krate).as_str());
  let mut packages: Vec<StrI<'s>> = Vec::new();
  for module_segment in named.iter().take(named.len().saturating_sub(1)) {
    packages.push(scout_arena.intern_str(module_segment));
  }
  scout_arena.intern_package_coordinate(crate_name, &packages)
}

/// Append every inherent-impl method of the item at `owner_idx` as a `Method(owner_idx)` entry.
/// Shared by construction (for each imported type) and by `Deref`-target discovery (a target the
/// import list didn't name needs its methods added here, since the construction loop never saw it).
fn add_inherent_methods<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  items: &mut Vec<RustItem<'s>>,
  owner_idx: usize,
) {
  let owner_def_id = items[owner_idx].def_id().expect("an imported type always has a rustc DefId");
  let package = items[owner_idx].package;
  for impl_def_id in tcx.inherent_impls(owner_def_id).iter() {
    for assoc in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc.as_tag() != rustc_middle::ty::AssocTag::Fn {
        continue;
      }
      items.push(RustItem {
        human_name: scout_arena.intern_str(assoc.name().as_str()),
        origin: RustItemOrigin::Rustc(assoc.def_id),
        package,
        kind: ItemKind::Method(owner_idx),
        // Parent-inclusive: a method's signature names the impl's params (`self Vec<T, A>`,
        // `value: T`), which live under the method's `.parent`, not its own params.
        generic_params: parent_inclusive_generic_param_names(tcx, scout_arena, assoc.def_id),
      });
    }
  }
}

/// Rustc defines no drop function for a type; drop glue is implicit. So each imported type gets a
/// synthesized `drop` item hanging off it like a method, which is what keeps "drop is just another
/// function" true through the importer: it finds `drop` among the type's `methods` and asks
/// `fn_sig` for it exactly as it does for `get` or `push`. The signature is answered here, in the
/// oracle, as `drop(self Owner<T…>) void` (see `fn_sig`); the importer never builds one itself.
fn add_synthesized_drop<'s>(keywords: &Keywords<'s>, items: &mut Vec<RustItem<'s>>, owner_idx: usize) {
  let owner = &items[owner_idx];
  let drop_item = RustItem {
    human_name: keywords.drop,
    origin: RustItemOrigin::SynthesizedDrop,
    package: owner.package,
    kind: ItemKind::Method(owner_idx),
    generic_params: owner.generic_params.clone(),
  };
  items.push(drop_item);
}

/// Single-step, shared-`Deref` discovery. For each imported ADT that implements `Deref<Target=U>`
/// where BOTH the source and `U` are non-generic named ADTs, register `U` (and its inherent methods
/// and drop, if `U` wasn't itself imported) plus a `deref` method on the source. Returns the item
/// indices of the targets that were newly added, so the import loop can declare them implicitly.
///
/// Scoped deliberately to non-generic source and target: a generic `Deref` like
/// `Vec<T>: Deref<Target=[T]>` (whose target is the unsized slice `[T]`, which `lower_primitive`
/// declines) is skipped, leaving those slice methods unresolved — that is the separate
/// slice/`usize`/`Option` work, not this. Only ORIGINALLY-imported types are probed (never an
/// auto-added target), so a chain never advances past one step.
fn discover_deref_targets<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  keywords: &Keywords<'s>,
  items: &mut Vec<RustItem<'s>>,
) -> Vec<usize> {
  let mut newly_added_targets: Vec<usize> = Vec::new();
  let Some(deref_did) = tcx.lang_items().deref_trait() else {
    return newly_added_targets;
  };
  let source_indices: Vec<usize> = items
    .iter()
    .enumerate()
    .filter(|(_, i)| matches!(i.kind, ItemKind::Type | ItemKind::Enum))
    .map(|(idx, _)| idx)
    .collect();
  for source_idx in source_indices {
    let source_def_id = items[source_idx].def_id().expect("an imported type always has a rustc DefId");
    let source_ty = tcx.type_of(source_def_id).instantiate_identity();
    // The shared `Deref` impl for this exact ADT, if any: its `deref` fn DefId and its `Target` ADT.
    let mut found: Option<(DefId, DerefTarget)> = None;
    tcx.for_each_relevant_impl(deref_did, source_ty, |impl_did| {
      if found.is_some() {
        return;
      }
      // `for_each_relevant_impl(deref_did, ..)` only yields impls of `Deref`, so each has a trait ref.
      let self_ty = tcx.impl_trait_ref(impl_did).instantiate_identity().self_ty();
      let TyKind::Adt(self_adt, _) = self_ty.kind() else {
        return;
      };
      if self_adt.did() != source_def_id {
        return;
      }
      let mut target: Option<DerefTarget> = None;
      let mut deref_fn: Option<DefId> = None;
      for assoc in tcx.associated_items(impl_did).in_definition_order() {
        match assoc.as_tag() {
          rustc_middle::ty::AssocTag::Type => {
            let ty = tcx.type_of(assoc.def_id).instantiate_identity();
            match ty.kind() {
              // A named, non-generic ADT target (`Sheath: Deref<Target=Core>`).
              TyKind::Adt(target_adt, target_args) if target_args.types().next().is_none() => {
                target = Some(DerefTarget::Adt(target_adt.did()));
              }
              // A slice `[T]` target (`Vec<T>: Deref<Target=[T]>`) — the element accessor path.
              TyKind::Slice(_) => {
                target = Some(DerefTarget::Slice);
              }
              // A generic ADT target, `str`, or `dyn`: out of scope.
              _ => {}
            }
          }
          rustc_middle::ty::AssocTag::Fn => deref_fn = Some(assoc.def_id),
          _ => {}
        }
      }
      if let (Some(t), Some(f)) = (target, deref_fn) {
        found = Some((f, t));
      }
    });
    let Some((deref_fn_did, target)) = found else {
      continue;
    };
    let target_idx = match target {
      DerefTarget::Adt(target_did) => {
        // A Rust enum imports as an opaque extern struct (`Type`), like any other imported enum.
        let target_kind = match tcx.def_kind(target_did) {
          DefKind::Struct | DefKind::Enum => ItemKind::Type,
          _ => continue,
        };
        match items.iter().position(|i| {
          matches!(i.kind, ItemKind::Type | ItemKind::Enum)
            && i.origin == RustItemOrigin::Rustc(target_did)
        }) {
          Some(idx) => idx,
          None => {
            items.push(RustItem {
              human_name: scout_arena.intern_str(tcx.item_name(target_did).as_str()),
              origin: RustItemOrigin::Rustc(target_did),
              package: package_coord_for(tcx, scout_arena, target_did),
              kind: target_kind,
              generic_params: own_generic_param_names(tcx, scout_arena, target_did),
            });
            let new_idx = items.len() - 1;
            add_inherent_methods(tcx, scout_arena, items, new_idx);
            add_synthesized_drop(keywords, items, new_idx);
            newly_added_targets.push(new_idx);
            new_idx
          }
        }
      }
      // The slice target has no ADT `DefId`: register the one synthesized opaque `__slice<T>`
      // citizen and attach its slice methods via `incoherent_inherent_impls`.
      DerefTarget::Slice => match items.iter().position(|i| i.origin == RustItemOrigin::Slice) {
        Some(idx) => idx,
        None => {
          let slice_idx = push_slice_citizen(scout_arena, items);
          add_slice_inherent_methods(tcx, scout_arena, items, slice_idx);
          newly_added_targets.push(slice_idx);
          slice_idx
        }
      },
    };
    // The synthesized `deref` carries the source's own generic parameters, so its receiver is
    // `&Source<T…>` and (for a slice) its return is `&__slice<element>` in that same generic space.
    // For a non-generic source this is the empty list, exactly as before.
    let source_generic_params = items[source_idx].generic_params.clone();
    let source_package = items[source_idx].package;
    items.push(RustItem {
      human_name: scout_arena.intern_str("deref"),
      origin: RustItemOrigin::Rustc(deref_fn_did),
      package: source_package,
      kind: ItemKind::DerefMethod { owner: source_idx, target: target_idx },
      generic_params: source_generic_params,
    });
  }
  newly_added_targets
}

#[derive(Copy, Clone)]
enum DerefTarget {
  /// A named, non-generic ADT (`Core`).
  Adt(DefId),
  /// A slice `[T]` — represented by the one synthesized `__slice<T>` citizen.
  Slice,
}

/// Push the one synthesized opaque `__slice<T>` citizen (origin `Slice`, no `DefId`). Its single
/// generic parameter `T` is the element type; its methods are attached by
/// `add_slice_inherent_methods`.
fn push_slice_citizen<'s>(scout_arena: &ScoutArena<'s>, items: &mut Vec<RustItem<'s>>) -> usize {
  let package = scout_arena.intern_package_coordinate(scout_arena.intern_str("core"), &[]);
  items.push(RustItem {
    human_name: scout_arena.intern_str("__slice"),
    origin: RustItemOrigin::Slice,
    package,
    kind: ItemKind::Type,
    generic_params: vec![scout_arena.intern_str("T")],
  });
  items.len() - 1
}

/// Attach the slice's inherent methods (the `impl<T> [T]` block), reached via
/// `incoherent_inherent_impls(SimplifiedType::Slice)` because a slice has no ADT `DefId` to key
/// `inherent_impls` on. Minimum scope: only `get` (the element accessor); other slice methods come
/// later. Signatures are synthesized in `fn_sig`'s slice-method arm, never lowered raw (the raw
/// `get` sig is a `SliceIndex` projection the oracle declines).
fn add_slice_inherent_methods<'s>(
  tcx: TyCtxt<'_>,
  scout_arena: &ScoutArena<'s>,
  items: &mut Vec<RustItem<'s>>,
  slice_idx: usize,
) {
  let package = items[slice_idx].package;
  let generic_params = items[slice_idx].generic_params.clone();
  for impl_def_id in
    tcx.incoherent_impls(rustc_middle::ty::fast_reject::SimplifiedType::Slice).iter()
  {
    for assoc in tcx.associated_items(*impl_def_id).in_definition_order() {
      if assoc.as_tag() != rustc_middle::ty::AssocTag::Fn {
        continue;
      }
      if assoc.name().as_str() != "get" {
        continue;
      }
      items.push(RustItem {
        human_name: scout_arena.intern_str(assoc.name().as_str()),
        origin: RustItemOrigin::Rustc(assoc.def_id),
        package,
        kind: ItemKind::Method(slice_idx),
        generic_params: generic_params.clone(),
      });
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ItemKind {
  Function,
  Type,
  /// A Rust enum — imported as an opaque sealed interface. Like `Type` it owns inherent methods and a
  /// drop; it differs only in lowering to an interface instead of a struct.
  Enum,
  /// A Rust trait — imported as a Vale interface a struct can implement. Unlike `Enum` it owns no
  /// inherent methods; its abstract methods are the trait's own associated functions.
  Trait,
  /// A method (or a type's synthesized drop), with the index of the item it hangs off.
  Method(usize),
  /// The `deref` method synthesized from a type's shared `Deref<Target=U>` impl. `owner` is the
  /// source type's item index (this method hangs off it, like `Method`), `target` the item index of
  /// `U`. `fn_sig` builds `&Source -> &Target` directly from these two, rather than lowering `deref`'s
  /// raw signature — whose `Self` is a `ty::Param` and whose return is the `<Self as Deref>::Target`
  /// projection the oracle declines (`UnnormalizableAlias`).
  DerefMethod { owner: usize, target: usize },
}

/// One resolved Rust item. `RustItemId` indexes a flat table of these — functions, types, methods
/// and synthesized drops share one id space so the trait needs only one handle type.
struct RustItem<'s> {
  human_name: StrI<'s>,
  origin: RustItemOrigin,
  package: &'s PackageCoordinate<'s>,
  kind: ItemKind,
  /// The item's generic parameter names, in declaration order: its own for a top-level item,
  /// parent-inclusive for a method, the owner's for a drop.
  ///
  /// Interned here rather than on demand because the oracle cannot hold the scout arena: a
  /// `&'s ScoutArena<'s>` field would force the arena to outlive `'s`, which is `'s` itself.
  /// Construction is the one place the arena is in hand, and these names never change, so
  /// computing them once is both simpler and the only shape that borrows.
  ///
  /// Names rather than a count because `lower_sig_ty` resolves a `ty::Param` against this list
  /// by name, sidestepping the parent-inclusive index arithmetic entirely.
  generic_params: Vec<StrI<'s>>,
}

impl<'s> RustItem<'s> {
  fn def_id(&self) -> Option<DefId> {
    match self.origin {
      RustItemOrigin::Rustc(def_id) => Some(def_id),
      RustItemOrigin::SynthesizedDrop => None,
      RustItemOrigin::Slice => None,
    }
  }

  /// The item this one hangs off, for a method, a `deref` or a drop. `None` for a top-level item.
  fn container(&self) -> Option<RustItemId> {
    match self.kind {
      ItemKind::Method(owner) | ItemKind::DerefMethod { owner, .. } => Some(RustItemId(owner as u32)),
      ItemKind::Function | ItemKind::Type | ItemKind::Enum | ItemKind::Trait => None,
    }
  }

  /// The kind this item has in a `ResolvedName`. A method is a function; only its container differs.
  fn imported_kind(&self) -> ImportedItemKind {
    match self.kind {
      ItemKind::Type => ImportedItemKind::Type,
      ItemKind::Enum => ImportedItemKind::Enum,
      ItemKind::Trait => ImportedItemKind::Trait,
      ItemKind::Function | ItemKind::Method(_) | ItemKind::DerefMethod { .. } => {
        ImportedItemKind::Function
      }
    }
  }

  fn resolved_name(&self) -> ResolvedName<'s> {
    ResolvedName {
      package_coord: self.package,
      importee_name: self.human_name,
      kind: self.imported_kind(),
    }
  }
}

pub struct TyCtxtOracle<'tcx, 's> {
  tcx: TyCtxt<'tcx>,
  /// Every importable item, resolved once at construction, with every imported type's methods and
  /// synthesized drop.
  ///
  /// Precomputed rather than resolved on demand because the trait's methods take `&self`, so they
  /// cannot memoize without interior mutability.
  items: Vec<RustItem<'s>>,
  /// Item indices of types added ONLY as the shared `Deref` target of an imported type — reached
  /// transitively, never imported by name. `deref_target_imports` hands these to the import loop so
  /// each is declared implicitly (its `StructS` seeded, its methods attached), which is what lets a
  /// `deref`-reached method resolve on it and a synthesized `deref`'s `&Target` return type be named.
  deref_target_indices: Vec<usize>,
}

/// Resolve one **crate-qualified** path (`crate`, then `modules`, then `item_name`) to a single item.
///
/// Because the crate is named, resolution is unambiguous: there is no cross-crate scan and no
/// plurality — `crate1.Widget` and `crate2.Widget` are different paths naming different items.
/// Returns `None` when the crate is not loaded, a module segment is missing, or the final item is
/// not a fn/struct/enum/trait.
///
/// A re-exported item resolves through the re-export chain to its canonical `DefId` (e.g.
/// `std.vec.Vec` reaches `alloc`'s `Vec`), so identity still comes from the `DefId`, never the path.
///
/// **Intermediate segments must be modules** — the `DefKind::Mod` filter stops a struct named `vec`
/// from swallowing the `vec` in `std::vec::Vec`, the same `DefKind` filter the final item needs.
pub(crate) fn resolve_crate_qualified_path<'tcx>(
  tcx: TyCtxt<'tcx>,
  crate_name: StrI<'_>,
  modules: &[StrI<'_>],
  item_name: StrI<'_>,
) -> Option<DefId> {
  // The named crate must be a loaded dependency. `tcx.crates(())` is exactly those external crates;
  // `LOCAL_CRATE` (the compiled crate) is deliberately not among them, so it is never importable.
  let cnum =
    tcx.crates(()).iter().copied().find(|c| tcx.crate_name(*c).as_str() == crate_name.0)?; // ataflbz-allow: selection

  let mut module = cnum.as_def_id();
  for segment in modules {
    module = tcx
      .module_children(module)
      .iter()
      .find(|c| c.ident.as_str() == segment.0) // ataflbz-allow: selection
      .and_then(|c| match c.res {
        Res::Def(DefKind::Mod, def_id) => Some(def_id),
        _ => None,
      })?;
  }

  for child in tcx.module_children(module) {
    // Selection — the final segment deciding which child is admitted. Identity comes from the
    // `DefId` captured below, not from this name match.
    if child.ident.as_str() != item_name.0 { // ataflbz-allow: selection
      continue;
    }
    // Filter on DefKind: a module's children include re-exported modules, `extern crate` entries,
    // etc., so a name match alone could hand back a module where a fn/struct was asked for.
    if let Res::Def(DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Trait, def_id) = child.res {
      return Some(def_id);
    }
  }
  None
}

impl<'tcx, 's> TyCtxtOracle<'tcx, 's> {
  /// Resolve the program's Rust imports against the loaded crate graph, then add every imported
  /// type's inherent methods and drop, every imported trait's methods, and single-step `Deref`
  /// targets.
  ///
  /// An import that resolves to nothing is simply left out of the table; `resolve_import` then
  /// answers `None` for it, which the import loop reports as `UnresolvableRustImport`.
  ///
  /// `crate_name` and `def_path` are the safe accessors — `def_path_str` ICEs outside
  /// diagnostic contexts, and its panic blames rustc internals rather than the call site,
  /// which is what makes it expensive to diagnose (@DPSFDOZ).
  pub fn new(
    tcx: TyCtxt<'tcx>,
    scout_arena: &ScoutArena<'s>,
    keywords: &Keywords<'s>,
    imports: &[&ImportS<'s>],
  ) -> Self {
    let mut items: Vec<RustItem<'s>> = Vec::new();

    for import in imports {
      let Some(def_id) =
        resolve_crate_qualified_path(tcx, import.module_name, import.package_names, import.importee_name)
      else {
        continue;
      };
      // The same item imported twice (from two files, say) is one table entry.
      if items.iter().any(|i| i.origin == RustItemOrigin::Rustc(def_id)) {
        continue;
      }
      let kind = match tcx.def_kind(def_id) {
        DefKind::Fn => ItemKind::Function,
        // A Rust enum imports as an opaque extern struct (`Type`), NOT a Vale interface. Horizon
        // imports no variants, so Vale can never match on it — it only holds and passes the value
        // (and calls inherent methods like `Option::unwrap`). An opaque struct is sized by rustc's
        // `layout_of` and crosses as an opaque blob (e.g. `Option<&T>`'s niche pointer), whereas a
        // Vale interface would be a two-word control-block+vtable fat ref it has no use for and
        // whose size wouldn't match rustc's.
        DefKind::Struct | DefKind::Enum => ItemKind::Type,
        DefKind::Trait => ItemKind::Trait,
        other => panic!("resolve_crate_qualified_path returned a {other:?}, which it filters out"),
      };
      let mut generic_params = own_generic_param_names(tcx, scout_arena, def_id);
      if kind == ItemKind::Trait {
        // A Rust trait's `own_params` include the implicit `Self` (index 0). `Self` is the
        // interface itself, not one of its generic parameters, so it must not become a Vale
        // interface generic — leaving it in makes `predict_interface_layer` panic trying to
        // infer a rune nothing binds. `Self` is reserved, so filtering by name is safe.
        generic_params.retain(|n| n.0 != "Self");
      }
      items.push(RustItem {
        human_name: scout_arena.intern_str(tcx.item_name(def_id).as_str()),
        origin: RustItemOrigin::Rustc(def_id),
        package: package_coord_for(tcx, scout_arena, def_id),
        kind,
        generic_params,
      });
    }

    // Methods come from inherent impls. Trait impls are deliberately not walked yet:
    // "all impls of a trait" is unbounded in Rust because of blanket impls, so that
    // question needs a design rather than a walk.
    let type_indices: Vec<usize> = items
      .iter()
      .enumerate()
      .filter(|(_, i)| matches!(i.kind, ItemKind::Type | ItemKind::Enum))
      .map(|(idx, _)| idx)
      .collect();
    for owner_idx in type_indices {
      add_inherent_methods(tcx, scout_arena, &mut items, owner_idx);
      add_synthesized_drop(keywords, &mut items, owner_idx);
    }

    // A trait's abstract methods come from its **own** associated items, not from inherent impls.
    // They are recorded as `Method(trait_idx)` exactly like a type's inherent methods, so
    // `methods(trait_item)` and `fn_sig` reach them through the same path. Their signatures name the
    // trait's implicit `Self` (index 0 of the parent-inclusive params); the trait synthesis maps that
    // `Self` to the interface itself when it builds the abstract method.
    let trait_indices: Vec<usize> =
      items.iter().enumerate().filter(|(_, i)| i.kind == ItemKind::Trait).map(|(idx, _)| idx).collect();
    for owner_idx in trait_indices {
      let owner_def_id = items[owner_idx].def_id().expect("an imported trait always has a rustc DefId");
      let package = items[owner_idx].package;
      for assoc in tcx.associated_items(owner_def_id).in_definition_order() {
        if assoc.as_tag() != rustc_middle::ty::AssocTag::Fn {
          continue;
        }
        items.push(RustItem {
          human_name: scout_arena.intern_str(assoc.name().as_str()),
          origin: RustItemOrigin::Rustc(assoc.def_id),
          package,
          kind: ItemKind::Method(owner_idx),
          generic_params: parent_inclusive_generic_param_names(tcx, scout_arena, assoc.def_id),
        });
      }
    }

    // Single-step, shared `Deref`: register each imported type's `Deref` target + a `deref` method,
    // so a Deref-reached method (`s.read()` where `read` lives on `Sheath`'s `Deref::Target`) can
    // resolve via a callsite receiver rewrite to `deref(s)`.
    let deref_target_indices = discover_deref_targets(tcx, scout_arena, keywords, &mut items);

    TyCtxtOracle { tcx, items, deref_target_indices }
  }

  /// Lower one position of a signature, keeping a generic parameter *as* a parameter.
  ///
  /// **Keyed on the parameter's name, deliberately, not on its index.** A `ty::Param`'s `index`
  /// is into the item's *parent-inclusive* generic list — for a method on `impl<T> Foo<T>` the
  /// impl's parameters occupy the low indices and the item's own follow — so using it directly
  /// against a declaration that names only the item's own parameters is an off-by-`parent_count`
  /// waiting to happen. Names sidestep the arithmetic entirely, and they are safe to key on
  /// because **Rust forbids an item from reusing a generic parameter name declared by its parent
  /// impl** (E0403), so within an item plus its parents the names are unique.
  ///
  /// Getting this wrong would be quiet — a well-formed reference to the wrong slot surfaces at a
  /// call site as a plausible *concrete* type rather than anything resembling a placeholder.
  /// Hence the `pick<A, B>` fixture instantiated at two different types: a swap yields `bool`
  /// where `int` belongs, which a test can see.
  ///
  /// `Err` for anything not representable, which drops the whole declaration rather than
  /// importing it with a hole — and carries *why*, so the eventual lookup failure can say
  /// something better than "couldn't find it".
  fn lower_sig_ty<'t>(
    &self,
    ty: Ty<'tcx>,
    own_param_names: &[StrI<'s>],
    interner: &TypingInterner<'s, 't>,
  ) -> Result<TypeR<'s, 't>, CouldNotPostparseReason>
  where
    's: 't,
  {
    match ty.kind() {
      TyKind::Param(param) => {
        let name = param.name.as_str();
        match own_param_names.iter().position(|p| p.0 == name) {
          Some(index) => Ok(TypeR::Generic(index as u32)),
          // Not among the item's parameters, so it was inherited from something this item does
          // not declare. Vale's declaration has no slot for it.
          None => Err(CouldNotPostparseReason::InheritedParameter),
        }
      }
      // A projection — `<I as Iterator>::Item` and friends. Not merely unbounded: resolving
      // it *requires* the `I: Iterator` predicate to find the impl, and we deliberately read
      // no predicates for it. So the type isn't unreadable-for-now, it's un-normalizable, and
      // importing it would put an alias in the declaration that nothing can resolve.
      TyKind::Alias(..) => Err(CouldNotPostparseReason::UnnormalizableAlias),
      // An imported citizen, kept **unapplied** with its arguments as signature positions of
      // their own. A `ty::Param` argument (`Holder<T>`) is a legal *position* even though it is
      // not a legal type, so recursing through `lower_sig_ty` handles `Holder<i32>` and
      // `Holder<T>` alike.
      TyKind::Adt(adt_def, adt_args) => {
        let did = adt_def.did();
        let idx = self
          .items
          .iter()
          .position(|i| matches!(i.kind, ItemKind::Type | ItemKind::Enum) && i.origin == RustItemOrigin::Rustc(did))
          .ok_or(CouldNotPostparseReason::UnimportedType)?;
        let args: Vec<TypeR<'s, 't>> = adt_args
          .types()
          .map(|arg| self.lower_sig_ty(arg, own_param_names, interner))
          .collect::<Result<Vec<_>, _>>()?;
        Ok(TypeR::Citizen {
          package: self.items[idx].package,
          name: self.items[idx].human_name,
          args: interner.alloc_slice_from_vec(args),
        })
      }
      // A reference — a `&self` receiver or a borrowed parameter. Kept structural (a borrow *of
      // a position*) so the inner citizen keeps its package path and an inner generic keeps its
      // slot.
      // A reference. A `&[T]` is special: the reference *is* the two-word fat pointer, so it
      // collapses into the **by-value** `__slice<T>` citizen rather than a one-word borrow of it (a
      // borrow could not hold two words). Every other reference stays a structural borrow of its
      // inner position.
      TyKind::Ref(_, inner, mutbl) => {
        if let TyKind::Slice(elem) = inner.kind() {
          self.slice_citizen_type(*elem, own_param_names, interner)
        } else {
          Ok(TypeR::Borrow {
            inner: interner.alloc(self.lower_sig_ty(*inner, own_param_names, interner)?),
            is_mut: mutbl.is_mut(),
          })
        }
      }
      // A bare (by-value) slice position reduces to the same `__slice<T>` citizen. A truly unsized
      // by-value slice never appears in a real signature; this keeps the mapping total.
      TyKind::Slice(elem) => self.slice_citizen_type(*elem, own_param_names, interner),
      _ => Ok(TypeR::Primitive(lower_primitive(ty)?)),
    }
  }

  /// The by-value `__slice<elem>` citizen `TypeR` for a Rust slice element. A slice has no ADT
  /// `DefId`, so it is the one synthesized opaque citizen keyed by `RustItemOrigin::Slice`; the
  /// element lowers as a signature position of its own, like any citizen type argument.
  fn slice_citizen_type<'t>(
    &self,
    elem: Ty<'tcx>,
    own_param_names: &[StrI<'s>],
    interner: &TypingInterner<'s, 't>,
  ) -> Result<TypeR<'s, 't>, CouldNotPostparseReason>
  where
    's: 't,
  {
    let idx = self
      .items
      .iter()
      .position(|i| i.origin == RustItemOrigin::Slice)
      .ok_or(CouldNotPostparseReason::UnimportedType)?;
    let elem_r = self.lower_sig_ty(elem, own_param_names, interner)?;
    Ok(TypeR::Citizen {
      package: self.items[idx].package,
      name: self.items[idx].human_name,
      args: interner.alloc_slice_from_vec(vec![elem_r]),
    })
  }

  /// The element type of the source's `Deref<Target=[elem]>` impl, lowered into the source's own
  /// generic space (so `Vec<T,A>: Deref<Target=[T]>` yields `Generic(0)`). Used to build the slice
  /// `deref`'s `&__slice<element>` return type. Re-walks the impl rather than threading the element
  /// through `ItemKind`, keeping the discovered item model unchanged.
  fn slice_deref_element<'t>(
    &self,
    owner_idx: usize,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<TypeR<'s, 't>, CouldNotPostparseReason>
  where
    's: 't,
  {
    let owner = &self.items[owner_idx];
    let owner_def_id = owner.def_id().expect("a deref source always has a DefId");
    let owner_ty = self.tcx.type_of(owner_def_id).instantiate_identity();
    let deref_did = self.tcx.lang_items().deref_trait().expect("the Deref lang item exists");
    let mut result: Option<Result<TypeR<'s, 't>, CouldNotPostparseReason>> = None;
    self.tcx.for_each_relevant_impl(deref_did, owner_ty, |impl_did| {
      if result.is_some() {
        return;
      }
      let self_ty = self.tcx.impl_trait_ref(impl_did).instantiate_identity().self_ty();
      let TyKind::Adt(self_adt, _) = self_ty.kind() else {
        return;
      };
      if self_adt.did() != owner_def_id {
        return;
      }
      for assoc in self.tcx.associated_items(impl_did).in_definition_order() {
        if assoc.as_tag() == rustc_middle::ty::AssocTag::Type {
          let ty = self.tcx.type_of(assoc.def_id).instantiate_identity();
          if let TyKind::Slice(elem) = ty.kind() {
            result = Some(self.lower_sig_ty(*elem, &owner.generic_params, interner));
          }
        }
      }
    });
    result.unwrap_or(Err(CouldNotPostparseReason::UnimportedType))
  }

  /// Signature for a synthesized slice method (minimum: `get`). The method's one Vale generic is
  /// the slice element (the impl's `T`); its own `SliceIndex` parameter is concretized to `usize`
  /// and normalized away, so `<usize as SliceIndex<[T]>>::Output` resolves to `T`. The resulting
  /// concrete signature (`&[T] × usize -> Option<&T>`) then lowers through `lower_sig_ty`, whose
  /// slice arm renders the `&[T]` receiver as `&__slice<T>`.
  fn slice_method_sig<'t>(
    &self,
    rust_item: &RustItem<'s>,
    owner_idx: usize,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<FuncSignatureR<'s, 't>, CouldNotPostparseReason>
  where
    's: 't,
  {
    let def_id = rust_item.def_id().expect("a slice method has a DefId");
    let elem_name = self.items[owner_idx].generic_params[0];
    let own_param_names = [elem_name];
    let parent_count = self.tcx.generics_of(def_id).parent_count;
    let usize_ty = self.tcx.types.usize;
    // Keep the impl's element parameter (`T`) as itself; concretize the method's own type
    // parameter (the `SliceIndex` `I`) to `usize`.
    let args = rustc_middle::ty::GenericArgs::for_item(self.tcx, def_id, |param, _| {
      match param.kind {
        rustc_middle::ty::GenericParamDefKind::Type { .. }
          if (param.index as usize) >= parent_count =>
        {
          usize_ty.into()
        }
        _ => self.tcx.mk_param_from_def(param),
      }
    });
    let sig = self.tcx.fn_sig(def_id).instantiate(self.tcx, args).skip_binder();
    let sig = self
      .tcx
      .normalize_erasing_regions(rustc_middle::ty::TypingEnv::post_analysis(self.tcx, def_id), sig);
    let params: Vec<TypeR<'s, 't>> = sig
      .inputs()
      .iter()
      .map(|ty| self.lower_sig_ty(*ty, &own_param_names, interner))
      .collect::<Result<Vec<_>, _>>()?;
    let ret = self.lower_sig_ty(sig.output(), &own_param_names, interner)?;
    Ok(FuncSignatureR {
      generic_param_names: interner.alloc_slice_copy(&own_param_names),
      generic_param_bounds: &[],
      params: interner.alloc_slice_from_vec(params),
      ret,
    })
  }
}

/// Lower a rustc scalar to Vale's closed primitive set.
///
/// `Err` rather than a panic: these fire when a *called* function's signature is read, and a
/// decline surfaces as a `CouldNotPostparseFunction` compile error naming why, rather than as a
/// crash. An imported-but-uncalled function never gets here at all, because signatures are read
/// lazily.
fn lower_primitive(ty: Ty<'_>) -> Result<PrimitiveR, CouldNotPostparseReason> {
  match ty.kind() {
    TyKind::Bool => Ok(PrimitiveR::Bool),
    TyKind::Tuple(tys) if tys.is_empty() => Ok(PrimitiveR::Void),
    TyKind::Int(rustc_middle::ty::IntTy::I32) => Ok(PrimitiveR::Int32),
    TyKind::Int(rustc_middle::ty::IntTy::I64) => Ok(PrimitiveR::Int64),
    TyKind::Int(_) => Err(CouldNotPostparseReason::IntWidth),
    // `usize` imports as the Vale `usize` primitive (a distinct kind, never unified with
    // `int`/`i64`). The other unsigned widths (`u8`..`u64`) still decline for now.
    TyKind::Uint(rustc_middle::ty::UintTy::Usize) => Ok(PrimitiveR::USize),
    TyKind::Uint(_) => Err(CouldNotPostparseReason::UnsignedInteger),
    TyKind::Float(_) => Err(CouldNotPostparseReason::Float),
    TyKind::Str | TyKind::Slice(_) | TyKind::Dynamic(..) => Err(CouldNotPostparseReason::Unsized),
    _ => Err(CouldNotPostparseReason::Unrepresentable),
  }
}

impl<'tcx, 's, 't> RustOracle<'s, 't> for TyCtxtOracle<'tcx, 's>
where
  's: 't,
{
  fn resolve(&self, container: Option<RustItemId>, name: &ResolvedName<'s>) -> Option<RustItemId> {
    // A name resolves to the one table item with that container, coordinate, short name and kind.
    // Identity still comes from the `DefId` behind the item; this match is *selection* — which
    // already-resolved item a canonical name picks out — keyed by the full coordinate and the
    // container rather than a bare short name, so a free `get` and `Counter::get` stay apart.
    self
      .items
      .iter()
      .position(|item| item.container() == container && item.resolved_name() == *name) // ataflbz-allow: selection
      .map(|idx| RustItemId(idx as u32))
  }

  fn resolve_import(&self, import: &ImportS<'s>) -> Option<ResolvedName<'s>> {
    // One crate-qualified path resolves to at most one item; find it in the already-resolved table
    // and hand back its canonical name. Only top-level items come back, since the resolver
    // filters to fn/struct/enum/trait.
    let def_id = resolve_crate_qualified_path(
      self.tcx,
      import.module_name,
      import.package_names,
      import.importee_name,
    )?;
    let item = self
      .items
      .iter()
      .find(|item| item.container().is_none() && item.origin == RustItemOrigin::Rustc(def_id))?;
    Some(item.resolved_name())
  }

  fn fn_sig(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<FuncSignatureR<'s, 't>, CouldNotPostparseReason> {
    let rust_item =
      self.items.get(item.0 as usize).expect("fn_sig: RustItemId out of range (internal bug)");

    // A synthesized drop: `drop(self Owner<T…>) void`, the receiver being the owner applied to its
    // own generic parameters.
    if rust_item.origin == RustItemOrigin::SynthesizedDrop {
      let owner = &self.items[rust_item.container().expect("a drop always hangs off its type").0 as usize];
      let receiver_args: Vec<TypeR<'s, 't>> =
        (0..owner.generic_params.len()).map(|i| TypeR::Generic(i as u32)).collect();
      let receiver = TypeR::Citizen {
        package: owner.package,
        name: owner.human_name,
        args: interner.alloc_slice_from_vec(receiver_args),
      };
      return Ok(FuncSignatureR {
        generic_param_names: interner.alloc_slice_copy(&rust_item.generic_params),
        generic_param_bounds: &[],
        params: interner.alloc_slice_from_vec(vec![receiver]),
        ret: TypeR::Primitive(PrimitiveR::Void),
      });
    }

    // A synthesized `deref`: build `&Source<T…> -> &Target<…>` directly from the discovered
    // (source, target), rather than lowering `deref`'s raw signature — whose `Self` is a `ty::Param`
    // and whose return is the `<Self as Deref>::Target` projection this oracle declines. The method
    // carries the source's own generics, so the receiver is `&Source<T…>`. A named non-generic
    // target returns `&Target` (no args); the `__slice<T>` target returns `&__slice<element>`, the
    // element read from the source's `Deref` impl in the source's generic space. The elided
    // `&Target` return ties to `&self`'s region, exactly a `&self` accessor's borrow-return shape.
    if let ItemKind::DerefMethod { owner, target } = rust_item.kind {
      let src = &self.items[owner];
      let tgt = &self.items[target];
      let src_args: Vec<TypeR<'s, 't>> =
        (0..src.generic_params.len()).map(|i| TypeR::Generic(i as u32)).collect();
      let src_citizen = interner.alloc(TypeR::Citizen {
        package: src.package,
        name: src.human_name,
        args: interner.alloc_slice_from_vec(src_args),
      });
      // The return. A `&[T]` deref target is the by-value fat pointer `__slice<element>` (not a
      // borrow of it); a named ADT target (single-step scope: non-generic) is reached by borrow.
      let ret = if tgt.origin == RustItemOrigin::Slice {
        let element = self.slice_deref_element(owner, interner)?;
        TypeR::Citizen {
          package: tgt.package,
          name: tgt.human_name,
          args: interner.alloc_slice_from_vec(vec![element]),
        }
      } else {
        let tgt_citizen =
          interner.alloc(TypeR::Citizen { package: tgt.package, name: tgt.human_name, args: &[] });
        TypeR::Borrow { inner: tgt_citizen, is_mut: false }
      };
      return Ok(FuncSignatureR {
        generic_param_names: interner.alloc_slice_copy(&src.generic_params),
        generic_param_bounds: &[],
        params: interner.alloc_slice_from_vec(vec![TypeR::Borrow { inner: src_citizen, is_mut: false }]),
        ret,
      });
    }

    // A synthesized slice method (`<[T]>::get`), hanging off the `__slice<T>` citizen. Its raw rustc
    // signature is generic over a `SliceIndex` with an `<I as SliceIndex<[T]>>::Output` projection
    // this oracle won't normalize, so `slice_method_sig` concretizes the index parameter to `usize`
    // and normalizes — yielding `get(&__slice<T>, usize) -> Option<&T>`.
    if let ItemKind::Method(owner_idx) = rust_item.kind {
      if self.items[owner_idx].origin == RustItemOrigin::Slice {
        return self.slice_method_sig(rust_item, owner_idx, interner);
      }
    }
    let def_id = rust_item.def_id().expect("only a synthesized drop lacks a DefId, handled above");

    // @EarlyBinder: deliberately NOT instantiating. `instantiate_identity` discards the
    // binder and leaves `ty::Param`s standing, which is exactly what structural reading
    // wants — one reading serves every instantiation.
    //
    // Only the outer `EarlyBinder` is opened. The inner `Binder` holds late-bound
    // *lifetimes* and nothing else — type and const parameters are always early-bound — so
    // there is no type information hiding behind `skip_binder`.
    let binder = self.tcx.fn_sig(def_id).instantiate_identity();
    let sig = binder.skip_binder();

    // A lifetime shared across two or more parameters (`fn f<'a>(x: &'a mut A, y: &'a B)`) would have
    // to tie those parameters into one Vale group, which needs lifetime decoding not built yet. Decline
    // rather than guess they are disjoint — the assumption per-parameter groups make. Only each
    // parameter's top-level reference is inspected: an explicit shared `'a` is an early-bound region,
    // equal across parameters after `instantiate_identity`, while elided lifetimes are distinct
    // late-bound regions, so region equality catches exactly the shared case.
    let param_regions: Vec<rustc_middle::ty::Region<'tcx>> = sig
      .inputs()
      .iter()
      .filter_map(|input| match input.kind() {
        TyKind::Ref(region, _, _) => Some(*region),
        _ => None,
      })
      .collect();
    for i in 0..param_regions.len() {
      for j in (i + 1)..param_regions.len() {
        if param_regions[i] == param_regions[j] {
          return Err(CouldNotPostparseReason::SharedParameterLifetime);
        }
      }
    }

    let generic_params = &rust_item.generic_params;

    // Each position lowers to a `TypeR` or declines with a `CouldNotPostparseReason`. The first
    // decline is propagated, so a *called* function whose signature Vale cannot represent fails
    // with the reason rather than a bare miss — `Compiler::illuminate_function` turns that reason
    // into a `CouldNotPostparseFunction` compile error.
    let params: Vec<TypeR<'s, 't>> = sig
      .inputs()
      .iter()
      .map(|ty| self.lower_sig_ty(*ty, generic_params, interner))
      .collect::<Result<Vec<_>, _>>()?;
    let ret = self.lower_sig_ty(sig.output(), generic_params, interner)?;

    // Surface each `where P: Trait` predicate whose subject P is one of this function's generic
    // params and whose trait is imported, as a Vale `where implements(P, Trait)` impl bound. Rust
    // discharges these itself for the forward direction; the reverse direction needs them, because
    // a rust caller `run<C: MainLoop>`'s `C: MainLoop` bound is what tells Vale an impl
    // `MyStruct: MainLoop` exists. Auto-traits / Sized / un-imported traits are dropped: none
    // appear in `self.items`. The trait is kept unapplied (its own type args, if any, lowered as
    // signature positions), matching how a parameter citizen is kept.
    let generic_param_bounds: Vec<ImplBoundR<'s, 't>> = self
      .tcx
      .predicates_of(def_id)
      .predicates
      .iter()
      .filter_map(|(clause, _span)| {
        let trait_pred = clause.as_trait_clause()?.skip_binder();
        if trait_pred.polarity != rustc_middle::ty::PredicatePolarity::Positive {
          return None;
        }
        let TyKind::Param(param) = trait_pred.self_ty().kind() else {
          return None;
        };
        let sub_generic_index =
          generic_params.iter().position(|p| p.0 == param.name.as_str())? as u32;
        let trait_did = trait_pred.trait_ref.def_id;
        let idx = self
          .items
          .iter()
          .position(|i| i.kind == ItemKind::Trait && i.origin == RustItemOrigin::Rustc(trait_did))?;
        // The trait's own type args, if any — skip `args[0]`, which is the `Self` (the sub param).
        let trait_args: Vec<TypeR<'s, 't>> = trait_pred
          .trait_ref
          .args
          .types()
          .skip(1)
          .map(|arg| self.lower_sig_ty(arg, generic_params, interner))
          .collect::<Result<Vec<_>, _>>()
          .ok()?;
        Some(ImplBoundR {
          sub_generic_index,
          super_trait: TypeR::Citizen {
            package: self.items[idx].package,
            name: self.items[idx].human_name,
            args: interner.alloc_slice_from_vec(trait_args),
          },
        })
      })
      .collect();

    Ok(FuncSignatureR {
      generic_param_names: interner.alloc_slice_copy(generic_params),
      generic_param_bounds: interner.alloc_slice_from_vec(generic_param_bounds),
      params: interner.alloc_slice_from_vec(params),
      ret,
    })
  }

  fn type_generic_params(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> &'t [StrI<'s>] {
    let rust_item =
      self.items.get(item.0 as usize).expect("type_generic_params: RustItemId out of range (internal bug)");
    interner.alloc_slice_copy(&rust_item.generic_params)
  }

  fn methods(&self, item: RustItemId) -> Vec<(StrI<'s>, RustItemId)> {
    // Everything hanging off `item`: inherent methods, a `deref` from a shared `Deref` impl, the
    // synthesized drop, or a trait's own methods.
    self
      .items
      .iter()
      .enumerate()
      .filter(|(_, i)| i.container() == Some(item))
      .map(|(idx, i)| (i.human_name, RustItemId(idx as u32)))
      .collect()
  }

  fn deref_target_imports(&self) -> Vec<ResolvedName<'s>> {
    self.deref_target_indices.iter().map(|&idx| self.items[idx].resolved_name()).collect()
  }

  fn rust_spelling(&self, package_coord: &PackageCoordinate<'s>, name: StrI<'s>) -> Option<String> {
    // How a top-level type, enum or trait is written in generated Rust: its canonical *visible* path,
    // e.g. `::nobiliav::MainLoopCallback` for an item defined in the private `nobiliav::window` and
    // re-exported at the root. Its coordinate stays the defining path, which is its identity; this is
    // only the spelling, because a private module in the defining path would be E0603 in the final
    // file. Functions share names with types in Rust's other namespace, so they are skipped.
    let item = self.items.iter().find(|item| {
      item.container().is_none()
        && matches!(item.kind, ItemKind::Type | ItemKind::Enum | ItemKind::Trait)
        && *item.package == *package_coord
        && item.human_name == name // ataflbz-allow: selection
    })?;
    Some(visible_rust_path(self.tcx, item.def_id()?))
  }
}
