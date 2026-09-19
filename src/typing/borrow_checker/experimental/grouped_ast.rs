//! Experimental-local scaffolding over the canonical grouped AST (`borrow_checker::ast_g`).
//!
//! The grouped body is the canonical `ExpressionGE`, built by `groupify_function` and walked by
//! `check_usages`. This module holds what the walk needs that the canonical nodes don't carry: the
//! child order (`children`), the flat group-path helpers, and `JointFact`, the shape of a
//! joint-argument violation at a call. See `docs/architecture/borrowing-design.md`.

use bumpalo::Bump;

use crate::interner::StrI;
use crate::typing::borrow_checker::ast_g::{ExpressionGE, GroupStep};
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

/// One joint-argument violation candidate at a call, in the two shapes the checker reports.
#[derive(Clone)]
pub enum JointFact<'s, 't> {
  /// A borrow argument rooted in a local that a sibling argument moves.
  BorrowIntoMoved { local: IVarNameT<'s, 't>, borrow_arg: usize, move_arg: usize, range: RangeS<'s> },
  /// Two aliasing borrow arguments bound to parameters in distinct mutated groups.
  AliasingDisjointMut {
    local: IVarNameT<'s, 't>,
    arg_a: usize,
    arg_b: usize,
    group_a: StrI<'s>,
    group_b: StrI<'s>,
    range: RangeS<'s>,
  },
}

impl<'s, 't, 'g> ExpressionGE<'s, 't, 'g> {
  /// This node's child sub-expressions, in evaluation order.
  pub fn children(&self) -> Vec<ExpressionGE<'s, 't, 'g>> {
    match self {
      ExpressionGE::LetAndLend(e) => vec![e.expr],
      ExpressionGE::LockWeak(e) => vec![e.inner_expr],
      ExpressionGE::BorrowToWeak(e) => vec![e.inner_expr],
      ExpressionGE::LetNormal(e) => vec![e.expr],
      ExpressionGE::Unlet(_) => vec![],
      ExpressionGE::Discard(e) => vec![e.expr],
      ExpressionGE::If(e) => vec![e.condition, e.then_call, e.else_call],
      ExpressionGE::While(e) => vec![e.block.inner],
      ExpressionGE::Mutate(e) => vec![e.destination_expr, e.source_expr],
      ExpressionGE::Restackify(e) => vec![e.source_expr],
      ExpressionGE::Return(e) => vec![e.source_expr],
      ExpressionGE::Break(_) => vec![],
      ExpressionGE::Block(e) => vec![e.inner],
      ExpressionGE::Consecutor(e) => e.exprs.to_vec(),
      ExpressionGE::StaticArrayFromValues(e) => e.elements.to_vec(),
      ExpressionGE::ArraySize(e) => vec![e.array],
      ExpressionGE::IsSameInstance(e) => vec![e.left, e.right],
      ExpressionGE::AsSubtype(e) => vec![e.source_expr],
      ExpressionGE::VoidLiteral(_)
      | ExpressionGE::ConstantInt(_)
      | ExpressionGE::ConstantBool(_)
      | ExpressionGE::ConstantStr(_)
      | ExpressionGE::ConstantFloat(_)
      | ExpressionGE::ArgLookup(_)
      | ExpressionGE::LocalLookup(_) => vec![],
      ExpressionGE::ArrayLength(e) => vec![e.array_expr],
      ExpressionGE::InterfaceFunctionCall(e) => e.args.to_vec(),
      ExpressionGE::ExternFunctionCall(e) => e.args.to_vec(),
      ExpressionGE::FunctionCall(e) => e.args.to_vec(),
      ExpressionGE::BoundFunctionCall(e) => e.args.to_vec(),
      ExpressionGE::Reinterpret(e) => vec![e.expr],
      ExpressionGE::Construct(e) => e.args.to_vec(),
      ExpressionGE::NewRuntimeSizedArray(e) => vec![e.capacity_expr],
      ExpressionGE::StaticArrayFromCallable(e) => vec![e.generator],
      ExpressionGE::DestroyStaticSizedArrayIntoFunction(e) => vec![e.array_expr, e.consumer],
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(e) => vec![e.expr],
      ExpressionGE::DestroyRuntimeSizedArray(e) => vec![e.array_expr],
      ExpressionGE::RuntimeSizedArrayCapacity(e) => vec![e.array_expr],
      ExpressionGE::PushRuntimeSizedArray(e) => vec![e.array_expr, e.new_element_expr],
      ExpressionGE::PopRuntimeSizedArray(e) => vec![e.array_expr],
      ExpressionGE::InterfaceToInterfaceUpcast(e) => vec![e.inner_expr],
      ExpressionGE::UpcastInterface(e) => vec![e.inner_expr],
      ExpressionGE::UpcastGeneric(e) => vec![e.inner_expr],
      ExpressionGE::Destroy(e) => vec![e.expr],
      ExpressionGE::CopyPrim(e) => vec![e.inner],
      ExpressionGE::StaticSizedArrayLookup(e) => vec![e.array_expr, e.index_expr],
      ExpressionGE::RuntimeSizedArrayLookup(e) => vec![e.array_expr, e.index_expr],
      ExpressionGE::MemberLookup(e) => vec![e.struct_expr],
      ExpressionGE::Deref(e) => vec![e.inner],
    }
  }
}

