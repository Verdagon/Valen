use crate::typing::ast::ast::LocT;
use crate::typing::ast::borrowing_ast::{FunctionAliasingInfoT, GroupIdT};
use crate::typing::compiler::Compiler;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub(crate) fn copy_aliasing_info_to_typing_arena(
    &self,
    info: &FunctionAliasingInfoT<'s, '_>,
  ) -> &'t FunctionAliasingInfoT<'s, 't> {
    let group_paths: Vec<GroupIdT<'s, 't>> = info
      .group_paths
      .iter()
      .map(|g| GroupIdT { steps: self.typing_interner.alloc_slice_copy(g.steps) })
      .collect();
    let instr: Vec<(LocT<'t>, &'t [u32])> = info
      .instruction_loc_to_accessed_groups
      .iter()
      .map(|(loc, set)| {
        (
          LocT { path: self.typing_interner.alloc_slice_copy(loc.path) },
          self.typing_interner.alloc_slice_copy(set),
        )
      })
      .collect();
    self.typing_interner.alloc(FunctionAliasingInfoT {
      param_index_to_noalias: self.typing_interner.alloc_slice_copy(info.param_index_to_noalias),
      group_paths: self.typing_interner.alloc_slice_from_vec(group_paths),
      instruction_loc_to_accessed_groups: self.typing_interner.alloc_slice_from_vec(instr),
    })
  }
}
