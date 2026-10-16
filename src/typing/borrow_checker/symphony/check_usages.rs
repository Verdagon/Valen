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
    let mut group_forest =
        GroupForest {
          name_to_tree: IndexMap::new(),
        };
    let mut next_held_num = 0;
    self.check_expr(
      coutputs,
      function_s,
      arena,
      &func_declared_mut_effects,
      &mut group_forest,
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
    group_forest: &mut GroupForest<'s, 't>,
    expr: ExpressionGE<'s, 't, 'g>,
    next_held_num: &mut u32,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    match expr {
      ExpressionGE::Block(BlockGE { range, inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner, next_held_num)?;
      }
      ExpressionGE::LetNormal(LetNormalGE { range, variable, expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *expr, next_held_num)?;
        // self.insert_new_variable(group_tree, RefKey::Named(variable.name), variable.tyype);
      }
      ExpressionGE::LetAndLend(LetAndLendGE { range, loct, variable, expr, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *expr, next_held_num)?;
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner, next_held_num)?;
      }
      ExpressionGE::Return(ReturnGE { range, source_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *source_expr, next_held_num)?;
      }
      ExpressionGE::Consecutor(ConsecutorGE { range, exprs, result: result_tt, .. }) => {
        for expr in exprs.iter() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *expr, next_held_num)?;
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *source_te, next_held_num)?;
      }
      ExpressionGE::FunctionCall(FunctionCallGE { loct, range, callable, args, result, mut_effects, .. }) => {
        for arg in args.iter() {
          // Careful, this may cause some mut effects, that could invalidate other arguments.
          // We check the argument types again below, in case that happened.
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *arg, next_held_num)?;
        }
        // Check each argument again, just in case any of the arguments invalidated any of the
        // other args.
        for arg in args.iter() {
          self.check_kind_still_valid(group_forest, arg.range(), arg.result())?;
        }

        // Conceptually, the call happens here

        let mel = MutEffectLoc { loct: *loct, range: range[0] };

        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_forest, mel, mut_effect.path);

          // If the effect affects a parameter, make sure it doesn't violate the function's declared
          // mut effects
          match mut_effect.path.root {
            GroupRootG::Rune(rune) => {
              let allowed =
                  func_declared_mut_effects.iter().any(|d| churn_matches_func_declared_mut_effect(d, &mut_effect.path));
              if !allowed {
                return Err(ICompileErrorT::BorrowCheckError { range: range[0], kind: BorrowErrorKind::UndeclaredChurn });
              }
            }
            GroupRootG::Local(_) => {} // we don't care about mutations to these
            GroupRootG::AmbientMulti() => unimplemented!(),
            GroupRootG::ParamAnonymousGroup(_) => unimplemented!(),
          }
        }
      }
      ExpressionGE::LockWeak(_) => unimplemented!(),
      ExpressionGE::BorrowToWeak(_) => unimplemented!(),
      ExpressionGE::If(IfGE { range, loct, condition: condition_ge, then_call: then_ge, else_call: else_ge, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *condition_ge, next_held_num)?;

        // TODO(optimize): possibly expensive clones here. i wonder if an arena would do well...

        let mut group_tree_for_then = group_forest.clone();
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut group_tree_for_then, *then_ge, next_held_num)?;

        let mut group_tree_for_else = group_forest.clone();
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut group_tree_for_else, *else_ge, next_held_num)?;

        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(then_ge.result(), KindGT::Never(_)) {
          merge_invalidations_into_from(group_forest, group_tree_for_then);
        }
        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(else_ge.result(), KindGT::Never(_)) {
          merge_invalidations_into_from(group_forest, group_tree_for_else);
        }
      }
      ExpressionGE::While(WhileGE { range, loct, pre_iteration_loct, post_iteration_loct, block, result, mut_effects }) => {
        // Before the loop, note all the mut effects that happen inside the loop.
        let mel = MutEffectLoc { loct: *pre_iteration_loct, range: *range };
        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_forest, mel, mut_effect.path);
        }

        // Now do the body. "Past iterations"'s mutations were noted above, so we'll correctly
        // detect invalidations inside the loop.
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, block.inner, next_held_num)?;

        // After the loop, note all the mut effects that happen inside the loop.
        let mel = MutEffectLoc { loct: *post_iteration_loct, range: *range };
        for mut_effect in mut_effects.iter() {
          self.note_mut_effect(group_forest, mel, mut_effect.path);
        }
      }
      ExpressionGE::Mutate(MutateGE { range, loct, destination_expr, source_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *source_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *destination_expr, next_held_num)?;

        match destination_expr.result() {
          KindGT::BorrowRef(BorrowRefGT { inner, group }) => {
            for path in group.group {
              self.note_mut_effect(group_forest, MutEffectLoc { loct: *loct, range: *range }, *path);
            }
          }
          _ => unimplemented!(),
        }
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
      ExpressionGE::ConstantRustStr(_) => {
        // Do nothing
      }
      ExpressionGE::InterfaceFunctionCall(_) => unimplemented!(),
      ExpressionGE::ExternFunctionCall(_) => unimplemented!(),
      ExpressionGE::BoundFunctionCall(_) => unimplemented!(),
      ExpressionGE::Reinterpret(ReinterpretGE { range, expr: source_ge, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *source_ge, next_held_num)?;
      }
      ExpressionGE::Construct(ConstructGE { args, .. }) => {
        for arg in args.iter() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *arg, next_held_num)?;
        }
      }
      ExpressionGE::NewRuntimeSizedArray(NewRuntimeSizedArrayGE { capacity_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *capacity_expr, next_held_num)?;
      }
      ExpressionGE::StaticArrayFromCallable(StaticArrayFromCallableGE { generator, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *generator, next_held_num)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoFunction(DestroyStaticSizedArrayIntoFunctionGE { array_expr, consumer, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *consumer, next_held_num)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(DestroyStaticSizedArrayIntoLocalsGE { expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *expr, next_held_num)?;
      }
      ExpressionGE::DestroyRuntimeSizedArray(DestroyRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
      }
      ExpressionGE::RuntimeSizedArrayCapacity(RuntimeSizedArrayCapacityGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
      }
      ExpressionGE::PushRuntimeSizedArray(PushRuntimeSizedArrayGE { array_expr, new_element_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *new_element_expr, next_held_num)?;
      }
      ExpressionGE::PopRuntimeSizedArray(PopRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
      }
      ExpressionGE::InterfaceToInterfaceUpcast(InterfaceToInterfaceUpcastGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner_expr, next_held_num)?;
      }
      ExpressionGE::UpcastInterface(UpcastInterfaceGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner_expr, next_held_num)?;
      }
      ExpressionGE::UpcastGeneric(UpcastGenericGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner_expr, next_held_num)?;
      }
      ExpressionGE::Destroy(DestroyGE { range, expr, struct_tt, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *expr, next_held_num)?;
      }
      ExpressionGE::CopyPrim(CopyPrimGE { range, loct, inner, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *inner, next_held_num)?;
      }
      ExpressionGE::StaticSizedArrayLookup(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupGE { range, array_expr, array_type, index_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *index_expr, next_held_num)?;

        // Check the array is still valid
        self.check_kind_still_valid(group_forest, array_expr.range(), array_expr.result())?;
      }
      ExpressionGE::MemberLookup(MemberLookupGE { struct_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, group_forest, *struct_expr, next_held_num)?;

        // Check the struct is still valid
        self.check_kind_still_valid(group_forest, struct_expr.range(), struct_expr.result())?;
      }
    }
    Ok(())
  }

  fn note_mut_effect<'g>(
    &self,
    group_subtree: &mut GroupForest<'s, 't>,
    effect_loc: MutEffectLoc<'s, 't>,
    path: GroupPathG<'s, 't, 'g>,
  ) {
    let child =
        group_subtree.name_to_tree
            .entry(path.root)
            .or_insert_with(|| GroupTree {
              last_mut_effect: None,
              name_to_child: IndexMap::new(),
            });
      self.note_mut_effect_inner(child, effect_loc, path.steps);
  }

  fn note_mut_effect_inner<'g>(
    &self,
    group_subtree: &mut GroupTree<'s, 't>,
    effect_loc: MutEffectLoc<'s, 't>,
    remaining_steps: &'g [GroupChildStepG<'s>],
  ) {
    if remaining_steps.len() == 0 {
      group_subtree.last_mut_effect = Some(effect_loc);
    } else {
      let next_step = remaining_steps[0];
      let child =
      group_subtree.name_to_child
          .entry(next_step)
          .or_insert_with(|| GroupTree {
            last_mut_effect: None,
            name_to_child: IndexMap::new(),
          });
      self.note_mut_effect_inner(child, effect_loc, &remaining_steps[1..]);
    }
  }

  // fn invalidate_descendant_groups_of<'g>(
  //   &self,
  //   group_subtree: &mut GroupForest<'s, 't>,
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
  //   group_subtree: &mut GroupForest<'s, 't>,
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

  fn lookup_group_subtree<'g, 'x>(
    &self,
    forest: &'x mut GroupForest<'s, 't>,
    path: GroupPathG<'s, 't, 'g>,
  ) -> &'x mut GroupTree<'s, 't> {
    let tree =
        forest.name_to_tree
            .entry(path.root)
            .or_insert_with(|| GroupTree {
              last_mut_effect: None,
              name_to_child: IndexMap::new(),
            });
    self.lookup_group_subtree_inner(tree, &path.steps)
  }

  fn lookup_group_subtree_inner<'g, 'x>(
    &self,
    subtree: &'x mut GroupTree<'s, 't>,
    remaining_path: &'g [GroupChildStepG<'s>]
  ) -> &'x mut GroupTree<'s, 't> {
    if remaining_path.len() == 0 {
      subtree
    } else {
      let subroot =
          subtree.name_to_child
              .entry(remaining_path[0])
              .or_insert_with(|| GroupTree {
                last_mut_effect: None,
                name_to_child: IndexMap::new(),
              });
      self.lookup_group_subtree_inner(subroot, &remaining_path[1..])
    }
  }

  fn check_kind_still_valid<'g>(
    &self,
    group_forest: &mut GroupForest<'s, 't>,
    use_range: RangeS<'s>,
    kind_g: KindGT<'s, 't, 'g>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let mut mentioned_group_templatas = Vec::new();
    self.collect_kind_mentioned_group_templatas(&mut mentioned_group_templatas, kind_g);
    for mentioned_group_templata in mentioned_group_templatas {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)
      self.check_templata_still_valid(
        group_forest, use_range, mentioned_group_templata)?;
    }
    Ok(())
  }

  fn check_templata_still_valid<'g>(
    &self,
    group_forest: &mut GroupForest<'s, 't>,
    use_range: RangeS<'s>,
    group_templata: GroupTemplataG<'s, 't, 'g>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    for mentioned_group in group_templata.group {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)

      // let subroot =
      //     group_forest.name_to_tree
      //         .entry(mentioned_group.root)
      //         .or_insert_with(|| GroupTree {
      //           last_mut_effect: None,
      //           name_to_child: IndexMap::new(),
      //         });
      match mentioned_group.ellipsis {
        false => {
          self.check_target_group_invalidated_since(
            group_forest, *mentioned_group, use_range, group_templata.born_at)?;
        }
        true => {
          // Will detect any mutations to any ancestors
          self.check_target_group_invalidated_since(
            group_forest, *mentioned_group, use_range, group_templata.born_at)?;
          // TODO: optimize, we're descending into the subtree twice
          // Now detect any mutations to the target itself, plus its descendant groups
          let subtree = self.lookup_group_subtree(group_forest, *mentioned_group);
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
    forest: &mut GroupForest<'s, 't>,
    path: GroupPathG<'s, 't, 'g>,
    use_range: RangeS<'s>,
    target_group_invalidated_since: LocT<'t>,
    // The bool is true iff the target group is an independent descendant of the current group.
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    // TODO: we really need to get a better term than child group. "independent descendant group"?
    let child_tree =
        forest.name_to_tree
            .entry(path.root)
            .or_insert_with(|| GroupTree {
              last_mut_effect: None,
              name_to_child: IndexMap::new(),
            });
    let _ =
        self.check_target_group_invalidated_since_inner(
          child_tree, &path.steps, use_range, target_group_invalidated_since)?;
    Ok(())
  }

  // This function visits each group down to the target group
  fn check_target_group_invalidated_since_inner<'g>(
    &self,
    subtree: &mut GroupTree<'s, 't>,
    remaining_path: &'g [GroupChildStepG<'s>],
    use_range: RangeS<'s>,
    target_group_invalidated_since: LocT<'t>,
    // The bool is true iff the target group is an independent descendant of the current group.
  ) -> Result<bool, ICompileErrorT<'s, 't>> {
    match remaining_path.first() {
      None => Ok(false),
      Some(first) => {
        let (child_is_independent) =
            match first {
              GroupChildStepG::Member { member_name } => false,
              GroupChildStepG::ChildElements {} => true,
              GroupChildStepG::InlineElements { .. } => false,
              GroupChildStepG::Variant { variant_name } => true,
            };

        // TODO: we really need to get a better term than child group. "independent descendant group"?
        let child_tree =
            subtree.name_to_child
                .entry(*first)
                .or_insert_with(|| GroupTree {
                  last_mut_effect: None,
                  name_to_child: IndexMap::new(),
                });
        // First, check if anyone has mutated anything closer to the target group.
        let target_is_independent_of_child =
            self.check_target_group_invalidated_since_inner(
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
  into: &mut GroupForest<'s, 't>,
  from: GroupForest<'s, 't>,
) {
  for (step, from_child) in from.name_to_tree {
    let into_child =
        into.name_to_tree
        .entry(step)
        .or_insert_with(|| GroupTree {
            last_mut_effect: None,
            name_to_child: IndexMap::new(),
        });
    merge_invalidations_into_from_inner(into_child, from_child);
  }
}

fn merge_invalidations_into_from_inner<'s, 't>(
  into: &mut GroupTree<'s, 't>,
  from: GroupTree<'s, 't>,
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
            .or_insert_with(|| GroupTree {
              last_mut_effect: None,
              name_to_child: IndexMap::new(),
            });
    merge_invalidations_into_from_inner(into_child, from_child);
  }
}


fn churn_matches_func_declared_mut_effect<'s, 't, 'g>(
  declared: &GroupPathG<'s, 't, 'g>,
  churn_path: &GroupPathG<'s, 't, 'g>,
) -> bool {
  match (declared.root, churn_path.root) {
    (GroupRootG::Rune(declared_rune), GroupRootG::Rune(churn_rune)) => {
      if declared_rune != churn_rune {
        return false;
      }
    },
    (GroupRootG::Local(declared_local), _) => return false, // Func mut declarations cant start with local
    _ => unimplemented!(),
  }
  if churn_path.steps.len() < declared.steps.len() {
    return false;
  }
  let num_steps = churn_path.steps.len().min(declared.steps.len());
  for i in 0..num_steps {
    let declared_step = declared.steps[i];
    let churn_step = churn_path.steps[i];
    match (declared_step, churn_step) {
      (GroupChildStepG::Member { member_name: a }, GroupChildStepG::Member { member_name: b }) => {
        if a != b {
          return false;
        }
      },
      (GroupChildStepG::ChildElements {}, GroupChildStepG::ChildElements {}) => {} // continue
      (GroupChildStepG::InlineElements {}, GroupChildStepG::InlineElements {}) => {} // continue
      (GroupChildStepG::Variant { variant_name: a }, GroupChildStepG::Variant { variant_name: b }) => {
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
