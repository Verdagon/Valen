use bumpalo::Bump;
use crate::postparsing::ast::FunctionS;
use crate::typing::ast::ast::FunctionDefinitionT;
use crate::typing::ast::borrowing_ast::FunctionAliasingInfoT;
use crate::typing::compiler::Compiler;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn calculate_aliasing_info<'g>(
    &self,
    _function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    check_arena: &'g Bump,
  ) -> &'g FunctionAliasingInfoT<'s, 'g>
  where
    's: 'g,
  {
    let param_index_to_noalias =
        check_arena.alloc_slice_fill_copy(function_t.header.params.len(), false);
    // TODO
    check_arena.alloc(FunctionAliasingInfoT {
      param_index_to_noalias,
      group_paths: &[],
      instruction_loc_to_accessed_groups: &[],
    })
  }
}