//! Phase 2: `check_usages` walks the grouped body and rejects a use of a reference a churn spoiled.
//!
//! It threads a `GroupSubtree` — a tree of groups as the function knows them — registering each live
//! reference under the group(s) it points at, and stamping `invalidated_by` on the references a churn
//! spoils. It walks the canonical `ExpressionGE` in evaluation order; the four nodes the checker acts on
//! are handled directly. A `let` of a borrow registers the local under the borrow's group; `if` runs
//! each branch from the pre-branch state and unions the non-diverging invalidations; `while` pre-applies
//! its body's churns, so there is no fixpoint. See `docs/architecture/borrowing-design.md`.

use bumpalo::Bump;
use indexmap::IndexMap;

use crate::postparsing::ast::FunctionS;
use crate::postparsing::names::IRuneS;
use crate::postparsing::rules::types::EffectS;
use crate::typing::ast::ast::{LocT, PrototypeT};
use crate::typing::borrow_checker::ast_g::{ExpressionGE, GroupStep};
use crate::typing::borrow_checker::borrow_error::BorrowErrorKind;
use crate::typing::borrow_checker::check_usages_types::{GroupSubtree, LocalEntry, RefKey};
use crate::typing::borrow_checker::experimental::borrow_types::group_expr_from_group_s;
use crate::typing::borrow_checker::experimental::grouped_ast::{flatten, paths_alias, sole_path, JointFact};
use crate::typing::borrow_checker::experimental::groupify::{
  diverges, effect_root_rune, expr_range, held_range, moved_local, param_group_rune, place_root_local,
  rune_name,
};
use crate::typing::borrow_checker::group_expr::GroupExprG;
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

