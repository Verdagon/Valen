use crate::interner::StrI;
use crate::typing::borrow_checker::check_usages_types::RefKey;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

#[derive(Debug)]
pub enum BorrowErrorKind<'s, 't> {
  AliasingIntoDisjointMutGroups {
    local: IVarNameT<'s, 't>,
    arg_a: usize,
    arg_b: usize,
    group_a: StrI<'s>,
    group_b: StrI<'s>,
  },
  BorrowIntoMovedArgument {
    local: IVarNameT<'s, 't>,
    borrow_arg: usize,
    move_arg: usize,
  },
  UseAfterChurn {
    local: RefKey<'s, 't>,
    churned_at: RangeS<'s>,
  },
  UseAfterChurnTemporary {
    churned_at: RangeS<'s>,
  },
  GrouplessReturnBorrow,
  UnderivableBorrowGroup,
  UndeclaredChurn,
}

impl<'s, 't> BorrowErrorKind<'s, 't> {
  pub fn humanize(&self) -> String {
    match self {
      BorrowErrorKind::AliasingIntoDisjointMutGroups { local, arg_a, arg_b, group_a, group_b } => {
        unimplemented!()
      }
      BorrowErrorKind::BorrowIntoMovedArgument { local, borrow_arg, move_arg } => {
        unimplemented!()
      }
      BorrowErrorKind::UseAfterChurn { local, .. } => {
        unimplemented!()
      }
      BorrowErrorKind::UseAfterChurnTemporary { .. } => {
        unimplemented!()
      }
      BorrowErrorKind::GrouplessReturnBorrow => {
        unimplemented!()
      }
      BorrowErrorKind::UnderivableBorrowGroup => {
        unimplemented!()
      }
      BorrowErrorKind::UndeclaredChurn => {
        unimplemented!()
      }
    }
  }
}

fn var_name<'s, 't>(name: &IVarNameT<'s, 't>) -> &'s str {
  match name {
    IVarNameT::Member(code_var) => code_var.imprecise_name.name.0,
    IVarNameT::Local(code_var) => code_var.imprecise_name.name.0,
    _ => "a local",
  }
}
