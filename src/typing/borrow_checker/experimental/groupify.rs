//! Phase 1: `groupify_function` walks the typed body and produces the canonical grouped `ExpressionGE`
//! (`borrow_checker::ast_g`), every node fully populated from its typed twin. For every
//! reference-typed local it records the group its referent lives in; at every call it records the
//! groups it churns (`mut_effects`). Everything is allocated in the `'g` check arena.

use crate::postparsing::ast::{FunctionS, ParameterS};
use crate::postparsing::names::{IFunctionDeclarationNameS, IRuneS};
use crate::postparsing::rules::types::{EffectS, GroupS, ITypeST, RegionS};
use crate::typing::ast::ast::{FunctionDefinitionT, PrototypeT};
use crate::typing::ast::expressions::{ExpressionTE, FunctionCallTE};
use crate::typing::borrow_checker::access_event::AccessEventG;
use crate::typing::borrow_checker::ast_g::*;
use crate::typing::borrow_checker::experimental::borrow_types::{group_expr_from_group_s, subst_group_expr};
use crate::typing::borrow_checker::experimental::grouped_ast::{flatten, single_path, sole_path, with_step};
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupRootG};
use crate::typing::borrow_checker::kind_g::{BorrowRefGT, KindGT, ShareRefGT, VoidGT, WeakRefGT};
use crate::typing::borrow_checker::templata_g::GroupTemplataG;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::env::function_environment_t::LocalVariable;
use crate::typing::names::names::{IdT, IdValT, INameT, IVarNameT};
use crate::typing::templata::templata::ITemplataT;
use crate::typing::types::types::{BorrowRefT, KindT};
use crate::utils::fx::IndexMap;
use crate::utils::range::RangeS;
use bumpalo::Bump;
use crate::interner::StrI;

