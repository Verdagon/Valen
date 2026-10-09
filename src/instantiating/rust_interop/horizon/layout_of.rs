use rustc_abi::{AbiAlign, Align, BackendRepr, FieldIdx, FieldsShape, LayoutData, Size, VariantIdx, Variants};
use rustc_hashes::Hash64;
use rustc_index::IndexVec;
use rustc_middle::ty::layout::{LayoutError, TyAndLayout};
use rustc_middle::ty::{self, PseudoCanonicalInput, Ty, TyCtxt};

use super::bifrost_state::OpaqueKindI;
use super::driver_state::horizon_state;
use super::override_queries::DEFAULT_LAYOUT_OF;
use super::rustc_ty::{kind_to_rustc_ty, read_opaque_typeid};

/// The `layout_of` override. A Vale type in a Rust slot crosses as `__ValeOpaque<typeid>`, whose own
/// fields are zero-sized markers — so rustc's layout would make a Vale struct passed by value
/// `PassMode::Ignore` (nothing crosses) and give a `Vec<Ship>` no stride. This answers with Vale's real
/// size and alignment instead: the typeid names an instantiated Vale struct in `opaque_universe`, each
/// member lowers to a rustc type that rustc sizes itself, and a C-offset walk over them yields the same
/// layout LLVM gives the backend's struct of those members. The result is `BackendRepr::Memory` (rustc
/// never splits it into scalars), with one offset per marker field so rustc's debuginfo walker, which
/// visits one layout field per source field, stays consistent.
///
/// Only the wrapper ADT itself is intercepted; `&__ValeOpaque<..>`, `*mut`, `Option<..>` and every other
/// type is rustc's (intercepting derived types corrupts their layouts). A typeid naming nothing in the
/// universe is rustc's too: a projected trait-implementing Vale struct carries a
/// `__ValeOpaque<typeid(name)>` *field* keyed on the struct's name, not an instantiated id, and that
/// wrapper crosses only by borrow, where its size is never read.
pub(super) fn lang_layout_of<'tcx>(
  tcx: TyCtxt<'tcx>,
  query: PseudoCanonicalInput<'tcx, Ty<'tcx>>,
) -> Result<TyAndLayout<'tcx>, &'tcx LayoutError<'tcx>> {
  let default = DEFAULT_LAYOUT_OF.get().expect("missing DEFAULT_LAYOUT_OF");
  let ty = query.value;
  let Some(tid) = read_opaque_typeid(tcx, ty) else {
    return default(tcx, query);
  };
  // `__ValeOpaque` exists only in a driven final file, and pass 2 arms the state in
  // `after_expansion`, before any layout is asked — so answering from rustc's zero-sized view here
  // would be silently wrong, never benign.
  let state = horizon_state("layout_of");
  let universe = state.opaque_universe.borrow();
  let def = match universe.get(&tid) {
    None => return default(tcx, query),
    Some(OpaqueKindI::Struct(def)) => def,
    Some(OpaqueKindI::Interface(id)) => panic!(
      "rust interop: Vale interface {id:?} crosses to Rust by value ({ty:?}); only a struct has a \
       by-value layout"
    ),
  };
  let struct_id = &def.instantiated_citizen.id;
  // Each member as rustc sizes it. A member no rustc type can stand for (a string, a float, a Vale
  // reference) has no honest size here, and a wrong size is a silent memory error — so fail loud.
  let mut member_tys = Vec::with_capacity(def.members.len());
  for member in def.members.iter() {
    match kind_to_rustc_ty(tcx, state.rust_crates, &member.tyype) {
      Some(member_ty) => member_tys.push(member_ty),
      None => panic!(
        "rust interop: Vale struct {struct_id:?} crosses to Rust by value ({ty:?}) but its member \
         {:?} of type {:?} has no rustc type to size it by",
        member.name, member.tyype
      ),
    }
  }
  // One layout field per source field of `__ValeOpaque` itself — its marker fields — so rustc's
  // debuginfo walker, which visits `layout.field(i)` for each source field, always finds one.
  let ty::TyKind::Adt(opaque_adt, _) = ty.kind() else {
    unreachable!("read_opaque_typeid accepted a non-ADT: {ty:?}")
  };
  let marker_field_count = opaque_adt.non_enum_variant().fields.len();
  Ok(TyAndLayout { ty, layout: tcx.mk_layout(c_layout_over(tcx, &member_tys, marker_field_count)?) })
}

/// The C struct layout of `member_tys` in declaration order — each member at the next offset aligned
/// to its own alignment, the whole padded to the largest alignment — which is also LLVM's layout of a
/// non-packed struct of those members, i.e. what the backend emits. Reported over `__ValeOpaque`'s own
/// `marker_field_count` zero-sized fields: the first at offset 0, where the payload starts, and every
/// other one at the end, past the payload.
fn c_layout_over<'tcx>(
  tcx: TyCtxt<'tcx>,
  member_tys: &[Ty<'tcx>],
  marker_field_count: usize,
) -> Result<LayoutData<FieldIdx, VariantIdx>, &'tcx LayoutError<'tcx>> {
  let mut offset = 0u64;
  let mut max_align = 1u64;
  for member_ty in member_tys {
    let member_layout = tcx.layout_of(PseudoCanonicalInput {
      value: *member_ty,
      typing_env: ty::TypingEnv::fully_monomorphized(),
    })?;
    let member_align = member_layout.align.abi.bytes();
    max_align = max_align.max(member_align);
    offset = align_up(offset, member_align) + member_layout.size.bytes();
  }
  let total_size = align_up(offset, max_align);
  let align = Align::from_bytes(max_align).expect("a member alignment is a power of two");
  Ok(LayoutData {
    fields: FieldsShape::Arbitrary {
      offsets: IndexVec::from_iter(
        (0..marker_field_count).map(|i| if i == 0 { Size::ZERO } else { Size::from_bytes(total_size) }),
      ),
      in_memory_order: IndexVec::from_iter((0..marker_field_count).map(FieldIdx::from_usize)),
    },
    variants: Variants::Single { index: VariantIdx::from_u32(0) },
    backend_repr: BackendRepr::Memory { sized: true },
    largest_niche: None,
    uninhabited: false,
    align: AbiAlign::new(align),
    size: Size::from_bytes(total_size),
    max_repr_align: None,
    unadjusted_abi_align: align,
    randomization_seed: Hash64::ZERO,
  })
}

fn align_up(offset: u64, align: u64) -> u64 {
  (offset + align - 1) & !(align - 1)
}
