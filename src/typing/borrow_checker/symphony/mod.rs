use bumpalo::Bump;
use crate::parsing::ast::ast::IMacroInclusionP;
use crate::postparsing::ast::{FunctionS, IFunctionAttributeS, MacroCallS};
use crate::postparsing::rules::types::ITypeST;
use crate::StrI;
use crate::typing::ast::ast::FunctionDefinitionT;
use crate::typing::ast::borrowing_ast::FunctionAliasingInfoT;
use crate::typing::borrow_checker::ast_g::ExpressionGE;
use crate::typing::borrow_checker::group_expr::GroupPathG;
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::borrow_checker::symphony::groupify_function::GroupifyResults;
use crate::typing::borrow_checker::templata_g::ITemplataG;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::templata::templata::ITemplataT;
use crate::typing::types::types::KindT;

mod check_usages;
mod groupify_function;
mod calculate_aliasing_info;
pub mod errors;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn check_function<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
  ) -> Result<&'g FunctionAliasingInfoT<'s, 'g>, ICompileErrorT<'s, 't>>
  where
    's: 'g,
    't: 'g,
  {
    let opted_out =
        function_s.attributes.iter().any(|attr| matches!(attr,
          IFunctionAttributeS::MacroCall(MacroCallS { include: IMacroInclusionP::DontCallMacro, macro_name, .. })
            if *macro_name == self.keywords.borrow_check));
    if opted_out {
      let param_index_to_noalias =
          bump_g.alloc_slice_fill_copy(function_t.header.params.len(), false);
      Ok(bump_g.alloc(FunctionAliasingInfoT {
        param_index_to_noalias,
        group_paths: &[],
        instruction_loc_to_accessed_groups: &[],
      }))
    } else {
      let GroupifyResults { params_gt, body_g, access_log, func_declared_mut_effects } =
          self.groupify_function(coutputs, function_s, function_t, bump_g)?;
      self.check_usages(coutputs, function_s, bump_g, body_g, func_declared_mut_effects)?;
      Ok(self.calculate_aliasing_info(function_s, function_t, &params_gt, bump_g))
    }
  }
}
