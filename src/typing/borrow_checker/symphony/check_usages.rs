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
use crate::typing::names::names::IVarNameT;
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
    let mut pending_uses = PendingUseList { map: IndexMap::new() };
    let mut next_held_num = 0;
    let mut churned_after_this_in_current_loop = Vec::new();
    let mut revived_in_current_loop = Vec::new();
    self.check_expr(
      coutputs,
      function_s,
      arena,
      &func_declared_mut_effects,
      &mut pending_uses,
      function_g_body,
      &mut next_held_num,
      &mut churned_after_this_in_current_loop,
      &mut revived_in_current_loop,
    )?;
    Ok(())
  }

  pub fn check_expr<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    arena: &'g Bump,
    func_declared_mut_effects: &Vec<GroupPathG<'s, 't, 'g>>,
    pending_uses: &mut PendingUseList<'s, 't, 'g>,
    expr: ExpressionGE<'s, 't, 'g>,
    next_held_num: &mut u32,
    // As we encounter churns inside the loop, we add them to this list.
    // Processing the start of a loop needs this information.
    churned_after_this_in_current_loop: &mut Vec<(GroupPathG<'s, 't, 'g>, RangeS<'s>)>,
    // As we (going backwards) encounter restackifies/mutates, and they're not inside
    // churned_after_this, we put them in this list, to note that they'll be alive at the end of the
    // loop.
    // Processing the start of a loop needs this information.
    // We never remove from this. Once something is added, we *know* it'll be live at loop end.
    revived_in_current_loop: &mut Vec<RefKey<'s, 't>>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    match expr {
      ExpressionGE::Block(BlockGE { range, inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::LetNormal(LetNormalGE { range, variable, expr, result, .. }) => {
        let _old_value = pending_uses.map.swap_remove(&RefKey::Named(variable.name));
        // Note, we dont do this: assert!(_old_value.is_some());
        // because we might not ever have used it, therefore it might not be in the map.

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::Restackify(RestackifyGE { range, variable, source_expr, result }) => {
        let _old_value = pending_uses.map.swap_remove(&RefKey::Named(variable.name));
        // Note, we dont do this: assert!(_old_value.is_some());
        // because we might not ever have used it, therefore it might not be in the map.

        self.revive(churned_after_this_in_current_loop, revived_in_current_loop, &variable.name, source_expr);

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *source_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::LetAndLend(LetAndLendGE { range, loct, variable, expr, result }) => {
        let _old_value = pending_uses.map.swap_remove(&RefKey::Named(variable.name));
        // Note, we dont do this: assert!(_old_value.is_some());
        // because we might not ever have used it, therefore it might not be in the map.

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::LocalLookup(LocalLookupGE { range, local_variable, result, .. }) => {
        self.note_pending_use(pending_uses, RefKey::Named(local_variable.name), *range, KindGT::BorrowRef(result));
      }
      ExpressionGE::Unlet(UnletGE { range, variable: local_variable, result, .. }) => {
        // Nothing needed
      }
      ExpressionGE::Discard(DiscardGE { range, expr: inner, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::Return(ReturnGE { range, source_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *source_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::Consecutor(ConsecutorGE { range, exprs, result: result_tt, .. }) => {
        for expr in exprs.iter().rev() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *source_te, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::FunctionCall(FunctionCallGE { loct, range, callable, args, result, mut_effects, .. }) => {
        // (Going backwards, remember)

        // Result is born here

        // Now the call happens, so handle the mut effects
        for path in mut_effects.iter() {
          churned_after_this_in_current_loop.push((*path, range[0]));
        }
        let mel = MutEffectLoc { loct: *loct, range: range[0] };
        self.check_mut_effect(func_declared_mut_effects, pending_uses, mel, mut_effects)?;

        // Now handle the arguments.
        // Register the args, so that they're all pending uses, so that the next loop can check
        // that they don't invalidate them while they're Held.
        let mut held_keys = Vec::new();
        for arg_ge in args.iter().rev() {
          let new_held_key = RefKey::Held(*next_held_num);
          *next_held_num += 1;
          self.note_pending_use(pending_uses, new_held_key, arg_ge.range(), arg_ge.result());
          held_keys.push(new_held_key);
        }
        // We've set our expectations of the past.
        // Now we go through and "create" the held registers (which for us means unregistering their
        // expectations).
        for (arg_ge, held_key) in args.iter().rev().zip(held_keys.iter()) {
          let old_value = pending_uses.map.swap_remove(held_key).unwrap();
          // Careful, this may cause some mut effects, that could invalidate other arguments.
          // We check the argument types again below, in case that happened.
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *arg_ge, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        }
      }
      ExpressionGE::LockWeak(_) => unimplemented!(),
      ExpressionGE::BorrowToWeak(_) => unimplemented!(),
      ExpressionGE::If(IfGE { range, loct, condition: condition_ge, then_call: then_ge, else_call: else_ge, result }) => {
        // TODO(optimize): possibly expensive clones here. i wonder if an arena would do well...
        let mut pending_uses_for_then = pending_uses.clone();
        let mut pending_uses_for_else = pending_uses.clone();

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut pending_uses_for_else, *else_ge, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, &mut pending_uses_for_then, *then_ge, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;

        // Now we're before the if, so let's merge the pending uses into the main one.

        // TODO(optimize): we can do this without clearing, and without an above clone
        pending_uses.map.clear();

        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(then_ge.result(), KindGT::Never(_)) {
          self.merge_pending_uses_into(pending_uses, pending_uses_for_then);
        }
        // Can be typed Never if a break or a return exited out of the branch, in which case,
        // don't incorporate its invalidations
        if !matches!(else_ge.result(), KindGT::Never(_)) {
          self.merge_pending_uses_into(pending_uses, pending_uses_for_else);
        }

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *condition_ge, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::While(WhileGE { range, loct, pre_iteration_loct, post_iteration_loct, block, result }) => {
        let mut churned_in_this_loop = Vec::new();
        let mut revived_in_this_loop = Vec::new();

        // Now do the body. "Past iterations"'s mutations were noted above, so we'll correctly
        // detect invalidations inside the loop.
        self.check_expr(
            coutputs,
            function_s,
            arena,
            func_declared_mut_effects,
            pending_uses,
            block.inner,
            next_held_num,
            // This *isnt* churned_after_this_in_current_loop.
            // This is just for *this* loop.
            &mut churned_in_this_loop,
            // This *isnt* revived_in_current_loop.
            // This is just for *this* loop.
            &mut revived_in_this_loop)?;

        // Now we're at the beginning of the loop. We need to check for cases like this:
        //     e = &arr[0];
        //     while (cond) {
        //       observe(e);   // <-- This is invalid...
        //       mutate(&arr); // ...because this churned e "in the previous iteration".
        //     }
        // Of course, we don't do any checking on that `observe(e)` line. Instead we added an entry
        // to `pending_uses`.
        // That entry is an expectation of two things:
        //  * `e` will be live *before* the loop.
        //  * `e` will be live *at the end* of the loop.
        // The former will be checked later like normal, we don't care about that.
        // We care about the latter. So how do we answer that?
        // We can use `revived_in_this_loop` and `churned_in_this_loop`.
        // If it's in *neither*, then we're good.
        // If it's in *both*, then we're good.
        // If it's only in `churned_in_this_loop`, then it's an error.
        for (ref_key, pending_use) in &pending_uses.map {
          for (churn_path, churn_range) in churned_in_this_loop.iter() {
            if self.mut_effect_invalidates_type(&[*churn_path], pending_use.kind) {
              if revived_in_this_loop.contains(ref_key) {
                // It was revived, so don't worry about it.
              } else {
                // It's a pending use, that was churned in this loop, and not revived.
                // Give an error.
                return Err(ICompileErrorT::BorrowCheckError {
                  range: pending_use.earliest_use_range,
                  kind: BorrowErrorKind::UseAfterChurn { local: *ref_key, churned_at: *churn_range },
                })
              }
            }
          }
        }
      }
      ExpressionGE::Mutate(MutateGE { range, loct, destination_expr, source_expr, .. }) => {
        let (dest_group, dest_inner_gt) =
            match destination_expr.result() {
              KindGT::BorrowRef(BorrowRefGT { inner, group }) => (group, inner),
              _ => unimplemented!(),
            };

        match dest_group.group {
          [GroupPathG { root: GroupRootG::Local(local_name), steps: [], ellipsis: false }] => {
            // If we're setting a local, then mark it revived.
            self.revive(churned_after_this_in_current_loop, revived_in_current_loop, local_name, source_expr);
            // Don't mark destination churned, only unlet does that.
          }
          _ => {
            self.check_mut_effect(func_declared_mut_effects, pending_uses, MutEffectLoc { loct: *loct, range: *range }, dest_group.group)?;
            for path in dest_group.group {
              churned_after_this_in_current_loop.push((*path, *range));
            }
            self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *destination_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
          }
        }

        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *source_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
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
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *source_ge, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::Construct(ConstructGE { args, .. }) => {
        for arg in args.iter().rev() {
          self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *arg, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        }
      }
      ExpressionGE::NewRuntimeSizedArray(NewRuntimeSizedArrayGE { capacity_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *capacity_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::StaticArrayFromCallable(StaticArrayFromCallableGE { generator, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *generator, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoFunction(DestroyStaticSizedArrayIntoFunctionGE { array_expr, consumer, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *consumer, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(DestroyStaticSizedArrayIntoLocalsGE { expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::DestroyRuntimeSizedArray(DestroyRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::RuntimeSizedArrayCapacity(RuntimeSizedArrayCapacityGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::PushRuntimeSizedArray(PushRuntimeSizedArrayGE { array_expr, new_element_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *new_element_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::PopRuntimeSizedArray(PopRuntimeSizedArrayGE { array_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::InterfaceToInterfaceUpcast(InterfaceToInterfaceUpcastGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::UpcastInterface(UpcastInterfaceGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::UpcastGeneric(UpcastGenericGE { inner_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::Destroy(DestroyGE { range, expr, struct_tt, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::CopyPrim(CopyPrimGE { range, loct, inner, result }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *inner, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::StaticSizedArrayLookup(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupGE { range, array_expr, array_type, index_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *index_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *array_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
      ExpressionGE::MemberLookup(MemberLookupGE { struct_expr, .. }) => {
        self.check_expr(coutputs, function_s, arena, func_declared_mut_effects, pending_uses, *struct_expr, next_held_num, churned_after_this_in_current_loop, revived_in_current_loop)?;
      }
    }
    Ok(())
  }

  fn revive<'g>(
    &self,
    churned_after_this_in_current_loop: &mut Vec<(GroupPathG<'s, 't, 'g>, RangeS<'s>)>,
    revived_in_current_loop: &mut Vec<RefKey<'s, 't>>,
    local_name: &IVarNameT<'s, 't>,
    source_expr: &ExpressionGE<'s, 't, 'g>
  ) {
    let mut invalidated_by_later_churn = false;
    for (churn, churn_range) in churned_after_this_in_current_loop.iter() {
      if self.mut_effect_invalidates_type(&[*churn], source_expr.result()) {
        invalidated_by_later_churn = true;
        break;
      }
    }
    if !invalidated_by_later_churn {
      revived_in_current_loop.push(RefKey::Named(*local_name));
    }
  }

  fn check_mut_effect<'g>(
    &self,
    func_declared_mut_effects: &Vec<GroupPathG<'s, 't, 'g>>,
    pending_uses: &PendingUseList<'s, 't, 'g>,
    effect_loc: MutEffectLoc<'s, 't>,
    mut_effect_paths: GroupExprG<'s, 't, 'g>,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    for (ref_key, pending_use) in &pending_uses.map {
      if self.mut_effect_invalidates_type(mut_effect_paths, pending_use.kind) {
        return Err(ICompileErrorT::BorrowCheckError {
          range: pending_use.earliest_use_range,
          kind: BorrowErrorKind::UseAfterChurn { local: *ref_key, churned_at: effect_loc.range },
        })
      }
    }

    // Also make sure that the current function's declared mut effects contain this mut path.
    for churn_path in mut_effect_paths.iter() {
      match churn_path.root {
        // Skip any changes to locals
        GroupRootG::Local(_) => continue,
        GroupRootG::Rune(_) => {}
        GroupRootG::ParamAnonymousGroup(_) => {}
        GroupRootG::AmbientMulti() => {}
      }
      let mut mentioned_in_func_decl = false;
      for func_declared_mut_effect in func_declared_mut_effects {
        if func_declared_mut_path_contains_churn_path(func_declared_mut_effect, churn_path) {
          mentioned_in_func_decl = true;
        }
      }
      if !mentioned_in_func_decl {
        return Err(ICompileErrorT::BorrowCheckError {
          range: effect_loc.range,
          kind: BorrowErrorKind::UndeclaredChurn,
        });
      }
    }

    Ok(())
  }

  fn mut_effect_invalidates_type<'g>(
    &self,
    mut_effect_paths: &[GroupPathG<'s, 't, 'g>],
    kind_gt: KindGT<'s, 't, 'g>,
  ) -> bool {
    match kind_gt {
      KindGT::Never(_) | KindGT::Void(_) | KindGT::Int(_) | KindGT::Bool(_) | KindGT::Str(_) | KindGT::Float(_) | KindGT::USize(_) | KindGT::OverloadSet(_) => {
      }
      KindGT::BorrowRef(BorrowRefGT { inner, group }) => {
        if self.mut_effect_invalidates_used_paths(mut_effect_paths, group.group) {
          return true;
        }
        if self.mut_effect_invalidates_type(mut_effect_paths, *inner) {
          return true;
        }
      }
      KindGT::OwnRef(_) => unimplemented!(),
      KindGT::ShareRef(_) => unimplemented!(),
      KindGT::WeakRef(_) => unimplemented!(),
      KindGT::Struct(StructGT { id, template_args, .. }) => {
        for templata in template_args.iter() {
          if self.mut_effect_invalidates_templata(mut_effect_paths, *templata) {
            return true;
          }
        }
      }
      KindGT::Interface(InterfaceGT { id, template_args, .. }) => {
        for templata in template_args.iter() {
          if self.mut_effect_invalidates_templata(mut_effect_paths, *templata) {
            return true;
          }
        }
      }
      KindGT::StaticSizedArray(StaticSizedArrayGT { name, size, element_type }) => {
        if self.mut_effect_invalidates_type(mut_effect_paths, *element_type) {
          return true;
        }
        if self.mut_effect_invalidates_templata(mut_effect_paths, *size) {
          return true;
        }
      }
      KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) => {
        if self.mut_effect_invalidates_type(mut_effect_paths, *element_type) {
          return true;
        }
      }
      KindGT::KindPlaceholder(_) => {}
    }
    false
  }
  fn mut_effect_invalidates_templata<'g>(
    &self,
    mut_effect_paths: &[GroupPathG<'s, 't, 'g>],
    templata_gt: ITemplataG<'s, 't, 'g>,
  ) -> bool {
    match templata_gt {
      ITemplataG::Group(GroupTemplataG { group, kind, .. }) => {
        if self.mut_effect_invalidates_used_paths(mut_effect_paths, group) {
          return true;
        }
      }
      ITemplataG::Kind(KindTemplataG { kind }) => {
        if self.mut_effect_invalidates_type(mut_effect_paths, kind) {
          return true;
        }
      }
      ITemplataG::Placeholder(_) => {}
      ITemplataG::Integer(_) => unimplemented!(),
      ITemplataG::Boolean(_) => unimplemented!(),
      ITemplataG::String(_) => unimplemented!(),
      ITemplataG::Prototype(_) => unimplemented!(),
      ITemplataG::Isa(_) => unimplemented!(),
      ITemplataG::CoordList(_) => unimplemented!(),
      ITemplataG::RuntimeSizedArrayTemplate(_) => unimplemented!(),
      ITemplataG::StaticSizedArrayTemplate(_) => unimplemented!(),
      ITemplataG::Function(_) => unimplemented!(),
      ITemplataG::StructDefinition(_) => unimplemented!(),
      ITemplataG::InterfaceDefinition(_) => unimplemented!(),
      ITemplataG::ImplDefinition(_) => unimplemented!(),
      ITemplataG::ExternFunction(_) => unimplemented!(),
    }
    false
  }

  fn mut_effect_invalidates_used_paths<'g>(
    &self,
    churn_paths: &[GroupPathG<'s, 't, 'g>],
    used_paths: GroupExprG<'s, 't, 'g>,
  ) -> bool {
    for churn_path in churn_paths {
      for used_path in used_paths {
        if used_path.ellipsis {
          // If a reference is pointing at a thing "or anywhere inside", then it's invalidated
          // by any mutation above or below it.
          if path_is_prefix_of_path(churn_path, used_path) ||
              path_is_prefix_of_path(used_path, churn_path) {
            return true;
          }
        } else {
          // "Are we churning the used path (or something above it)?"
          if path_is_prefix_of_path(churn_path, used_path) &&
              // "Are we're churning something above it?"
              used_path.steps.len() > churn_path.steps.len() &&
              // "Is used_path is an independent descendant group of the churned path?"
              used_path.steps[churn_path.steps.len()..].iter().any(|step| {
                match step {
                  GroupChildStepG::Member { .. } => false,
                  GroupChildStepG::ChildElements { .. } => true,
                  GroupChildStepG::InlineElements { .. } => false,
                  GroupChildStepG::Variant { .. } => true,
                }
              }) {
            // Then yes, this churn path just invalidated the use path.
            return true;
          }
        }
      }
    }
    false
  }

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

  fn note_pending_use<'g>(
    &self,
    pending_uses: &mut PendingUseList<'s, 't, 'g>,
    ref_key: RefKey<'s, 't>,
    use_range: RangeS<'s>,
    kind: KindGT<'s, 't, 'g>
  ) {
    let entry =
        pending_uses.map
            .entry(ref_key)
            .or_insert_with(|| {
              PendingUse { earliest_use_range: use_range, kind: kind, }
            });
    if use_range.begin.offset < entry.earliest_use_range.begin.offset {
      entry.earliest_use_range = use_range;
    }
  }

  fn merge_pending_uses_into<'g>(
    &self,
    into: &mut PendingUseList<'s, 't, 'g>,
    from: PendingUseList<'s, 't, 'g>,
  ) {
    for (ref_key, pending_use) in from.map {
      self.note_pending_use(into, ref_key, pending_use.earliest_use_range, pending_use.kind);
    }
  }
}

// This checks whether the given churn is declared in the function.
// In other words, it checks whether the function's declared mut effects contain this churn path.
// It returns true if declared *contains* churn_path.
fn func_declared_mut_path_contains_churn_path<'s, 't, 'g>(
  declared: &GroupPathG<'s, 't, 'g>,
  churn_path: &GroupPathG<'s, 't, 'g>,
) -> bool {
  path_is_prefix_of_path(declared, churn_path)
}

// Returns true if prefix_path is in the front of longer_path.
// Also returns true if they're equal.
fn path_is_prefix_of_path<'s, 't, 'g>(
  prefix_path: &GroupPathG<'s, 't, 'g>,
  longer_path: &GroupPathG<'s, 't, 'g>,
) -> bool {
  match (prefix_path.root, longer_path.root) {
    (GroupRootG::Rune(prefix_root_rune), GroupRootG::Rune(longer_root_rune)) => {
      if prefix_root_rune != longer_root_rune {
        return false;
      }
    },
    (GroupRootG::Local(prefix_root_local), GroupRootG::Local(longer_root_local)) => {
      if prefix_root_local != longer_root_local {
        return false;
      }
    }
    (GroupRootG::ParamAnonymousGroup(prefix_root_local), GroupRootG::ParamAnonymousGroup(longer_root_local)) => {
      if prefix_root_local != longer_root_local {
        return false;
      }
    },
    (GroupRootG::AmbientMulti(), GroupRootG::AmbientMulti()) => unimplemented!(),
    _ => return false,
  }
  if longer_path.steps.len() < prefix_path.steps.len() {
    return false;
  }
  let num_steps = longer_path.steps.len().min(prefix_path.steps.len());
  for i in 0..num_steps {
    let churn_path_step = prefix_path.steps[i];
    let churn_step = longer_path.steps[i];
    match (churn_path_step, churn_step) {
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