/// Flatten one group path to its root-to-leaf step path. `ellipsis` is not a step: `mut(g...)` churns
/// exactly `mut(g)`, and an ellipsis reference's own invalidation is handled directly, not via
/// flattening.
pub fn flatten<'s, 't, 'g>(path: &GroupPathG<'s, 't, 'g>) -> Vec<GroupStep<'s, 't>> {
  let mut v = Vec::with_capacity(path.steps.len() + 1);
  v.push(match path.root {
    GroupRootG::Rune(r) => GroupStep::Rune(r),
    GroupRootG::ParamAnonymousGroup(n) => GroupStep::ParamAnonymousGroup(n),
    GroupRootG::Local(n) => GroupStep::Local(n),
  });
  for step in path.steps {
    v.push(match *step {
      GroupChildStepG::Member { member_name } => GroupStep::Member { member_name },
      GroupChildStepG::ChildElements {} => GroupStep::ChildElements,
      GroupChildStepG::InlineElements {} => GroupStep::InlineElements,
      GroupChildStepG::Variant { variant_name } => GroupStep::Variant { variant_name },
    });
  }
  v
}

/// The one path of a non-union group. A borrow into a union has no single path; nothing writes one yet.
pub fn sole_path<'s, 't, 'g>(group: GroupExprG<'s, 't, 'g>) -> &'g GroupPathG<'s, 't, 'g> {
  match group {
    [path] => path,
    _ => panic!("vfail: union borrow, unimplemented: {:?}", group),
  }
}

/// A group of exactly one path: `root` with no steps.
pub fn single_path<'s, 't, 'g>(arena: &'g Bump, root: GroupRootG<'s, 't>) -> GroupExprG<'s, 't, 'g> {
  arena.alloc_slice_copy(&[GroupPathG { root, steps: &[], ellipsis: false }])
}

/// `group` with `step` appended to every path: the child group one step below each.
pub fn with_step<'s, 't, 'g>(
  arena: &'g Bump,
  group: GroupExprG<'s, 't, 'g>,
  step: GroupChildStepG<'s>,
) -> GroupExprG<'s, 't, 'g> {
  let paths: Vec<GroupPathG<'s, 't, 'g>> = group
    .iter()
    .map(|p| {
      let mut steps = p.steps.to_vec();
      steps.push(step);
      GroupPathG { root: p.root, steps: arena.alloc_slice_copy(&steps), ellipsis: p.ellipsis }
    })
    .collect();
  arena.alloc_slice_copy(&paths)
}

/// Whether two flattened group paths overlap: one is a prefix of the other (nested), including equal.
pub(crate) fn paths_alias<'s, 't>(a: &[GroupStep<'s, 't>], b: &[GroupStep<'s, 't>]) -> bool {
  let n = a.len().min(b.len());
  a[..n] == b[..n]
}
