use crate::typing::borrow_checker::ast_g::*;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use bumpalo::Bump;
use indexmap::IndexMap;
use crate::postparsing::ast::FunctionS;
use crate::postparsing::rules::types::ITypeST;
use crate::StrI;
use crate::typing::ast::ast::{FunctionDefinitionT, LocT};
use crate::typing::borrow_checker::borrow_error::BorrowErrorKind;
use crate::typing::borrow_checker::check_usages_types::*;
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::*;
use crate::typing::borrow_checker::templata_g::*;
use crate::typing::templata::templata::*;
use crate::typing::types::types::*;
use crate::utils::range::RangeS;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn check_usages<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    arena: &'g Bump,
    function_g_body: ExpressionGE<'s, 't, 'g>,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let mut group_tree =
        GroupSubtree {
          last_mut_effect: None,
          name_to_child: IndexMap::new(),
        };
    let mut next_held_num = 0;
    self.check_expr(coutputs, function_s, arena, &mut group_tree, function_g_body, &mut next_held_num)?;
    Ok(())
  }

  pub fn check_expr<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    arena: &'g Bump,
    group_tree: &mut GroupSubtree<'s, 't>,
    expr: ExpressionGE<'s, 't, 'g>,
    next_held_num: &mut u32,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    match expr {
      ExpressionGE::Block(BlockGE { range, inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::LetNormal(LetNormalGE { range, variable, expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *expr, next_held_num)?;
        // self.insert_new_variable(group_tree, RefKey::Named(variable.name), variable.tyype);
      }
      ExpressionGE::LetAndLend(LetAndLendGE { range, loct, variable, expr, result }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *expr, next_held_num)?;
      }
      ExpressionGE::LocalLookup(LocalLookupGE { range, local_variable, result, .. }) => {
        // self.check_variable_still_valid(group_tree, *range, RefKey::Named(local_variable.name), local_variable.tyype)?;

        // if is_use_after_churn(group_tree, RefKey::Named(local_variable.name)) {
        //   return Err(ICompileErrorT::BorrowCheckError {
        //     range,
        //     kind: BorrowErrorKind::UseAfterChurn { local: local_variable.name },
        //   });
        // }
        // fn is_use_after_churn(node, key) -> bool {
        //   node.locals.get(&key).is_some_and(|e| e.invalidated_by.is_some())
        //       || node.locals_in_ellipsis.get(&key).is_some_and(|e| e.invalidated_by.is_some())
        //       || node.name_to_child.values().any(|child| is_use_after_churn(child, key))
        // }
      }
      ExpressionGE::Unlet(UnletGE { range, variable: local_variable, result, .. }) => {
        // Nothing needed
      }
      ExpressionGE::Discard(DiscardGE { range, expr: inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::Return(ReturnGE { range, source_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *source_expr, next_held_num)?;
      }
      ExpressionGE::Consecutor(ConsecutorGE { range, exprs, result: result_tt, .. }) => {
        for expr in exprs.iter() {
          self.check_expr(coutputs, function_s, arena, group_tree, *expr, next_held_num)?;
        }
      }
      ExpressionGE::ConstantInt(ConstantIntGE { range, value, bits, .. }) => {
        // Do nothing
      }
      ExpressionGE::ConstantBool(ConstantBoolGE { range, value, .. }) => {
        // Do nothing
      }
      ExpressionGE::ConstantFloat(ConstantFloatGE { range, value, .. }) => {
        // Do nothing
      }
      ExpressionGE::ArgLookup(ArgLookupGE { range, param_index, result, .. }) => {
        // Do nothing
      }
      ExpressionGE::ArrayLength(ArrayLengthGE { range, array_expr, result, .. }) => {
        unimplemented!()
      }
      ExpressionGE::Deref(DerefGE { range, loct, inner: source_te, result: result_tt, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *source_te, next_held_num)?;
      }
      ExpressionGE::FunctionCall(FunctionCallGE { loct, range, callable, args, result, mut_effects, .. }) => {
        for arg in args.iter() {
          // Careful, this may cause some mut effects, that could invalidate other arguments.
          // We check the argument types again below, in case that happened.
          self.check_expr(coutputs, function_s, arena, group_tree, *arg, next_held_num)?;
        }
        // Check each argument again, just in case any of the arguments invalidated any of the
        // other args.
        for arg in args.iter() {
          self.check_kind_still_valid(group_tree, arg.range(), arg.result())?;
        }

        // Conceptually, the call happens here

        // Process the calls' effects
        let mel = MutEffectLoc { loct: *loct, range: range[0] };
        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_tree, mel, mut_effect.steps);
        }
      }
      ExpressionGE::LockWeak(_) => unimplemented!(),
      ExpressionGE::BorrowToWeak(_) => unimplemented!(),
      ExpressionGE::If(_) => unimplemented!(),
      ExpressionGE::While(_) => unimplemented!(),
      ExpressionGE::Mutate(_) => unimplemented!(),
      ExpressionGE::Restackify(_) => unimplemented!(),
      ExpressionGE::Break(_) => unimplemented!(),
      ExpressionGE::StaticArrayFromValues(_) => unimplemented!(),
      ExpressionGE::ArraySize(_) => unimplemented!(),
      ExpressionGE::IsSameInstance(_) => unimplemented!(),
      ExpressionGE::AsSubtype(_) => unimplemented!(),
      ExpressionGE::VoidLiteral(_) => {
        // Do nothing
      }
      ExpressionGE::ConstantStr(_) => {
        // Do nothing
      }
      ExpressionGE::InterfaceFunctionCall(_) => unimplemented!(),
      ExpressionGE::ExternFunctionCall(_) => unimplemented!(),
      ExpressionGE::BoundFunctionCall(_) => unimplemented!(),
      ExpressionGE::Reinterpret(ReinterpretGE { range, expr: source_ge, result }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *source_ge, next_held_num)?;
      }
      ExpressionGE::Construct(_) => unimplemented!(),
      ExpressionGE::NewRuntimeSizedArray(_) => unimplemented!(),
      ExpressionGE::StaticArrayFromCallable(_) => unimplemented!(),
      ExpressionGE::DestroyStaticSizedArrayIntoFunction(_) => unimplemented!(),
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(_) => unimplemented!(),
      ExpressionGE::DestroyRuntimeSizedArray(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayCapacity(_) => unimplemented!(),
      ExpressionGE::PushRuntimeSizedArray(_) => unimplemented!(),
      ExpressionGE::PopRuntimeSizedArray(_) => unimplemented!(),
      ExpressionGE::InterfaceToInterfaceUpcast(_) => unimplemented!(),
      ExpressionGE::UpcastInterface(_) => unimplemented!(),
      ExpressionGE::UpcastGeneric(_) => unimplemented!(),
      ExpressionGE::Destroy(DestroyGE { range, expr, struct_tt, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *expr, next_held_num)?;
      }
      ExpressionGE::CopyPrim(CopyPrimGE { range, loct, inner, result }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::StaticSizedArrayLookup(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupGE { range, array_expr, array_type, index_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, group_tree, *index_expr, next_held_num)?;
      }
      ExpressionGE::MemberLookup(MemberLookupGE { struct_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *struct_expr, next_held_num)?;
      }
    }
    Ok(())
  }

  fn note_mut_effect<'g>(
    &self,
    group_subtree: &mut GroupSubtree<'s, 't>,
    effect_loc: MutEffectLoc<'s, 't>,
    mut_effect_steps: &'g [GroupStep<'s, 't>],
  ) {
    match mut_effect_steps.split_first() {
      None => {
        group_subtree.last_mut_effect = Some(effect_loc);
      }
      Some((first, rest)) => {
        let child =
            group_subtree.name_to_child
            .entry(*first)
            .or_insert_with(|| GroupSubtree {
                last_mut_effect: None,
                name_to_child: IndexMap::new(),
            });
        self.note_mut_effect(child, effect_loc, rest);
      }
    }
  }

  // fn invalidate_descendant_groups_of<'g>(
  //   &self,
  //   group_subtree: &mut GroupSubtree<'s, 't>,
  //   effect_range: RangeS<'s>
  // ) {
  //   // DON'T invalidate every local in this group. We don't invalidate references into this group,
  //   // we invalidate references into descendant groups.
  //
  //   // Recurse
  //   for (group_step, group_child_subtree) in &mut group_subtree.name_to_child {
  //     match group_step {
  //       // These aren't child groups, so keep looking for child groups in them...
  //       GroupStep::Member { .. } => self.invalidate_descendant_groups_of(group_child_subtree, effect_range),
  //       GroupStep::InlineElements => self.invalidate_descendant_groups_of(group_child_subtree, effect_range),
  //       // These are actually child groups, so deep invalidate them.
  //       GroupStep::ChildElements => self.deep_invalidate(group_child_subtree, effect_range),
  //       GroupStep::Variant { .. } => self.deep_invalidate(group_child_subtree, effect_range),
  //       // TODO: Not sure about these cases
  //       GroupStep::Rune(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_range),
  //       GroupStep::ParamAnonymousGroup(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_range),
  //       GroupStep::Local(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_range),
  //     }
  //   }
  // }

  // fn deep_invalidate<'g>(
  //   &self,
  //   group_subtree: &mut GroupSubtree<'s, 't>,
  //   effect_range: RangeS<'s>
  // ) {
  //   // Invalidate every local in this group
  //   for (local_key, local) in &mut group_subtree.locals {
  //     local.invalidated_by = Some(effect_range);
  //   }
  //   // Invalidate every local in every descendant group
  //   for (group_step, group_child_subtree) in &mut group_subtree.name_to_child {
  //     self.deep_invalidate(group_child_subtree, effect_range);
  //   }
  // }

  fn collect_kind_mentioned_group_templatas<'g>(
    &self,
    group_templatas: &mut Vec<GroupTemplataG<'s, 't, 'g>>,
    type_gt: KindGT<'s, 't, 'g>
  ) {
    match type_gt {
      KindGT::Never(_) => {}
      KindGT::Void(_) => {}
      KindGT::Int(_) => {}
      KindGT::Bool(_) => {}
      KindGT::Str(_) => {}
      KindGT::Float(_) => {}
      KindGT::USize(_) => {}
      KindGT::Struct(StructGT { id, template_args }) => {
        for template_arg in template_args.iter() {
          self.collect_templata_mentioned_group_templatas(group_templatas, *template_arg);
        }
      }
      KindGT::Interface(InterfaceGT { id, template_args }) => {
        for template_arg in template_args.iter() {
          self.collect_templata_mentioned_group_templatas(group_templatas, *template_arg);
        }
      }
      KindGT::StaticSizedArray(StaticSizedArrayGT { name, element_type }) => {
        self.collect_kind_mentioned_group_templatas(group_templatas, *element_type);
      }
      KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) => {
        self.collect_kind_mentioned_group_templatas(group_templatas, *element_type);
      }
      KindGT::KindPlaceholder(_) => {}
      KindGT::OverloadSet(_) => {}
      KindGT::BorrowRef(BorrowRefGT { inner, group }) => {
        self.collect_kind_mentioned_group_templatas(group_templatas, *inner);
        self.collect_templata_mentioned_group_templatas(group_templatas, ITemplataG::Group(*group));
      }
      KindGT::OwnRef(_) => {}
      KindGT::ShareRef(_) => {}
      KindGT::WeakRef(_) => {}
    }
  }

  fn collect_templata_mentioned_group_templatas<'g>(
    &self,
    group_templatas: &mut Vec<GroupTemplataG<'s, 't, 'g>>,
    templata_gt: ITemplataG<'s, 't, 'g>
  ) {
    match templata_gt {
      ITemplataG::Group(group) => {
        group_templatas.push(group);
      }
      ITemplataG::Kind(KindTemplataG { kind }) => {
        self.collect_kind_mentioned_group_templatas(group_templatas, kind);
      }
      ITemplataG::Placeholder(_) => {}
      ITemplataG::Integer(_) => {}
      ITemplataG::Boolean(_) => {}
      ITemplataG::String(_) => {}
      ITemplataG::Prototype(_) => {}
      ITemplataG::Isa(_) => {}
      ITemplataG::CoordList(_) => {}
      ITemplataG::RuntimeSizedArrayTemplate(_) => {}
      ITemplataG::StaticSizedArrayTemplate(_) => {}
      ITemplataG::Function(_) => {}
      ITemplataG::StructDefinition(_) => {}
      ITemplataG::InterfaceDefinition(_) => {}
      ITemplataG::ImplDefinition(_) => {}
      ITemplataG::ExternFunction(_) => {}
    }
  }

  fn lookup_group_subtree_inner<'g, 'x>(
    &self,
    subtree: &'x mut GroupSubtree<'s, 't>,
    remaining_path: &'g [GroupChildStepG<'s>]
  ) -> &'x mut GroupSubtree<'s, 't> {
    match remaining_path.first() {
      None => subtree,
      Some(first) => {
        let key =
            match first {
              GroupChildStepG::Member { member_name } => GroupStep::Member { member_name: *member_name },
              GroupChildStepG::ChildElements {} => GroupStep::ChildElements {},
              GroupChildStepG::InlineElements { .. } => GroupStep::InlineElements {},
              GroupChildStepG::Variant { variant_name } => GroupStep::Variant { variant_name: *variant_name }
            };
        let subroot =
            subtree.name_to_child
                .entry(key)
                .or_insert_with(|| GroupSubtree {
                  last_mut_effect: None,
                  name_to_child: IndexMap::new(),
                });
        self.lookup_group_subtree_inner(subroot, &remaining_path[1..])
      }
    }
  }

  fn check_kind_still_valid<'g>(
    &self,
    group_tree: &mut GroupSubtree<'s, 't>,
    use_range: RangeS<'s>,
    kind_g: KindGT<'s, 't, 'g>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let mut mentioned_group_templatas = Vec::new();
    self.collect_kind_mentioned_group_templatas(&mut mentioned_group_templatas, kind_g);
    for mentioned_group_templata in mentioned_group_templatas {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)
      self.check_templata_still_valid(
        group_tree, use_range, mentioned_group_templata)?;
    }
    Ok(())
  }

  fn check_templata_still_valid<'g>(
    &self,
    group_tree: &mut GroupSubtree<'s, 't>,
    use_range: RangeS<'s>,
    group_templata: GroupTemplataG<'s, 't, 'g>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    for mentioned_group in group_templata.group {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)

      let key =
          match mentioned_group.root {
            GroupRootG::Rune(rune) => GroupStep::Rune(rune),
            GroupRootG::ParamAnonymousGroup(_) => unimplemented!(),
            GroupRootG::Local(var_name) => GroupStep::Local(var_name),
            GroupRootG::AmbientMulti() => GroupStep::AmbientMulti { }
          };
      let subroot =
      group_tree.name_to_child
          .entry(key)
          .or_insert_with(|| GroupSubtree {
            last_mut_effect: None,
            name_to_child: IndexMap::new(),
          });
      self.check_target_group_invalidated_since(
        subroot, mentioned_group.steps, use_range, group_templata.born_at)?;
    }
    Ok(())
  }

  // This function visits each group down to the target group
  fn check_target_group_invalidated_since<'g>(
    &self,
    subtree: &mut GroupSubtree<'s, 't>,
    remaining_path: &'g [GroupChildStepG<'s>],
    use_range: RangeS<'s>,
    target_group_invalidated_since: LocT<'t>,
    // The bool is true iff the target group is an independent descendant of the current group.
  ) -> Result<bool, ICompileErrorT<'s, 't>> {
    match remaining_path.first() {
      None => Ok(false),
      Some(first) => {
        let (child_is_independent, group_step) =
            match first {
              GroupChildStepG::Member { member_name } => (false, GroupStep::Member { member_name: *member_name }),
              GroupChildStepG::ChildElements {} => (true, GroupStep::ChildElements {}),
              GroupChildStepG::InlineElements { .. } => (false, GroupStep::InlineElements {}),
              GroupChildStepG::Variant { variant_name } => (true, GroupStep::Variant { variant_name: *variant_name })
            };
        // TODO: we really need to get a better term than child group. "independent descendant group"?
        let child_tree =
        subtree.name_to_child
                .entry(group_step)
                .or_insert_with(|| GroupSubtree {
                  last_mut_effect: None,
                  name_to_child: IndexMap::new(),
                });
        // First, check if anyone has mutated anything closer to the target group.
        let target_is_independent_of_child =
            self.check_target_group_invalidated_since(
              child_tree, &remaining_path[1..], use_range, target_group_invalidated_since)?;
        let target_is_independent = child_is_independent || target_is_independent_of_child;

        // If we get here, then there was no problem closer to the target group.
        // Now let's check if our group was modified since then. If so, and the target group is a
        // child group compared to us, throw an error pointing at the argument's own location.

        if let Some(last_mut_effect_loc) = subtree.last_mut_effect {
          if last_mut_effect_loc.loct.path > target_group_invalidated_since.path {
            if target_is_independent {
              return Err(ICompileErrorT::BorrowCheckError {
                range: use_range,
                kind: BorrowErrorKind::UseAfterChurn { local: RefKey::Held(0), churned_at: last_mut_effect_loc.range }
              });
            }
          }
        }

        Ok(target_is_independent)
      }
    }
  }
}
