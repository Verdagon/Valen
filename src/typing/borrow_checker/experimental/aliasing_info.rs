//! `calculate_aliasing_info` — the borrow checker's third phase, on the canonical value types.
//!
//! It reports which parameters may be treated as `noalias` (a signature-only per-parameter verdict) and
//! the region-free group ground truth (which groups each instruction accesses), for the backend to feed
//! to LLVM as `restrict`-equivalent aliasing information. The result is arena-allocated in the check
//! arena; `function_compiler_core` copies it into the typing arena. The ground truth comes from the
//! access log `groupify_function` recorded, not a second tree walk.

use bumpalo::Bump;

use crate::interner::StrI;
use crate::postparsing::ast::FunctionS;
use crate::postparsing::rules::types::{ITypeST, RegionS};
use crate::typing::ast::ast::{FunctionDefinitionT, LocT};
use crate::typing::ast::borrowing_ast::{FunctionAliasingInfoT, GroupIdStepT, GroupIdT};
use indexmap::IndexMap;
use crate::typing::borrow_checker::access_event::AccessEventG;
use crate::typing::borrow_checker::ast_g::GroupStep;
use crate::typing::borrow_checker::experimental::grouped_ast::{flatten, paths_alias, sole_path};
use crate::typing::borrow_checker::experimental::groupify::rune_name;
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::compiler::Compiler;
use crate::typing::names::names::IVarNameT;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  /// Report which parameters are `noalias` and the region-free group ground truth, arena-allocated in
  /// `check_arena`, to feed the backend for optimization. The per-parameter verdict is signature-only: a
  /// borrow parameter qualifies when its group path is not aliased by any other parameter's, exactly as
  /// `groupify` derives it. The group facts come from the `access_log` walked out of `groupify_function`.
  pub(crate) fn calculate_aliasing_info<'g>(
    &self,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    access_log: &[AccessEventG<'s, 't>],
    check_arena: &'g Bump,
  ) -> &'g FunctionAliasingInfoT<'s, 'g>
  where
    's: 'g,
  {
    let params_t = &function_t.header.params;
    let paths: Vec<Option<Vec<GroupStep<'s, 't>>>> = (0..params_t.len())
      .map(|i| {
        let ps = function_s.params.get(i)?;
        match &ps.tyype {
          ITypeST::BorrowRef(st) if !matches!(st.region, RegionS::Held) => {
            match self.make_kind_g(params_t[i].tyype, &ps.tyype, Some(&params_t[i].name), check_arena) {
              KindGT::BorrowRef(b) => Some(flatten(sole_path(b.group.group))),
              _ => None,
            }
          }
          _ => None,
        }
      })
      .collect();
    let param_noalias: Vec<bool> = (0..paths.len())
      .map(|i| match &paths[i] {
        None => false,
        Some(path_i) => !paths.iter().enumerate().any(|(j, other)| {
          j != i && other.as_ref().map_or(false, |path_j| paths_alias(path_i, path_j))
        }),
      })
      .collect();
    let (group_paths, instruction_loc_to_accessed_groups) =
      compute_group_facts(access_log, &paths, check_arena);
    check_arena.alloc(FunctionAliasingInfoT {
      param_index_to_noalias: check_arena.alloc_slice_copy(&param_noalias),
      group_paths,
      instruction_loc_to_accessed_groups,
    })
  }

  /// Copy check-arena aliasing info into the typing arena, so it outlives the per-function check arena
  /// and can ride on `HinputsT` to the backend. A deep copy of small slices; interned `StrI` names ride
  /// along unchanged.
  pub(crate) fn copy_aliasing_info_to_typing_arena(
    &self,
    info: &FunctionAliasingInfoT<'s, '_>,
  ) -> &'t FunctionAliasingInfoT<'s, 't> {
    let group_paths: Vec<GroupIdT<'s, 't>> = info
      .group_paths
      .iter()
      .map(|g| GroupIdT { steps: self.typing_interner.alloc_slice_copy(g.steps) })
      .collect();
    let instr: Vec<(LocT<'t>, &'t [u32])> = info
      .instruction_loc_to_accessed_groups
      .iter()
      .map(|(loc, set)| {
        (
          LocT { path: self.typing_interner.alloc_slice_copy(loc.path) },
          self.typing_interner.alloc_slice_copy(set),
        )
      })
      .collect();
    self.typing_interner.alloc(FunctionAliasingInfoT {
      param_index_to_noalias: self.typing_interner.alloc_slice_copy(info.param_index_to_noalias),
      group_paths: self.typing_interner.alloc_slice_from_vec(group_paths),
      instruction_loc_to_accessed_groups: self.typing_interner.alloc_slice_from_vec(instr),
    })
  }
}

