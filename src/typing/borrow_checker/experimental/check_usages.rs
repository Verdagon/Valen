//! Phase 2: `check_usages` walks the grouped body **backward**, in reverse evaluation order, carrying
//! the reference values that still have a use later in the program. A churn met on the way back that
//! reaches a pending value is a use-after-churn, reported at that use. A node that produces a
//! reference (an element or member lookup, a call, a lend) retires its pending entry; a node that only
//! forwards a value (a local read, a `&&T→&T` decay, a block's last expression, an `if`'s arms, a
//! `let`) re-keys the entry to its source. Nothing is registered at a binding and no churn is
//! pre-applied: liveness runs from each use back to the birth, so `return e`, `set e.hp = 1`,
//! `__copy_prim(e)`, and a copied reference are caught exactly as a call argument is.
//!
//! Control flow: `if` walks each arm from the post-`if` set and unions the results; `while` walks its
//! body from the post-loop set and then checks what is still pending at the body's start against the
//! loop's churns (the back edge); `break` resumes from the innermost loop's post-loop set; `return`
//! from nothing. Errors are collected, never returned early, and all of them are reported in source
//! order. See `src/typing/docs/architecture/borrowing-design.md`.

use bumpalo::Bump;
use indexmap::IndexMap;

use crate::postparsing::ast::FunctionS;
use crate::postparsing::names::IRuneS;
use crate::postparsing::rules::types::EffectS;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::borrow_checker::ast_g::{ExpressionGE, GroupStep};
use crate::typing::borrow_checker::borrow_error::BorrowErrorKind;
use crate::typing::borrow_checker::check_usages_types::RefKey;
use crate::typing::borrow_checker::experimental::grouped_ast::{
  effect_root_rune, expr_range, flatten, group_expr_from_group_s, moved_local, node_range, param_group_rune,
  paths_alias, place_root_local, rune_name, sole_path, JointFact,
};
use crate::typing::borrow_checker::group_expr::GroupPathG;
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::borrow_checker::templata_g::ITemplataG;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::names::names::IVarNameT;
use crate::utils::range::RangeS;

/// A reference value with a use later in the program: where that use is, and every group path the
/// value may point into (its outer borrow, nested borrows, and citizen template args alike).
#[derive(Clone)]
struct Pending<'s, 't, 'g> {
  use_range: RangeS<'s>,
  mentions: Vec<GroupPathG<'s, 't, 'g>>,
}

/// The values still awaiting a later use, by the local holding them or the temporary they are.
type PendingMap<'s, 't, 'g> = IndexMap<RefKey<'s, 't>, Pending<'s, 't, 'g>>;

