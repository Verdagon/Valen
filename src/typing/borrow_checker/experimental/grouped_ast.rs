//! Experimental-local scaffolding over the canonical grouped AST (`borrow_checker::ast_g`).
//!
//! The grouped body is the canonical `ExpressionGE`, built by `groupify_function` and walked by
//! `check_usages`. This module holds what the walk needs that the canonical nodes don't carry: the
//! child order (`children`), the flat group-path helpers, and `JointFact`, the shape of a
//! joint-argument violation at a call. See `src/typing/docs/architecture/borrowing-design.md`.

use bumpalo::Bump;

use crate::interner::StrI;
use crate::postparsing::ast::ParameterS;
use crate::postparsing::names::IRuneS;
use crate::postparsing::rules::types::{GroupS, ITypeST, RegionS};
use crate::typing::borrow_checker::ast_g::{ExpressionGE, GroupStep, MutEffectPath};
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::KindGT;
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
      ExpressionGE::NarrowInterface(e) => vec![e.inner_expr],
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

/// Whether two flattened group paths overlap: one is a prefix of the other (nested), including equal.
pub(crate) fn paths_alias<'s, 't>(a: &[GroupStep<'s, 't>], b: &[GroupStep<'s, 't>]) -> bool {
  let n = a.len().min(b.len());
  a[..n] == b[..n]
}

/// Convert a scout-side `GroupS` to a `GroupExprG`: one path per union member, each walked root to leaf,
/// allocated in `arena`. A group rune carries its own scout identity, so a root needs no frame.
/// `Elements` maps to `ChildElements` (the destructible collection child group — the only kind a written
/// group produces).
pub(crate) fn group_expr_from_group_s<'s, 't, 'g>(
  group: &'s GroupS<'s>,
  arena: &'g Bump,
) -> GroupExprG<'s, 't, 'g> {
  match group {
    GroupS::Union { members } => {
      let paths: Vec<GroupPathG<'s, 't, 'g>> =
        members.iter().flat_map(|m| group_expr_from_group_s(m, arena).iter().copied()).collect();
      arena.alloc_slice_copy(&paths)
    }
    other => {
      let (root, steps, ellipsis) = group_path_from_group_s(other);
      arena.alloc_slice_copy(&[GroupPathG { root, steps: arena.alloc_slice_copy(&steps), ellipsis }])
    }
  }
}

/// One non-union written group as its root, its root-to-leaf steps, and whether it ends in `...`.
fn group_path_from_group_s<'s, 't>(
  group: &'s GroupS<'s>,
) -> (GroupRootG<'s, 't>, Vec<GroupChildStepG<'s>>, bool) {
  match group {
    GroupS::Rune(ru) => (GroupRootG::Rune(ru.rune), vec![], false),
    GroupS::Local(_) => panic!(
      "vfail: a group written as a local name (`in x`) is not yet supported"
    ),
    GroupS::Member { base, member_name } => {
      let (root, mut steps, ellipsis) = group_path_from_group_s(base);
      steps.push(GroupChildStepG::Member { member_name: *member_name });
      (root, steps, ellipsis)
    }
    GroupS::Elements { base } => {
      let (root, mut steps, ellipsis) = group_path_from_group_s(base);
      steps.push(GroupChildStepG::ChildElements {});
      (root, steps, ellipsis)
    }
    GroupS::Ellipsis { base } => {
      let (root, steps, _) = group_path_from_group_s(base);
      (root, steps, true)
    }
    GroupS::Union { .. } => panic!("vfail: a union nested inside a group path"),
  }
}

/// Every churn inside a grouped subtree, for a loop's aggregated `mut_effects`: a reference is spoiled
/// on the loop's first iteration by a churn from any later one. The loop shares the calls' paths.
pub(crate) fn collect_subtree_churns<'s, 't, 'g>(
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

/// The innermost local a grouped place expression is rooted in.
// VLOOOOK: Option return — needs VOPT approval or removal
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

/// Any grouped node's source range. A call carries several; the first is the call itself.
pub(crate) fn node_range<'s, 't, 'g>(expr: ExpressionGE<'s, 't, 'g>) -> RangeS<'s> {
  match expr {
    ExpressionGE::LetAndLend(e) => e.range,
    ExpressionGE::LockWeak(e) => e.range,
    ExpressionGE::BorrowToWeak(e) => e.range,
    ExpressionGE::LetNormal(e) => e.range,
    ExpressionGE::Unlet(e) => e.range,
    ExpressionGE::Discard(e) => e.range,
    ExpressionGE::If(e) => e.range,
    ExpressionGE::While(e) => e.range,
    ExpressionGE::Mutate(e) => e.range,
    ExpressionGE::Restackify(e) => e.range,
    ExpressionGE::Return(e) => e.range,
    ExpressionGE::Break(e) => e.range,
    ExpressionGE::Block(e) => e.range,
    ExpressionGE::Consecutor(e) => e.range,
    ExpressionGE::StaticArrayFromValues(e) => e.range,
    ExpressionGE::ArraySize(e) => e.range,
    ExpressionGE::IsSameInstance(e) => e.range,
    ExpressionGE::AsSubtype(e) => e.range,
    ExpressionGE::VoidLiteral(e) => e.range,
    ExpressionGE::ConstantInt(e) => e.range,
    ExpressionGE::ConstantBool(e) => e.range,
    ExpressionGE::ConstantStr(e) => e.range,
    ExpressionGE::ConstantFloat(e) => e.range,
    ExpressionGE::ArgLookup(e) => e.range,
    ExpressionGE::LocalLookup(e) => e.range,
    ExpressionGE::ArrayLength(e) => e.range,
    ExpressionGE::InterfaceFunctionCall(e) => e.range,
    ExpressionGE::ExternFunctionCall(e) => e.range,
    ExpressionGE::FunctionCall(e) => e.range.first().copied().expect("vfail: a call with no range"),
    ExpressionGE::BoundFunctionCall(e) => e.range,
    ExpressionGE::Reinterpret(e) => e.range,
    ExpressionGE::Construct(e) => e.range,
    ExpressionGE::NewRuntimeSizedArray(e) => e.range,
    ExpressionGE::StaticArrayFromCallable(e) => e.range,
    ExpressionGE::DestroyStaticSizedArrayIntoFunction(e) => e.range,
    ExpressionGE::DestroyStaticSizedArrayIntoLocals(e) => e.range,
    ExpressionGE::DestroyRuntimeSizedArray(e) => e.range,
    ExpressionGE::RuntimeSizedArrayCapacity(e) => e.range,
    ExpressionGE::PushRuntimeSizedArray(e) => e.range,
    ExpressionGE::PopRuntimeSizedArray(e) => e.range,
    ExpressionGE::InterfaceToInterfaceUpcast(e) => e.range,
    ExpressionGE::UpcastInterface(e) => e.range,
    ExpressionGE::UpcastGeneric(e) => e.range,
    ExpressionGE::Destroy(e) => e.range,
    ExpressionGE::CopyPrim(e) => e.range,
    ExpressionGE::StaticSizedArrayLookup(e) => e.range,
    ExpressionGE::RuntimeSizedArrayLookup(e) => e.range,
    ExpressionGE::MemberLookup(e) => e.range,
    ExpressionGE::Deref(e) => e.range,
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
