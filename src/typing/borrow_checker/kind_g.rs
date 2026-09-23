use std::marker::PhantomData;
use crate::postparsing::names::IImpreciseNameS;
use crate::typing::borrow_checker::templata_g::{GroupTemplataG, ITemplataG, KindTemplataG};
use crate::typing::borrow_checker::group_expr::GroupExprG;
use crate::typing::env::environment::*;
use crate::typing::names::names::*;
use crate::typing::templata::templata::ITemplataT;
use crate::typing::types::types::{InterfaceTT, KindPlaceholderT, KindT};
use crate::typing::typing_interner::MustIntern;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BorrowRefGT<'s, 't, 'g> {
  pub inner: KindGT<'s, 't, 'g>,
  pub group: GroupTemplataG<'s, 't, 'g>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SharednessG {
  Single,
  Shared,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum RegionG {
  Iso,
  // TODO: Get rid of this when we have an actual default region
  Default,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OwnRefGT<'s, 't, 'g> {
  pub inner: KindGT<'s, 't, 'g>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ShareRefGT<'s, 't, 'g> {
  pub inner: KindGT<'s, 't, 'g>,
  pub group: GroupTemplataG<'s, 't, 'g>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct WeakRefGT<'s, 't, 'g> {
  pub inner: KindGT<'s, 't, 'g>,
}

// KindGT is inline-owned (not arena-interned). Concrete non-primitive payloads
// (StructGT, InterfaceGT, etc.) are arena-interned and held as &'t refs here.
// Primitives inline by value; compound types use &'t to keep the enum small (see @WVSBIZ).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum KindGT<'s, 't, 'g> {
  Never(NeverGT),
  Void(VoidGT),
  Int(IntGT),
  Bool(BoolGT),
  Str(StrGT),
  Float(FloatGT),
  USize(USizeGT),
  Struct(&'g StructGT<'s, 't, 'g>),
  Interface(&'g InterfaceGT<'s, 't, 'g>),
  StaticSizedArray(&'g StaticSizedArrayGT<'s, 't, 'g>),
  RuntimeSizedArray(&'g RuntimeSizedArrayGT<'s, 't, 'g>),
  KindPlaceholder(&'g KindPlaceholderGT<'s, 't, 'g>),
  OverloadSet(&'g OverloadSetGT<'s, 't, 'g>),
  BorrowRef(&'g BorrowRefGT<'s, 't, 'g>),
  OwnRef(&'g OwnRefGT<'s, 't, 'g>),
  ShareRef(&'g ShareRefGT<'s, 't, 'g>),
  WeakRef(&'g WeakRefGT<'s, 't, 'g>),
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct NeverGT {
  pub from_break: bool,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct VoidGT;

impl IntGT {
  pub const I32: IntGT = IntGT { bits: 32 };
  pub const I64: IntGT = IntGT { bits: 64 };
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct IntGT {
  pub bits: i32,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoolGT;


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StrGT;


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FloatGT;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct USizeGT;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticSizedArrayGT<'s, 't, 'g> {
  pub name: IdT<'s, 't>,
  pub element_type: KindGT<'s, 't, 'g>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuntimeSizedArrayGT<'s, 't, 'g> {
  pub name: IdT<'s, 't>,
  pub element_type: KindGT<'s, 't, 'g>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StructGT<'s, 't, 'g> {
  pub id: &'t IdT<'s, 't>,
  pub template_args: &'g [ITemplataG<'s, 't, 'g>],
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct InterfaceGT<'s, 't, 'g> {
  pub id: &'t IdT<'s, 't>,
  pub template_args: &'g [ITemplataG<'s, 't, 'g>],
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OverloadSetGT<'s, 't, 'g> {
  // pub env: IInDenizenEnvironmentT<'s, 't>,
  // pub name: &'s IImpreciseNameS<'s>,
  pub env_id: IdT<'s, 't>,
  pub _phantom: PhantomData<&'g ()>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindPlaceholderGT<'s, 't, 'g> {
  pub id: IdT<'s, 't>,
  pub _phantom: PhantomData<&'g ()>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ISuperKindGT<'s, 't, 'g> {
  Interface(&'g InterfaceGT<'s, 't, 'g>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ISubKindGT<'s, 't, 'g> {
  Struct(&'g StructGT<'s, 't, 'g>),
  Interface(&'g InterfaceGT<'s, 't, 'g>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
}

pub fn expect_borrowref_gt<'s, 't, 'g>(kind: KindGT<'s, 't, 'g>) -> &'g BorrowRefGT<'s, 't, 'g> {
  match kind {
    KindGT::BorrowRef(bgt) => bgt,
    other => panic!("vfail: {:?}", other),
  }
}
