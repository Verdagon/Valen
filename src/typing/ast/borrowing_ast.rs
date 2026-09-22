
use crate::interner::StrI;
use crate::typing::ast::ast::LocT;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GroupIdT<'s, 'x> {
  pub steps: &'x [GroupIdStepT<'s>],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupIdStepT<'s> {
  Rune(StrI<'s>),
  ParamAnonymousGroup(StrI<'s>),
  Local(StrI<'s>),
  Member(StrI<'s>),
  Elements,
}

impl<'s, 'x> GroupIdT<'s, 'x> {
  pub fn name(&self) -> String {
    let mut s = String::new();
    for step in self.steps {
      match step {
        GroupIdStepT::Rune(n) | GroupIdStepT::ParamAnonymousGroup(n) | GroupIdStepT::Local(n) => {
          s.push_str(n.0)
        }
        GroupIdStepT::Member(n) => {
          s.push('.');
          s.push_str(n.0)
        }
        GroupIdStepT::Elements => s.push_str("[]"),
      }
    }
    s
  }
}

#[derive(Clone, Copy, Debug)]
pub struct FunctionAliasingInfoT<'s, 'x> {
  pub param_index_to_noalias: &'x [bool],
  pub group_paths: &'x [GroupIdT<'s, 'x>],
  pub instruction_loc_to_accessed_groups: &'x [(LocT<'x>, &'x [u32])],
}
