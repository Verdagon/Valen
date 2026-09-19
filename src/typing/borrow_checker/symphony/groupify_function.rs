use std::marker::PhantomData;
use bumpalo::Bump;
use indexmap::IndexMap;
use crate::postparsing::ast::{FunctionS, GenericParameterS, IBodyS, ICitizenDenizenS, ICitizenS, IGenericParameterTypeS, IStructMemberS, KindGenericParameterTypeS, StructS};
use crate::postparsing::names::{CodeNameS, IImpreciseNameS, IRuneS, IVarDeclarationNameS};
use crate::postparsing::rules::RuneUsage;
use crate::postparsing::rules::types::*;
use crate::StrI;
use crate::typing::ast::ast::FunctionDefinitionT;
use crate::typing::ast::citizens::{CitizenDefinitionT, StructDefinitionT};
use crate::typing::ast::expressions::*;
use crate::typing::borrow_checker::access_event::AccessEventG;
use crate::typing::borrow_checker::ast_g::*;
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::*;
use crate::typing::borrow_checker::templata_g::*;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::env::function_environment_t::LocalVariable;
use crate::typing::names::names::*;
use crate::typing::templata::templata::*;
use crate::typing::types::types::*;

struct WrittenContext<'s> {
  type_s: ITypeST<'s>,
  name: Option<IVarDeclarationNameS<'s>>,
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn groupify_function<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
  ) -> Result<(ExpressionGE<'s, 't, 'g>, Vec<&'g AccessEventG<'s, 't>>), ICompileErrorT<'s, 't>> {
    let mut access_log = Vec::new();

    // Insert PlaceholderTemplataG's for any non-group generic parameters, like the `T` in:
    //     func observe<T, tg'>(x &T in tg) { }
    let mut local_rune_to_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> = IndexMap::new();
    let func_local_name = IFunctionNameT::try_from(function_t.header.id.local_name).expect("Function doesn't have a function name?");
    for (generic_param_s, templata_t) in function_s.generic_params.iter().zip(func_local_name.template_args().iter()) {
      match templata_t {
        ITemplataT::Placeholder(PlaceholderTemplataT { id: placeholder_id_t, tyype: placeholder_templata_type_t }) => {
          local_rune_to_templata.insert(
            generic_param_s.rune.rune,
            ITemplataG::Placeholder(
              bump_g.alloc(PlaceholderTemplataG {
                id: *placeholder_id_t,
                tyype: *placeholder_templata_type_t
              })));
        }
        ITemplataT::Kind(KindTemplataT { kind: KindT::KindPlaceholder(KindPlaceholderT { id }) }) => {
          local_rune_to_templata.insert(
            generic_param_s.rune.rune,
            ITemplataG::Kind(KindTemplataG {
              kind: KindGT::KindPlaceholder(
                bump_g.alloc(KindPlaceholderGT {
                  id: *id,
                  _phantom: PhantomData
                })
              )
            }));
        }
        ITemplataT::Group(GroupTemplataT {}) => {} // Skip, we'll handle them below.
        _ => panic!("Expected placeholder, unexpected templata: {:?}", templata_t),
      };
    }

    // Now that we have PlaceholderTemplataG's for every non-group generic parameter,
    // let's populate our map with GroupTemplataG's for every group parameter, like the `tg` in:
    //     func observe<T, tg'>(x &T in tg) { }
    //
    // Let's do a simple pass over all of the function parameters to try and deduce their types.
    // For example, in:
    //     func observe<T, tg'>(x &T in tg) { }
    // we already have T = PlaceholderTemplataG("T"),
    // and we now want to deduce that "tg is a group of T".
    // So we do a recurse over each parameter (`&T in tg`) looking for any borrow refs
    // whose group is just a rune (like this one, `in tg`) so we can assign their value (`T` because
    // `&T in tg`) as the group's type and register it into our map.
    for (param_st, param_tt) in function_s.params.iter().zip(function_t.header.params.iter()) {
      let param_gt =
          self.simple_match_group_rune_types(
            coutputs, bump_g, &mut local_rune_to_templata, param_st.tyype, ITemplataT::Kind(KindTemplataT { kind: param_tt.tyype }));
    }

    // Will be populated as we go.
    let mut local_to_type_g = IndexMap::new();

    // TODO: use function_s to compare to any expression_t that we find, that will let us manually
    // specify things' groups, for example in let statements.
    // For now, just use the typed expressions.
    let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, &mut access_log, function_t.body, &local_rune_to_templata, &mut local_to_type_g)?;

    Ok((expr_ge, access_log))
  }

  fn new_rune_group_expr<'g>(bump_g: &'g Bump, rune: IRuneS<'s>) -> &'g [GroupPathG<'s, 't, 'g>] {
    bump_g.alloc_slice_copy(&[
      *bump_g.alloc(GroupPathG {
        root: GroupRootG::Rune(rune),
        steps: &[],
        ellipsis: false
      })
    ])
  }

  fn new_local_group_expr<'g>(bump_g: &'g Bump, var_name: IVarNameT<'s, 't>) -> &'g [GroupPathG<'s, 't, 'g>] {
    bump_g.alloc_slice_copy(&[
      *bump_g.alloc(GroupPathG {
        root: GroupRootG::Local(var_name),
        steps: &[],
        ellipsis: false
      })
    ])
  }

  fn groupify_expression<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
    access_log: &mut Vec<&'g AccessEventG<'s, 't>>,
    expression_te: ExpressionTE<'s, 't>,
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &mut IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
  ) -> Result<ExpressionGE<'s, 't, 'g>, ICompileErrorT<'s, 't>> {
    match expression_te {
      ExpressionTE::LetNormal(LetNormalTE { range, variable, expr, result, .. }) => {
        let result_gt = KindGT::Void(VoidGT { });
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        let variable_g =
          bump_g.alloc(LocalVariableG {
            name: variable.name,
            tyype: expr_ge.result()
          });
        local_to_type_g.insert(variable.name,expr_ge.result());
        // let variable_g = self.groupify_var(bump_g, *variable, local_rune_to_templata, local_to_type_g);
        Ok(ExpressionGE::LetNormal(bump_g.alloc(LetNormalGE { range: *range, variable: variable_g, expr: expr_ge, result: result_gt, })))
      }
      ExpressionTE::LocalLookup(LocalLookupTE { range, local_variable, result, .. }) => {
        let local_gt = local_to_type_g.get(&local_variable.name).expect("Couldn't find local variable");
        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: local_variable.name,
              tyype: *local_gt
            });
        let group_expr = Self::new_local_group_expr(bump_g, local_variable.name);
        let group_templata_g =
            GroupTemplataG {
              group: group_expr,
              kind: *local_gt,
            };
        Ok(
          ExpressionGE::LocalLookup(
            bump_g.alloc(LocalLookupGE {
              range: *range,
              local_variable: variable_g,
              result: bump_g.alloc(BorrowRefGT { inner: *local_gt, group: group_templata_g })
            })))
      }
      ExpressionTE::LetAndLend(LetAndLendTE { range, variable, expr, result, .. }) => {
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        let local_gt = expr_ge.result();

        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: variable.name,
              tyype: local_gt
            });
        local_to_type_g.insert(variable.name,expr_ge.result());

        let group_templata_g =
            GroupTemplataG {
              group: Self::new_local_group_expr(bump_g, variable.name),
              kind: local_gt,
            };
        Ok(
          ExpressionGE::LetAndLend(
            bump_g.alloc(LetAndLendGE {
              range: *range,
              variable: variable_g,
              expr: expr_ge,
              result: bump_g.alloc(BorrowRefGT { inner: local_gt, group: group_templata_g })
            })))
      }
      ExpressionTE::Unlet(UnletTE { range, variable: local_variable, result, .. }) => {
        let local_gt = local_to_type_g.get(&local_variable.name).expect("Couldn't find local variable");
        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: local_variable.name,
              tyype: *local_gt
            });
        Ok(
          ExpressionGE::Unlet(
            bump_g.alloc(UnletGE {
              range: *range,
              variable: variable_g,
              result: *local_gt,
            })))
      }
      ExpressionTE::Discard(DiscardTE { range, expr, result, .. }) => {
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        Ok(
          ExpressionGE::Discard(
            bump_g.alloc(DiscardGE {
              range: *range,
              expr: expr_ge,
              result: KindGT::Void(VoidGT { }),
            })))
      }
      ExpressionTE::Return(ReturnTE { range, source_expr, result, .. }) => {
        let result_gt = KindGT::Never(NeverGT { from_break: false });
        let source_expr_g = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Return(bump_g.alloc(ReturnGE { range: *range, source_expr: source_expr_g, result: source_expr_g.result(), })))
      }
      ExpressionTE::Block(BlockTE { range, inner, result, .. }) => {
        let inner_g = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *inner, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Block(bump_g.alloc(BlockGE { range: *range, inner: inner_g, result: inner_g.result(), })))
      }
      ExpressionTE::Consecutor(ConsecutorTE { range, exprs: exprs_te, result: result_tt, .. }) => {
        let mut exprs_ge = Vec::new();
        for expr_te in exprs_te.iter() {
          exprs_ge.push(
            self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr_te, local_rune_to_templata, local_to_type_g)?);
        }
        let exprs_ge_slice = bump_g.alloc_slice_copy(exprs_ge.as_slice());
        let result_gt = exprs_ge_slice.last().expect("Expected nonempty Consecutor").result();
        Ok(ExpressionGE::Consecutor(bump_g.alloc(ConsecutorGE { range: *range, exprs: exprs_ge_slice, result: result_gt, })))
      }
      ExpressionTE::ConstantInt(ConstantIntTE { range, value, bits, .. }) => {
        let value_gt = self.groupify_templata(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *value);
        let result_gt = KindGT::Int(IntGT { bits: *bits });
        Ok(ExpressionGE::ConstantInt(bump_g.alloc(ConstantIntGE { range: *range, value: value_gt, bits: *bits, result: result_gt, })))
      }
      ExpressionTE::ConstantBool(ConstantBoolTE { range, value, .. }) => {
        let result_gt = KindGT::Bool(BoolGT { });
        Ok(ExpressionGE::ConstantBool(bump_g.alloc(ConstantBoolGE { range: *range, value: *value, result: result_gt, })))
      }
      ExpressionTE::ConstantFloat(ConstantFloatTE { range, value, .. }) => {
        let result_gt = KindGT::Float(FloatGT { });
        Ok(ExpressionGE::ConstantFloat(bump_g.alloc(ConstantFloatGE { range: *range, value: *value, result: result_gt, })))
      }
      ExpressionTE::ArgLookup(ArgLookupTE { range, param_index, result, .. }) => {
        let param_type_t = function_t.header.params[*param_index as usize].tyype;
        let param_type_s = function_s.params[*param_index as usize].tyype;
        let param_name = function_s.params[*param_index as usize].name;
        let written_context = WrittenContext {
          type_s: param_type_s,
          name: Some(param_name),
        };
        let result_gt = self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, param_type_t, Some(&written_context));
        Ok(ExpressionGE::ArgLookup(bump_g.alloc(ArgLookupGE { range: *range, param_index: *param_index, result: result_gt, })))
      }
      ExpressionTE::ArrayLength(ArrayLengthTE { range, array_expr, result, .. }) => {
        let array_expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *array_expr, local_rune_to_templata, local_to_type_g)?;
        let result_gt = KindGT::Int(IntGT { bits: 32 });
        Ok(ExpressionGE::ArrayLength(bump_g.alloc(ArrayLengthGE { range: *range, array_expr: array_expr_ge, result: result_gt, })))
      }
      ExpressionTE::Deref(DerefTE { range, loct, inner: source_te, result: result_tt, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        let source_borrow_gt = expect_borrowref_gt(source_ge.result());
        let result_gt = source_borrow_gt.inner;
        Ok(ExpressionGE::Deref(bump_g.alloc(DerefGE { range: *range, loct: *loct, inner: source_ge, result: result_gt, })))
      }
      ExpressionTE::FunctionCall(FunctionCallTE { loct, range, callable, args: arg_exprs_te, result, .. }) => {
        let mut arg_exprs_ge: Vec<ExpressionGE<'s, 't, 'g>> = Vec::new();
        for expr_te in arg_exprs_te.iter() {
          arg_exprs_ge.push(
            self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr_te, local_rune_to_templata, local_to_type_g)?);
        }
        let arg_exprs_ge_slice: &'g [ExpressionGE<'s, 't, 'g>] =
            bump_g.alloc_slice_copy(arg_exprs_ge.as_slice());

        // let callable_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *callable)?;

        let callee_template_id = Compiler::get_template(self.typing_interner, callable.id);
        let callee_func_s =
            coutputs.peek_postparsed_function(callee_template_id)
                .expect("Callee not present");
        let callee_rune_to_caller_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> =
            self.calculate_callee_rune_to_caller_templata(coutputs, bump_g, callee_func_s, &arg_exprs_ge);
        let callee_return_type_written_context =
            WrittenContext {
              type_s: callee_func_s.maybe_return_type.unwrap_or_else(|| panic!("Callee doesn't have return {:?} {:?}", callee_template_id, callee_func_s.name)),
              name: None
            };
        let result_gt =
            self.groupify_type(
              coutputs,
              bump_g,
              &callee_rune_to_caller_templata,
              local_to_type_g,
              *result,
              Some(&callee_return_type_written_context));
        let mut stuff: Vec<&'g MutEffectPath> = Vec::new();
        for callee_effect_s in callee_func_s.effects {
          stuff.push(
            bump_g.alloc(
              MutEffectPath {
                effecting_node_loc: *loct,
                steps: self.groupify_effect(bump_g, callee_effect_s, &callee_rune_to_caller_templata),
              }
            )
          );
        }
        let mut_effects_slice = bump_g.alloc_slice_copy(stuff.as_slice());

        Ok(ExpressionGE::FunctionCall(bump_g.alloc(FunctionCallGE {
          loct: *loct,
          range: *range,
          callable: callable,
          args: arg_exprs_ge_slice,
          result: result_gt,
          mut_effects: mut_effects_slice
        })))
      }
      ExpressionTE::Destroy(DestroyTE { range, expr: source_te, struct_tt, destination_reference_variables, result, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        let source_struct_gt =
          match source_ge.result() {
            KindGT::Struct(s) => s,
            _ => panic!("Expected struct for Destroy"),
          };

        let member_name_to_locally_phrased_member_type =
            self.translate_struct_members(coutputs, bump_g, *source_struct_gt);
        assert!(member_name_to_locally_phrased_member_type.len() == destination_reference_variables.len());
        let mut dest_vars_g = Vec::new();
        for ((_, local_type_g), dest_var) in member_name_to_locally_phrased_member_type.iter().zip(destination_reference_variables.iter()) {
          let LocalVariable { name: var_name, tyype: _ } = dest_var;
          let variable_g =
              bump_g.alloc(LocalVariableG {
                name: *var_name,
                tyype: *local_type_g
              });
          dest_vars_g.push(&*variable_g);
          local_to_type_g.insert(*var_name, *local_type_g);
        }
        let dest_vars_g_slice = bump_g.alloc_slice_copy(dest_vars_g.as_slice());

        Ok(ExpressionGE::Destroy(bump_g.alloc(DestroyGE {
          range: *range,
          expr: source_ge,
          struct_tt: source_struct_gt,
          destination_reference_variables: dest_vars_g_slice,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::VoidLiteral(VoidLiteralTE { range, result, .. }) => {
        Ok(ExpressionGE::VoidLiteral(bump_g.alloc(VoidLiteralGE {
          range: *range,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::LockWeak(LockWeakTE { .. }) => unimplemented!(),
      ExpressionTE::BorrowToWeak(BorrowToWeakTE { .. }) => unimplemented!(),
      ExpressionTE::If(IfTE { range, loct, condition, then_call, else_call, result, .. }) => {
        let condition_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *condition, local_rune_to_templata, local_to_type_g)?;
        let then_call_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *then_call, local_rune_to_templata, local_to_type_g)?;
        let else_call_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *else_call, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::If(bump_g.alloc(IfGE {
          range: *range,
          loct: *loct,
          condition: condition_ge,
          then_call: then_call_ge,
          else_call: else_call_ge,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::While(WhileTE { range, loct, block: block_te, result, .. }) => {
        let BlockTE { range: block_range, inner: block_inner, .. } = block_te;
        let block_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *block_inner, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::While(bump_g.alloc(WhileGE {
          range: *range,
          loct: *loct,
          block: BlockGE { range: *block_range, inner: block_ge, result: block_ge.result() },
          result: KindGT::Void(VoidGT { }),
          mut_effects: &[], // TODO
        })))
      }
      ExpressionTE::Mutate(MutateTE { .. }) => unimplemented!(),
      ExpressionTE::Restackify(RestackifyTE { .. }) => unimplemented!(),
      ExpressionTE::Break(BreakTE { range, result, .. }) => {
        Ok(ExpressionGE::Break(bump_g.alloc(BreakGE {
          range: *range,
          result: KindGT::Never(NeverGT { from_break: true }),
        })))
      }
      ExpressionTE::StaticArrayFromValues(StaticArrayFromValuesTE { .. }) => unimplemented!(),
      ExpressionTE::ArraySize(ArraySizeTE { .. }) => unimplemented!(),
      ExpressionTE::IsSameInstance(IsSameInstanceTE { .. }) => unimplemented!(),
      ExpressionTE::AsSubtype(AsSubtypeTE { .. }) => unimplemented!(),
      ExpressionTE::ConstantStr(ConstantStrTE { .. }) => unimplemented!(),
      ExpressionTE::InterfaceFunctionCall(InterfaceFunctionCallTE { .. }) => unimplemented!(),
      ExpressionTE::ExternFunctionCall(ExternFunctionCallTE { .. }) => unimplemented!(),
      ExpressionTE::BoundFunctionCall(BoundFunctionCallTE { .. }) => unimplemented!(),
      ExpressionTE::Reinterpret(ReinterpretTE { .. }) => unimplemented!(),
      ExpressionTE::Construct(ConstructTE { .. }) => unimplemented!(),
      ExpressionTE::NewRuntimeSizedArray(NewRuntimeSizedArrayTE { .. }) => unimplemented!(),
      ExpressionTE::StaticArrayFromCallable(StaticArrayFromCallableTE { .. }) => unimplemented!(),
      ExpressionTE::DestroyStaticSizedArrayIntoFunction(DestroyStaticSizedArrayIntoFunctionTE { .. }) => unimplemented!(),
      ExpressionTE::DestroyStaticSizedArrayIntoLocals(DestroyStaticSizedArrayIntoLocalsTE { .. }) => unimplemented!(),
      ExpressionTE::DestroyRuntimeSizedArray(DestroyRuntimeSizedArrayTE { .. }) => unimplemented!(),
      ExpressionTE::RuntimeSizedArrayCapacity(RuntimeSizedArrayCapacityTE { .. }) => unimplemented!(),
      ExpressionTE::PushRuntimeSizedArray(PushRuntimeSizedArrayTE { .. }) => unimplemented!(),
      ExpressionTE::PopRuntimeSizedArray(PopRuntimeSizedArrayTE { .. }) => unimplemented!(),
      ExpressionTE::InterfaceToInterfaceUpcast(InterfaceToInterfaceUpcastTE { .. }) => unimplemented!(),
      ExpressionTE::UpcastInterface(UpcastInterfaceTE { .. }) => unimplemented!(),
      ExpressionTE::UpcastGeneric(UpcastGenericTE { .. }) => unimplemented!(),
      ExpressionTE::CopyPrim(CopyPrimTE { range, loct, inner: source_te, result, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        let inner_ge =
          match source_ge.result() {
            KindGT::BorrowRef(BorrowRefGT { inner, group }) => *inner,
            _ => panic!("CopyPrimt encountered non-borrow ref"),
          };
        Ok(ExpressionGE::CopyPrim(bump_g.alloc(CopyPrimGE {
          range: *range,
          loct: *loct,
          inner: source_ge,
          result: inner_ge,
        })))
      },
      ExpressionTE::StaticSizedArrayLookup(StaticSizedArrayLookupTE { .. }) => unimplemented!(),
      ExpressionTE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupTE { range, array_expr, array_type, index_expr, result, .. }) => {
        let array_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *array_expr, local_rune_to_templata, local_to_type_g)?;
        let index_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *index_expr, local_rune_to_templata, local_to_type_g)?;
        let (array_group, array_gt) =
            match array_ge.result() {
              KindGT::BorrowRef(BorrowRefGT {
                inner: KindGT::RuntimeSizedArray(rsa_gt),
                group: group_expr
              }) => (group_expr, rsa_gt),
              _ => panic!("Expected RSA"),
            };

        Ok(ExpressionGE::RuntimeSizedArrayLookup(bump_g.alloc(RuntimeSizedArrayLookupGE {
          range: *range,
          array_expr: array_ge,
          array_type: array_gt,
          index_expr: index_ge,
          result: bump_g.alloc(BorrowRefGT{ inner: array_gt.element_type, group: *array_group }),
        })))
      }
      ExpressionTE::MemberLookup(MemberLookupTE { .. }) => unimplemented!(),
    }
  }

  fn translate_struct_members<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    source_struct_gt: StructGT<'s, 't, 'g>
  ) -> IndexMap<StrI<'s>, KindGT<'s, 't, 'g>> {
    let struct_template_id = Compiler::get_template(self.typing_interner, *source_struct_gt.id);
    let citizen_def_s =
        coutputs.peek_postparsed_type(struct_template_id)
            .expect("Struct not present");
    let struct_def_s =
        match citizen_def_s {
          ICitizenDenizenS::TopLevelStruct(s) => s,
          ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct, got interface"),
        };
    let struct_def_t = coutputs.lookup_struct_template(*struct_template_id);

    let callee_rune_to_caller_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> =
        self.calculate_struct_callee_rune_to_caller_templata(coutputs, bump_g, struct_def_s, struct_def_t, source_struct_gt.template_args);

    assert!(struct_def_s.members.len() == struct_def_t.members.len());
    let mut locally_phrased_member_types_gt = Vec::new();
    for (i_member_s, member_t) in struct_def_s.members.iter().zip(struct_def_t.members) {
      let member_s =
          match i_member_s {
            IStructMemberS::NormalStructMember(nsm) => nsm,
            IStructMemberS::VariadicStructMember(_) => unimplemented!(),
          };
      let written_context =
          WrittenContext {
            type_s: member_s.tyype,
            name: None,
          };
      let type_g =
          self.groupify_type(coutputs, bump_g, &callee_rune_to_caller_templata, &IndexMap::new(), member_t.tyype, Some(&written_context));
      locally_phrased_member_types_gt.push((member_s.name, type_g));
    }
    IndexMap::from_iter(locally_phrased_member_types_gt)
  }

  fn groupify_var<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    var: &'t LocalVariable<'s, 't>,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) -> &'g LocalVariableG<'s, 't, 'g> {
    bump_g.alloc(LocalVariableG {
      name: var.name,
      tyype: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, var.tyype, None)
    })
  }

  fn groupify_type<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    type_t: KindT<'s, 't>,
    maybe_written: Option<&WrittenContext<'s>>,
  ) -> KindGT<'s, 't, 'g> {
    match type_t {
      KindT::BorrowRef(BorrowRefT { inner: inner_tt }) => {
        let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, None);
        let written = maybe_written.expect("Encountered a borrow ref with no written group");
        let type_s = written.type_s;
        let BorrowRefST { range: bst_range, inner: bst_inner, region: bst_group_s } =
            match type_s {
              ITypeST::BorrowRef(bst) => *bst,
              _ => panic!("Encountered a borrow ref not matching up with a written borrow ref"),
            };
        let bst_specified_group_s =
          match bst_group_s {
            RegionS::Unspecified => panic!("Encountered a borrow ref with unspecified region"),
            RegionS::Held => panic!("Encountered a borrow ref with held region"),
            RegionS::Group(b) => *b,
          };
        let group_path_g =
              self.groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, bst_specified_group_s);
        let group_expr_g: GroupExprG<'s, 't, 'g> =
            bump_g.alloc_slice_copy(&[group_path_g]);
        let group_templata_g = GroupTemplataG { kind: inner_gt, group: group_expr_g, };
        KindGT::BorrowRef(bump_g.alloc(BorrowRefGT {
          inner: inner_gt,
          group: group_templata_g,
        }))
      },
      KindT::Never(NeverT { from_break }) => KindGT::Never(NeverGT { from_break }),
      KindT::Void(VoidT { }) => KindGT::Void(VoidGT { }),
      KindT::Int(IntT { bits }) => KindGT::Int(IntGT { bits }),
      KindT::Bool(BoolT { }) => KindGT::Bool(BoolGT { }),
      KindT::Str(StrT { }) => KindGT::Str(StrGT { }),
      KindT::Float(FloatT { }) => KindGT::Float(FloatGT { }),
      KindT::USize(USizeT { }) => KindGT::USize(USizeGT { }),
      KindT::Struct(StructTT { id, .. }) => {
        let template_args_t: &'t [ITemplataT<'s, 't>] =
            ICitizenNameT::try_from(id.local_name)
                .expect("Struct without ICitizenNameT")
                .template_args();
        let template_args_g =
            bump_g.alloc_slice_copy(
                template_args_t
                    .iter()
                    .map(|x| self.groupify_templata(coutputs, bump_g, rune_to_templata, local_to_type_g, *x))
                    .collect::<Vec<_>>()
                    .as_slice());
        KindGT::Struct(bump_g.alloc(StructGT { id, template_args: template_args_g }))
      }
      KindT::KindPlaceholder(kp @ KindPlaceholderT { id: id_t }) => {
        let rune =
            match id_t.local_name {
              INameT::KindPlaceholder(KindPlaceholderNameT { template: KindPlaceholderTemplateNameT { index, rune } }) => {
                rune
              }
              _ => panic!("Unexpected name for a placeholder"),
            };
        expect_kind_templata_g(
            *rune_to_templata
                .get(rune)
                .expect("Couldn't find rune in rune_to_templata map")).kind
      }
      KindT::Interface(InterfaceTT { .. }) => unimplemented!(), // KindGT::Interface(InterfaceGT { }),
      KindT::StaticSizedArray(StaticSizedArrayTT { .. }) => unimplemented!(), // KindGT::StaticSizedArray(StaticSizedArrayGT { }),
      KindT::RuntimeSizedArray(RuntimeSizedArrayTT { name: id_t, .. }) => {
        let rsa_local_name =
          match id_t.local_name {
            INameT::RuntimeSizedArray(rsa_name) => rsa_name,
            _ => panic!("Expected RSA"),
          };
        let element_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, rsa_local_name.arr.element_type, None);
        KindGT::RuntimeSizedArray(bump_g.alloc(RuntimeSizedArrayGT {
          name: *id_t,
          element_type: element_gt,
        }))
      }
      KindT::OverloadSet(OverloadSetT { .. }) => unimplemented!(), // KindGT::OverloadSet(OverloadSetGT { }),
      KindT::OwnRef(OwnRefT { .. }) => unimplemented!(), // KindGT::OwnRef(OwnRefGT { }),
      KindT::ShareRef(ShareRefT { .. }) => unimplemented!(), // KindGT::ShareRef(ShareRefGT { }),
      KindT::WeakRef(WeakRefT { .. }) => unimplemented!(), // KindGT::WeakRef(WeakRefGT { }),
    }
  }

  fn groupify_templata<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    templata_t: ITemplataT<'s, 't>,
  ) -> ITemplataG<'s, 't, 'g> {
    match templata_t {
      ITemplataT::Kind(KindTemplataT { kind }) => {
        let kind_g = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, kind, None);
        ITemplataG::Kind(KindTemplataG { kind: kind_g })
      },
      ITemplataT::Placeholder(_) => unimplemented!(),
      ITemplataT::Integer(num) => ITemplataG::Integer(num),
      ITemplataT::Boolean(_) => unimplemented!(),
      ITemplataT::String(_) => unimplemented!(),
      ITemplataT::Prototype(_) => unimplemented!(),
      ITemplataT::Isa(_) => unimplemented!(),
      ITemplataT::CoordList(_) => unimplemented!(),
      ITemplataT::RuntimeSizedArrayTemplate(_) => unimplemented!(),
      ITemplataT::StaticSizedArrayTemplate(_) => unimplemented!(),
      ITemplataT::Group(_) => unimplemented!(),
      ITemplataT::Function(_) => unimplemented!(),
      ITemplataT::StructDefinition(_) => unimplemented!(),
      ITemplataT::InterfaceDefinition(_) => unimplemented!(),
      ITemplataT::ImplDefinition(_) => unimplemented!(),
      ITemplataT::ExternFunction(_) => unimplemented!(),
    }
  }

  // "Struct callee" = the struct whose template we're calling
  fn calculate_struct_callee_rune_to_caller_templata<'g>(
      &self,
      coutputs: &CompilerOutputs<'s, 't>,
      bump_g: &'g Bump,
      struct_def_s: &'s StructS<'s>,
      struct_def_t: &'t StructDefinitionT<'s, 't>,
      template_args_ge: &'g [ITemplataG<'s, 't, 'g>]
  ) -> IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> {
    let mut map = IndexMap::new();
    assert!(template_args_ge.len() == struct_def_s.generic_params.len());
    for i in 0..template_args_ge.len() {
      map.insert(struct_def_s.generic_params[i].rune.rune, template_args_ge[i]);
    }
    map
  }

  fn calculate_callee_rune_to_caller_templata<'g>(
      &self,
      coutputs: &CompilerOutputs<'s, 't>,
      bump_g: &'g Bump,
      function_s: &'s FunctionS<'s>,
      args_ge: &Vec<ExpressionGE<'s, 't, 'g>>
  ) -> IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> {
    let mut map = IndexMap::new();
    assert!(args_ge.len() == function_s.params.len());
    for i in 0..args_ge.len() {
      self.match_types(coutputs, bump_g, function_s.params[i].tyype, ITemplataG::Kind(KindTemplataG { kind: args_ge[i].result() }), &mut map);
    }
    map
  }

  fn match_types<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    type_s: ITypeST<'s>,
    type_g: ITemplataG<'s, 't, 'g>,
    map: &mut IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) {
    match type_s {
      ITypeST::BorrowRef(BorrowRefST { range, inner: inner_st, region: group_s }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::BorrowRef(BorrowRefGT{ inner: inner_gt, group: group_templata_g }) }) => {
            self.match_types(coutputs, bump_g, **inner_st, ITemplataG::Kind(KindTemplataG { kind: *inner_gt }), map);

            match group_s {
              RegionS::Unspecified => unimplemented!(),
              RegionS::Held => unimplemented!(),
              RegionS::Group(group_s) => {
                match group_s {
                  GroupS::Rune(RuneUsage { rune, .. }) => {
                    map.insert(*rune, ITemplataG::Group(*group_templata_g));
                  }
                  GroupS::Local(_) => {}
                  GroupS::Member { .. } => {}
                  GroupS::Elements { .. } => {}
                  GroupS::Ellipsis { .. } => {}
                  GroupS::Union { .. } => {}
                }
              }
            }
          }
          _ => panic!("Unexpected non-borrow ref"),
        }
      }
      ITypeST::Rune(RuneUsageST { rune: RuneUsage { range, rune }}) => {
        map.insert(*rune, type_g);
      }
      ITypeST::Function(_) => unimplemented!(),
      ITypeST::AnonymousRune(_) => unimplemented!(),
      ITypeST::Bool(_) => {}
      ITypeST::Call(CallST { range, template: template_s, args: template_args_s }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::Struct(StructGT { id: struct_id_t, template_args: template_args_g}) }) => {
            let struct_template_id_t = Compiler::get_template(self.typing_interner, **struct_id_t);
            let citizen_denizen_s = coutputs.peek_postparsed_type(struct_template_id_t).expect("Couldn't find struct template");
            let struct_def_templata_type =
                match citizen_denizen_s {
                  ICitizenDenizenS::TopLevelStruct(struct_s) => struct_s.tyype,
                  ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct"),
                };
            let struct_templata_g =
                ITemplataG::StructDefinition(
                    bump_g.alloc(
                        StructDefinitionTemplataG {
                          struct_template_id: struct_template_id_t,
                          tyype: struct_def_templata_type
                        }));
            self.match_types(coutputs, bump_g, **template_s, struct_templata_g, map);
            for (template_arg_s, template_arg_g) in template_args_s.iter().zip(template_args_g.iter()) {
              self.match_types(coutputs, bump_g, **template_arg_s, *template_arg_g, map);
            }
          }
          ITemplataG::Kind(KindTemplataG { kind: KindGT::Interface(InterfaceGT { id: interface_id_t, template_args: template_args_g}) }) => {
            let interface_template_id_t = Compiler::get_template(self.typing_interner, **interface_id_t);
            let citizen_denizen_s = coutputs.peek_postparsed_type(interface_template_id_t).expect("Couldn't find interface template");
            let interface_def_templata_type =
                match citizen_denizen_s {
                  ICitizenDenizenS::TopLevelStruct(_) => panic!("Expected interface"),
                  ICitizenDenizenS::TopLevelInterface(interface_s) => interface_s.tyype,
                };
            let interface_templata_g =
                ITemplataG::InterfaceDefinition(
                  bump_g.alloc(
                    InterfaceDefinitionTemplataG {
                      interface_template_id: interface_template_id_t,
                      tyype: interface_def_templata_type
                    }));
            self.match_types(coutputs, bump_g, **template_s, interface_templata_g, map);
            for (template_arg_s, template_arg_g) in template_args_s.iter().zip(template_args_g.iter()) {
              self.match_types(coutputs, bump_g, **template_arg_s, *template_arg_g, map);
            }
          }
          ITemplataG::Kind(KindTemplataG {
                             kind: KindGT::Int(_) | KindGT::Bool(_) | KindGT::Float(_) | KindGT::Str(_) | KindGT::Void(_) | KindGT::USize(_) | KindGT::Never(_),
                           }) => {
            // Postparser makes zero-arg calls to primitives.
            assert!(template_args_s.is_empty());
          },
          _ => panic!("Unexpected non-template type"),
        }
      }
      ITypeST::Int(_) => {}
      ITypeST::Tuple(_) => unimplemented!(),
      ITypeST::Name(_) => unimplemented!(),
      ITypeST::WeakRef(_) => unimplemented!(),
      ITypeST::OwnRef(_) => unimplemented!(),
      ITypeST::Pack(_) => unimplemented!(),
      ITypeST::RuntimeSizedArray(RuntimeSizedArrayST { range, element: element_st }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) }) => {
            // let struct_template_id_t = Compiler::get_template(self.typing_interner, **name);
            // let citizen_denizen_s = coutputs.peek_postparsed_type(struct_template_id_t).expect("Couldn't find struct template");
            // let struct_def_templata_type =
            //     match citizen_denizen_s {
            //       ICitizenDenizenS::TopLevelStruct(struct_s) => struct_s.tyype,
            //       ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct"),
            //     };
            // let struct_templata_g =
            //     ITemplataG::StructDefinition(
            //       bump_g.alloc(
            //         StructDefinitionTemplataG {
            //           struct_template_id: struct_template_id_t,
            //           tyype: struct_def_templata_type
            //         }));
            // self.match_types(coutputs, bump_g, **template_s, struct_templata_g, map);
            self.match_types(coutputs, bump_g, **element_st, ITemplataG::Kind(KindTemplataG { kind: *element_type }), map);
          }
          _ => panic!("Unexpected non-runtime sized array type"),
        }
      }
      ITypeST::String(_) => {}
    }
  }

  fn groupify_group<'g>(
    &self,
    bump_g: &'g Bump,
    group_s: &'s GroupS<'s>,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) -> &'g [GroupStep<'s, 't>] {
    match group_s {
      GroupS::Rune(RuneUsage { range, rune: rune_s }) => {
        // rune_s might be from the callee, so look it up in our (the caller) map.
        let path =
            match rune_to_templata.get(rune_s).expect("vfail: group rune missing") {
              ITemplataG::Group(GroupTemplataG { group, .. }) => {
                assert!(group.len() == 1); // unimplemented
                group[0]
              }
              other => panic!("vfail: group rune bound to a non-group: {other:?}"),
            };
        // Translate it from GroupRootG/GroupChildStepG to GroupStep
        let mut result = Vec::new();
        result.push(match path.root {
          GroupRootG::Rune(r) => GroupStep::Rune(r),
          GroupRootG::ParamAnonymousGroup(n) => GroupStep::ParamAnonymousGroup(n),
          GroupRootG::Local(n) => GroupStep::Local(n),
        });
        for step in path.steps {
          result.push(match step {
            GroupChildStepG::Member { member_name } => GroupStep::Member { member_name: *member_name },
            GroupChildStepG::ChildElements {} => GroupStep::ChildElements,
            GroupChildStepG::InlineElements {} => GroupStep::InlineElements,
            GroupChildStepG::Variant { variant_name } => GroupStep::Variant { variant_name: *variant_name },
          });
        }
        bump_g.alloc_slice_copy(result.as_slice())
      }
      GroupS::Local(_) => unimplemented!(),
      GroupS::Member { .. } => unimplemented!(),
      GroupS::Elements { .. } => unimplemented!(),
      GroupS::Ellipsis { .. } => unimplemented!(),
      GroupS::Union { .. } => unimplemented!(),
    }
  }

  fn groupify_effect<'g>(
    &self,
    bump_g: &'g Bump,
    effect_s: &'s EffectS<'s>,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) -> &'g [GroupStep<'s, 't>] {
    let something =
      match effect_s {
        EffectS::Mut(group_s) => {
          self.groupify_group(bump_g, group_s, rune_to_templata)
        }
        EffectS::NotMut(_) => unimplemented!(),
      };
    something
  }

  fn groupify_group_expr<'g>(
     &self,
     coutputs: &CompilerOutputs<'s, 't>,
     bump_g: &'g Bump,
     local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
     local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
     group_s: GroupS<'s>,
  ) -> GroupPathG<'s, 't, 'g> {
    let (root, mut path, type_gt) =
      self.groupify_group_expr_inner(coutputs, bump_g, local_rune_to_templata, local_to_type_g, group_s);
    GroupPathG {
      root: root,
      steps: bump_g.alloc_slice_copy(path.as_slice()),
      ellipsis: false,
    }
  }

  fn groupify_group_expr_inner<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    group_s: GroupS<'s>, // TODO: flatten GroupS so we dont have to do this recursion
  ) ->
  // Returns:
  // - Root
  // - Path, starting from root, for the given group_s path
  // - The type at this path so far
  (GroupRootG<'s, 't>, Vec<GroupChildStepG<'s>>, KindGT<'s, 't, 'g>) {
    match group_s {
      GroupS::Rune(RuneUsage { range: ru_range, rune: group_expr_g_rune }) => {
        // group_expr_g_rune might be appearing in someone else's definition, we need to translate
        // it back to how we (the caller) see it.
        let local_templata =
            local_rune_to_templata.get(group_expr_g_rune).expect("Missing templata for rune");
        match local_templata {
          ITemplataG::Group(GroupTemplataG { group, kind: group_kind }) => {
            assert!(group.len() == 1); // unimplemented
            let path = group[0];
            // Use these instead of group_expr_g_rune because, remember, it might be in someone
            // else's definition.
            (path.root, path.steps.to_vec(), *group_kind)
          }
          _ => panic!("Unexpected templata type"),
        }
      }
      GroupS::Local(imprecise_name) => {
        // TODO: perhaps key local_to_type_g on imprecise name instead, this is slow
        let (var_name_t, var_kind_gt) =
            local_to_type_g.iter()
                .find(|(v, k)| v.imprecise_name() == Some(imprecise_name))
                .expect("Local not found");
        (GroupRootG::Local(*var_name_t), vec![], *var_kind_gt)
      }
      GroupS::Member { base, member_name } => {
        let (root, mut path, type_gt) =
            self.groupify_group_expr_inner(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *base);
        let member_type_gt =
          match type_gt {
            KindGT::Struct(struct_gt) => {
              let name_to_member_type_gt =
                self.translate_struct_members(coutputs, bump_g, *struct_gt);
              *name_to_member_type_gt.get(&member_name).expect("No member in struct with that name")
            }
            _ => panic!("Unexpected type in group member expr"),
          };
        path.push(GroupChildStepG::Member { member_name: member_name});
        (root, path, member_type_gt)
      }
      GroupS::Elements { base } => {
        let (root, mut path, type_gt) =
            self.groupify_group_expr_inner(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *base);
        match type_gt {
          KindGT::StaticSizedArray(StaticSizedArrayGT { name, element_type }) => {
            path.push(GroupChildStepG::InlineElements { });
            (root, path, *element_type)
          }
          KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) => {
            path.push(GroupChildStepG::ChildElements { });
            (root, path, *element_type)
          }
          _ => panic!("Unexpected type in group elements expr"),
        }
      }
      GroupS::Ellipsis { .. } => unimplemented!(),
      GroupS::Union { .. } => unimplemented!(),
    }
  }

  fn get_type_at_group_expr<'g>(
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    group_expr_g_rune: IRuneS<'s>
  ) -> KindGT<'s, 't, 'g> {
    let templata =
        local_rune_to_templata.get(&group_expr_g_rune)
            .expect("Rune not found");
    match templata {
      ITemplataG::Kind(KindTemplataG { kind: kind_gt }) => *kind_gt,
      ITemplataG::Group(GroupTemplataG { group: group_expr, kind }) => {
        *kind
        // assert!(group_expr.len() == 1); // more is unimplemented
        // let group_path = group_expr.first().unwrap();
        // match group_path.root {
        //   GroupRootG::Rune(rune) => {
        //     Self::get_type_at_group_expr(local_rune_to_templata, rune)
        //   }
        //   GroupRootG::ParamAnonymousGroup(_) => unimplemented!(),
        //   GroupRootG::Local(_) => unimplemented!(),
        // }
      }
      _ => unimplemented!(),
    }
  }

  fn simple_match_group_rune_types<'g>(
      &self,
      coutputs: &CompilerOutputs<'s, 't>,
      bump_g: &'g Bump,
      group_rune_to_type_gt: &mut IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
      type_st: ITypeST<'s>,
      templata_t: ITemplataT<'s, 't>
  ) -> ITemplataG<'s, 't, 'g>{
    match type_st {
      ITypeST::BorrowRef(BorrowRefST{ inner: inner_type_s, region: group_s, .. }) => {
        match templata_t {
          ITemplataT::Kind(KindTemplataT { kind: KindT::BorrowRef(BorrowRefT { inner: inner_type_t }) }) => {
            let inner_gt =
                self.simple_match_group_rune_types(coutputs, bump_g, group_rune_to_type_gt, **inner_type_s, ITemplataT::Kind(KindTemplataT { kind: *inner_type_t }));
            let inner_kind_gt = expect_kind_templata_g(inner_gt).kind;

            match group_s {
              RegionS::Unspecified => unimplemented!(),
              RegionS::Held => unimplemented!(),
              RegionS::Group(group_s) => {
                match group_s {
                  GroupS::Rune(RuneUsage { rune, .. }) => {
                    // The purpose of this whole function is to register GroupTemplataG's.
                    // We've finally found one.

                    let group_expr_g =
                      match group_rune_to_type_gt.get(rune) {
                        Some(existing) => {
                          unimplemented!()
                        }
                        None => {
                          let group_templata_g =
                              GroupTemplataG {
                                group: Self::new_rune_group_expr(bump_g, *rune),
                                kind: inner_kind_gt,
                              };
                          group_rune_to_type_gt.insert(*rune, ITemplataG::Group(group_templata_g));
                          group_templata_g
                        }
                      };

                    ITemplataG::Kind(KindTemplataG {
                      kind: KindGT::BorrowRef(
                        bump_g.alloc(BorrowRefGT {
                          inner: inner_kind_gt,
                          group: group_expr_g
                        }))
                    })
                  }
                  GroupS::Local(_) => unimplemented!(),
                  GroupS::Member { .. } => unimplemented!(),
                  GroupS::Elements { .. } => unimplemented!(),
                  GroupS::Ellipsis { .. } => unimplemented!(),
                  GroupS::Union { .. } => unimplemented!(),
                }
              }
            }
          }
          _ => panic!("Unexpected type for borrow reference"),
        }
      }
      ITypeST::RuntimeSizedArray(RuntimeSizedArrayST { element: element_st, .. }) => {
        match templata_t {
          ITemplataT::Kind(KindTemplataT { kind: KindT::RuntimeSizedArray(RuntimeSizedArrayTT { name: rsa_id_t, .. }) }) => {
            let rsa_local_name =
                match rsa_id_t.local_name {
                  INameT::RuntimeSizedArray(rsa_name) => rsa_name,
                  _ => panic!("Expected RSA"),
                };
            let element_gt =
                expect_kind_templata_g(
                    self.simple_match_group_rune_types(
                      coutputs,
                      bump_g,
                      group_rune_to_type_gt,
                      **element_st,
                      ITemplataT::Kind(KindTemplataT{ kind: rsa_local_name.arr.element_type })));
            ITemplataG::Kind(KindTemplataG{
              kind: KindGT::RuntimeSizedArray(bump_g.alloc(RuntimeSizedArrayGT {
                name: *rsa_id_t,
                element_type: element_gt.kind,
              }))
            })
          }
          _ => panic!("Unexpected type in array expr"),
        }
      }
      ITypeST::AnonymousRune(_) => unimplemented!(),
      ITypeST::Bool(_) => unimplemented!(),
      ITypeST::Call(CallST { range, template: template_s, args: template_args_s }) => {
        match templata_t {
          ITemplataT::Kind(KindTemplataT { kind: KindT::Struct(StructTT { id: struct_id_t, ..}) }) => {
            let struct_template_id_t = Compiler::get_template(self.typing_interner, **struct_id_t);
            let citizen_denizen_s = coutputs.peek_postparsed_type(struct_template_id_t).expect("Couldn't find struct template");
            let struct_def_templata_type =
                match citizen_denizen_s {
                  ICitizenDenizenS::TopLevelStruct(struct_s) => struct_s.tyype,
                  ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct"),
                };
            let struct_templata_g =
                ITemplataG::StructDefinition(
                  bump_g.alloc(
                    StructDefinitionTemplataG {
                      struct_template_id: struct_template_id_t,
                      tyype: struct_def_templata_type
                    }));
            let template_args_tt =
                IStructNameT::try_from(struct_id_t.local_name)
                    .expect("Expected struct name")
                    .template_args();
            let mut template_args_g_vec = Vec::new();
            for (template_arg_s, template_arg_t) in template_args_s.iter().zip(template_args_tt.iter()) {
              template_args_g_vec.push(
                self.simple_match_group_rune_types(coutputs, bump_g, group_rune_to_type_gt, **template_arg_s, *template_arg_t));
            }
            let template_args_g = bump_g.alloc_slice_copy(template_args_g_vec.as_slice());
            ITemplataG::Kind(KindTemplataG {
              kind: KindGT::Struct(
                bump_g.alloc(StructGT {
                  id: struct_id_t,
                  template_args: template_args_g
                }))
            })
          }
          ITemplataT::Kind(KindTemplataT { kind: KindT::Int(IntT { bits }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Int(IntGT { bits }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::Bool(BoolT { }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Bool(BoolGT { }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::Void(VoidT { }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Void(VoidGT { }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::Float(FloatT { }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Float(FloatGT { }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::Str(StrT { }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Str(StrGT { }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::USize(USizeT { }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::USize(USizeGT { }) }),
          ITemplataT::Kind(KindTemplataT { kind: KindT::Never(NeverT { from_break }) }) => ITemplataG::Kind(KindTemplataG { kind: KindGT::Never(NeverGT { from_break }) }),
          _ => panic!("Unexpected non-template type: {:?}", templata_t),
        }
      }
      ITypeST::Function(_) => unimplemented!(),
      ITypeST::Int(_) => unimplemented!(),
      ITypeST::Tuple(_) => unimplemented!(),
      ITypeST::Name(_) => unimplemented!(),
      ITypeST::Rune(RuneUsageST{ rune: RuneUsage { rune, ..} }) => {
        match templata_t {
          ITemplataT::Kind(KindTemplataT { kind: KindT::KindPlaceholder(KindPlaceholderT { id: placeholder_name_t }) }) => {
            match placeholder_name_t.local_name {
              INameT::KindPlaceholder(KindPlaceholderNameT { template: KindPlaceholderTemplateNameT { index, rune } }) => {
                // Placeholders were seeded before
                *group_rune_to_type_gt.get(rune).expect("Couldn't find rune")
              }
              _ => unimplemented!(),
            }
          },
          ITemplataT::Placeholder(_) => unimplemented!(),
          _ => unimplemented!(),
        }
      }
      ITypeST::WeakRef(_) => unimplemented!(),
      ITypeST::OwnRef(_) => unimplemented!(),
      ITypeST::Pack(_) => unimplemented!(),
      ITypeST::String(_) => unimplemented!(),
    }
  }
}