/// Phase-1 state: the reference-typed locals seen so far and where each points, plus the function being
/// grouped, and the access log for `calculate_aliasing_info`.
struct GCtx<'s, 't, 'g> {
  locals: Vec<(IVarNameT<'s, 't>, KindGT<'s, 't, 'g>)>,
  function_s: &'s FunctionS<'s>,
  function_t: &'t FunctionDefinitionT<'s, 't>,
  access_log: Vec<AccessEventG<'s, 't>>,
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  /// Build the grouped body for one function, or an error if a borrow's group is underivable.
  pub fn groupify_function<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    arena: &'g Bump,
  ) -> Result<(ExpressionGE<'s, 't, 'g>, Vec<AccessEventG<'s, 't>>), ICompileErrorT<'s, 't>> {
    let mut ctx = GCtx { locals: vec![], function_s, function_t, access_log: vec![] };
    let body = self.groupify(coutputs, &function_t.body, &mut ctx, arena);
    Ok((body, ctx.access_log))
  }

  /// Rebuild one typed expression as its grouped mirror, allocating the node and its children in
  /// `arena`. Every field of the typed node is carried over; the checker-facing additions are the
  /// grouped `result`s, the `LocalVariableG`s, and a call's `mut_effects`.
  fn groupify<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    expr: &ExpressionTE<'s, 't>,
    ctx: &mut GCtx<'s, 't, 'g>,
    arena: &'g Bump,
  ) -> ExpressionGE<'s, 't, 'g> {
    match expr {
      ExpressionTE::LetAndLend(e) => {
        let child = self.groupify(coutputs, &e.expr, ctx, arena);
        ctx.locals.push((e.variable.name, child.result()));
        let variable = self.local_var_g(ctx, e.variable, arena);
        let inner = child.result();
        let result = arena.alloc(BorrowRefGT {
          inner,
          group: GroupTemplataG { group: single_path(arena, GroupRootG::Local(e.variable.name)), kind: inner },
        });
        ExpressionGE::LetAndLend(arena.alloc(LetAndLendGE { range: e.range, variable, expr: child, result }))
      }
      ExpressionTE::LockWeak(e) => {
        let inner_expr = self.groupify(coutputs, &e.inner_expr, ctx, arena);
        ExpressionGE::LockWeak(arena.alloc(LockWeakGE {
          range: e.range,
          inner_expr,
          result: self.make_kind_g_groupless(e.result, arena),
          some_constructor: e.some_constructor,
          none_constructor: e.none_constructor,
          some_impl_name: e.some_impl_name,
          none_impl_name: e.none_impl_name,
        }))
      }
      ExpressionTE::BorrowToWeak(e) => {
        let inner_expr = self.groupify(coutputs, &e.inner_expr, ctx, arena);
        let result = arena.alloc(WeakRefGT { inner: self.make_kind_g_groupless(e.result.inner, arena) });
        ExpressionGE::BorrowToWeak(arena.alloc(BorrowToWeakGE { range: e.range, inner_expr, result }))
      }
      ExpressionTE::LetNormal(l) => {
        let child = self.groupify(coutputs, &l.expr, ctx, arena);
        ctx.locals.push((l.variable.name, child.result()));
        let variable = self.local_var_g(ctx, l.variable, arena);
        ExpressionGE::LetNormal(arena.alloc(LetNormalGE {
          range: l.range,
          variable,
          expr: child,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::Unlet(u) => {
        let variable = self.local_var_g(ctx, u.variable, arena);
        ExpressionGE::Unlet(arena.alloc(UnletGE { range: u.range, variable, result: variable.tyype }))
      }
      ExpressionTE::Discard(e) => {
        let child = self.groupify(coutputs, &e.expr, ctx, arena);
        ExpressionGE::Discard(arena.alloc(DiscardGE { range: e.range, expr: child, result: void_kind_g() }))
      }
      ExpressionTE::If(if_te) => {
        let condition = self.groupify(coutputs, &if_te.condition, ctx, arena);
        let then_call = self.groupify(coutputs, &if_te.then_call, ctx, arena);
        let else_call = self.groupify(coutputs, &if_te.else_call, ctx, arena);
        let result = if diverges(then_call) { else_call.result() } else { then_call.result() };
        ExpressionGE::If(arena.alloc(IfGE {
          range: if_te.range,
          loct: if_te.loct,
          condition,
          then_call,
          else_call,
          result,
        }))
      }
      ExpressionTE::While(w) => {
        let inner = self.groupify(coutputs, &w.block.inner, ctx, arena);
        let block = BlockGE { range: w.block.range, inner, result: inner.result() };
        let mut churns = vec![];
        collect_subtree_churns(inner, &mut churns);
        ExpressionGE::While(arena.alloc(WhileGE {
          range: w.range,
          loct: w.loct,
          block,
          result: self.make_kind_g_groupless(w.result, arena),
          mut_effects: arena.alloc_slice_fill_iter(churns),
        }))
      }
      ExpressionTE::Mutate(m) => {
        let destination_expr = self.groupify(coutputs, &m.destination_expr, ctx, arena);
        let source_expr = self.groupify(coutputs, &m.source_expr, ctx, arena);
        if let Some((base_ref, group)) = self.base_ref_and_group(ctx, &m.destination_expr, arena) {
          ctx.access_log.push(AccessEventG::Store { base_ref, group, loct: m.loct });
        }
        // The replaced value: the destination place's referent, with its groups.
        let result = deref_kind_g(destination_expr.result());
        ExpressionGE::Mutate(arena.alloc(MutateGE {
          range: m.range,
          loct: m.loct,
          destination_expr,
          source_expr,
          result,
        }))
      }
      ExpressionTE::Restackify(e) => {
        let variable = self.local_var_g(ctx, e.variable, arena);
        let source_expr = self.groupify(coutputs, &e.source_expr, ctx, arena);
        ExpressionGE::Restackify(arena.alloc(RestackifyGE {
          range: e.range,
          variable,
          source_expr,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::Return(r) => {
        let source_expr = self.groupify(coutputs, &r.source_expr, ctx, arena);
        ExpressionGE::Return(arena.alloc(ReturnGE {
          range: r.range,
          source_expr,
          result: self.make_kind_g_groupless(r.result, arena),
        }))
      }
      ExpressionTE::Break(b) => ExpressionGE::Break(
        arena.alloc(BreakGE { range: b.range, result: self.make_kind_g_groupless(b.result, arena) }),
      ),
      ExpressionTE::Block(b) => {
        let inner = self.groupify(coutputs, &b.inner, ctx, arena);
        ExpressionGE::Block(arena.alloc(BlockGE { range: b.range, inner, result: inner.result() }))
      }
      ExpressionTE::Consecutor(c) => {
        // Group each statement, recording a bare-integer statement as a marker between its neighbors'
        // accesses so a restrict region can be pinned by value.
        let mut grouped = Vec::with_capacity(c.exprs.len());
        for e in c.exprs.iter() {
          let g = self.groupify(coutputs, e, ctx, arena);
          if let Some(value) = statement_marker(e) {
            ctx.access_log.push(AccessEventG::Marker { value });
          }
          grouped.push(g);
        }
        let exprs = arena.alloc_slice_fill_iter(grouped.into_iter());
        // As in the typed node: a `Never` anywhere makes the sequence `Never`, else the last result.
        let result = match exprs.iter().copied().find(|e| diverges(*e)) {
          Some(n) => n.result(),
          None => exprs.last().expect("consecutor with no expressions").result(),
        };
        ExpressionGE::Consecutor(arena.alloc(ConsecutorGE { range: c.range, exprs, result }))
      }
      ExpressionTE::StaticArrayFromValues(e) => {
        let elements =
          arena.alloc_slice_fill_iter(e.elements.iter().map(|x| self.groupify(coutputs, x, ctx, arena)));
        ExpressionGE::StaticArrayFromValues(arena.alloc(StaticArrayFromValuesGE {
          range: e.range,
          elements,
          result: self.make_kind_g_groupless(e.result, arena),
          array_type: self.ssa_gt(e.array_type, arena),
        }))
      }
      ExpressionTE::ArraySize(e) => {
        let array = self.groupify(coutputs, &e.array, ctx, arena);
        ExpressionGE::ArraySize(arena.alloc(ArraySizeGE {
          range: e.range,
          array,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::IsSameInstance(e) => {
        let left = self.groupify(coutputs, &e.left, ctx, arena);
        let right = self.groupify(coutputs, &e.right, ctx, arena);
        ExpressionGE::IsSameInstance(arena.alloc(IsSameInstanceGE {
          range: e.range,
          left,
          right,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::AsSubtype(e) => {
        let source_expr = self.groupify(coutputs, &e.source_expr, ctx, arena);
        ExpressionGE::AsSubtype(arena.alloc(AsSubtypeGE {
          range: e.range,
          source_expr,
          target_type: self.make_kind_g_groupless(e.target_type, arena),
          result: self.cast_result(e.result, source_expr.result(), arena),
          ok_constructor: e.ok_constructor,
          err_constructor: e.err_constructor,
          impl_name: e.impl_name,
          ok_impl_name: e.ok_impl_name,
          err_impl_name: e.err_impl_name,
        }))
      }
      ExpressionTE::VoidLiteral(v) => {
        ExpressionGE::VoidLiteral(arena.alloc(VoidLiteralGE { range: v.range, result: void_kind_g() }))
      }
      ExpressionTE::ConstantInt(c) => ExpressionGE::ConstantInt(arena.alloc(ConstantIntGE {
        range: c.range,
        value: self.templata_g(c.value, arena),
        bits: c.bits,
        result: self.make_kind_g_groupless(c.result, arena),
      })),
      ExpressionTE::ConstantBool(c) => ExpressionGE::ConstantBool(arena.alloc(ConstantBoolGE {
        range: c.range,
        value: c.value,
        result: self.make_kind_g_groupless(c.result, arena),
      })),
      ExpressionTE::ConstantStr(c) => {
        let result = arena.alloc(ShareRefGT { inner: self.make_kind_g_groupless(c.result.inner, arena) });
        ExpressionGE::ConstantStr(arena.alloc(ConstantStrGE { range: c.range, value: c.value, result }))
      }
      ExpressionTE::ConstantFloat(c) => ExpressionGE::ConstantFloat(arena.alloc(ConstantFloatGE {
        range: c.range,
        value: c.value,
        result: self.make_kind_g_groupless(c.result, arena),
      })),
      ExpressionTE::ArgLookup(a) => ExpressionGE::ArgLookup(arena.alloc(ArgLookupGE {
        range: a.range,
        param_index: a.param_index,
        result: self.arg_type(a.param_index as usize, ctx, arena),
      })),
      ExpressionTE::ArrayLength(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        ExpressionGE::ArrayLength(arena.alloc(ArrayLengthGE {
          range: e.range,
          array_expr,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::InterfaceFunctionCall(e) => {
        let args = arena.alloc_slice_fill_iter(e.args.iter().map(|a| self.groupify(coutputs, a, ctx, arena)));
        ExpressionGE::InterfaceFunctionCall(arena.alloc(InterfaceFunctionCallGE {
          range: e.range,
          super_function_prototype: e.super_function_prototype,
          virtual_param_index: e.virtual_param_index,
          result: self.make_kind_g_groupless(e.result, arena),
          args,
          mut_effects: &[],
        }))
      }
      ExpressionTE::BoundFunctionCall(e) => {
        let args = arena.alloc_slice_fill_iter(e.args.iter().map(|a| self.groupify(coutputs, a, ctx, arena)));
        ExpressionGE::BoundFunctionCall(arena.alloc(BoundFunctionCallGE {
          range: e.range,
          impl_name: e.impl_name,
          abstract_prototype: e.abstract_prototype,
          virtual_param_index: e.virtual_param_index,
          result: self.make_kind_g_groupless(e.result, arena),
          args,
          mut_effects: &[],
        }))
      }
      ExpressionTE::ExternFunctionCall(e) => {
        let args = arena.alloc_slice_fill_iter(e.args.iter().map(|a| self.groupify(coutputs, a, ctx, arena)));
        ExpressionGE::ExternFunctionCall(arena.alloc(ExternFunctionCallGE {
          range: e.range,
          prototype2: e.prototype2,
          args,
          result: self.make_kind_g_groupless(e.result, arena),
          mut_effects: &[],
        }))
      }
      ExpressionTE::FunctionCall(call) => {
        let args = arena
          .alloc_slice_fill_iter(call.args.iter().map(|a| self.groupify(coutputs, a, ctx, arena)));
        let (result, paths) = match self.resolve_callee(coutputs, call.callable) {
          Some(callee) => {
            let subst = arg_rune_subst(callee, args);
            (
              self.call_result_kind(call, callee, &subst, arena),
              self.call_mut_effects(call, callee, &subst, arena),
            )
          }
          None => (self.make_kind_g_groupless(call.callable.return_type, arena), vec![]),
        };
        let touched: Vec<Vec<GroupStep<'s, 't>>> = paths.iter().map(|m| m.steps.to_vec()).collect();
        ctx.access_log.push(AccessEventG::Call { touched, loct: call.loct });
        let mut_effects = arena.alloc_slice_fill_iter(paths.into_iter().map(|p| &*arena.alloc(p)));
        ExpressionGE::FunctionCall(arena.alloc(FunctionCallGE {
          loct: call.loct,
          range: call.range,
          callable: call.callable,
          args,
          result,
          mut_effects,
        }))
      }
      ExpressionTE::Reinterpret(e) => {
        let child = self.groupify(coutputs, &e.expr, ctx, arena);
        ExpressionGE::Reinterpret(arena.alloc(ReinterpretGE {
          range: e.range,
          expr: child,
          result: self.cast_result(e.result, child.result(), arena),
        }))
      }
      ExpressionTE::Construct(e) => {
        let args = arena.alloc_slice_fill_iter(e.args.iter().map(|a| self.groupify(coutputs, a, ctx, arena)));
        ExpressionGE::Construct(arena.alloc(ConstructGE {
          range: e.range,
          struct_tt: self.struct_gt(e.struct_tt, arena),
          result: self.make_kind_g_groupless(e.result, arena),
          args,
        }))
      }
      ExpressionTE::NewRuntimeSizedArray(e) => {
        let capacity_expr = self.groupify(coutputs, &e.capacity_expr, ctx, arena);
        ExpressionGE::NewRuntimeSizedArray(arena.alloc(NewRuntimeSizedArrayGE {
          range: e.range,
          array_type: self.rsa_gt(e.array_type, arena),
          capacity_expr,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::StaticArrayFromCallable(e) => {
        let generator = self.groupify(coutputs, &e.generator, ctx, arena);
        ExpressionGE::StaticArrayFromCallable(arena.alloc(StaticArrayFromCallableGE {
          range: e.range,
          array_type: self.ssa_gt(e.array_type, arena),
          generator,
          generator_method: e.generator_method,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::DestroyStaticSizedArrayIntoFunction(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        let consumer = self.groupify(coutputs, &e.consumer, ctx, arena);
        ExpressionGE::DestroyStaticSizedArrayIntoFunction(arena.alloc(DestroyStaticSizedArrayIntoFunctionGE {
          range: e.range,
          array_expr,
          array_type: self.ssa_gt(e.array_type, arena),
          consumer,
          consumer_method: e.consumer_method,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::DestroyStaticSizedArrayIntoLocals(e) => {
        let child = self.groupify(coutputs, &e.expr, ctx, arena);
        let destination_reference_variables = self.local_vars_g(ctx, e.destination_reference_variables, arena);
        ExpressionGE::DestroyStaticSizedArrayIntoLocals(arena.alloc(DestroyStaticSizedArrayIntoLocalsGE {
          range: e.range,
          expr: child,
          static_sized_array: self.ssa_gt(e.static_sized_array, arena),
          destination_reference_variables,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::DestroyRuntimeSizedArray(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        ExpressionGE::DestroyRuntimeSizedArray(arena.alloc(DestroyRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::RuntimeSizedArrayCapacity(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        ExpressionGE::RuntimeSizedArrayCapacity(arena.alloc(RuntimeSizedArrayCapacityGE {
          range: e.range,
          array_expr,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::PushRuntimeSizedArray(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        let new_element_expr = self.groupify(coutputs, &e.new_element_expr, ctx, arena);
        ExpressionGE::PushRuntimeSizedArray(arena.alloc(PushRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          new_element_expr,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::PopRuntimeSizedArray(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        ExpressionGE::PopRuntimeSizedArray(arena.alloc(PopRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::InterfaceToInterfaceUpcast(e) => {
        let inner_expr = self.groupify(coutputs, &e.inner_expr, ctx, arena);
        ExpressionGE::InterfaceToInterfaceUpcast(arena.alloc(InterfaceToInterfaceUpcastGE {
          range: e.range,
          inner_expr,
          target_interface: self.interface_gt(e.target_interface, arena),
          result: self.cast_result(e.result, inner_expr.result(), arena),
        }))
      }
      ExpressionTE::UpcastInterface(e) => {
        let inner_expr = self.groupify(coutputs, &e.inner_expr, ctx, arena);
        ExpressionGE::UpcastInterface(arena.alloc(UpcastInterfaceGE {
          range: e.range,
          inner_expr,
          target_super_kind: self.super_kind_gt(e.target_super_kind, arena),
          impl_name: e.impl_name,
          result: self.cast_result(e.result, inner_expr.result(), arena),
        }))
      }
      ExpressionTE::UpcastGeneric(e) => {
        let inner_expr = self.groupify(coutputs, &e.inner_expr, ctx, arena);
        ExpressionGE::UpcastGeneric(arena.alloc(UpcastGenericGE {
          range: e.range,
          inner_expr,
          target_super_kind: self.super_kind_gt(e.target_super_kind, arena),
          impl_name: e.impl_name,
          result: self.cast_result(e.result, inner_expr.result(), arena),
        }))
      }
      ExpressionTE::Destroy(e) => {
        let child = self.groupify(coutputs, &e.expr, ctx, arena);
        let destination_reference_variables = self.local_vars_g(ctx, e.destination_reference_variables, arena);
        ExpressionGE::Destroy(arena.alloc(DestroyGE {
          range: e.range,
          expr: child,
          struct_tt: self.struct_gt(e.struct_tt, arena),
          destination_reference_variables,
          result: void_kind_g(),
        }))
      }
      ExpressionTE::CopyPrim(e) => {
        let inner = self.groupify(coutputs, &e.inner, ctx, arena);
        if let Some((base_ref, group)) = self.base_ref_and_group(ctx, &e.inner, arena) {
          ctx.access_log.push(AccessEventG::Read { base_ref, group, loct: e.loct });
        }
        ExpressionGE::CopyPrim(arena.alloc(CopyPrimGE {
          range: e.range,
          loct: e.loct,
          inner,
          result: self.make_kind_g_groupless(e.result, arena),
        }))
      }
      ExpressionTE::LocalLookup(l) => {
        let local_variable = self.local_var_g(ctx, l.local_variable, arena);
        let result = arena.alloc(BorrowRefGT {
          inner: local_variable.tyype,
          group: GroupTemplataG {
            group: single_path(arena, GroupRootG::Local(l.local_variable.name)),
            kind: local_variable.tyype,
          },
        });
        ExpressionGE::LocalLookup(arena.alloc(LocalLookupGE { range: l.range, local_variable, result }))
      }
      ExpressionTE::StaticSizedArrayLookup(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        let index_expr = self.groupify(coutputs, &e.index_expr, ctx, arena);
        ExpressionGE::StaticSizedArrayLookup(arena.alloc(StaticSizedArrayLookupGE {
          range: e.range,
          array_expr,
          array_type: self.ssa_gt(e.array_type, arena),
          index_expr,
          result: self.element_borrow(e.result, array_expr.result(), arena),
        }))
      }
      ExpressionTE::RuntimeSizedArrayLookup(e) => {
        let array_expr = self.groupify(coutputs, &e.array_expr, ctx, arena);
        let index_expr = self.groupify(coutputs, &e.index_expr, ctx, arena);
        ExpressionGE::RuntimeSizedArrayLookup(arena.alloc(RuntimeSizedArrayLookupGE {
          range: e.range,
          array_expr,
          array_type: self.rsa_gt(e.array_type, arena),
          index_expr,
          result: self.element_borrow(e.result, array_expr.result(), arena),
        }))
      }
      ExpressionTE::MemberLookup(e) => {
        let struct_expr = self.groupify(coutputs, &e.struct_expr, ctx, arena);
        ExpressionGE::MemberLookup(arena.alloc(MemberLookupGE {
          range: e.range,
          struct_expr,
          member_name: e.member_name,
          result: self.member_borrow(e.result, struct_expr.result(), &e.member_name, arena),
        }))
      }
      ExpressionTE::Deref(d) => {
        let inner = self.groupify(coutputs, &d.inner, ctx, arena);
        let result = deref_kind_g(inner.result());
        if !matches!(result, KindGT::BorrowRef(_)) {
          if let Some((base_ref, group)) = self.base_ref_and_group(ctx, &d.inner, arena) {
            ctx.access_log.push(AccessEventG::Read { base_ref, group, loct: d.loct });
          }
        }
        ExpressionGE::Deref(arena.alloc(DerefGE { range: d.range, loct: d.loct, inner, result }))
      }
    }
  }

  /// A local's grouped type: a tracked reference binding carries groups at every depth; anything else is
  /// its plain typed type, groupless.
  fn local_type<'g>(
    &self,
    ctx: &GCtx<'s, 't, 'g>,
    name: IVarNameT<'s, 't>,
    typed: KindT<'s, 't>,
    arena: &'g Bump,
  ) -> KindGT<'s, 't, 'g> {
    match ctx.locals.iter().find(|(n, _)| *n == name) {
      Some((_, k)) => *k,
      None => self.make_kind_g_groupless(typed, arena),
    }
  }

  /// The grouped mirror of a typed local variable: its name and its grouped type (`local_type`).
  fn local_var_g<'g>(
    &self,
    ctx: &GCtx<'s, 't, 'g>,
    var: &LocalVariable<'s, 't>,
    arena: &'g Bump,
  ) -> &'g LocalVariableG<'s, 't, 'g> {
    arena.alloc(LocalVariableG { name: var.name, tyype: self.local_type(ctx, var.name, var.tyype, arena) })
  }

  /// The grouped mirrors of a destructure's destination locals.
  fn local_vars_g<'g>(
    &self,
    ctx: &GCtx<'s, 't, 'g>,
    vars: &[&LocalVariable<'s, 't>],
    arena: &'g Bump,
  ) -> &'g [&'g LocalVariableG<'s, 't, 'g>] {
    arena.alloc_slice_fill_iter(vars.iter().map(|v| self.local_var_g(ctx, v, arena)))
  }

  /// A parameter's full grouped type, read from its written type at every depth.
  fn arg_type<'g>(&self, i: usize, ctx: &GCtx<'s, 't, 'g>, arena: &'g Bump) -> KindGT<'s, 't, 'g> {
    let ps = ctx.function_s.params.get(i).expect("arg index out of range");
    let pt = ctx.function_t.header.params.get(i).expect("arg index out of range");
    self.make_kind_g(pt.tyype, &ps.tyype, Some(&pt.name), arena)
  }

  /// A call's grouped result: the callee's declared return type, groups crossed into the caller frame.
  /// Every non-lambda callee has a written return type; only a lambda's is inferred, so only a lambda
  /// falls back to the typed return, groupless.
  fn call_result_kind<'g>(
    &self,
    call: &FunctionCallTE<'s, 't>,
    callee: &'s FunctionS<'s>,
    subst: &IndexMap<IRuneS<'s>, GroupExprG<'s, 't, 'g>>,
    arena: &'g Bump,
  ) -> KindGT<'s, 't, 'g> {
    match (callee.maybe_return_type.as_ref(), callee.name) {
      (Some(return_st), _) => {
        let return_kind_g = self.make_kind_g(call.callable.return_type, return_st, None, arena);
        self.substitute_groups(return_kind_g, subst, arena)
      }
      (None, IFunctionDeclarationNameS::LambdaDeclarationName(_)) => {
        self.make_kind_g_groupless(call.callable.return_type, arena)
      }
      (None, name) => panic!("vfail: non-lambda callee has no written return type: {:?}", name),
    }
  }

  /// A cast keeps the operand's outer group and re-expresses the referent's structure.
  fn cast_result<'g>(
    &self,
    cast_kind: KindT<'s, 't>,
    operand: KindGT<'s, 't, 'g>,
    arena: &'g Bump,
  ) -> KindGT<'s, 't, 'g> {
    match (cast_kind, operand) {
      (KindT::BorrowRef(b), KindGT::BorrowRef(ob)) => {
        ref_kind_g(ob.group.group, self.make_kind_g_groupless(b.inner, arena), arena)
      }
      (other, _) => self.make_kind_g_groupless(other, arena),
    }
  }

  /// An array-element access yields a borrow into the array's child-elements group.
  fn element_borrow<'g>(
    &self,
    access: &'t BorrowRefT<'s, 't>,
    array: KindGT<'s, 't, 'g>,
    arena: &'g Bump,
  ) -> &'g BorrowRefGT<'s, 't, 'g> {
    match array {
      KindGT::BorrowRef(ab) => {
        let inner = self.make_kind_g_groupless(access.inner, arena);
        let group = with_step(arena, ab.group.group, GroupChildStepG::ChildElements {});
        arena.alloc(BorrowRefGT { group: GroupTemplataG { group, kind: inner }, inner })
      }
      other => panic!("vfail: element access through a non-borrow array: {:?}", other),
    }
  }

  /// A member access yields a borrow into the struct's member child group.
  fn member_borrow<'g>(
    &self,
    access: &'t BorrowRefT<'s, 't>,
    struct_val: KindGT<'s, 't, 'g>,
    member_name_t: &IVarNameT<'s, 't>,
    arena: &'g Bump,
  ) -> &'g BorrowRefGT<'s, 't, 'g> {
    match struct_val {
      KindGT::BorrowRef(sb) => {
        let member_name = match member_name_t {
          IVarNameT::Member(cv) => cv.imprecise_name.name,
          IVarNameT::Local(cv) => cv.imprecise_name.name,
          _ => panic!("vfail: member lookup with a non-member name"),
        };
        let inner = self.make_kind_g_groupless(access.inner, arena);
        let group = with_step(arena, sb.group.group, GroupChildStepG::Member { member_name });
        arena.alloc(BorrowRefGT { group: GroupTemplataG { group, kind: inner }, inner })
      }
      other => panic!("vfail: member access through a non-borrow struct: {:?}", other),
    }
  }

  /// The caller-side groups a call churns, from the callee's declared effects and parameter groups.
  fn call_mut_effects<'g>(
    &self,
    call: &FunctionCallTE<'s, 't>,
    callee: &'s FunctionS<'s>,
    subst: &IndexMap<IRuneS<'s>, GroupExprG<'s, 't, 'g>>,
    arena: &'g Bump,
  ) -> Vec<MutEffectPath<'s, 't, 'g>> {
    let mut paths = vec![];
    for effect in callee.effects {
      if let EffectS::Mut(gs) = effect {
        let caller = subst_group_expr(group_expr_from_group_s(gs, arena), subst, arena);
        for path in caller {
          paths.push(MutEffectPath {
            effecting_node_loc: call.loct,
            steps: arena.alloc_slice_fill_iter(flatten(path)),
          });
        }
      }
    }
    paths
  }

  /// Resolve a call's callee to its scout `FunctionS` via the template id.
  pub(crate) fn resolve_callee(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    callable: &PrototypeT<'s, 't>,
  ) -> Option<&'s FunctionS<'s>> {
    let inst_id = callable.id;
    let template_local = match inst_id.local_name {
      INameT::Function(fnt) => INameT::FunctionTemplate(fnt.template),
      _ => return None,
    };
    let template_id: &'t IdT<'s, 't> = self.typing_interner.intern_id(IdValT {
      package_coord: inst_id.package_coord,
      init_steps: inst_id.init_steps,
      local_name: template_local,
    });
    coutputs.peek_postparsed_function(template_id)
  }

  /// The root reference an access chain goes through, and the flat group that reference points into.
  fn base_ref_and_group<'g>(
    &self,
    ctx: &GCtx<'s, 't, 'g>,
    expr: &ExpressionTE<'s, 't>,
    arena: &'g Bump,
  ) -> Option<(IVarNameT<'s, 't>, Vec<GroupStep<'s, 't>>)> {
    match expr {
      ExpressionTE::LocalLookup(l) => {
        let ty = self.local_type(ctx, l.local_variable.name, l.local_variable.tyype, arena);
        Some((l.local_variable.name, borrowref_group(ty)?))
      }
      ExpressionTE::ArgLookup(a) => {
        let name = ctx.function_t.header.params.get(a.param_index as usize)?.name;
        Some((name, borrowref_group(self.arg_type(a.param_index as usize, ctx, arena))?))
      }
      ExpressionTE::Deref(d) => self.base_ref_and_group(ctx, &d.inner, arena),
      ExpressionTE::CopyPrim(e) => self.base_ref_and_group(ctx, &e.inner, arena),
      ExpressionTE::MemberLookup(e) => self.base_ref_and_group(ctx, &e.struct_expr, arena),
      ExpressionTE::StaticSizedArrayLookup(e) => self.base_ref_and_group(ctx, &e.array_expr, arena),
      ExpressionTE::RuntimeSizedArrayLookup(e) => self.base_ref_and_group(ctx, &e.array_expr, arena),
      _ => None,
    }
  }
}

/// The flattened group a borrow result points into, or `None` for a non-borrow.
fn borrowref_group<'s, 't, 'g>(k: KindGT<'s, 't, 'g>) -> Option<Vec<GroupStep<'s, 't>>> {
  match k {
    KindGT::BorrowRef(b) => Some(flatten(sole_path(b.group.group))),
    _ => None,
  }
}

/// Every churn inside a grouped subtree, for a loop's aggregated `mut_effects`: a reference is spoiled
/// on the loop's first iteration by a churn from any later one. The loop shares the calls' paths.
fn collect_subtree_churns<'s, 't, 'g>(
  node: ExpressionGE<'s, 't, 'g>,
  out: &mut Vec<&'g MutEffectPath<'s, 't, 'g>>,
) {
  let effects: &'g [&'g MutEffectPath<'s, 't, 'g>] = match node {
    ExpressionGE::FunctionCall(c) => c.mut_effects,
    ExpressionGE::InterfaceFunctionCall(c) => c.mut_effects,
    ExpressionGE::ExternFunctionCall(c) => c.mut_effects,
    ExpressionGE::BoundFunctionCall(c) => c.mut_effects,
    _ => &[],
  };
  out.extend(effects.iter().copied());
  for child in node.children() {
    collect_subtree_churns(child, out);
  }
}

/// Whether a grouped node's result is `Never` (it returns, breaks, or contains something that does).
pub(crate) fn diverges<'s, 't, 'g>(node: ExpressionGE<'s, 't, 'g>) -> bool {
  matches!(node.result(), KindGT::Never(_))
}

/// A borrow reference `KindGT` from a group and its referent type.
fn ref_kind_g<'s, 't, 'g>(
  group: GroupExprG<'s, 't, 'g>,
  inner: KindGT<'s, 't, 'g>,
  arena: &'g Bump,
) -> KindGT<'s, 't, 'g> {
  KindGT::BorrowRef(arena.alloc(BorrowRefGT { inner, group: GroupTemplataG { group, kind: inner } }))
}

/// The `void` result `KindGT`, for statement-like nodes.
fn void_kind_g<'s, 't, 'g>() -> KindGT<'s, 't, 'g> {
  KindGT::Void(VoidGT)
}

/// Peel one borrow: a `Deref`'s result is its operand's referent.
fn deref_kind_g<'s, 't, 'g>(operand: KindGT<'s, 't, 'g>) -> KindGT<'s, 't, 'g> {
  match operand {
    KindGT::BorrowRef(b) => b.inner,
    other => panic!("vfail: deref of a non-borrow: {:?}", other),
  }
}

/// A statement-position bare integer landmark (`103;`), for tests to pin a restrict region by value.
/// Matches a `ConstantInt` standing on its own — with or without the `Discard` the typing pass wraps a
/// dropped value in.
fn statement_marker<'s, 't>(expr: &ExpressionTE<'s, 't>) -> Option<i32> {
  let inner = match expr {
    ExpressionTE::Discard(d) => &d.expr,
    other => other,
  };
  match inner {
    ExpressionTE::ConstantInt(c) => match &c.value {
      ITemplataT::Integer(n) => Some(*n as i32),
      _ => None,
    },
    _ => None,
  }
}

/// The innermost local a grouped place expression is rooted in.
pub(crate) fn place_root_local<'s, 't, 'g>(expr: ExpressionGE<'s, 't, 'g>) -> Option<IVarNameT<'s, 't>> {
  match expr {
    ExpressionGE::LocalLookup(l) => Some(l.local_variable.name),
    ExpressionGE::RuntimeSizedArrayLookup(a) => place_root_local(a.array_expr),
    ExpressionGE::StaticSizedArrayLookup(a) => place_root_local(a.array_expr),
    ExpressionGE::MemberLookup(m) => place_root_local(m.struct_expr),
    ExpressionGE::Deref(d) => place_root_local(d.inner),
    _ => None,
  }
}

/// The local an argument moves (`^local` lowers to an `Unlet`), if any.
pub(crate) fn moved_local<'s, 't, 'g>(expr: ExpressionGE<'s, 't, 'g>) -> Option<IVarNameT<'s, 't>> {
  match expr {
    ExpressionGE::Unlet(u) => Some(u.variable.name),
    _ => None,
  }
}

/// The source range to point a held-register diagnostic at: the argument's own range.
pub(crate) fn held_range<'s, 't, 'g>(arg: ExpressionGE<'s, 't, 'g>) -> Option<RangeS<'s>> {
  match arg {
    ExpressionGE::FunctionCall(c) => c.range.first().copied(),
    _ => expr_range(arg),
  }
}

/// The source range of a grouped place expression, for a diagnostic at the use site.
pub(crate) fn expr_range<'s, 't, 'g>(expr: ExpressionGE<'s, 't, 'g>) -> Option<RangeS<'s>> {
  match expr {
    ExpressionGE::LocalLookup(l) => Some(l.range),
    ExpressionGE::RuntimeSizedArrayLookup(a) => Some(a.range),
    ExpressionGE::StaticSizedArrayLookup(a) => Some(a.range),
    ExpressionGE::MemberLookup(m) => Some(m.range),
    ExpressionGE::Deref(d) => Some(d.range),
    _ => None,
  }
}

/// The group rune a borrow parameter declares (`&T in g`), if any.
pub(crate) fn param_group_rune<'s>(param: &ParameterS<'s>) -> Option<IRuneS<'s>> {
  if let ITypeST::BorrowRef(st) = param.tyype {
    if let RegionS::Group(GroupS::Rune(ru)) = st.region {
      return Some(ru.rune);
    }
  }
  None
}

/// The root rune of an effect's group.
pub(crate) fn effect_root_rune<'s>(gs: &GroupS<'s>) -> Option<IRuneS<'s>> {
  match gs {
    GroupS::Rune(ru) => Some(ru.rune),
    GroupS::Member { base, .. } => effect_root_rune(base),
    GroupS::Elements { base } => effect_root_rune(base),
    GroupS::Ellipsis { base } => effect_root_rune(base),
    _ => None,
  }
}

/// The human name of a group rune (only code runes have one).
pub(crate) fn rune_name<'s>(rune: IRuneS<'s>) -> Option<StrI<'s>> {
  match rune {
    IRuneS::CodeRune(cn) => Some(cn.name),
    _ => None,
  }
}

/// The callee-rune → caller-group substitution for a call.
fn arg_rune_subst<'s, 't, 'g>(
  callee: &'s FunctionS<'s>,
  grouped_args: &[ExpressionGE<'s, 't, 'g>],
) -> IndexMap<IRuneS<'s>, GroupExprG<'s, 't, 'g>> {
  let mut subst = IndexMap::default();
  for (i, param) in callee.params.iter().enumerate() {
    if let ITypeST::BorrowRef(st) = param.tyype {
      if let RegionS::Group(GroupS::Rune(ru)) = st.region {
        if let Some(arg) = grouped_args.get(i) {
          if let KindGT::BorrowRef(b) = arg.result() {
            subst.insert(ru.rune, b.group.group);
          }
        }
      }
    }
  }
  subst
}