/// A fresh empty group subtree — canonical `GroupSubtree` has no `Default`, so build it explicitly.
fn empty_subtree<'s, 't>() -> GroupSubtree<'s, 't> {
  GroupSubtree {
    locals: IndexMap::default(),
    locals_in_ellipsis: IndexMap::default(),
    name_to_child: IndexMap::default(),
  }
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  /// Check the grouped body, returning the first violation.
  pub fn check_usages<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    body: ExpressionGE<'s, 't, 'g>,
    arena: &'g Bump,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    let declared_mut: Vec<Vec<GroupStep<'s, 't>>> = function_s
      .effects
      .iter()
      .filter_map(|e| match e {
        EffectS::Mut(gs) => Some(gs),
        _ => None,
      })
      .flat_map(|gs| group_expr_from_group_s(gs, arena).iter().map(flatten))
      .collect();
    let mut tree = empty_subtree();
    let mut next_held = 0;
    self.check_ge(coutputs, &declared_mut, body, &mut tree, &mut next_held)
  }

  fn check_ge<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    declared_mut: &[Vec<GroupStep<'s, 't>>],
    node: ExpressionGE<'s, 't, 'g>,
    tree: &mut GroupSubtree<'s, 't>,
    next_held: &mut u32,
  ) -> Result<(), ICompileErrorT<'s, 't>> {
    match node {
      ExpressionGE::LetNormal(e) => {
        self.check_ge(coutputs, declared_mut, e.expr, tree, next_held)?;
        // A `let` of a borrow binds the local to the group the borrow points into.
        if let KindGT::BorrowRef(b) = e.expr.result() {
          register(tree, RefKey::Named(e.variable.name), b.group.group);
        }
        Ok(())
      }
      ExpressionGE::FunctionCall(call) => {
        let mut held_keys = vec![];
        for arg in call.args.iter().copied() {
          self.check_ge(coutputs, declared_mut, arg, tree, next_held)?;
          if let Some((group, range)) = held_register_of(arg) {
            let key = RefKey::Held(*next_held);
            *next_held += 1;
            register(tree, key, group);
            held_keys.push((key, range));
          }
        }
        for arg in call.args.iter().copied() {
          if let Some((local, range)) = arg_ref_use(arg) {
            if is_use_after_churn(tree, RefKey::Named(local)) {
              return Err(self.borrow_error(BorrowErrorKind::UseAfterChurn { local: RefKey::Named(local) }, range));
            }
          }
        }
        for (key, range) in &held_keys {
          if is_use_after_churn(tree, *key) {
            return Err(self.borrow_error(BorrowErrorKind::UseAfterChurnTemporary, *range));
          }
        }
        if let Some(fact) = self.joint_facts(coutputs, call.callable, call.args).first() {
          return Err(self.joint_error(fact));
        }
        for path in call.mut_effects.iter() {
          let steps: Vec<GroupStep<'s, 't>> = path.steps.to_vec();
          // Producer gate: a churn rooted at one of this function's parameter groups must be covered by
          // a declared `mut(...)`. A churn of a local the function owns needs no declaration.
          if is_param_rooted(&steps)
            && !declared_mut.iter().any(|declared| path_covers(declared, &steps))
          {
            return Err(self.borrow_error(BorrowErrorKind::UndeclaredChurn, call.range[0]));
          }
          churn(tree, &steps, path.effecting_node_loc);
        }
        Ok(())
      }
      ExpressionGE::If(e) => {
        self.check_ge(coutputs, declared_mut, e.condition, tree, next_held)?;
        let mut then_tree = tree.clone();
        self.check_ge(coutputs, declared_mut, e.then_call, &mut then_tree, next_held)?;
        let mut else_tree = tree.clone();
        self.check_ge(coutputs, declared_mut, e.else_call, &mut else_tree, next_held)?;
        merge(tree, &then_tree, &else_tree, diverges(e.then_call), diverges(e.else_call));
        Ok(())
      }
      ExpressionGE::While(w) => {
        // Pre-apply every churn the body can perform: a reference is spoiled on the loop's first
        // iteration by a churn from any later iteration.
        for path in w.mut_effects.iter() {
          churn(tree, path.steps, path.effecting_node_loc);
        }
        self.check_ge(coutputs, declared_mut, w.block.inner, tree, next_held)
      }
      other => {
        for child in other.children() {
          self.check_ge(coutputs, declared_mut, child, tree, next_held)?;
        }
        Ok(())
      }
    }
  }

  /// The joint-argument facts at a call: a borrow into a moved argument, and aliasing borrows into
  /// distinct mutated groups. Empty when the callee cannot be resolved.
  fn joint_facts<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    callable: &'t PrototypeT<'s, 't>,
    args: &'g [ExpressionGE<'s, 't, 'g>],
  ) -> Vec<JointFact<'s, 't>> {
    let Some(callee) = self.resolve_callee(coutputs, callable) else {
      return vec![];
    };
    let places: Vec<Option<(IVarNameT<'s, 't>, Vec<GroupStep<'s, 't>>, RangeS<'s>)>> = args
      .iter()
      .copied()
      .map(|arg| {
        let root = place_root_local(arg)?;
        let group = result_borrow_group(arg.result())?;
        let range = expr_range(arg)?;
        Some((root, flatten(sole_path(group)), range))
      })
      .collect();
    let moves: Vec<Option<IVarNameT<'s, 't>>> = args.iter().copied().map(moved_local).collect();

    let mut facts = vec![];
    for (i, place) in places.iter().enumerate() {
      if let Some((root_i, _, range_i)) = place {
        for (j, moved) in moves.iter().enumerate() {
          if i != j {
            if let Some(moved) = moved {
              if root_i == moved {
                facts.push(JointFact::BorrowIntoMoved {
                  local: *moved,
                  borrow_arg: i,
                  move_arg: j,
                  range: *range_i,
                });
              }
            }
          }
        }
      }
    }

    let param_runes: Vec<Option<IRuneS<'s>>> = callee.params.iter().map(param_group_rune).collect();
    let mutated: Vec<IRuneS<'s>> = callee
      .effects
      .iter()
      .filter_map(|e| match e {
        EffectS::Mut(gs) => effect_root_rune(gs),
        _ => None,
      })
      .collect();
    for i in 0..args.len() {
      for j in (i + 1)..args.len() {
        if let (Some((root_i, path_i, range_i)), Some((_, path_j, _))) = (&places[i], &places[j]) {
          if let (Some(ri), Some(rj)) =
            (param_runes.get(i).copied().flatten(), param_runes.get(j).copied().flatten())
          {
            if ri != rj && (mutated.contains(&ri) || mutated.contains(&rj)) && paths_alias(path_i, path_j) {
              if let (Some(ga), Some(gb)) = (rune_name(ri), rune_name(rj)) {
                facts.push(JointFact::AliasingDisjointMut {
                  local: *root_i,
                  arg_a: i,
                  arg_b: j,
                  group_a: ga,
                  group_b: gb,
                  range: *range_i,
                });
              }
            }
          }
        }
      }
    }
    facts
  }

  /// Build the compile error for a joint-argument fact.
  fn joint_error(&self, fact: &JointFact<'s, 't>) -> ICompileErrorT<'s, 't> {
    match fact {
      JointFact::BorrowIntoMoved { local, borrow_arg, move_arg, range } => self.borrow_error(
        BorrowErrorKind::BorrowIntoMovedArgument {
          local: *local,
          borrow_arg: *borrow_arg,
          move_arg: *move_arg,
        },
        *range,
      ),
      JointFact::AliasingDisjointMut { local, arg_a, arg_b, group_a, group_b, range } => self
        .borrow_error(
          BorrowErrorKind::AliasingIntoDisjointMutGroups {
            local: *local,
            arg_a: *arg_a,
            arg_b: *arg_b,
            group_a: *group_a,
            group_b: *group_b,
          },
          *range,
        ),
    }
  }
}