/// The region-free ground truth, arena-allocated. Number each distinct group — by its full path, so a
/// child group like `l.tiles[]` is its own scope, distinct from its parent `l` and its sibling `l.foes[]`
/// — by first appearance (P0: deterministic, no map iteration), numbering every parameter group first
/// (even one never accessed) then any further accessed group. Then record, per instruction, the set of
/// group indices it accesses: a load/store its single group, a call the downward closure of its
/// arguments' groups. The returned map is sorted by `LocT` path. The universe is the *accessed* groups
/// plus the parameter groups; a group an argument reaches but that nothing accesses is dropped rather
/// than inflating the scope count.
fn compute_group_facts<'s, 't, 'g>(
  log: &[AccessEventG<'s, 't>],
  param_paths: &[Option<Vec<GroupStep<'s, 't>>>],
  check_arena: &'g Bump,
) -> (&'g [GroupIdT<'s, 'g>], &'g [(LocT<'g>, &'g [u32])])
where
  's: 'g,
{
  let mut index_by_name: IndexMap<String, u32> = IndexMap::new();
  let mut groups: Vec<GroupIdT<'s, 'g>> = vec![];
  // Number every parameter's group first, in parameter order (so an unaccessed param group like the
  // design's `l`/`s` still gets a stable scope id), then any further groups reached only via access.
  for path in param_paths {
    if let Some(steps) = path {
      intern_group(steps, &mut index_by_name, &mut groups, check_arena);
    }
  }
  for ev in log {
    if let AccessEventG::Read { group, .. } | AccessEventG::Store { group, .. } = ev {
      intern_group(group, &mut index_by_name, &mut groups, check_arena);
    }
  }
  let group_index = |steps: &[GroupStep<'s, 't>]| -> Option<u32> {
    if steps.is_empty() {
      return None;
    }
    index_by_name.get(&group_name(&group_id_steps(steps))).copied()
  };
  // The unified map: each load/store/call's LocT -> the set of group indices it accesses.
  let mut entries: Vec<(LocT<'g>, &'g [u32])> = vec![];
  for ev in log {
    match ev {
      AccessEventG::Read { group, loct, .. } | AccessEventG::Store { group, loct, .. } => {
        if let Some(idx) = group_index(group) {
          let set: &'g [u32] = check_arena.alloc_slice_copy(&[idx]);
          entries.push((copy_loc(loct, check_arena), set));
        }
      }
      AccessEventG::Call { touched: arg_groups, loct } => {
        // A call reaches, through each argument, that argument's group and every descendant group —
        // the downward closure over the universe (a group whose path has the argument's as a prefix).
        // It does NOT reach ancestors (you can't get to the parent from an element reference).
        let mut touched: Vec<u32> = vec![];
        for ag in arg_groups {
          if ag.first().is_none() {
            continue;
          }
          let ag_steps = group_id_steps(ag);
          for (j, g) in groups.iter().enumerate() {
            if g.steps.len() >= ag_steps.len() && g.steps[..ag_steps.len()] == ag_steps[..] {
              let idx = j as u32;
              if !touched.contains(&idx) {
                touched.push(idx);
              }
            }
          }
        }
        touched.sort_unstable();
        let set: &'g [u32] = check_arena.alloc_slice_copy(&touched);
        entries.push((copy_loc(loct, check_arena), set));
      }
      AccessEventG::Marker { .. } => {}
    }
  }
  entries.sort_by(|a, b| a.0.path.cmp(b.0.path));
  (check_arena.alloc_slice_copy(&groups), check_arena.alloc_slice_copy(&entries))
}

/// Copy a `LocT`'s path into the check arena, so the returned facts hold no reference to the typed AST.
fn copy_loc<'g>(loct: &LocT<'_>, check_arena: &'g Bump) -> LocT<'g> {
  LocT { path: check_arena.alloc_slice_copy(loct.path) }
}

/// Intern a group path by name into the scope-id table, assigning the next id on first appearance and
/// arena-allocating its steps.
fn intern_group<'s, 't, 'g>(
  steps: &[GroupStep<'s, 't>],
  index_by_name: &mut IndexMap<String, u32>,
  groups: &mut Vec<GroupIdT<'s, 'g>>,
  check_arena: &'g Bump,
) where
  's: 'g,
{
  if steps.first().is_none() {
    return;
  }
  let id_steps = group_id_steps(steps);
  let name = group_name(&id_steps);
  if !index_by_name.contains_key(&name) {
    let next = index_by_name.len() as u32;
    index_by_name.insert(name, next);
    groups.push(GroupIdT { steps: check_arena.alloc_slice_copy(&id_steps) });
  }
}

/// The owned mirror of a flattened group path, for interning and comparison (no arena needed).
fn group_id_steps<'s, 't>(steps: &[GroupStep<'s, 't>]) -> Vec<GroupIdStepT<'s>> {
  steps.iter().map(group_id_step).collect()
}

fn group_id_step<'s, 't>(step: &GroupStep<'s, 't>) -> GroupIdStepT<'s> {
  match step {
    GroupStep::Rune(r) => GroupIdStepT::Rune(rune_name(*r).unwrap_or(StrI("?"))),
    GroupStep::ParamAnonymousGroup(v) => GroupIdStepT::ParamAnonymousGroup(var_name_stri(v)),
    GroupStep::Local(v) => GroupIdStepT::Local(var_name_stri(v)),
    GroupStep::Member { member_name } => GroupIdStepT::Member(*member_name),
    // Both `[]` element groups render as the elements step; the destructibility distinction matters to
    // the checker, not to the backend's region id.
    GroupStep::ChildElements | GroupStep::InlineElements => GroupIdStepT::Elements,
    GroupStep::Variant { .. } => {
      panic!("vfail: variant group step in aliasing info is not yet supported")
    }
  }
}

/// The source-level name of a group path, for the scope-id table key and diagnostics.
fn group_name(steps: &[GroupIdStepT]) -> String {
  let mut s = String::new();
  for step in steps {
    match step {
      GroupIdStepT::Rune(n) | GroupIdStepT::ParamAnonymousGroup(n) | GroupIdStepT::Local(n) => {
        s.push_str(n.0)
      }
      GroupIdStepT::Member(n) => {
        s.push('.');
        s.push_str(n.0)
      }
      GroupIdStepT::Elements => s.push_str("[]"),
    }
  }
  s
}

/// A variable's interned name, for rendering a group that names a parameter or local.
fn var_name_stri<'s, 't>(v: &IVarNameT<'s, 't>) -> StrI<'s> {
  match v {
    IVarNameT::Local(ln) => ln.imprecise_name.name,
    IVarNameT::Member(mn) => mn.imprecise_name.name,
    _ => StrI("?"),
  }
}
