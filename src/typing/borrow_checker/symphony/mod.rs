use bumpalo::Bump;
use crate::parsing::ast::ast::IMacroInclusionP;
use crate::postparsing::ast::{FunctionS, IFunctionAttributeS, MacroCallS};
use crate::postparsing::rules::types::ITypeST;
use crate::StrI;
use crate::typing::ast::ast::FunctionDefinitionT;
use crate::typing::ast::borrowing_ast::FunctionAliasingInfoT;
use crate::typing::borrow_checker::ast_g::ExpressionGE;
use crate::typing::borrow_checker::kind_g::KindGT;
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
  {
    let opted_out =
        function_s.attributes.iter().any(|attr| matches!(attr,
          IFunctionAttributeS::MacroCall(MacroCallS { include: IMacroInclusionP::DontCallMacro, macro_name, .. })
            if *macro_name == self.keywords.borrow_check));
    if !opted_out {
      let (body_g, _access_log) =
          self.groupify_function(coutputs, function_s, function_t, bump_g)?;
      self.check_usages(coutputs, function_s, bump_g, body_g)?;
    }
    Ok(self.calculate_aliasing_info(function_s, function_t, bump_g))
  }
}
