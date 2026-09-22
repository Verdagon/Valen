use crate::typing::borrow_checker::borrow_error::BorrowErrorKind;
use crate::typing::borrow_checker::check_usages_types::RefKey;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

pub fn humanize_borrow_error<'s, 't>(
  _range: RangeS<'s>,
  kind: &BorrowErrorKind<'s, 't>,
) -> String {
  match kind {
    BorrowErrorKind::AliasingIntoDisjointMutGroups { local, arg_a, arg_b, group_a, group_b } => {
      unimplemented!()
    }
    BorrowErrorKind::BorrowIntoMovedArgument { local, borrow_arg, move_arg } => {
      unimplemented!()
    }
    // The `At <pos>:` header already points the caret at the argument's own source location,
    // so the message doesn't name a variable.
    BorrowErrorKind::UseAfterChurn { .. } | BorrowErrorKind::UseAfterChurnTemporary { .. } => {
      format!(
        "Used a borrow after invalidated."
      )
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

fn var_name<'s, 't>(name: &IVarNameT<'s, 't>) -> &'s str {
  match name {
    IVarNameT::Member(code_var) => code_var.imprecise_name.name.0,
    IVarNameT::Local(code_var) => code_var.imprecise_name.name.0,
    _ => "a local",
  }
}
