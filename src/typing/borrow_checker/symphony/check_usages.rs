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
    func_declared_mut_effects: Vec<GroupPathG<'s, 't, 'g>>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let mut group_tree =
        GroupSubtree {
          last_mut_effect: None,
          name_to_child: IndexMap::new(),
        };
    let mut next_held_num = 0;
    self.check_expr(
      coutputs,
      function_s,
      arena,
      &func_declared_mut_effects,
      &mut group_tree,
      function_g_body,
      &mut next_held_num)?;
    Ok(())
  }

  pub fn check_expr<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    arena: &'g Bump,
    func_declared_mut_effects: &Vec<GroupPathG<'s, 't, 'g>>,
    group_tree: &mut GroupSubtree<'s, 't>,
    expr: ExpressionGE<'s, 't, 'g>,
    next_held_num: &mut u32,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    match expr {
      ExpressionGE::Block(BlockGE { range, inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::LetNormal(LetNormalGE { range, variable, expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *expr, next_held_num)?;
        // self.insert_new_variable(group_tree, RefKey::Named(variable.name), variable.tyype);
      }
      ExpressionGE::LetAndLend(LetAndLendGE { range, loct, variable, expr, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *expr, next_held_num)?;
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::Return(ReturnGE { range, source_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *source_expr, next_held_num)?;
      }
      ExpressionGE::Consecutor(ConsecutorGE { range, exprs, result: result_tt, .. }) => {
        for expr in exprs.iter() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *expr, next_held_num)?;
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *source_te, next_held_num)?;
      }
      ExpressionGE::FunctionCall(FunctionCallGE { loct, range, callable, args, result, mut_effects, .. }) => {
        for arg in args.iter() {
          // Careful, this may cause some mut effects, that could invalidate other arguments.
          // We check the argument types again below, in case that happened.
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *arg, next_held_num)?;
        }
        // Check each argument again, just in case any of the arguments invalidated any of the
        // other args.
        for arg in args.iter() {
          self.check_kind_still_valid(group_tree, arg.range(), arg.result())?;
        }

        // Conceptually, the call happens here

        let mel = MutEffectLoc { loct: *loct, range: range[0] };

        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_tree, mel, mut_effect.steps);

          // If the effect affects a parameter, make sure it doesn't violate the function's declared
          // mut effects
          match mut_effect.steps[0] {
            GroupStep::Rune(rune) => {
              let allowed =
                  func_declared_mut_effects.iter().any(|d| churn_matches_func_declared_mut_effect(d, mut_effect.steps));
              if !allowed {
                return Err(ICompileErrorT::BorrowCheckError { range: range[0], kind: BorrowErrorKind::UndeclaredChurn });
              }
            }
            GroupStep::Local(_) => {} // we don't care about mutations to these
            GroupStep::AmbientMulti() => unimplemented!(),
            GroupStep::ParamAnonymousGroup(_) => unimplemented!(),
            // I think all the below should be impossible
            GroupStep::Member { .. } => panic!("Impossible effect root"),
            GroupStep::ChildElements => panic!("Impossible effect root"),
            GroupStep::InlineElements => panic!("Impossible effect root"),
            GroupStep::Variant { .. } => panic!("Impossible effect root"),
          }
        }
      }
      ExpressionGE::LockWeak(_) => unimplemented!(),
      ExpressionGE::BorrowToWeak(_) => unimplemented!(),
      ExpressionGE::If(IfGE { range, loct, condition: condition_ge, then_call: then_ge, else_call: else_ge, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *condition_ge, next_held_num)?;

        let mut group_tree_for_then = group_tree.clone();
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut group_tree_for_then, *then_ge, next_held_num)?;

        let mut group_tree_for_else = group_tree.clone();
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut group_tree_for_else, *else_ge, next_held_num)?;

        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(then_ge.result(), KindGT::Never(_)) {
          merge_invalidations_into_from(group_tree, group_tree_for_then);
        }
        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(else_ge.result(), KindGT::Never(_)) {
          merge_invalidations_into_from(group_tree, group_tree_for_else);
        }
      }
      ExpressionGE::While(WhileGE { range, loct, pre_iteration_loct, post_iteration_loct, block, result, mut_effects }) => {
        // Before the loop, note all the mut effects that happen inside the loop.
        let mel = MutEffectLoc { loct: *pre_iteration_loct, range: *range };
        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_tree, mel, mut_effect.steps);
        }

        // Now do the body. "Past iterations"'s mutations were noted above, so we'll correctly
        // detect invalidations inside the loop.
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, block.inner, next_held_num)?;

        // After the loop, note all the mut effects that happen inside the loop.
        let mel = MutEffectLoc { loct: *post_iteration_loct, range: *range };
        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_tree, mel, mut_effect.steps);
        }
      }
      ExpressionGE::Mutate(MutateGE { destination_expr, source_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *source_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *destination_expr, next_held_num)?;

        // TODO: issue mut effects for `set` statements
        // let dest_group_path = ...
        // self.note_mut_effect(group_tree, MutEffectLoc { loct, range }, dest_group_path);
      }
      ExpressionGE::Restackify(_) => unimplemented!(),
      ExpressionGE::Break(_) => {
        // Do nothing
      }
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *source_ge, next_held_num)?;
      }
      ExpressionGE::Construct(ConstructGE { args, .. }) => {
        for arg in args.iter() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *arg, next_held_num)?;
        }
      }
      ExpressionGE::NewRuntimeSizedArray(NewRuntimeSizedArrayGE { capacity_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *capacity_expr, next_held_num)?;
      }
      ExpressionGE::StaticArrayFromCallable(StaticArrayFromCallableGE { generator, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *generator, next_held_num)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoFunction(DestroyStaticSizedArrayIntoFunctionGE { array_expr, consumer, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *consumer, next_held_num)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(DestroyStaticSizedArrayIntoLocalsGE { expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *expr, next_held_num)?;
      }
      ExpressionGE::DestroyRuntimeSizedArray(DestroyRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
      }
      ExpressionGE::RuntimeSizedArrayCapacity(RuntimeSizedArrayCapacityGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
      }
      ExpressionGE::PushRuntimeSizedArray(PushRuntimeSizedArrayGE { array_expr, new_element_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *new_element_expr, next_held_num)?;
      }
      ExpressionGE::PopRuntimeSizedArray(PopRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
      }
      ExpressionGE::InterfaceToInterfaceUpcast(InterfaceToInterfaceUpcastGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner_expr, next_held_num)?;
      }
      ExpressionGE::UpcastInterface(UpcastInterfaceGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner_expr, next_held_num)?;
      }
      ExpressionGE::UpcastGeneric(UpcastGenericGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner_expr, next_held_num)?;
      }
      ExpressionGE::Destroy(DestroyGE { range, expr, struct_tt, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *expr, next_held_num)?;
      }
      ExpressionGE::CopyPrim(CopyPrimGE { range, loct, inner, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::StaticSizedArrayLookup(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupGE { range, array_expr, array_type, index_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *index_expr, next_held_num)?;
      }
      ExpressionGE::MemberLookup(MemberLookupGE { struct_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_tree, *struct_expr, next_held_num)?;
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
      KindGT::StaticSizedArray(StaticSizedArrayGT { name, size, element_type }) => {
        self.collect_templata_mentioned_group_templatas(group_templatas, *size);
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
            GroupRootG::AmbientMulti() => GroupStep::AmbientMulti {}
          };
      let subroot =
          group_tree.name_to_child
              .entry(key)
              .or_insert_with(|| GroupSubtree {
                last_mut_effect: None,
                name_to_child: IndexMap::new(),
              });
      match mentioned_group.ellipsis {
        false => {
          self.check_target_group_invalidated_since(
            subroot, mentioned_group.steps, use_range, group_templata.born_at)?;
        }
        true => {
          // Will detect any mutations to any ancestors
          self.check_target_group_invalidated_since(
            subroot, mentioned_group.steps, use_range, group_templata.born_at)?;
          // TODO: optimize, we're descending into the subtree twice
          // Now detect any mutations to the target itself, plus its descendant groups
          let subtree = self.lookup_group_subtree_inner(subroot, mentioned_group.steps);
          if let Some(last_mut_effect_loc) = subtree.last_mut_effect {
            if last_mut_effect_loc.loct.path > group_templata.born_at.path {
              return Err(ICompileErrorT::BorrowCheckError {
                range: use_range,
                kind: BorrowErrorKind::UseAfterChurn { local: RefKey::Held(0), churned_at: last_mut_effect_loc.range }
              });
            }
          }
          // TODO: look for any changes to any descendants
        }
      }
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

fn merge_invalidations_into_from<'s, 't>(
  into: &mut GroupSubtree<'s, 't>,
  from: GroupSubtree<'s, 't>,
) {
  match (into.last_mut_effect, from.last_mut_effect) {
    (Some(into_mel), Some(from_mel)) => {
      if from_mel.loct.path > into_mel.loct.path {
        into.last_mut_effect = Some(from_mel);
      }
    }
    (None, Some(from_mel)) => {
      into.last_mut_effect = Some(from_mel);
    }
    (Some(_), None) | (None, None) => {}
  }
  for (step, from_child) in from.name_to_child {
    let into_child =
        into.name_to_child
        .entry(step)
        .or_insert_with(|| GroupSubtree {
            last_mut_effect: None,
            name_to_child: IndexMap::new(),
        });
    merge_invalidations_into_from(into_child, from_child);
  }
}


fn churn_matches_func_declared_mut_effect<'s, 't, 'g>(
  declared: &GroupPathG<'s, 't, 'g>,
  churn_steps: &[GroupStep<'s, 't>],
) -> bool {
  match (declared.root, churn_steps[0]) {
    (GroupRootG::Rune(declared_rune), GroupStep::Rune(churn_rune)) => {
      if declared_rune != churn_rune {
        return false;
      }
    },
    (GroupRootG::Local(declared_local), _) => return false, // Func mut declarations cant start with local
    _ => unimplemented!(),
  }
  if churn_steps.len() < declared.steps.len() {
    return false;
  }
  let churn_children = &churn_steps[1..];
  let num_steps = churn_children.len().min(declared.steps.len());
  for i in 0..num_steps {
    let declared_step = declared.steps[i];
    let churn_step = churn_children[i];
    match (declared_step, churn_step) {
      (GroupChildStepG::Member { member_name: a }, GroupStep::Member { member_name: b }) => {
        if a != b {
          return false;
        }
      },
      (GroupChildStepG::ChildElements {}, GroupStep::ChildElements) => {} // continue
      (GroupChildStepG::InlineElements {}, GroupStep::InlineElements) => {} // continue
      (GroupChildStepG::Variant { variant_name: a }, GroupStep::Variant { variant_name: b }) => {
        if a != b {
          return false;
        }
      },
      _ => {
        return false;
      }
    }
  }
  true
}