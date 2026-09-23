// The reserved `rust` package coordinate.
//
// A Rust-backed type is an ordinary Vale kind — `KindT::Struct` for a Rust struct,
// `KindT::Interface` for a Rust enum or trait — whose name carries this package
// coordinate. There is no new `KindT` arm and no new name type; "Rust-backed" is a
// property of the name, and this module is the single source of truth for testing it.
//
// See docs/convos/rust_interop/vale-rust-interop-architecture.md §8.10.

use crate::typing::names::names::IdT;
use crate::typing::types::types::*;

/// The reserved module name. `Keywords::rust` holds the interned form; this is the
/// raw string, for comparing a `PackageCoordinate`'s module without needing arena
/// access at every seam.
pub const RUST_MODULE: &str = "rust";

/// The reserved module that owns compiler-generated siblings of an imported Rust trait — the
/// anonymous-substruct forwarder/constructor/drop the anon-substruct macro synthesizes so
/// `SomeRustTrait((x) => {…})` compiles. These MUST NOT live in the `rust` module: the
/// function-compile loop skips `rust` (it postparses Rust methods lazily), so a forwarder there
/// would never compile. And they must be a real (non-builtin, non-`rust`) native package so
/// `citizen_def_id_and_args` routes the substruct through `resolve_local_type` (the projected-type
/// path), exactly like a hand-written forwarder struct. It is a single canonical package so a trait
/// imported from several files yields one substruct, not one per import site.
pub const RUST_TRAIT_ANON_MODULE: &str = "rust_trait_anon";

/// Is this id's package the reserved `rust` package?
pub fn is_rust_backed(id: &IdT) -> bool {
  id.package_coord.module.0 == RUST_MODULE
}

/// Strip the reference-mode onion (borrow/own/share/weak) down to the bare kind.
pub fn peel_refs<'s, 't>(kind: KindT<'s, 't>) -> KindT<'s, 't> {
  let mut current = kind;
  loop {
    current = match current {
      KindT::BorrowRef(r) => r.inner,
      KindT::OwnRef(r) => r.inner,
      KindT::ShareRef(r) => r.inner,
      KindT::WeakRef(r) => r.inner,
      other => return other,
    };
  }
}

/// The citizen id behind a kind, looking through any reference wraps. `None` for
/// primitives, arrays, placeholders, and overload sets.
pub fn citizen_id<'s, 't>(kind: KindT<'s, 't>) -> Option<&'t IdT<'s, 't>> {
  match peel_refs(kind) {
    KindT::Struct(s) => Some(&s.id),
    KindT::RawInterface(i) => Some(&i.inner.id),
    _ => None,
  }
}

/// Is this kind (or the referent behind its reference wraps) Rust-backed?
pub fn is_rust_backed_kind(kind: KindT) -> bool {
  citizen_id(kind).is_some_and(is_rust_backed)
}
