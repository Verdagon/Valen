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
use crate::typing::borrow_checker::check_usages_types::{GroupSubtree, LocalEntry, RefKey};
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
          locals: IndexMap::new(),
          locals_in_ellipsis: IndexMap::new(),
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
        self.insert_new_variable(group_tree, RefKey::Named(variable.name), variable.tyype);
      }
      ExpressionGE::LocalLookup(LocalLookupGE { range, local_variable, result, .. }) => {
        self.check_variable_still_valid(group_tree, *range, RefKey::Named(local_variable.name), local_variable.tyype)?;

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
        unimplemented!()
      }
      ExpressionGE::ConstantFloat(ConstantFloatGE { range, value, .. }) => {
        unimplemented!()
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

        let mut held_keys = Vec::new();
        for arg in args.iter() {
          // Check the expr that produces the argument
          self.check_expr(coutputs, function_s, arena, group_tree, *arg, next_held_num)?;

          // Insert it as a held variable, *before* checking the remaining args.
          let held_key = RefKey::Held(*next_held_num);
          *next_held_num += 1;
          self.insert_new_variable(group_tree, held_key, arg.result());
          held_keys.push(held_key);
        }

        for (held_key, arg_gt) in held_keys.iter().zip(args.iter()) {
          self.check_variable_still_valid(group_tree, *range.iter().last().unwrap(), *held_key, arg_gt.result())?;
        }

        // Conceptually, the call happens here

        // Process the calls' effects
        for mut_effect in mut_effects.iter() {
          self.churn(group_tree, mut_effect.effecting_node_loc, mut_effect.steps);
        }
      }
      ExpressionGE::LetAndLend(_) => unimplemented!(),
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
      ExpressionGE::ConstantStr(_) => unimplemented!(),
      ExpressionGE::InterfaceFunctionCall(_) => unimplemented!(),
      ExpressionGE::ExternFunctionCall(_) => unimplemented!(),
      ExpressionGE::BoundFunctionCall(_) => unimplemented!(),
      ExpressionGE::Reinterpret(_) => unimplemented!(),
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
      ExpressionGE::CopyPrim(CopyPrimGE{ range, loct, inner, result }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *inner, next_held_num)?;
      }
      ExpressionGE::StaticSizedArrayLookup(_) => unimplemented!(),
      ExpressionGE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupGE { range, array_expr, array_type, index_expr, result, .. }) => {
        self.check_expr(coutputs, function_s, arena, group_tree, *array_expr, next_held_num)?;
        self.check_expr(coutputs, function_s, arena, group_tree, *index_expr, next_held_num)?;
      }
      ExpressionGE::MemberLookup(_) => unimplemented!(),
    }
    Ok(())
  }

  fn churn<'g>(
    &self,
    group_subtree: &mut GroupSubtree<'s, 't>,
    effect_loc: LocT<'t>,
    mut_effect_steps: &'g [GroupStep<'s, 't>],
  ) {
    match mut_effect_steps.split_first() {
      None => {
        // Invalidate every local in descendant groups
        self.invalidate_descendant_groups_of(group_subtree, effect_loc);
      }
      Some((first, rest)) => {
        match group_subtree.name_to_child.get_mut(first) {
          None => {} // If there's nothing there, then there's nothing we need to churn.
          Some(child) => {
            self.churn(child, effect_loc, rest);
          }
        }
      }
    }
  }

  fn invalidate_descendant_groups_of<'g>(
    &self,
    group_subtree: &mut GroupSubtree<'s, 't>,
    effect_loc: LocT<'t>
  ) {
    // DON'T invalidate every local in this group. We don't invalidate references into this group,
    // we invalidate references into descendant groups.

    // Recurse
    for (group_step, group_child_subtree) in &mut group_subtree.name_to_child {
      match group_step {
        // These aren't child groups, so keep looking for child groups in them...
        GroupStep::Member { .. } => self.invalidate_descendant_groups_of(group_child_subtree, effect_loc),
        GroupStep::InlineElements => self.invalidate_descendant_groups_of(group_child_subtree, effect_loc),
        // These are actually child groups, so deep invalidate them.
        GroupStep::ChildElements => self.deep_invalidate(group_child_subtree, effect_loc),
        GroupStep::Variant { .. } => self.deep_invalidate(group_child_subtree, effect_loc),
        // TODO: Not sure about these cases
        GroupStep::Rune(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_loc),
        GroupStep::ParamAnonymousGroup(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_loc),
        GroupStep::Local(_) => self.invalidate_descendant_groups_of(group_child_subtree, effect_loc),
      }
    }
  }

  fn deep_invalidate<'g>(
    &self,
    group_subtree: &mut GroupSubtree<'s, 't>,
    effect_loc: LocT<'t>
  ) {
    // Invalidate every local in this group
    for (local_key, local) in &mut group_subtree.locals {
      local.invalidated_by = Some(effect_loc);
    }
    // Invalidate every local in every descendant group
    for (group_step, group_child_subtree) in &mut group_subtree.name_to_child {
      self.deep_invalidate(group_child_subtree, effect_loc);
    }
  }

  fn check_variable_still_valid<'g>(
    &self,
    group_tree: &mut GroupSubtree<'s, 't>,
    range: RangeS<'s>,
    var_key: RefKey<'s, 't>,
    type_gt: KindGT<'s, 't, 'g>
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let mut mentioned_groups = Vec::new();
    self.collect_type_mentioned_groups(&mut mentioned_groups, type_gt);
    for mentioned_group in mentioned_groups {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)
      let mentioned_group_subtree = self.lookup_or_create_group_subtree(group_tree, mentioned_group);
      let local_entry =
        if mentioned_group.ellipsis {
          mentioned_group_subtree.locals_in_ellipsis.get(&var_key).expect("Missing subtree")
        } else {
          mentioned_group_subtree.locals.get(&var_key).expect("Missing subtree")
        };
      if let Some(loct) = local_entry.invalidated_by {
        return Err(ICompileErrorT::BorrowCheckError {
          range: range,
          kind: BorrowErrorKind::UseAfterChurn { local: var_key }
        });
      }
    }
    Ok(())
  }

  fn insert_new_variable<'g>(&self, group_tree: &mut GroupSubtree<'s, 't>, new_var_key: RefKey<'s, 't>, type_gt: KindGT<'s, 't, 'g>) {
    let mut mentioned_groups = Vec::new();
    self.collect_type_mentioned_groups(&mut mentioned_groups, type_gt);
    for mentioned_group in mentioned_groups {
      // Note this *doesn't* look up the ellipsis part of the group, because ellipsis isn't a subtree.
      // (Perhaps we should make it one)
      let mentioned_group_subtree = self.lookup_or_create_group_subtree(group_tree, mentioned_group);
      if mentioned_group.ellipsis {
        mentioned_group_subtree.locals_in_ellipsis.insert(new_var_key, LocalEntry {
          invalidated_by: None
        });
      } else {
        mentioned_group_subtree.locals.insert(new_var_key, LocalEntry {
          invalidated_by: None
        });
      }
    }
    // - for each group that this value is pointing at:
    //   - fetch its subtree
    //   - insert an entry
  }

  fn collect_type_mentioned_groups<'g>(
    &self,
    group_exprs_g: &mut Vec<GroupPathG<'s, 't, 'g>>,
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
          self.collect_templata_mentioned_groups(group_exprs_g, *template_arg);
        }
      }
      KindGT::Interface(InterfaceGT { id, template_args }) => {
        for template_arg in template_args.iter() {
          self.collect_templata_mentioned_groups(group_exprs_g, *template_arg);
        }
      }
      KindGT::StaticSizedArray(StaticSizedArrayGT { name, element_type }) => {
        self.collect_type_mentioned_groups(group_exprs_g, *element_type);
      }
      KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) => {
        self.collect_type_mentioned_groups(group_exprs_g, *element_type);
      }
      KindGT::KindPlaceholder(_) => {}
      KindGT::OverloadSet(_) => {}
      KindGT::BorrowRef(BorrowRefGT { inner, group }) => {
        self.collect_type_mentioned_groups(group_exprs_g, *inner);
        for group in group.group.iter() {
          // VCOORD: dedup?
          group_exprs_g.push(*group);
        }
      }
      KindGT::OwnRef(_) => {}
      KindGT::ShareRef(_) => {}
      KindGT::WeakRef(_) => {}
    }
  }

  fn collect_templata_mentioned_groups<'g>(
    &self,
    group_exprs_g: &mut Vec<GroupPathG<'s, 't, 'g>>,
    templata_gt: ITemplataG<'s, 't, 'g>
  ) {
    match templata_gt {
      ITemplataG::Kind(KindTemplataG { kind }) => {
        self.collect_type_mentioned_groups(group_exprs_g, kind);
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
      ITemplataG::Group(_) => {}
      ITemplataG::Function(_) => {}
      ITemplataG::StructDefinition(_) => {}
      ITemplataG::InterfaceDefinition(_) => {}
      ITemplataG::ImplDefinition(_) => {}
      ITemplataG::ExternFunction(_) => {}
    }
  }

  fn lookup_or_create_group_subtree<'g, 'x>(
    &self,
    tree: &'x mut GroupSubtree<'s, 't>,
    path: GroupPathG<'s, 't, 'g>
  ) -> &'x mut GroupSubtree<'s, 't> {
    let key =
      match path.root {
        GroupRootG::Rune(rune) => GroupStep::Rune(rune),
        GroupRootG::ParamAnonymousGroup(_) => unimplemented!(),
        GroupRootG::Local(var_name) => GroupStep::Local(var_name),
      };
    let subroot =
      tree.name_to_child
          .entry(key)
          .or_insert_with(|| GroupSubtree {
            locals: IndexMap::new(),
            locals_in_ellipsis: IndexMap::new(),
            name_to_child: IndexMap::new(),
          });
    self.lookup_group_subtree_inner(subroot, path.steps)
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
            GroupChildStepG::Member { member_name } => GroupStep::Member{member_name: *member_name},
            GroupChildStepG::ChildElements { } => GroupStep::ChildElements{},
            GroupChildStepG::InlineElements { .. } => GroupStep::InlineElements{},
            GroupChildStepG::Variant { variant_name } => GroupStep::Variant{ variant_name: *variant_name }
          };
        let subroot =
            subtree.name_to_child
                .entry(key)
                .or_insert_with(|| GroupSubtree {
                  locals: IndexMap::new(),
                  locals_in_ellipsis: IndexMap::new(),
                  name_to_child: IndexMap::new(),
                });
        self.lookup_group_subtree_inner(subroot, &remaining_path[1..])
      }
    }
  }
}
