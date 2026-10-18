use indexmap::IndexMap;
use crate::typing::ast::ast::LocT;
use crate::typing::borrow_checker::ast_g::GroupStep;
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::borrow_checker::templata_g::GroupTemplataG;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefKey<'s, 't> {
  Named(IVarNameT<'s, 't>),
  Held(u32),
}

#[derive(Clone, Debug)]
pub struct PendingUseList<'s, 't, 'g> {
  pub map: IndexMap<RefKey<'s, 't>, PendingUse<'s, 't, 'g>>,
}

// As we're going backwards, and we encounter a use (like `observe(r)`), we'll upsert this into the
// PendingUseList.
#[derive(Clone, Debug)]
pub struct PendingUse<'s, 't, 'g> {
  pub earliest_use_range: RangeS<'s>,
  // This will often be a BorrowRefGT, we use the contained group for check_usages.
  // But it will also often be a StructGT/InterfaceGT, and their group parameters also matter.
  pub kind: KindGT<'s, 't, 'g>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MutEffectLoc<'s, 't> {
  pub loct: LocT<'t>,
  pub range: RangeS<'s>
}