/// The walk's state: the pending uses, the post-loop set each enclosing loop resumes from at a
/// `break`, the errors found so far, and the counter that names held temporaries.
struct Walk<'s, 't, 'g> {
  declared_mut: Vec<Vec<GroupStep<'s, 't>>>,
  pending: PendingMap<'s, 't, 'g>,
  break_targets: Vec<PendingMap<'s, 't, 'g>>,
  errors: Vec<ICompileErrorT<'s, 't>>,
  next_held: u32,
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  /// Check the grouped body, returning every violation in source order.
  pub fn check_usages<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    body: ExpressionGE<'s, 't, 'g>,
    arena: &'g Bump,
  ) -> Result<(), Vec<ICompileErrorT<'s, 't>>> {
    let declared_mut: Vec<Vec<GroupStep<'s, 't>>> = function_s
      .effects
      .iter()
      .filter_map(|e| match e {
        EffectS::Mut(gs) => Some(gs),
        _ => None,
      })
      .flat_map(|gs| group_expr_from_group_s(gs, arena).iter().map(flatten))
      .collect();
    let mut walk = Walk {
      declared_mut,
      pending: IndexMap::default(),
      break_targets: vec![],
      errors: vec![],
      next_held: 0,
    };
    self.check_ge(coutputs, &mut walk, body, None);
    if walk.errors.is_empty() {
      return Ok(());
    }
    // The walk meets the last violation first; report them in program order. The sort is stable, so
    // two at one offset keep the order they were found in.
    let mut errors = walk.errors;
    errors.sort_by_key(|e| match e {
      ICompileErrorT::BorrowCheckError { range, .. } => range.begin.offset,
      _ => i32::MAX,
    });
    // One use is reported once, naming the first churn that reaches it: a use after an `if` whose
    // both arms churn, or after a loop whose body churns, is met once per path.
    errors.dedup_by(|later, earlier| same_use(earlier, later));
    Err(errors)
  }

  /// Walk one node backward. `key` is the pending entry this node's value is held under, if a later
  /// use wants it: the node either retires it (the value is born here) or forwards it to the child that
  /// supplies the value.
  fn check_ge<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    walk: &mut Walk<'s, 't, 'g>,
    node: ExpressionGE<'s, 't, 'g>,
    key: Option<RefKey<'s, 't>>,
  ) {
    match node {
      // A local read forwards its value: whoever uses this value uses the local.
      ExpressionGE::LocalLookup(l) => {
        if let Some(from) = key {
          walk.rekey(from, RefKey::Named(l.local_variable.name));
        }
      }
      ExpressionGE::Unlet(u) => {
        if let Some(from) = key {
          walk.rekey(from, RefKey::Named(u.variable.name));
        }
      }
      // Born here: a parameter is valid at entry, and a constant carries nothing.
      ExpressionGE::ArgLookup(_)
      | ExpressionGE::VoidLiteral(_)
      | ExpressionGE::ConstantInt(_)
      | ExpressionGE::ConstantBool(_)
      | ExpressionGE::ConstantStr(_)
      | ExpressionGE::ConstantFloat(_) => walk.retire(key),
      // `&&T→&T` decay forwards the value; a deref of a value is a load through the reference.
      ExpressionGE::Deref(d) => {
        if matches!(d.result, KindGT::BorrowRef(_)) {
          self.check_ge(coutputs, walk, d.inner, key);
        } else {
          walk.retire(key);
          let inner_key = walk.register(d.inner);
          self.check_ge(coutputs, walk, d.inner, inner_key);
        }
      }
      // A value load through the reference: a use of it.
      ExpressionGE::CopyPrim(e) => {
        walk.retire(key);
        let inner_key = walk.register(e.inner);
        self.check_ge(coutputs, walk, e.inner, inner_key);
      }
      // An element or member reference is born here, by reading through the base reference.
      ExpressionGE::RuntimeSizedArrayLookup(e) => {
        walk.retire(key);
        let base_key = walk.register(e.array_expr);
        self.check_ge(coutputs, walk, e.index_expr, None);
        self.check_ge(coutputs, walk, e.array_expr, base_key);
      }
      ExpressionGE::StaticSizedArrayLookup(e) => {
        walk.retire(key);
        let base_key = walk.register(e.array_expr);
        self.check_ge(coutputs, walk, e.index_expr, None);
        self.check_ge(coutputs, walk, e.array_expr, base_key);
      }
      ExpressionGE::MemberLookup(e) => {
        walk.retire(key);
        let base_key = walk.register(e.struct_expr);
        self.check_ge(coutputs, walk, e.struct_expr, base_key);
      }
      // A call: its result is born here (the callee guarantees a returned reference is valid at
      // return), then its churns happen — after every argument was evaluated, so before any argument
      // is walked — then each argument is a use.
      ExpressionGE::FunctionCall(call) => {
        walk.retire(key);
        for path in call.mut_effects.iter() {
          let steps: Vec<GroupStep<'s, 't>> = path.steps.to_vec();
          // Producer gate: a churn rooted at one of this function's parameter groups must be covered by
          // a declared `mut(...)`. A churn of a local the function owns needs no declaration.
          if is_param_rooted(&steps) && !walk.declared_mut.iter().any(|declared| path_covers(declared, &steps)) {
            walk.errors.push(self.borrow_error(BorrowErrorKind::UndeclaredChurn, call.range[0]));
          }
          self.churn(walk, &steps, path.range);
        }
        if let Some(fact) = self.joint_facts(coutputs, call.callable, call.args).first() {
          walk.errors.push(self.joint_error(fact));
        }
        self.check_args(coutputs, walk, call.args);
      }
      // A virtual, bound, or extern call declares no churns; its arguments are uses all the same.
      ExpressionGE::InterfaceFunctionCall(e) => {
        walk.retire(key);
        self.check_args(coutputs, walk, e.args);
      }
      ExpressionGE::ExternFunctionCall(e) => {
        walk.retire(key);
        self.check_args(coutputs, walk, e.args);
      }
      ExpressionGE::BoundFunctionCall(e) => {
        walk.retire(key);
        self.check_args(coutputs, walk, e.args);
      }
      // A binding forwards the local's pending use to its initializer. Not a use in itself: a stale
      // reference may be bound, and its staleness travels with it.
      ExpressionGE::LetNormal(e) => {
        walk.retire(key);
        let bound_key = walk.pending_key(RefKey::Named(e.variable.name));
        self.check_ge(coutputs, walk, e.expr, bound_key);
      }
      // The `&local` result is born here; the binding itself is as `LetNormal`.
      ExpressionGE::LetAndLend(e) => {
        walk.retire(key);
        let bound_key = walk.pending_key(RefKey::Named(e.variable.name));
        self.check_ge(coutputs, walk, e.expr, bound_key);
      }
      ExpressionGE::Restackify(e) => {
        walk.retire(key);
        let bound_key = walk.pending_key(RefKey::Named(e.variable.name));
        self.check_ge(coutputs, walk, e.source_expr, bound_key);
      }
      // A store. Into a bare local, the source supplies the local's later uses. Into a place reached
      // through a reference, the source is a use and the place's base reference is read (the lookup
      // rule registers it).
      ExpressionGE::Mutate(m) => {
        walk.retire(key);
        let source_key = match m.destination_expr {
          ExpressionGE::LocalLookup(l) => walk.pending_key(RefKey::Named(l.local_variable.name)),
          _ => walk.register(m.source_expr),
        };
        self.check_ge(coutputs, walk, m.source_expr, source_key);
        self.check_ge(coutputs, walk, m.destination_expr, None);
      }
      // Nothing after a return is reachable; the returned value is a use.
      ExpressionGE::Return(r) => {
        walk.retire(key);
        walk.pending.clear();
        let source_key = walk.register(r.source_expr);
        self.check_ge(coutputs, walk, r.source_expr, source_key);
      }
      // A break resumes from whatever is pending after the innermost loop.
      ExpressionGE::Break(_) => {
        walk.retire(key);
        walk.pending = walk.break_targets.last().cloned().expect("vfail: a break outside any loop");
      }
      // Each arm runs from the post-`if` set; before the `if`, whatever either arm left pending is.
      ExpressionGE::If(e) => {
        let post = walk.pending.clone();
        self.check_ge(coutputs, walk, e.then_call, key);
        let then_pending = std::mem::replace(&mut walk.pending, post);
        self.check_ge(coutputs, walk, e.else_call, key);
        let else_pending = std::mem::take(&mut walk.pending);
        walk.pending = then_pending;
        for (entry_key, entry) in else_pending {
          walk.insert(entry_key, entry);
        }
        self.check_ge(coutputs, walk, e.condition, None);
      }
      // The body runs from the post-loop set (the back edge and the exit both lead there). What is
      // still pending at the body's start was born outside the loop, so a churn anywhere in the body
      // spoils it on the next iteration.
      ExpressionGE::While(w) => {
        walk.retire(key);
        walk.break_targets.push(walk.pending.clone());
        self.check_ge(coutputs, walk, w.block.inner, None);
        walk.break_targets.pop();
        for path in w.mut_effects.iter() {
          self.churn(walk, path.steps, path.range);
        }
      }
      ExpressionGE::Block(b) => self.check_ge(coutputs, walk, b.inner, key),
      ExpressionGE::Consecutor(c) => {
        let last = c.exprs.len().saturating_sub(1);
        for (i, e) in c.exprs.iter().enumerate().rev() {
          self.check_ge(coutputs, walk, *e, if i == last { key } else { None });
        }
      }
      ExpressionGE::Discard(e) => {
        walk.retire(key);
        self.check_ge(coutputs, walk, e.expr, None);
      }
      // A destructure binds its destinations; the destructured value is a use.
      ExpressionGE::Destroy(e) => {
        walk.retire(key);
        for variable in e.destination_reference_variables.iter() {
          walk.pending.shift_remove(&RefKey::Named(variable.name));
        }
        let source_key = walk.register(e.expr);
        self.check_ge(coutputs, walk, e.expr, source_key);
      }
      ExpressionGE::DestroyStaticSizedArrayIntoLocals(e) => {
        walk.retire(key);
        for variable in e.destination_reference_variables.iter() {
          walk.pending.shift_remove(&RefKey::Named(variable.name));
        }
        let source_key = walk.register(e.expr);
        self.check_ge(coutputs, walk, e.expr, source_key);
      }
      // A cast keeps the operand's group, so it forwards the value.
      ExpressionGE::Reinterpret(e) => self.check_ge(coutputs, walk, e.expr, key),
      ExpressionGE::AsSubtype(e) => self.check_ge(coutputs, walk, e.source_expr, key),
      ExpressionGE::InterfaceToInterfaceUpcast(e) => self.check_ge(coutputs, walk, e.inner_expr, key),
      ExpressionGE::UpcastInterface(e) => self.check_ge(coutputs, walk, e.inner_expr, key),
      ExpressionGE::UpcastGeneric(e) => self.check_ge(coutputs, walk, e.inner_expr, key),
      // Every other node consumes its children: each reference-valued child is a use.
      other => {
        walk.retire(key);
        self.check_args(coutputs, walk, &other.children());
      }
    }
  }

  /// Consume a node's children: register every reference-valued child as a use first (a churn met in a
  /// later child must see the earlier ones pending), then walk them in reverse evaluation order.
  fn check_args<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    walk: &mut Walk<'s, 't, 'g>,
    args: &[ExpressionGE<'s, 't, 'g>],
  ) {
    let keys: Vec<Option<RefKey<'s, 't>>> = args.iter().map(|arg| walk.register(*arg)).collect();
    for (arg, arg_key) in args.iter().zip(keys).rev() {
      self.check_ge(coutputs, walk, *arg, arg_key);
    }
  }

  /// Apply a churn of `steps`, by the call at `churned_at`, to everything pending: each reached value
  /// is a use-after-churn, reported at its use naming the churn, and dropped (one report per use
  /// suffices).
  fn churn<'g>(&self, walk: &mut Walk<'s, 't, 'g>, steps: &[GroupStep<'s, 't>], churned_at: RangeS<'s>) {
    let reached: Vec<RefKey<'s, 't>> = walk
      .pending
      .iter()
      .filter(|(_, entry)| entry.mentions.iter().any(|mention| reaches(steps, mention)))
      .map(|(reached_key, _)| *reached_key)
      .collect();
    for reached_key in reached {
      let entry = walk.pending.shift_remove(&reached_key).expect("reached key vanished");
      let kind = match reached_key {
        RefKey::Named(_) => BorrowErrorKind::UseAfterChurn { local: reached_key, churned_at },
        RefKey::Held(_) => BorrowErrorKind::UseAfterChurnTemporary { churned_at },
      };
      walk.errors.push(self.borrow_error(kind, entry.use_range));
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

impl<'s, 't, 'g> Walk<'s, 't, 'g> {
  /// Mark a consumed child's value as pending, if it carries any group at all. The key names the
  /// local it reads (so a later churn reports "Used x") or a fresh held temporary otherwise.
  fn register(&mut self, expr: ExpressionGE<'s, 't, 'g>) -> Option<RefKey<'s, 't>> {
    let mentions = mentions_of(expr.result());
    if mentions.is_empty() {
      return None;
    }
    let (key, use_range) = self.use_key(expr);
    self.insert(key, Pending { use_range, mentions });
    Some(key)
  }

  /// How a consumed value is keyed and where its use is reported: a read of a local (through any
  /// decay) is that local at its own range; a call result is a held temporary at the call; anything
  /// else a held temporary at the node.
  fn use_key(&mut self, expr: ExpressionGE<'s, 't, 'g>) -> (RefKey<'s, 't>, RangeS<'s>) {
    let mut cur = expr;
    while let ExpressionGE::Deref(d) = cur {
      cur = d.inner;
    }
    match cur {
      ExpressionGE::LocalLookup(l) => (RefKey::Named(l.local_variable.name), l.range),
      ExpressionGE::Unlet(u) => (RefKey::Named(u.variable.name), u.range),
      other => {
        let held = RefKey::Held(self.next_held);
        self.next_held += 1;
        (held, node_range(other))
      }
    }
  }

  /// The key to hand a local's initializer: the local's pending entry, if any later use wants it.
  fn pending_key(&self, key: RefKey<'s, 't>) -> Option<RefKey<'s, 't>> {
    if self.pending.contains_key(&key) { Some(key) } else { None }
  }

  /// Insert a pending entry, merging into an existing one: the earlier use in program order is the
  /// one reported, and the value may point into either's groups.
  fn insert(&mut self, key: RefKey<'s, 't>, entry: Pending<'s, 't, 'g>) {
    match self.pending.get_mut(&key) {
      Some(existing) => {
        if entry.use_range.begin.offset < existing.use_range.begin.offset {
          existing.use_range = entry.use_range;
        }
        for mention in entry.mentions {
          if !existing.mentions.contains(&mention) {
            existing.mentions.push(mention);
          }
        }
      }
      None => {
        self.pending.insert(key, entry);
      }
    }
  }

  /// The value is born here: nothing before this point can spoil it.
  fn retire(&mut self, key: Option<RefKey<'s, 't>>) {
    if let Some(retired) = key {
      self.pending.shift_remove(&retired);
    }
  }

  /// The value under `from` is supplied by `to`: whoever spoils `to` before this point spoils it.
  fn rekey(&mut self, from: RefKey<'s, 't>, to: RefKey<'s, 't>) {
    if let Some(entry) = self.pending.shift_remove(&from) {
      self.insert(to, entry);
    }
  }
}

/// Whether two errors report the same use of the same thing, differing at most in which churn they
/// name.
fn same_use<'s, 't>(a: &ICompileErrorT<'s, 't>, b: &ICompileErrorT<'s, 't>) -> bool {
  match (a, b) {
    (
      ICompileErrorT::BorrowCheckError { range: range_a, kind: kind_a },
      ICompileErrorT::BorrowCheckError { range: range_b, kind: kind_b },
    ) => {
      range_a == range_b
        && match (kind_a, kind_b) {
          (
            BorrowErrorKind::UseAfterChurn { local: local_a, .. },
            BorrowErrorKind::UseAfterChurn { local: local_b, .. },
          ) => local_a == local_b,
          (BorrowErrorKind::UseAfterChurnTemporary { .. }, BorrowErrorKind::UseAfterChurnTemporary { .. }) => true,
          (BorrowErrorKind::UndeclaredChurn, BorrowErrorKind::UndeclaredChurn) => true,
          _ => false,
        }
    }
    _ => false,
  }
}

/// Whether a churn of `churned` reaches a reference into `mention`: a reference to a group *below*
/// the churned one dies if the way down crosses a destructible child edge (a collection element); a
/// reference to the group itself, or to an inline member, survives. An ellipsis reference, somewhere
/// in a group's territory, dies to a churn at, above, or below that group.
fn reaches<'s, 't, 'g>(churned: &[GroupStep<'s, 't>], mention: &GroupPathG<'s, 't, 'g>) -> bool {
  let mentioned = flatten(mention);
  if mention.ellipsis {
    let n = churned.len().min(mentioned.len());
    return churned[..n] == mentioned[..n];
  }
  churned.len() < mentioned.len()
    && mentioned[..churned.len()] == *churned
    && mentioned[churned.len()..].iter().any(|step| matches!(step, GroupStep::ChildElements))
}

