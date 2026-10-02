use bumpalo::Bump;
use crate::postparsing::ast::FunctionS;
use crate::typing::ast::ast::FunctionDefinitionT;
use crate::typing::ast::borrowing_ast::FunctionAliasingInfoT;
use crate::typing::borrow_checker::group_expr::{GroupExprG, GroupPathG};
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::compiler::Compiler;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn calculate_aliasing_info<'g>(
    &self,
    _function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    params_gt: &Vec<KindGT<'s, 't, 'g>>,
    check_arena: &'g Bump,
  ) -> &'g FunctionAliasingInfoT<'s, 'g>
  where
    's: 'g,
    't: 'g,
  {
    let param_index_to_maybe_borrowing_group =
        params_gt.iter()
            .map(|kind_gt| {
              match kind_gt {
                KindGT::BorrowRef(b) => Some(b.group.group),
                _ => None
              }
            })
            .collect::<Vec<_>>();
    let param_index_to_noalias =
        param_index_to_maybe_borrowing_group
            .iter()
            .enumerate()
            .map(|(param_index, maybe_borrowing_group)| {
              match maybe_borrowing_group {
                None => false,
                Some(borrowing_group) => {
                  let mut mentioned_in_other_param = false;
                  for other_param_index in 0..param_index_to_maybe_borrowing_group.len() {
                    if param_index != other_param_index {
                      if let Some(other_borrowing_group) =
                          param_index_to_maybe_borrowing_group[other_param_index] {
                        if groups_alias(borrowing_group, other_borrowing_group) {
                          mentioned_in_other_param = true;
                          break;
                        }
                      }
                    }
                  }
                  let noalias = !mentioned_in_other_param;
                  noalias
                }
              }
            })
            .collect::<Vec<_>>();
    check_arena.alloc(FunctionAliasingInfoT {
      param_index_to_noalias: check_arena.alloc_slice_copy(param_index_to_noalias.as_slice()),
      group_paths: &[],
      instruction_loc_to_accessed_groups: &[],
    })
  }
}


fn groups_alias<'s, 't, 'g>(a: GroupExprG<'s, 't, 'g>, b: GroupExprG<'s, 't, 'g>) -> bool {
  for a_path in a {
    for b_path in b {
      if paths_overlap(a_path, b_path) {
        return true;
      }
    }
  }
  return false;
}

fn paths_overlap<'s, 't, 'g>(a: &GroupPathG<'s, 't, 'g>, b: &GroupPathG<'s, 't, 'g>) -> bool {
  if a.root != b.root {
    return false;
  }
  // If we get here, they're the same roots. See if one path overlaps the other.
  let min_length = a.steps.len().min(b.steps.len());
  if a.steps[..min_length] == b.steps[..min_length] {
    // Their min_length first steps are the same, which means they're either the same path,
    // or they overlap each other.
    return true;
  } else {
    return false;
  }
}
