use crate::StrI;
use crate::utils::code_hierarchy::PackageCoordinate;
use rustc_hir::def::{DefKind, Res};
use rustc_middle::ty::{Ty, TyCtxt, TyKind};
use rustc_span::def_id::DefId;
use crate::postparsing::ast::ImportS;
use crate::typing::compiler_error_reporter::CouldNotPostparseReason;
use crate::typing::env::environment::{ImportedItemKind, ResolvedName};
use crate::typing::types::types::KindT;
use crate::typing::typing_interner::TypingInterner;

#[derive(Copy, Clone, Debug)]
pub enum PrimitiveR {
  Int32,
  Int64,
  Bool,
  Void,
  USize,
}

#[derive(Copy, Clone, Debug)]
pub enum TypeR<'s, 't> {
  Primitive(PrimitiveR),
  Citizen { package: &'s PackageCoordinate<'s>, name: StrI<'s>, args: &'t [TypeR<'s, 't>] },
  // Eg in `fn moo<T, Y>(t: T, y: Y)`, t's type is Generic(0), y's type is Generic(1)
  Generic(u32),
  Borrow { inner: &'t TypeR<'s, 't>, is_mut: bool },
}

#[derive(Copy, Clone, Debug)]
pub struct ImplBoundR<'s, 't> {
  pub sub_generic_index: u32,
  pub super_trait: TypeR<'s, 't>,
}

#[derive(Copy, Clone, Debug)]
pub struct FuncSignatureR<'s, 't> {
  pub generic_param_names: &'t [StrI<'s>],
  pub generic_param_bounds: &'t [ImplBoundR<'s, 't>],
  pub params: &'t [TypeR<'s, 't>],
  pub ret: TypeR<'s, 't>,
}

// An index into the oracle's list of resolved `RustItem`s.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RustItemId(pub u32);

// Description of a Rust thing, without types (TypeR etc) yet.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum RustItemOrigin {
  Rustc(DefId),
  // drop doesn't exist as a Rust function, there's no drop DefID.
  // We can think of there being a conceptual/synthesized drop<T> function, that's what
  // this refers to.
  SynthesizedDrop,
  // Slice doesn't exist as a Rust struct, there's no slice DefId.
  // We can think of there being a conceptual/synthesized `struct __slice<T>`, that's
  // what this refers to.
  Slice,
}

pub(crate) struct RustItem<'s> {
  pub name: ResolvedName<'s>,
  pub origin: RustItemOrigin,
  // If this is a method (kind == Function), this is the containing type's ID.
  pub container_id: Option<RustItemId>,
  pub generic_param_names: Vec<StrI<'s>>,
}

// We're intentionally making it produce a simplified set of types rather than the full postparseds,
// for better decoupling/mockability.
pub trait RustOracle<'s, 't> {
  // Get the RustItemId for the given ResolvedName.
  fn resolve(&self, container: Option<RustItemId>, name: &ResolvedName<'s>) -> Option<RustItemId>;

  // Ask rustc what this import *actually* refers to.
  // For example, this will turn a `import rust.std.vec.Vec` into a ResolvedName for the real Vec
  // which is at std::alloc::vec::Vec.
  fn resolve_import(&self, import: &ImportS<'s>) -> Option<ResolvedName<'s>>;

  // Generate the Valen signature for the given function RustItemId.
  fn fn_sig(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> Result<FuncSignatureR<'s, 't>, CouldNotPostparseReason>;

  // Fetch the generic params for a given Rust type.
  fn type_generic_params(
    &self,
    item: RustItemId,
    interner: &TypingInterner<'s, 't>,
  ) -> &'t [StrI<'s>];

  // Fetch the methods of a Rust type.
  fn methods(&self, item: RustItemId) -> Vec<(StrI<'s>, RustItemId)>;

  // Get the types from Deref'ing a type.
  // TODO: rename
  fn deref_target_imports(&self) -> Vec<ResolvedName<'s>>;

  // Get the publicly importable path for a type, because some types are private to their own
  // crate but are publicly exported under a different name.
  fn rust_spelling(&self, package_coord: &PackageCoordinate<'s>, name: StrI<'s>) -> Option<String>;
}