/// Every group path a value of this kind may point into: its outer borrow, nested borrows, and the
/// group arguments of the citizens it holds (so an owned container of references is spoiled with them).
fn mentions_of<'s, 't, 'g>(kind: KindGT<'s, 't, 'g>) -> Vec<GroupPathG<'s, 't, 'g>> {
  let mut out = vec![];
  collect_mentions(kind, &mut out);
  out
}

fn collect_mentions<'s, 't, 'g>(kind: KindGT<'s, 't, 'g>, out: &mut Vec<GroupPathG<'s, 't, 'g>>) {
  match kind {
    KindGT::BorrowRef(b) => {
      out.extend(b.group.group.iter().copied());
      collect_mentions(b.inner, out);
    }
    KindGT::Struct(s) => collect_templata_mentions(s.template_args, out),
    KindGT::Interface(i) => collect_templata_mentions(i.template_args, out),
    KindGT::StaticSizedArray(a) => collect_mentions(a.element_type, out),
    KindGT::RuntimeSizedArray(a) => collect_mentions(a.element_type, out),
    KindGT::OwnRef(w) => collect_mentions(w.inner, out),
    KindGT::ShareRef(w) => collect_mentions(w.inner, out),
    KindGT::WeakRef(w) => collect_mentions(w.inner, out),
    KindGT::Never(_)
    | KindGT::Void(_)
    | KindGT::Int(_)
    | KindGT::Bool(_)
    | KindGT::Str(_)
    | KindGT::Float(_)
    | KindGT::USize(_)
    | KindGT::KindPlaceholder(_)
    | KindGT::OverloadSet(_) => {}
  }
}

fn collect_templata_mentions<'s, 't, 'g>(
  template_args: &[ITemplataG<'s, 't, 'g>],
  out: &mut Vec<GroupPathG<'s, 't, 'g>>,
) {
  for templata in template_args {
    match templata {
      ITemplataG::Kind(k) => collect_mentions(k.kind, out),
      ITemplataG::Group(g) => out.extend(g.group.iter().copied()),
      _ => {}
    }
  }
}

/// The group a result type borrows into, if it is a borrow reference.
fn result_borrow_group<'s, 't, 'g>(kind: KindGT<'s, 't, 'g>) -> Option<&'g [GroupPathG<'s, 't, 'g>]> {
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