/// A named-local reference used as a value argument (`local` or `*local`): the local and its range.
fn arg_ref_use<'s, 't, 'g>(arg: ExpressionGE<'s, 't, 'g>) -> Option<(IVarNameT<'s, 't>, RangeS<'s>)> {
  let inner = match arg {
    ExpressionGE::Deref(d) => d.inner,
    other => other,
  };
  if let ExpressionGE::LocalLookup(l) = inner {
    return Some((l.local_variable.name, l.range));
  }
  None
}

/// If an argument is an unnamed held reference, its group and the range to diagnose at.
fn held_register_of<'s, 't, 'g>(
  arg: ExpressionGE<'s, 't, 'g>,
) -> Option<(GroupExprG<'s, 't, 'g>, RangeS<'s>)> {
  if arg_ref_use(arg).is_some() {
    return None;
  }
  let group = result_borrow_group(arg.result())?;
  let range = held_range(arg)?;
  Some((group, range))
}

/// The group a result type borrows into, if it is a borrow reference.
fn result_borrow_group<'s, 't, 'g>(kind: KindGT<'s, 't, 'g>) -> Option<GroupExprG<'s, 't, 'g>> {
  match kind {
    KindGT::BorrowRef(b) => Some(b.group.group),
    _ => None,
  }
}

/// Whether a churn path is rooted at a parameter's group (so it needs a declared `mut`).
fn is_param_rooted<'s, 't>(steps: &[GroupStep<'s, 't>]) -> bool {
  matches!(steps.first(), Some(GroupStep::Rune(_)) | Some(GroupStep::ParamAnonymousGroup(_)))
}

/// Whether a declared `mut` path covers a churn: the declared group's path is a prefix of the churned.
fn path_covers<'s, 't>(declared: &[GroupStep<'s, 't>], churn: &[GroupStep<'s, 't>]) -> bool {
  declared.len() <= churn.len() && churn[..declared.len()] == *declared
}

/// Register a live reference under each group its `GroupExprG` names: at the path's node, in
/// `locals_in_ellipsis` for a `g...` reference and in `locals` otherwise.
fn register<'s, 't, 'g>(root: &mut GroupSubtree<'s, 't>, key: RefKey<'s, 't>, group: GroupExprG<'s, 't, 'g>) {
  for path in group {
    let node = navigate(root, &flatten(path));
    let entries = if path.ellipsis { &mut node.locals_in_ellipsis } else { &mut node.locals };
    entries.entry(key).or_insert_with(|| LocalEntry { invalidated_by: None });
  }
}

/// Walk (creating as needed) to the subtree at `steps`.
fn navigate<'a, 's, 't>(
  node: &'a mut GroupSubtree<'s, 't>,
  steps: &[GroupStep<'s, 't>],
) -> &'a mut GroupSubtree<'s, 't> {
  let mut cur = node;
  for step in steps {
    cur = cur.name_to_child.entry(*step).or_insert_with(empty_subtree);
  }
  cur
}

