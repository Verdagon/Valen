
use crate::interner::StrI;
use crate::typing::ast::ast::LocT;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GroupIdT<'s, 'x> {
  pub steps: &'x [GroupIdStepT<'s>],
}

/// One step of a group path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupIdStepT<'s> {
  /// A named group rune, e.g. `g` in `func f<g'>(a &Ship in g)`.
  Rune(StrI<'s>),
  /// A borrow parameter's anonymous group, named by the parameter.
  ParamAnonymousGroup(StrI<'s>),
  /// A local reference's group, named by the local.
  Local(StrI<'s>),
  /// A member step into a group, e.g. `.tiles`.
  Member(StrI<'s>),
  /// The elements-group step, e.g. `[]`.
  Elements,
}

impl<'s, 'x> GroupIdT<'s, 'x> {
  /// The group's user-visible name, e.g. `g` or `g.tiles[]`.
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

/// All the metadata the backend will need to do its aliasing optimizations.
#[derive(Clone, Copy, Debug)]
pub struct FunctionAliasingInfoT<'s, 'x> {
  pub param_index_to_noalias: &'x [bool],
  /// Every distinct group by its full path, in scope-id order.
  pub group_paths: &'x [GroupIdT<'s, 'x>],
  /// Each instruction's `LocT`, to the set of group indices it accesses.
  pub instruction_loc_to_accessed_groups: &'x [(LocT<'x>, &'x [u32])],
}
