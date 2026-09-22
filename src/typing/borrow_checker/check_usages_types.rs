use indexmap::IndexMap;
use crate::typing::ast::ast::LocT;
use crate::typing::borrow_checker::ast_g::GroupStep;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefKey<'s, 't> {
  Named(IVarNameT<'s, 't>),
  Held(u32),
}

// A subtree for a group as the containing function knows it.
// This grows over time as the function learns about new groups.
#[derive(Debug, Clone)] // Has clone because of if-statements
pub struct GroupSubtree<'s, 't> {
  // The last mutation effect to hit this group, which invalidates all child groups.
  pub last_mut_effect: Option<MutEffectLoc<'s, 't>>,

  pub name_to_child: IndexMap<GroupStep<'s, 't>, GroupSubtree<'s, 't>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MutEffectLoc<'s, 't> {
  pub loct: LocT<'t>,
  pub range: RangeS<'s>
}