/// Apply a `mut(g)` churn. Invalidate every reference into `g`'s child-elements descendants, and every
/// ellipsis reference at, below, or above `g`. A reference to `g` itself, or to an inline member,
/// survives.
fn churn<'s, 't>(root: &mut GroupSubtree<'s, 't>, steps: &[GroupStep<'s, 't>], loc: LocT<'t>) {
  match steps.split_first() {
    Some((first, rest)) => {
      stamp(&mut root.locals_in_ellipsis, loc);
      if let Some(child) = root.name_to_child.get_mut(first) {
        churn(child, rest, loc);
      }
    }
    None => invalidate_from_churned(root, loc, false),
  }
}

/// Invalidate everything at and below the churned group: its own ellipsis references and every
/// descendant's, plus the `locals` of any descendant reached by crossing a child-elements edge. The
/// churned group's own `locals` survive.
fn invalidate_from_churned<'s, 't>(
  node: &mut GroupSubtree<'s, 't>,
  loc: LocT<'t>,
  crossed_elements: bool,
) {
  stamp(&mut node.locals_in_ellipsis, loc);
  if crossed_elements {
    stamp(&mut node.locals, loc);
  }
  for (step, child) in node.name_to_child.iter_mut() {
    let crossed = crossed_elements || matches!(step, GroupStep::ChildElements);
    invalidate_from_churned(child, loc, crossed);
  }
}

/// Stamp `invalidated_by` on every not-yet-invalidated entry.
fn stamp<'s, 't>(entries: &mut IndexMap<RefKey<'s, 't>, LocalEntry<'t>>, loc: LocT<'t>) {
  for entry in entries.values_mut() {
    if entry.invalidated_by.is_none() {
      entry.invalidated_by = Some(loc);
    }
  }
}

/// Whether any registered entry for `key` (at any group it points into) has been invalidated.
fn is_use_after_churn<'s, 't>(node: &GroupSubtree<'s, 't>, key: RefKey<'s, 't>) -> bool {
  node.locals.get(&key).is_some_and(|e| e.invalidated_by.is_some())
    || node.locals_in_ellipsis.get(&key).is_some_and(|e| e.invalidated_by.is_some())
    || node.name_to_child.values().any(|c| is_use_after_churn(c, key))
}

/// Merge two branch states back into the pre-branch tree.
fn merge<'s, 't>(
  orig: &mut GroupSubtree<'s, 't>,
  then_tree: &GroupSubtree<'s, 't>,
  else_tree: &GroupSubtree<'s, 't>,
  then_diverges: bool,
  else_diverges: bool,
) {
  merge_entries(&mut orig.locals, &then_tree.locals, &else_tree.locals, then_diverges, else_diverges);
  merge_entries(
    &mut orig.locals_in_ellipsis,
    &then_tree.locals_in_ellipsis,
    &else_tree.locals_in_ellipsis,
    then_diverges,
    else_diverges,
  );
  for (step, child) in orig.name_to_child.iter_mut() {
    let then_child = then_tree.name_to_child.get(step).expect("branch dropped a group edge");
    let else_child = else_tree.name_to_child.get(step).expect("branch dropped a group edge");
    merge(child, then_child, else_child, then_diverges, else_diverges);
  }
}

/// Merge one node's entries: an entry stays invalidated iff a non-diverging branch invalidated it.
fn merge_entries<'s, 't>(
  orig: &mut IndexMap<RefKey<'s, 't>, LocalEntry<'t>>,
  then_m: &IndexMap<RefKey<'s, 't>, LocalEntry<'t>>,
  else_m: &IndexMap<RefKey<'s, 't>, LocalEntry<'t>>,
  then_diverges: bool,
  else_diverges: bool,
) {
  for (key, base) in orig.iter_mut() {
    let then_inv =
      if then_diverges { None } else { then_m.get(key).expect("branch dropped an entry").invalidated_by };
    let else_inv =
      if else_diverges { None } else { else_m.get(key).expect("branch dropped an entry").invalidated_by };
    base.invalidated_by =
      if then_diverges && else_diverges { base.invalidated_by } else { then_inv.or(else_inv) };
  }
}
