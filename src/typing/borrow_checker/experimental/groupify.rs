//! Phase 1: `groupify_function` walks the typed body and produces the canonical grouped `ExpressionGE`
//! (`borrow_checker::ast_g`), every node fully populated from its typed twin. For every
//! reference-typed local it records the group its referent lives in; at every call it records the
//! groups it churns (`mut_effects`) and the groups its arguments reach (the access log). Everything is
//! allocated in the `'g` check arena.
//!
//! Groups are read off written types: a function's own group runes are registered from its parameter
//! list (`build_rune_map`), and a callee's runes are bound to the caller's groups at each call
//! (`match_types`), so every written `in g` resolves through whichever rune map it is handed.

use std::marker::PhantomData;
use bumpalo::Bump;
use indexmap::IndexMap;
use crate::postparsing::ast::{FunctionS, ICitizenDenizenS, IStructMemberS, StructS};
use crate::postparsing::names::{CodeNameS, IFunctionDeclarationNameS, IRuneS};
use crate::postparsing::rules::RuneUsage;
use crate::postparsing::rules::types::*;
use crate::StrI;
use crate::typing::ast::ast::{FunctionDefinitionT, LocT, PrototypeT};
use crate::typing::ast::citizens::StructDefinitionT;
use crate::typing::ast::expressions::*;
use crate::typing::borrow_checker::access_event::AccessEventG;
use crate::typing::borrow_checker::ast_g::*;
use crate::typing::borrow_checker::experimental::grouped_ast::{
  collect_subtree_churns, diverges, flatten, group_expr_from_group_s, sole_path,
};
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::*;
use crate::typing::borrow_checker::templata_g::*;
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::env::function_environment_t::LocalVariable;
use crate::typing::names::names::*;
use crate::typing::templata::templata::*;
use crate::typing::types::types::*;

/// The flattened group a borrow result points into, or `None` for a non-borrow.
fn borrowref_group<'s, 't, 'g>(k: KindGT<'s, 't, 'g>) -> Option<Vec<GroupStep<'s, 't>>> {
  match k {
    KindGT::BorrowRef(b) => Some(flatten(sole_path(b.group.group))),
    _ => None,
  }
}

/// The written type a typed kind is walked against, plus the parameter it belongs to (`None` outside a
/// parameter), which keys an unannotated borrow's anonymous group.
struct WrittenContext<'s, 't> {
  type_s: ITypeST<'s>,
  name: Option<IVarNameT<'s, 't>>,
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  pub fn groupify_function<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
  ) -> Result<(ExpressionGE<'s, 't, 'g>, Vec<AccessEventG<'s, 't>>), ICompileErrorT<'s, 't>> {
    let mut access_log = Vec::new();
    let local_rune_to_templata = self.build_rune_map(coutputs, function_s, function_t, bump_g);

    // Will be populated as we go.
    let mut local_to_type_g = IndexMap::new();

    // TODO: use function_s to compare to any expression_t that we find, that will let us manually
    // specify things' groups, for example in let statements.
    // For now, just use the typed expressions.
    let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, &mut access_log, function_t.body, &local_rune_to_templata, &mut local_to_type_g)?;

    Ok((expr_ge, access_log))
  }

  /// Each parameter's flattened group path — a borrow parameter's `in g` (or anonymous) group, exactly
  /// as `ArgLookup` derives it — or `None` for a non-borrow or `held` parameter.
  /// `calculate_aliasing_info` reads the per-parameter `noalias` verdict off these.
  pub(crate) fn param_group_paths<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
  ) -> Vec<Option<Vec<GroupStep<'s, 't>>>> {
    let local_rune_to_templata = self.build_rune_map(coutputs, function_s, function_t, bump_g);
    let local_to_type_g = IndexMap::new();
    function_s
      .params
      .iter()
      .zip(function_t.header.params.iter())
      .map(|(param_st, param_tt)| match param_st.tyype {
        ITypeST::BorrowRef(st) if !matches!(st.region, RegionS::Held) => {
          let written_context = WrittenContext { type_s: param_st.tyype, name: Some(param_tt.name) };
          let param_gt = self.groupify_type(
            coutputs,
            bump_g,
            &local_rune_to_templata,
            &local_to_type_g,
            param_tt.tyype,
            Some(&written_context),
          );
          match param_gt {
            KindGT::BorrowRef(b) => Some(flatten(sole_path(b.group.group))),
            _ => None,
          }
        }
        _ => None,
      })
      .collect()
  }

  /// Resolve a call's callee to its scout `FunctionS` via the template id. `None` for a callee whose
  /// typed name is not a function instantiation (a lambda's `__call`, a forwarder reached by dispatch),
  /// which the call then treats as groupless with no churns.
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

  /// The function's own rune map: a placeholder for each non-group generic parameter, then a
  /// `GroupTemplataG` for each group rune a parameter's written type names.
  fn build_rune_map<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
  ) -> IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> {
    // Insert PlaceholderTemplataG's for any non-group generic parameters, like the `T` in:
    //     func observe<T, tg'>(x &T in tg) { }
    let mut local_rune_to_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> = IndexMap::new();
    let func_local_name = IFunctionNameT::try_from(function_t.header.id.local_name).expect("Function doesn't have a function name?");
    for (generic_param_s, templata_t) in function_s.generic_params.iter().zip(func_local_name.template_args().iter()) {
      match templata_t {
        ITemplataT::Placeholder(PlaceholderTemplataT { id: placeholder_id_t, tyype: placeholder_templata_type_t }) => {
          local_rune_to_templata.insert(
            generic_param_s.rune.rune,
            ITemplataG::Placeholder(
              bump_g.alloc(PlaceholderTemplataG {
                id: *placeholder_id_t,
                tyype: *placeholder_templata_type_t
              })));
        }
        ITemplataT::Kind(KindTemplataT { kind: KindT::KindPlaceholder(KindPlaceholderT { id }) }) => {
          local_rune_to_templata.insert(
            generic_param_s.rune.rune,
            ITemplataG::Kind(KindTemplataG {
              kind: KindGT::KindPlaceholder(
                bump_g.alloc(KindPlaceholderGT {
                  id: *id,
                  _phantom: PhantomData
                })
              )
            }));
        }
        ITemplataT::Group(GroupTemplataT {}) => {} // Skip, we'll handle them below.
        // A borrow argument's group is only knowable where the rune is used with a parameter at hand
        // (a lambda called at `&int`): leave the rune unbound, and the parameter's own anonymous group
        // takes over there.
        ITemplataT::Kind(KindTemplataT { kind: KindT::BorrowRef(_) }) => {}
        // An instantiation being checked with a concrete argument in place of its generic parameter
        // (a lambda called at a type, say): the rune stands for that argument.
        other => {
          local_rune_to_templata.insert(
            generic_param_s.rune.rune,
            self.groupify_templata(coutputs, bump_g, &local_rune_to_templata, &IndexMap::new(), *other));
        }
      };
    }

    // Now that we have PlaceholderTemplataG's for every non-group generic parameter,
    // let's populate our map with GroupTemplataG's for every group parameter, like the `tg` in:
    //     func observe<T, tg'>(x &T in tg) { }
    //
    // Let's do a simple pass over all of the function parameters to try and deduce their types.
    // For example, in:
    //     func observe<T, tg'>(x &T in tg) { }
    // we already have T = PlaceholderTemplataG("T"),
    // and we now want to deduce that "tg is a group of T".
    // So we do a recurse over each parameter (`&T in tg`) looking for any borrow refs
    // whose group is just a rune (like this one, `in tg`) so we can assign their value (`T` because
    // `&T in tg`) as the group's type and register it into our map.
    for (param_st, param_tt) in function_s.params.iter().zip(function_t.header.params.iter()) {
      self.register_group_runes(
        coutputs, bump_g, &mut local_rune_to_templata, Some(param_tt.name), param_st.tyype, param_tt.tyype);
    }
    local_rune_to_templata
  }

  fn new_rune_group_expr<'g>(bump_g: &'g Bump, rune: IRuneS<'s>) -> &'g [GroupPathG<'s, 't, 'g>] {
    bump_g.alloc_slice_copy(&[
      *bump_g.alloc(GroupPathG {
        root: GroupRootG::Rune(rune),
        steps: &[],
        ellipsis: false
      })
    ])
  }

  fn new_local_group_expr<'g>(bump_g: &'g Bump, var_name: IVarNameT<'s, 't>) -> &'g [GroupPathG<'s, 't, 'g>] {
    bump_g.alloc_slice_copy(&[
      *bump_g.alloc(GroupPathG {
        root: GroupRootG::Local(var_name),
        steps: &[],
        ellipsis: false
      })
    ])
  }

  /// A borrow's group when its written type carries no `in g`: the parameter's anonymous group. Only
  /// a parameter's own borrow layers reach here; a borrow with no `in g` and no parameter context has
  /// no derivable group (a deferred case) and panics.
  fn anonymous_group_expr<'g>(
    bump_g: &'g Bump,
    param_name: Option<IVarNameT<'s, 't>>,
  ) -> &'g [GroupPathG<'s, 't, 'g>] {
    match param_name {
      Some(name) => bump_g.alloc_slice_copy(&[GroupPathG {
        root: GroupRootG::ParamAnonymousGroup(name),
        steps: &[],
        ellipsis: false,
      }]),
      None => panic!("vfail: borrow with no group and no parameter context — a deferred case"),
    }
  }

  /// The root reference an access chain goes through, and the flat group that reference points into.
  /// Walks the `member`/`deref`/`lookup` wrappers down to the root `LocalLookup`/`ArgLookup`, capturing
  /// the base reference's name and its group plus a `Member` per member step and a `ChildElements` per
  /// runtime-sized-array element step (a static-sized array is inline, so its element step adds nothing —
  /// the fold). The composed path is truncated after its last elements step, since trailing inline members
  /// share that heap element's group — so `a.fuel` reports `g` (root, no element crossed) while
  /// `lvl.tiles[0]` reports `l.tiles[]` and its sibling `lvl.foes[0]` reports `l.foes[]`. `None` for a
  /// chain rooted in something unnamed (a temporary, a call result) or a non-borrow.
  fn base_ref_and_group<'g>(
    &self,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    expr: ExpressionGE<'s, 't, 'g>,
  ) -> Option<(IVarNameT<'s, 't>, Vec<GroupStep<'s, 't>>)> {
    // access→root order; reversed below into root→access order.
    let mut steps_rev: Vec<GroupStep<'s, 't>> = vec![];
    let mut cur = expr;
    let (base_ref, root) = loop {
      match cur {
        ExpressionGE::LocalLookup(l) => {
          break (l.local_variable.name, borrowref_group(l.local_variable.tyype)?);
        }
        ExpressionGE::ArgLookup(a) => {
          let name = function_t.header.params.get(a.param_index as usize)?.name;
          break (name, borrowref_group(a.result)?);
        }
        ExpressionGE::Deref(d) => cur = d.inner,
        ExpressionGE::CopyPrim(e) => cur = e.inner,
        ExpressionGE::MemberLookup(e) => {
          let member_name = match &e.member_name {
            IVarNameT::Member(cv) => cv.imprecise_name.name,
            IVarNameT::Local(cv) => cv.imprecise_name.name,
            _ => return None,
          };
          steps_rev.push(GroupStep::Member { member_name });
          cur = e.struct_expr;
        }
        ExpressionGE::RuntimeSizedArrayLookup(e) => {
          steps_rev.push(GroupStep::ChildElements);
          cur = e.array_expr;
        }
        // Inline static-sized array: its elements share the parent group, so no group step (the fold).
        ExpressionGE::StaticSizedArrayLookup(e) => cur = e.array_expr,
        _ => return None,
      }
    };
    let root_len = root.len();
    let mut full = root;
    full.extend(steps_rev.into_iter().rev());
    match full.iter().rposition(|s| matches!(s, GroupStep::ChildElements | GroupStep::InlineElements)) {
      Some(last) => full.truncate(last + 1),
      None => full.truncate(root_len),
    }
    Some((base_ref, full))
  }

  /// Groupify a node's child expressions in order, into an arena slice.
  fn groupify_exprs<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
    access_log: &mut Vec<AccessEventG<'s, 't>>,
    exprs_te: &[ExpressionTE<'s, 't>],
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &mut IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
  ) -> Result<&'g [ExpressionGE<'s, 't, 'g>], ICompileErrorT<'s, 't>> {
    let mut exprs_ge = Vec::new();
    for expr_te in exprs_te.iter() {
      exprs_ge.push(
        self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr_te, local_rune_to_templata, local_to_type_g)?);
    }
    Ok(bump_g.alloc_slice_copy(exprs_ge.as_slice()))
  }

  /// The grouped mirror of a typed struct kind (its args groupless).
  fn struct_gt<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    s: &'t StructTT<'s, 't>,
  ) -> &'g StructGT<'s, 't, 'g> {
    match self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, KindT::Struct(s), None) {
      KindGT::Struct(struct_gt) => struct_gt,
      other => panic!("vfail: {:?}", other),
    }
  }

  /// The grouped mirror of a typed interface kind (its args groupless).
  fn interface_gt<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    i: &'t InterfaceTT<'s, 't>,
  ) -> &'g InterfaceGT<'s, 't, 'g> {
    match self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, KindT::Interface(i), None) {
      KindGT::Interface(interface_gt) => interface_gt,
      other => panic!("vfail: {:?}", other),
    }
  }

  /// The grouped mirror of a typed static-sized array kind, its element groupless.
  fn ssa_gt<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    a: &'t StaticSizedArrayTT<'s, 't>,
  ) -> &'g StaticSizedArrayGT<'s, 't, 'g> {
    match self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, KindT::StaticSizedArray(a), None) {
      KindGT::StaticSizedArray(ssa_gt) => ssa_gt,
      other => panic!("vfail: {:?}", other),
    }
  }

  /// The grouped mirror of a typed runtime-sized array kind, its element groupless.
  fn rsa_gt<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    a: &'t RuntimeSizedArrayTT<'s, 't>,
  ) -> &'g RuntimeSizedArrayGT<'s, 't, 'g> {
    match self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, KindT::RuntimeSizedArray(a), None) {
      KindGT::RuntimeSizedArray(rsa_gt) => rsa_gt,
      other => panic!("vfail: {:?}", other),
    }
  }

  /// The grouped mirror of an upcast's target super kind.
  fn super_kind_gt<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    super_kind: ISuperKindTT<'s, 't>,
  ) -> ISuperKindGT<'s, 't, 'g> {
    match super_kind {
      ISuperKindTT::Interface(i) => ISuperKindGT::Interface(self.interface_gt(coutputs, bump_g, rune_to_templata, local_to_type_g, i)),
      ISuperKindTT::KindPlaceholder(p) => ISuperKindGT::KindPlaceholder(p),
    }
  }

  /// A cast keeps the operand's outer group and re-expresses the referent's structure groupless.
  fn cast_result<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    cast_kind: KindT<'s, 't>,
    operand: KindGT<'s, 't, 'g>,
  ) -> KindGT<'s, 't, 'g> {
    match (cast_kind, operand) {
      (KindT::BorrowRef(b), KindGT::BorrowRef(ob)) => {
        let inner = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, b.inner, None);
        KindGT::BorrowRef(bump_g.alloc(BorrowRefGT {
          inner,
          group: GroupTemplataG { group: ob.group.group, kind: inner, born_at: ob.group.born_at },
        }))
      }
      (other, _) => self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, other, None),
    }
  }

  /// A citizen's grouped generic args. With the citizen's written type at hand (`List<&Header in g>`,
  /// a `Call` with one argument per templata) each `Kind` arg is groupified against its written
  /// argument, so a borrow inside the args gets its group as a top-level borrow does. Written as
  /// anything else, the args mirror groupless.
  fn citizen_args_in<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    template_args_t: &'t [ITemplataT<'s, 't>],
    maybe_written: Option<&WrittenContext<'s, 't>>,
  ) -> &'g [ITemplataG<'s, 't, 'g>] {
    let written_args: Option<&'s [&'s ITypeST<'s>]> = match maybe_written.map(|w| w.type_s) {
      Some(ITypeST::Call(c)) if c.args.len() == template_args_t.len() => Some(c.args),
      _ => None,
    };
    let template_args_g: Vec<ITemplataG<'s, 't, 'g>> = template_args_t
      .iter()
      .enumerate()
      .map(|(i, templata_t)| match (templata_t, written_args) {
        (ITemplataT::Kind(k), Some(args)) => {
          let written_arg = WrittenContext { type_s: *args[i], name: maybe_written.and_then(|w| w.name) };
          ITemplataG::Kind(KindTemplataG {
            kind: self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, k.kind, Some(&written_arg)),
          })
        }
        // A group argument written as a rune, e.g. the `g` in `Vec<T, g>`. No citizen declares a group
        // parameter yet, and the group's referent type is not knowable from the argument alone.
        (ITemplataT::Group(_), Some(args)) if matches!(args[i], ITypeST::Rune(_)) => {
          unimplemented!("vfail: citizen group parameter")
        }
        (other, _) => self.groupify_templata(coutputs, bump_g, rune_to_templata, local_to_type_g, *other),
      })
      .collect();
    bump_g.alloc_slice_copy(template_args_g.as_slice())
  }

  fn groupify_expression<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    function_s: &'s FunctionS<'s>,
    function_t: &'t FunctionDefinitionT<'s, 't>,
    bump_g: &'g Bump,
    access_log: &mut Vec<AccessEventG<'s, 't>>,
    expression_te: ExpressionTE<'s, 't>,
    local_rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &mut IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
  ) -> Result<ExpressionGE<'s, 't, 'g>, ICompileErrorT<'s, 't>> {
    match expression_te {
      ExpressionTE::LetNormal(LetNormalTE { range, variable, expr, result, .. }) => {
        let result_gt = KindGT::Void(VoidGT { });
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        let variable_g =
          bump_g.alloc(LocalVariableG {
            name: variable.name,
            tyype: expr_ge.result()
          });
        local_to_type_g.insert(variable.name,expr_ge.result());
        // let variable_g = self.groupify_var(bump_g, *variable, local_rune_to_templata, local_to_type_g);
        Ok(ExpressionGE::LetNormal(bump_g.alloc(LetNormalGE { range: *range, variable: variable_g, expr: expr_ge, result: result_gt, })))
      }
      ExpressionTE::LocalLookup(LocalLookupTE { range, loct, local_variable, result, .. }) => {
        let local_gt = local_to_type_g.get(&local_variable.name).expect("Couldn't find local variable");
        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: local_variable.name,
              tyype: *local_gt
            });
        let group_expr = Self::new_local_group_expr(bump_g, local_variable.name);
        let group_templata_g =
            GroupTemplataG {
              group: group_expr,
              kind: *local_gt,
              born_at: *loct,
            };
        Ok(
          ExpressionGE::LocalLookup(
            bump_g.alloc(LocalLookupGE {
              range: *range,
              loct: *loct,
              local_variable: variable_g,
              result: bump_g.alloc(BorrowRefGT { inner: *local_gt, group: group_templata_g })
            })))
      }
      ExpressionTE::LetAndLend(LetAndLendTE { range, loct, variable, expr, result, .. }) => {
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        let local_gt = expr_ge.result();

        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: variable.name,
              tyype: local_gt
            });
        local_to_type_g.insert(variable.name,expr_ge.result());

        let group_templata_g =
            GroupTemplataG {
              group: Self::new_local_group_expr(bump_g, variable.name),
              kind: local_gt,
              born_at: *loct,
            };
        Ok(
          ExpressionGE::LetAndLend(
            bump_g.alloc(LetAndLendGE {
              range: *range,
              loct: *loct,
              variable: variable_g,
              expr: expr_ge,
              result: bump_g.alloc(BorrowRefGT { inner: local_gt, group: group_templata_g })
            })))
      }
      ExpressionTE::Unlet(UnletTE { range, variable: local_variable, result, .. }) => {
        let local_gt = local_to_type_g.get(&local_variable.name).expect("Couldn't find local variable");
        let variable_g =
            bump_g.alloc(LocalVariableG {
              name: local_variable.name,
              tyype: *local_gt
            });
        Ok(
          ExpressionGE::Unlet(
            bump_g.alloc(UnletGE {
              range: *range,
              variable: variable_g,
              result: *local_gt,
            })))
      }
      ExpressionTE::Discard(DiscardTE { range, expr, result, .. }) => {
        let expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr, local_rune_to_templata, local_to_type_g)?;
        Ok(
          ExpressionGE::Discard(
            bump_g.alloc(DiscardGE {
              range: *range,
              expr: expr_ge,
              result: KindGT::Void(VoidGT { }),
            })))
      }
      ExpressionTE::Return(ReturnTE { range, source_expr, result, .. }) => {
        let source_expr_g = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_expr, local_rune_to_templata, local_to_type_g)?;
        // `Never`: the typed node's result, which is what `diverges` reads.
        let result_gt = self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *result, None);
        Ok(ExpressionGE::Return(bump_g.alloc(ReturnGE { range: *range, source_expr: source_expr_g, result: result_gt, })))
      }
      ExpressionTE::Block(BlockTE { range, inner, result, .. }) => {
        let inner_g = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *inner, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Block(bump_g.alloc(BlockGE { range: *range, inner: inner_g, result: inner_g.result(), })))
      }
      ExpressionTE::Consecutor(ConsecutorTE { range, exprs: exprs_te, result: result_tt, .. }) => {
        let mut exprs_ge = Vec::new();
        for expr_te in exprs_te.iter() {
          exprs_ge.push(
            self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr_te, local_rune_to_templata, local_to_type_g)?);
        }
        let exprs_ge_slice = bump_g.alloc_slice_copy(exprs_ge.as_slice());
        // As in the typed node: a `Never` anywhere makes the sequence `Never`, else the last result.
        let result_gt = match exprs_ge_slice.iter().copied().find(|e| diverges(*e)) {
          Some(diverging) => diverging.result(),
          None => exprs_ge_slice.last().expect("Expected nonempty Consecutor").result(),
        };
        Ok(ExpressionGE::Consecutor(bump_g.alloc(ConsecutorGE { range: *range, exprs: exprs_ge_slice, result: result_gt, })))
      }
      ExpressionTE::ConstantInt(ConstantIntTE { range, value, bits, .. }) => {
        let value_gt = self.groupify_templata(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *value);
        let result_gt = KindGT::Int(IntGT { bits: *bits });
        Ok(ExpressionGE::ConstantInt(bump_g.alloc(ConstantIntGE { range: *range, value: value_gt, bits: *bits, result: result_gt, })))
      }
      ExpressionTE::ConstantBool(ConstantBoolTE { range, value, .. }) => {
        let result_gt = KindGT::Bool(BoolGT { });
        Ok(ExpressionGE::ConstantBool(bump_g.alloc(ConstantBoolGE { range: *range, value: *value, result: result_gt, })))
      }
      ExpressionTE::ConstantFloat(ConstantFloatTE { range, value, .. }) => {
        let result_gt = KindGT::Float(FloatGT { });
        Ok(ExpressionGE::ConstantFloat(bump_g.alloc(ConstantFloatGE { range: *range, value: *value, result: result_gt, })))
      }
      ExpressionTE::ArgLookup(ArgLookupTE { range, loct, param_index, result, .. }) => {
        let param_type_t = function_t.header.params[*param_index as usize].tyype;
        let param_type_s = function_s.params[*param_index as usize].tyype;
        let param_name = function_t.header.params[*param_index as usize].name;
        let written_context = WrittenContext {
          type_s: param_type_s,
          name: Some(param_name),
        };
        let result_gt = self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, param_type_t, Some(&written_context));
        Ok(ExpressionGE::ArgLookup(bump_g.alloc(ArgLookupGE { range: *range, loct: *loct, param_index: *param_index, result: result_gt, })))
      }
      ExpressionTE::ArrayLength(ArrayLengthTE { range, array_expr, result, .. }) => {
        let array_expr_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *array_expr, local_rune_to_templata, local_to_type_g)?;
        let result_gt = KindGT::Int(IntGT { bits: 32 });
        Ok(ExpressionGE::ArrayLength(bump_g.alloc(ArrayLengthGE { range: *range, array_expr: array_expr_ge, result: result_gt, })))
      }
      ExpressionTE::Deref(DerefTE { range, loct, inner: source_te, result: result_tt, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        let source_borrow_gt = expect_borrowref_gt(source_ge.result());
        let result_gt = source_borrow_gt.inner;
        // A deref that yields a reference is `&&T→&T` decay, not a load; only a deref of a value reads.
        if !matches!(result_gt, KindGT::BorrowRef(_)) {
          if let Some((base_ref, group)) = self.base_ref_and_group(function_t, source_ge) {
            access_log.push(AccessEventG::Read { base_ref, group, loct: *loct });
          }
        }
        Ok(ExpressionGE::Deref(bump_g.alloc(DerefGE { range: *range, loct: *loct, inner: source_ge, result: result_gt, })))
      }
      ExpressionTE::FunctionCall(FunctionCallTE { loct, range, callable, args: arg_exprs_te, result, .. }) => {
        let mut arg_exprs_ge: Vec<ExpressionGE<'s, 't, 'g>> = Vec::new();
        for expr_te in arg_exprs_te.iter() {
          arg_exprs_ge.push(
            self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *expr_te, local_rune_to_templata, local_to_type_g)?);
        }
        let arg_exprs_ge_slice: &'g [ExpressionGE<'s, 't, 'g>] =
            bump_g.alloc_slice_copy(arg_exprs_ge.as_slice());

        // let callable_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *callable)?;

        let (result_gt, mut_effects_slice) =
          match self.resolve_callee(coutputs, callable) {
            Some(callee_func_s) => {
              let callee_rune_to_caller_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> =
                  self.calculate_callee_rune_to_caller_templata(coutputs, bump_g, callee_func_s, &arg_exprs_ge);
              // Every non-lambda callee has a written return type; only a lambda's is inferred, so only
              // a lambda falls back to the typed return, groupless.
              let result_gt =
                match (callee_func_s.maybe_return_type, callee_func_s.name) {
                  (Some(return_st), _) => {
                    let written_context = WrittenContext { type_s: return_st, name: None };
                    self.groupify_type(coutputs, bump_g, &callee_rune_to_caller_templata, local_to_type_g, *result, Some(&written_context))
                  }
                  (None, IFunctionDeclarationNameS::LambdaDeclarationName(_)) => {
                    self.groupify_type(coutputs, bump_g, &callee_rune_to_caller_templata, local_to_type_g, *result, None)
                  }
                  (None, name) => panic!("vfail: non-lambda callee has no written return type: {:?}", name),
                };
              let mut mut_effects: Vec<&'g MutEffectPath> = Vec::new();
              for callee_effect_s in callee_func_s.effects {
                for steps in self.groupify_effect(coutputs, bump_g, callee_effect_s, &callee_rune_to_caller_templata) {
                  mut_effects.push(bump_g.alloc(MutEffectPath { effecting_node_loc: *loct, range: range[0], steps }));
                }
              }
              let mut_effects_slice: &'g [&'g MutEffectPath] = bump_g.alloc_slice_copy(mut_effects.as_slice());
              (result_gt, mut_effects_slice)
            }
            // No callee signature to read groups or effects off: groupless, churns nothing.
            None => {
              let result_gt = self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *result, None);
              (result_gt, &[][..])
            }
          };
        // A call reaches every group its arguments carry — not merely the groups it declares it mutates.
        // Valen declares only mutation (`mut(g)`), never reads, and read-only aliasing is legal, so the
        // backend `!noalias`'s a call only against groups its arguments cannot reach (argument-reachability
        // keeps it sound). `compute_group_facts` closes these downward to their descendants.
        let touched: Vec<Vec<GroupStep<'s, 't>>> =
          arg_exprs_ge_slice.iter().filter_map(|a| borrowref_group(a.result())).collect();
        access_log.push(AccessEventG::Call { touched, loct: *loct });

        Ok(ExpressionGE::FunctionCall(bump_g.alloc(FunctionCallGE {
          loct: *loct,
          range: *range,
          callable: callable,
          args: arg_exprs_ge_slice,
          result: result_gt,
          mut_effects: mut_effects_slice
        })))
      }
      ExpressionTE::Destroy(DestroyTE { range, loct, expr: source_te, struct_tt, destination_reference_variables, result, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        let source_struct_gt =
          match source_ge.result() {
            KindGT::Struct(s) => s,
            _ => panic!("Expected struct for Destroy"),
          };

        let member_name_to_locally_phrased_member_type =
            self.translate_struct_members(coutputs, bump_g, *source_struct_gt);
        assert!(member_name_to_locally_phrased_member_type.len() == destination_reference_variables.len());
        let mut dest_vars_g = Vec::new();
        for ((_, local_type_g), dest_var) in member_name_to_locally_phrased_member_type.iter().zip(destination_reference_variables.iter()) {
          let LocalVariable { name: var_name, tyype: _ } = dest_var;
          let variable_g =
              bump_g.alloc(LocalVariableG {
                name: *var_name,
                tyype: *local_type_g
              });
          dest_vars_g.push(&*variable_g);
          local_to_type_g.insert(*var_name, *local_type_g);
        }
        let dest_vars_g_slice = bump_g.alloc_slice_copy(dest_vars_g.as_slice());

        Ok(ExpressionGE::Destroy(bump_g.alloc(DestroyGE {
          range: *range,
          loct: *loct,
          expr: source_ge,
          struct_tt: source_struct_gt,
          destination_reference_variables: dest_vars_g_slice,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::VoidLiteral(VoidLiteralTE { range, result, .. }) => {
        Ok(ExpressionGE::VoidLiteral(bump_g.alloc(VoidLiteralGE {
          range: *range,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::LockWeak(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::LockWeak(bump_g.alloc(LockWeakGE {
          range: e.range,
          inner_expr,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          some_constructor: e.some_constructor,
          none_constructor: e.none_constructor,
          some_impl_name: e.some_impl_name,
          none_impl_name: e.none_impl_name,
        })))
      }
      ExpressionTE::BorrowToWeak(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        // A weak reference carries no group.
        let result = bump_g.alloc(WeakRefGT {
          inner: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result.inner, None),
        });
        Ok(ExpressionGE::BorrowToWeak(bump_g.alloc(BorrowToWeakGE { range: e.range, inner_expr, result })))
      }
      ExpressionTE::If(IfTE { range, loct, condition, then_call, else_call, result, .. }) => {
        let condition_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *condition, local_rune_to_templata, local_to_type_g)?;
        let then_call_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *then_call, local_rune_to_templata, local_to_type_g)?;
        let else_call_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *else_call, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::If(bump_g.alloc(IfGE {
          range: *range,
          loct: *loct,
          condition: condition_ge,
          then_call: then_call_ge,
          else_call: else_call_ge,
          result: if diverges(then_call_ge) { else_call_ge.result() } else { then_call_ge.result() },
        })))
      }
      ExpressionTE::While(WhileTE { range, loct, block: block_te, result, .. }) => {
        let BlockTE { range: block_range, inner: block_inner, .. } = block_te;
        let block_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *block_inner, local_rune_to_templata, local_to_type_g)?;
        // The loop churns everything its body churns: a reference is spoiled on the first iteration by a
        // churn from any later one.
        let mut churns = Vec::new();
        collect_subtree_churns(block_ge, &mut churns);
        Ok(ExpressionGE::While(bump_g.alloc(WhileGE {
          range: *range,
          loct: *loct,
          block: BlockGE { range: *block_range, inner: block_ge, result: block_ge.result() },
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *result, None),
          mut_effects: bump_g.alloc_slice_copy(churns.as_slice()),
        })))
      }
      ExpressionTE::Mutate(m) => {
        let destination_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, m.destination_expr, local_rune_to_templata, local_to_type_g)?;
        let source_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, m.source_expr, local_rune_to_templata, local_to_type_g)?;
        if let Some((base_ref, group)) = self.base_ref_and_group(function_t, destination_expr) {
          access_log.push(AccessEventG::Store { base_ref, group, loct: m.loct });
        }
        // The replaced value: the destination place's referent, with its groups.
        let result = expect_borrowref_gt(destination_expr.result()).inner;
        Ok(ExpressionGE::Mutate(bump_g.alloc(MutateGE {
          range: m.range,
          loct: m.loct,
          destination_expr,
          source_expr,
          result,
        })))
      }
      ExpressionTE::Restackify(e) => {
        // The local keeps the type it was bound with.
        let local_gt = *local_to_type_g.get(&e.variable.name).expect("Couldn't find local variable");
        let variable = bump_g.alloc(LocalVariableG { name: e.variable.name, tyype: local_gt });
        let source_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.source_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Restackify(bump_g.alloc(RestackifyGE {
          range: e.range,
          variable,
          source_expr,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::Break(BreakTE { range, result, .. }) => {
        Ok(ExpressionGE::Break(bump_g.alloc(BreakGE {
          range: *range,
          result: KindGT::Never(NeverGT { from_break: true }),
        })))
      }
      ExpressionTE::StaticArrayFromValues(e) => {
        let elements = self.groupify_exprs(coutputs, function_s, function_t, bump_g, access_log, e.elements, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::StaticArrayFromValues(bump_g.alloc(StaticArrayFromValuesGE {
          range: e.range,
          elements,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          array_type: self.ssa_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.array_type),
        })))
      }
      ExpressionTE::ArraySize(e) => {
        let array = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::ArraySize(bump_g.alloc(ArraySizeGE {
          range: e.range,
          array,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::IsSameInstance(e) => {
        let left = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.left, local_rune_to_templata, local_to_type_g)?;
        let right = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.right, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::IsSameInstance(bump_g.alloc(IsSameInstanceGE {
          range: e.range,
          left,
          right,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::AsSubtype(e) => {
        let source_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.source_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::AsSubtype(bump_g.alloc(AsSubtypeGE {
          range: e.range,
          source_expr,
          target_type: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.target_type, None),
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, source_expr.result()),
          ok_constructor: e.ok_constructor,
          err_constructor: e.err_constructor,
          impl_name: e.impl_name,
          ok_impl_name: e.ok_impl_name,
          err_impl_name: e.err_impl_name,
        })))
      }
      ExpressionTE::ConstantStr(c) => {
        // Str is share-flavored, so a string literal is a share reference, which carries no group.
        let result = bump_g.alloc(ShareRefGT {
          inner: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, c.result.inner, None),
        });
        Ok(ExpressionGE::ConstantStr(bump_g.alloc(ConstantStrGE { range: c.range, value: c.value, result })))
      }
      // A virtual, bound, or extern call: no callee signature to read groups or effects off, so the
      // result is groupless and the call churns nothing.
      ExpressionTE::InterfaceFunctionCall(e) => {
        let args = self.groupify_exprs(coutputs, function_s, function_t, bump_g, access_log, e.args, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::InterfaceFunctionCall(bump_g.alloc(InterfaceFunctionCallGE {
          range: e.range,
          super_function_prototype: e.super_function_prototype,
          virtual_param_index: e.virtual_param_index,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          args,
          mut_effects: &[],
        })))
      }
      ExpressionTE::ExternFunctionCall(e) => {
        let args = self.groupify_exprs(coutputs, function_s, function_t, bump_g, access_log, e.args, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::ExternFunctionCall(bump_g.alloc(ExternFunctionCallGE {
          range: e.range,
          prototype2: e.prototype2,
          args,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          mut_effects: &[],
        })))
      }
      ExpressionTE::BoundFunctionCall(e) => {
        let args = self.groupify_exprs(coutputs, function_s, function_t, bump_g, access_log, e.args, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::BoundFunctionCall(bump_g.alloc(BoundFunctionCallGE {
          range: e.range,
          impl_name: e.impl_name,
          abstract_prototype: e.abstract_prototype,
          virtual_param_index: e.virtual_param_index,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          args,
          mut_effects: &[],
        })))
      }
      ExpressionTE::Reinterpret(e) => {
        let expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Reinterpret(bump_g.alloc(ReinterpretGE {
          range: e.range,
          expr,
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, expr.result()),
        })))
      }
      ExpressionTE::Construct(e) => {
        let args = self.groupify_exprs(coutputs, function_s, function_t, bump_g, access_log, e.args, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::Construct(bump_g.alloc(ConstructGE {
          range: e.range,
          struct_tt: self.struct_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.struct_tt),
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
          args,
        })))
      }
      ExpressionTE::NewRuntimeSizedArray(e) => {
        let capacity_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.capacity_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::NewRuntimeSizedArray(bump_g.alloc(NewRuntimeSizedArrayGE {
          range: e.range,
          array_type: self.rsa_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.array_type),
          capacity_expr,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::StaticArrayFromCallable(e) => {
        let generator = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.generator, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::StaticArrayFromCallable(bump_g.alloc(StaticArrayFromCallableGE {
          range: e.range,
          array_type: self.ssa_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.array_type),
          generator,
          generator_method: e.generator_method,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::DestroyStaticSizedArrayIntoFunction(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        let consumer = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.consumer, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::DestroyStaticSizedArrayIntoFunction(bump_g.alloc(DestroyStaticSizedArrayIntoFunctionGE {
          range: e.range,
          array_expr,
          array_type: self.ssa_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.array_type),
          consumer,
          consumer_method: e.consumer_method,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::DestroyStaticSizedArrayIntoLocals(e) => {
        let expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.expr, local_rune_to_templata, local_to_type_g)?;
        let static_sized_array = self.ssa_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.static_sized_array);
        // Each destination local receives one element.
        let mut dest_vars_g = Vec::new();
        for dest_var in e.destination_reference_variables.iter() {
          let variable_g = bump_g.alloc(LocalVariableG { name: dest_var.name, tyype: static_sized_array.element_type });
          dest_vars_g.push(&*variable_g);
          local_to_type_g.insert(dest_var.name, static_sized_array.element_type);
        }
        Ok(ExpressionGE::DestroyStaticSizedArrayIntoLocals(bump_g.alloc(DestroyStaticSizedArrayIntoLocalsGE {
          range: e.range,
          expr,
          static_sized_array,
          destination_reference_variables: bump_g.alloc_slice_copy(dest_vars_g.as_slice()),
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::DestroyRuntimeSizedArray(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::DestroyRuntimeSizedArray(bump_g.alloc(DestroyRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::RuntimeSizedArrayCapacity(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::RuntimeSizedArrayCapacity(bump_g.alloc(RuntimeSizedArrayCapacityGE {
          range: e.range,
          array_expr,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::PushRuntimeSizedArray(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        let new_element_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.new_element_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::PushRuntimeSizedArray(bump_g.alloc(PushRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          new_element_expr,
          result: KindGT::Void(VoidGT { }),
        })))
      }
      ExpressionTE::PopRuntimeSizedArray(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::PopRuntimeSizedArray(bump_g.alloc(PopRuntimeSizedArrayGE {
          range: e.range,
          array_expr,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, None),
        })))
      }
      ExpressionTE::InterfaceToInterfaceUpcast(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::InterfaceToInterfaceUpcast(bump_g.alloc(InterfaceToInterfaceUpcastGE {
          range: e.range,
          inner_expr,
          target_interface: self.interface_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.target_interface),
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, inner_expr.result()),
        })))
      }
      ExpressionTE::UpcastInterface(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::UpcastInterface(bump_g.alloc(UpcastInterfaceGE {
          range: e.range,
          inner_expr,
          target_super_kind: self.super_kind_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.target_super_kind),
          impl_name: e.impl_name,
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, inner_expr.result()),
        })))
      }
      ExpressionTE::UpcastGeneric(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::UpcastGeneric(bump_g.alloc(UpcastGenericGE {
          range: e.range,
          inner_expr,
          target_super_kind: self.super_kind_gt(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.target_super_kind),
          impl_name: e.impl_name,
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, inner_expr.result()),
        })))
      }
      ExpressionTE::NarrowInterface(e) => {
        let inner_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.inner_expr, local_rune_to_templata, local_to_type_g)?;
        Ok(ExpressionGE::NarrowInterface(bump_g.alloc(NarrowInterfaceGE {
          range: e.range,
          inner_expr,
          result: self.cast_result(coutputs, bump_g, local_rune_to_templata, local_to_type_g, e.result, inner_expr.result()),
        })))
      }
      ExpressionTE::CopyPrim(CopyPrimTE { range, loct, inner: source_te, result, .. }) => {
        let source_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *source_te, local_rune_to_templata, local_to_type_g)?;
        // A primitive copy reads a value through its reference — a real load into the base's group.
        if let Some((base_ref, group)) = self.base_ref_and_group(function_t, source_ge) {
          access_log.push(AccessEventG::Read { base_ref, group, loct: *loct });
        }
        Ok(ExpressionGE::CopyPrim(bump_g.alloc(CopyPrimGE {
          range: *range,
          loct: *loct,
          inner: source_ge,
          result: self.groupify_type(coutputs, bump_g, local_rune_to_templata, local_to_type_g, *result, None),
        })))
      },
      ExpressionTE::StaticSizedArrayLookup(e) => {
        let array_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.array_expr, local_rune_to_templata, local_to_type_g)?;
        let index_expr = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, e.index_expr, local_rune_to_templata, local_to_type_g)?;
        let (array_group, array_gt) =
          match array_expr.result() {
            KindGT::BorrowRef(BorrowRefGT { inner: KindGT::StaticSizedArray(ssa_gt), group }) => (group, ssa_gt),
            other => panic!("vfail: element access through a non-borrow array: {:?}", other),
          };
        // A static-sized array's elements are inline (they share the array's own storage), so the element
        // borrow carries the array's group directly, with no `ChildElements` step: the fold. Numbering
        // them apart would tell LLVM an access to the whole array and an access to an element never
        // overlap.
        Ok(ExpressionGE::StaticSizedArrayLookup(bump_g.alloc(StaticSizedArrayLookupGE {
          range: e.range,
          loct: e.loct,
          array_expr,
          array_type: array_gt,
          index_expr,
          result: bump_g.alloc(BorrowRefGT {
            inner: array_gt.element_type,
            group: GroupTemplataG { group: array_group.group, kind: array_gt.element_type, born_at: e.loct },
          }),
        })))
      }
      ExpressionTE::RuntimeSizedArrayLookup(RuntimeSizedArrayLookupTE { range, loct, array_expr, array_type, index_expr, result, .. }) => {
        let array_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *array_expr, local_rune_to_templata, local_to_type_g)?;
        let index_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *index_expr, local_rune_to_templata, local_to_type_g)?;
        let (array_group, array_gt) =
            match array_ge.result() {
              KindGT::BorrowRef(BorrowRefGT {
                inner: KindGT::RuntimeSizedArray(rsa_gt),
                group: group_expr
              }) => (group_expr, rsa_gt),
              _ => panic!("Expected RSA"),
            };

        Ok(ExpressionGE::RuntimeSizedArrayLookup(bump_g.alloc(RuntimeSizedArrayLookupGE {
          range: *range,
          loct: *loct,
          array_expr: array_ge,
          array_type: array_gt,
          index_expr: index_ge,
          result: bump_g.alloc(BorrowRefGT{
            inner: array_gt.element_type,
            group: Self::group_path_child(
              &bump_g,
              *array_group,
              GroupChildStepG::ChildElements { },
              array_gt.element_type,
              *loct)
          }),
        })))
      }
      ExpressionTE::MemberLookup(MemberLookupTE { range, loct, struct_expr: struct_expr_te, member_name, result, .. }) => {
        let source_struct_ge = self.groupify_expression(coutputs, function_s, function_t, bump_g, access_log, *struct_expr_te, local_rune_to_templata, local_to_type_g)?;
        // The base is a borrow of the struct. A destructure of a borrow (`[x, y] = &v`) reads its
        // members through a borrow-typed temporary with no `Deref`, so peel to the borrow that points
        // at the struct: the member lives in the referent's group, not the temporary's slot.
        let mut base_borrow_gt = expect_borrowref_gt(source_struct_ge.result());
        while let KindGT::BorrowRef(inner_borrow_gt) = base_borrow_gt.inner {
          base_borrow_gt = inner_borrow_gt;
        }
        let struct_group_templata = &base_borrow_gt.group;
        let source_struct_gt =
            match base_borrow_gt.inner {
              KindGT::Struct(s) => s,
              other => panic!("vfail: member access through a non-struct: {:?}", other),
            };

        let member_name_str =
            match member_name {
              IVarNameT::Member(MemberNameT { imprecise_name: CodeNameS { name, .. }, .. }) => name,
              _ => panic!("Unexpected var name {:?}", member_name),
            };

        let member_name_to_locally_phrased_member_type =
            self.translate_struct_members(coutputs, bump_g, *source_struct_gt);
        let member_gt =
            member_name_to_locally_phrased_member_type.get(member_name_str)
                .expect("Couldn't find member");

        let result_gt =
            bump_g.alloc(BorrowRefGT{
              inner: *member_gt,
              group: Self::group_path_child(
                &bump_g,
                *struct_group_templata,
                GroupChildStepG::Member { member_name: *member_name_str },
                *member_gt,
                *loct)
            });
        Ok(ExpressionGE::MemberLookup(bump_g.alloc(MemberLookupGE {
          range: *range,
          loct: *loct,
          struct_expr: source_struct_ge,
          member_name: *member_name,
          result: result_gt,
        })))
      }
    }
  }

  /// The group one step below each of `existing`'s paths: the member or element child group. Each
  /// path keeps its `...` (`&x.hp` on a `&Ship in g...` is still somewhere in `g`'s territory).
  fn group_path_child<'g>(
      bump_g: &'g Bump,
      existing: GroupTemplataG<'s, 't, 'g>,
      new_step: GroupChildStepG<'s>,
      new_type: KindGT<'s, 't, 'g>,
      born_at: LocT<'t>,
  ) -> GroupTemplataG<'s, 't, 'g> {
    let paths: Vec<GroupPathG<'s, 't, 'g>> =
      existing.group.iter().map(|path| Self::path_with_step(bump_g, *path, new_step)).collect();
    GroupTemplataG { group: bump_g.alloc_slice_copy(paths.as_slice()), kind: new_type, born_at }
  }

  /// `path` with `step` appended.
  fn path_with_step<'g>(
    bump_g: &'g Bump,
    path: GroupPathG<'s, 't, 'g>,
    step: GroupChildStepG<'s>,
  ) -> GroupPathG<'s, 't, 'g> {
    let mut steps = path.steps.to_vec();
    steps.push(step);
    GroupPathG { root: path.root, steps: bump_g.alloc_slice_copy(steps.as_slice()), ellipsis: path.ellipsis }
  }

  fn translate_struct_members<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    source_struct_gt: StructGT<'s, 't, 'g>
  ) -> IndexMap<StrI<'s>, KindGT<'s, 't, 'g>> {
    let struct_template_id = Compiler::get_template(self.typing_interner, *source_struct_gt.id);
    let citizen_def_s =
        coutputs.peek_postparsed_type(struct_template_id)
            .expect("Struct not present");
    let struct_def_s =
        match citizen_def_s {
          ICitizenDenizenS::TopLevelStruct(s) => s,
          ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct, got interface"),
        };
    let struct_def_t = coutputs.lookup_struct_template(*struct_template_id);

    let callee_rune_to_caller_templata: IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> =
        self.calculate_struct_callee_rune_to_caller_templata(coutputs, bump_g, struct_def_s, struct_def_t, source_struct_gt.template_args);

    assert!(struct_def_s.members.len() == struct_def_t.members.len());
    let mut locally_phrased_member_types_gt = Vec::new();
    for (i_member_s, member_t) in struct_def_s.members.iter().zip(struct_def_t.members) {
      let member_s =
          match i_member_s {
            IStructMemberS::NormalStructMember(nsm) => nsm,
            IStructMemberS::VariadicStructMember(_) => unimplemented!(),
          };
      let written_context =
          WrittenContext {
            type_s: member_s.tyype,
            name: None,
          };
      let type_g =
          self.groupify_type(coutputs, bump_g, &callee_rune_to_caller_templata, &IndexMap::new(), member_t.tyype, Some(&written_context));
      locally_phrased_member_types_gt.push((member_s.name, type_g));
    }
    IndexMap::from_iter(locally_phrased_member_types_gt)
  }

  fn groupify_type<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    type_t: KindT<'s, 't>,
    maybe_written: Option<&WrittenContext<'s, 't>>,
  ) -> KindGT<'s, 't, 'g> {
    match type_t {
      KindT::BorrowRef(BorrowRefT { inner: inner_tt }) => {
        let written = maybe_written.expect("Encountered a borrow ref with no written group");
        match written.type_s {
          ITypeST::BorrowRef(bst) => {
            let BorrowRefST { range: _, inner: bst_inner, region: bst_group_s } = *bst;
            // The referent's groups come from the written referent, under the same parameter.
            let inner_written = WrittenContext { type_s: *bst_inner, name: written.name };
            let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, Some(&inner_written));
            let group_expr_g: GroupExprG<'s, 't, 'g> =
              match bst_group_s {
                // An unannotated borrow (and a `held` one) lives in its parameter's own anonymous group.
                RegionS::Unspecified | RegionS::Held => Self::anonymous_group_expr(bump_g, written.name),
                RegionS::Group(b) => {
                  let paths: Vec<GroupPathG<'s, 't, 'g>> = self
                    .groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, b)
                    .into_iter()
                    .map(|(path, _)| path)
                    .collect();
                  bump_g.alloc_slice_copy(paths.as_slice())
                }
              };
            // born_at is only read by Symphony's forward check; experimental's backward walk never
            // consults it, and groupify_type has no producing node's loct in scope, so it is left at
            // function entry here.
            let group_templata_g = GroupTemplataG { kind: inner_gt, group: group_expr_g, born_at: LocT { path: &[] } };
            KindGT::BorrowRef(bump_g.alloc(BorrowRefGT {
              inner: inner_gt,
              group: group_templata_g,
            }))
          }
          // The borrow is written as a rune (`x T` with `T` bound to a borrow): the bound argument
          // already carries the caller's groups.
          ITypeST::Rune(RuneUsageST { rune: RuneUsage { rune, .. } }) => {
            match rune_to_templata.get(rune) {
              Some(ITemplataG::Kind(KindTemplataG { kind: bound_gt @ KindGT::BorrowRef(_) })) => *bound_gt,
              _ => {
                // The typed kind carries a borrow the written type never had: the outer layer takes the
                // parameter's anonymous group and the referent is groupless.
                let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, None);
                let group_expr_g = Self::anonymous_group_expr(bump_g, written.name);
                KindGT::BorrowRef(bump_g.alloc(BorrowRefGT {
                  inner: inner_gt,
                  group: GroupTemplataG { kind: inner_gt, group: group_expr_g, born_at: LocT { path: &[] } },
                }))
              }
            }
          }
          _ => {
            let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, None);
            let group_expr_g = Self::anonymous_group_expr(bump_g, written.name);
            KindGT::BorrowRef(bump_g.alloc(BorrowRefGT {
              inner: inner_gt,
              group: GroupTemplataG { kind: inner_gt, group: group_expr_g, born_at: LocT { path: &[] } },
            }))
          }
        }
      },
      KindT::Never(NeverT { from_break }) => KindGT::Never(NeverGT { from_break }),
      KindT::Void(VoidT { }) => KindGT::Void(VoidGT { }),
      KindT::Int(IntT { bits }) => KindGT::Int(IntGT { bits }),
      KindT::Bool(BoolT { }) => KindGT::Bool(BoolGT { }),
      KindT::Str(StrT { }) => KindGT::Str(StrGT { }),
      KindT::Float(FloatT { }) => KindGT::Float(FloatGT { }),
      KindT::USize(USizeT { }) => KindGT::USize(USizeGT { }),
      KindT::Struct(StructTT { id, .. }) => {
        let template_args_t: &'t [ITemplataT<'s, 't>] =
            ICitizenNameT::try_from(id.local_name)
                .expect("Struct without ICitizenNameT")
                .template_args();
        let template_args_g =
          self.citizen_args_in(coutputs, bump_g, rune_to_templata, local_to_type_g, template_args_t, maybe_written);
        KindGT::Struct(bump_g.alloc(StructGT { id, template_args: template_args_g }))
      }
      KindT::Interface(InterfaceTT { id, .. }) => {
        let template_args_t: &'t [ITemplataT<'s, 't>] =
            ICitizenNameT::try_from(id.local_name)
                .expect("Interface without ICitizenNameT")
                .template_args();
        let template_args_g =
          self.citizen_args_in(coutputs, bump_g, rune_to_templata, local_to_type_g, template_args_t, maybe_written);
        KindGT::Interface(bump_g.alloc(InterfaceGT { id, template_args: template_args_g }))
      }
      // A placeholder the rune map binds is substituted (a struct's `x T` member read under the
      // struct's template args); one it does not bind is its own identity (the caller's `T` seen
      // through a callee's rune map). VLOOOOK: the map is keyed by rune name, so a caller placeholder
      // sharing a name with a bound callee rune would be mis-substituted; no fixture does.
      KindT::KindPlaceholder(KindPlaceholderT { id: id_t }) => {
        let rune = match id_t.local_name {
          INameT::KindPlaceholder(KindPlaceholderNameT { template: KindPlaceholderTemplateNameT { rune, .. } }) => rune,
          _ => panic!("Unexpected name for a placeholder"),
        };
        match rune_to_templata.get(rune) {
          Some(ITemplataG::Kind(KindTemplataG { kind })) => *kind,
          Some(other) => panic!("vfail: kind rune {:?} bound to a non-kind: {:?}", rune, other),
          None => KindGT::KindPlaceholder(bump_g.alloc(KindPlaceholderGT { id: *id_t, _phantom: PhantomData })),
        }
      }
      // A static-sized array is written `StaticArray<N, T>` — a `Call` whose second arg is the element.
      KindT::StaticSizedArray(a) => {
        let element_written = match maybe_written.map(|w| w.type_s) {
          Some(ITypeST::Call(c)) if c.args.len() == 2 => {
            Some(WrittenContext { type_s: *c.args[1], name: maybe_written.and_then(|w| w.name) })
          }
          _ => None,
        };
        let element_gt =
          self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, a.element_type(), element_written.as_ref());
        KindGT::StaticSizedArray(bump_g.alloc(StaticSizedArrayGT { name: a.name, element_type: element_gt }))
      }
      KindT::RuntimeSizedArray(a) => {
        let element_written = match maybe_written.map(|w| w.type_s) {
          Some(ITypeST::RuntimeSizedArray(st)) => {
            Some(WrittenContext { type_s: *st.element, name: maybe_written.and_then(|w| w.name) })
          }
          _ => None,
        };
        let element_gt =
          self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, a.element_type(), element_written.as_ref());
        KindGT::RuntimeSizedArray(bump_g.alloc(RuntimeSizedArrayGT { name: a.name, element_type: element_gt }))
      }
      KindT::OverloadSet(o) => {
        KindGT::OverloadSet(bump_g.alloc(OverloadSetGT { env_id: o.env.id(), _phantom: PhantomData }))
      }
      KindT::OwnRef(OwnRefT { inner: inner_tt }) => {
        let inner_written = match maybe_written.map(|w| w.type_s) {
          Some(ITypeST::OwnRef(st)) => Some(WrittenContext { type_s: *st.inner, name: maybe_written.and_then(|w| w.name) }),
          _ => None,
        };
        let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, inner_written.as_ref());
        KindGT::OwnRef(bump_g.alloc(OwnRefGT { inner: inner_gt }))
      }
      KindT::WeakRef(WeakRefT { inner: inner_tt }) => {
        let inner_written = match maybe_written.map(|w| w.type_s) {
          Some(ITypeST::WeakRef(st)) => Some(WrittenContext { type_s: *st.inner, name: maybe_written.and_then(|w| w.name) }),
          _ => None,
        };
        let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, inner_written.as_ref());
        KindGT::WeakRef(bump_g.alloc(WeakRefGT { inner: inner_gt }))
      }
      // A claim roots the ambient multi `rc`, so this layer carries no group; the written type is a bare
      // citizen, so the payload recurses against the same written type.
      KindT::ShareRef(ShareRefT { inner: inner_tt }) => {
        let inner_gt = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *inner_tt, maybe_written);
        KindGT::ShareRef(bump_g.alloc(ShareRefGT { inner: inner_gt }))
      }
    }
  }

  fn groupify_templata<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    templata_t: ITemplataT<'s, 't>,
  ) -> ITemplataG<'s, 't, 'g> {
    match templata_t {
      ITemplataT::Kind(KindTemplataT { kind }) => {
        let kind_g = self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, kind, None);
        ITemplataG::Kind(KindTemplataG { kind: kind_g })
      },
      ITemplataT::Placeholder(p) => {
        ITemplataG::Placeholder(bump_g.alloc(PlaceholderTemplataG { id: p.id, tyype: p.tyype }))
      }
      ITemplataT::Integer(num) => ITemplataG::Integer(num),
      ITemplataT::Boolean(v) => ITemplataG::Boolean(v),
      ITemplataT::String(v) => ITemplataG::String(v),
      ITemplataT::Prototype(p) => {
        ITemplataG::Prototype(bump_g.alloc(PrototypeTemplataG { prototype: p.prototype }))
      }
      ITemplataT::Isa(isa) => ITemplataG::Isa(bump_g.alloc(IsaTemplataG {
        declaration_range: isa.declaration_range,
        impl_name: isa.impl_name,
        sub_kind: isa.sub_kind,
        super_kind: isa.super_kind,
      })),
      ITemplataT::CoordList(list) => {
        let kinds: Vec<KindGT<'s, 't, 'g>> = list
          .kinds
          .iter()
          .map(|k| self.groupify_type(coutputs, bump_g, rune_to_templata, local_to_type_g, *k, None))
          .collect();
        ITemplataG::CoordList(bump_g.alloc(KindListTemplataG { kinds: bump_g.alloc_slice_copy(kinds.as_slice()) }))
      }
      ITemplataT::RuntimeSizedArrayTemplate(_) => {
        ITemplataG::RuntimeSizedArrayTemplate(RuntimeSizedArrayTemplateTemplataG {})
      }
      ITemplataT::StaticSizedArrayTemplate(_) => {
        ITemplataG::StaticSizedArrayTemplate(StaticSizedArrayTemplateTemplataG {})
      }
      // A group argument's group is only knowable from its written form (`citizen_args_in`).
      ITemplataT::Group(_) => panic!(
        "vfail: a group template argument with no written group — a deferred case"
      ),
      ITemplataT::Function(f) => ITemplataG::Function(
        bump_g.alloc(FunctionTemplataG { function_template_id: f.function_template_id }),
      ),
      ITemplataT::StructDefinition(d) => {
        ITemplataG::StructDefinition(bump_g.alloc(StructDefinitionTemplataG {
          struct_template_id: d.struct_template_id,
          tyype: d.tyype,
        }))
      }
      ITemplataT::InterfaceDefinition(d) => {
        ITemplataG::InterfaceDefinition(bump_g.alloc(InterfaceDefinitionTemplataG {
          interface_template_id: d.interface_template_id,
          tyype: d.tyype,
        }))
      }
      ITemplataT::ImplDefinition(d) => ITemplataG::ImplDefinition(
        bump_g.alloc(ImplDefinitionTemplataG { impl_template_id: d.impl_template_id }),
      ),
      ITemplataT::ExternFunction(f) => {
        ITemplataG::ExternFunction(bump_g.alloc(ExternFunctionTemplataG { header: f.header }))
      }
    }
  }

  // "Struct callee" = the struct whose template we're calling
  fn calculate_struct_callee_rune_to_caller_templata<'g>(
      &self,
      coutputs: &CompilerOutputs<'s, 't>,
      bump_g: &'g Bump,
      struct_def_s: &'s StructS<'s>,
      struct_def_t: &'t StructDefinitionT<'s, 't>,
      template_args_ge: &'g [ITemplataG<'s, 't, 'g>]
  ) -> IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> {
    let mut map = IndexMap::new();
    assert!(template_args_ge.len() == struct_def_s.generic_params.len());
    for i in 0..template_args_ge.len() {
      map.insert(struct_def_s.generic_params[i].rune.rune, template_args_ge[i]);
    }
    map
  }

  fn calculate_callee_rune_to_caller_templata<'g>(
      &self,
      coutputs: &CompilerOutputs<'s, 't>,
      bump_g: &'g Bump,
      function_s: &'s FunctionS<'s>,
      args_ge: &Vec<ExpressionGE<'s, 't, 'g>>
  ) -> IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>> {
    let mut map = IndexMap::new();
    assert!(args_ge.len() == function_s.params.len());
    for i in 0..args_ge.len() {
      self.match_types(coutputs, bump_g, function_s.params[i].tyype, ITemplataG::Kind(KindTemplataG { kind: args_ge[i].result() }), &mut map);
    }
    map
  }

  fn match_types<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    type_s: ITypeST<'s>,
    type_g: ITemplataG<'s, 't, 'g>,
    map: &mut IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) {
    match type_s {
      ITypeST::BorrowRef(BorrowRefST { range, inner: inner_st, region: group_s }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::BorrowRef(BorrowRefGT{ inner: inner_gt, group: group_templata_g }) }) => {
            self.match_types(coutputs, bump_g, **inner_st, ITemplataG::Kind(KindTemplataG { kind: *inner_gt }), map);

            match group_s {
              // An unannotated parameter names no rune, so the argument binds nothing.
              RegionS::Unspecified | RegionS::Held => {}
              RegionS::Group(group_s) => {
                match group_s {
                  GroupS::Rune(RuneUsage { rune, .. }) => {
                    map.insert(*rune, ITemplataG::Group(*group_templata_g));
                  }
                  GroupS::Local(_) => {}
                  GroupS::Member { .. } => {}
                  GroupS::Elements { .. } => {}
                  GroupS::Ellipsis { .. } => {}
                  GroupS::Union { .. } => {}
                }
              }
            }
          }
          _ => panic!("Unexpected non-borrow ref"),
        }
      }
      ITypeST::Rune(RuneUsageST { rune: RuneUsage { range, rune }}) => {
        map.insert(*rune, type_g);
      }
      ITypeST::Function(_) => unimplemented!(),
      ITypeST::AnonymousRune(_) => unimplemented!(),
      ITypeST::Bool(_) => {}
      ITypeST::Call(CallST { range, template: template_s, args: template_args_s }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::Struct(StructGT { id: struct_id_t, template_args: template_args_g}) }) => {
            let struct_template_id_t = Compiler::get_template(self.typing_interner, **struct_id_t);
            let citizen_denizen_s = coutputs.peek_postparsed_type(struct_template_id_t).expect("Couldn't find struct template");
            let struct_def_templata_type =
                match citizen_denizen_s {
                  ICitizenDenizenS::TopLevelStruct(struct_s) => struct_s.tyype,
                  ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct"),
                };
            let struct_templata_g =
                ITemplataG::StructDefinition(
                    bump_g.alloc(
                        StructDefinitionTemplataG {
                          struct_template_id: struct_template_id_t,
                          tyype: struct_def_templata_type
                        }));
            self.match_types(coutputs, bump_g, **template_s, struct_templata_g, map);
            for (template_arg_s, template_arg_g) in template_args_s.iter().zip(template_args_g.iter()) {
              self.match_types(coutputs, bump_g, **template_arg_s, *template_arg_g, map);
            }
          }
          ITemplataG::Kind(KindTemplataG { kind: KindGT::Interface(InterfaceGT { id: interface_id_t, template_args: template_args_g}) }) => {
            let interface_template_id_t = Compiler::get_template(self.typing_interner, **interface_id_t);
            let citizen_denizen_s = coutputs.peek_postparsed_type(interface_template_id_t).expect("Couldn't find interface template");
            let interface_def_templata_type =
                match citizen_denizen_s {
                  ICitizenDenizenS::TopLevelStruct(_) => panic!("Expected interface"),
                  ICitizenDenizenS::TopLevelInterface(interface_s) => interface_s.tyype,
                };
            let interface_templata_g =
                ITemplataG::InterfaceDefinition(
                  bump_g.alloc(
                    InterfaceDefinitionTemplataG {
                      interface_template_id: interface_template_id_t,
                      tyype: interface_def_templata_type
                    }));
            self.match_types(coutputs, bump_g, **template_s, interface_templata_g, map);
            for (template_arg_s, template_arg_g) in template_args_s.iter().zip(template_args_g.iter()) {
              self.match_types(coutputs, bump_g, **template_arg_s, *template_arg_g, map);
            }
          }
          ITemplataG::Kind(KindTemplataG {
                             kind: KindGT::Int(_) | KindGT::Bool(_) | KindGT::Float(_) | KindGT::Str(_) | KindGT::Void(_) | KindGT::USize(_) | KindGT::Never(_),
                           }) => {
            // Postparser makes zero-arg calls to primitives.
            assert!(template_args_s.is_empty());
          },
          // A written citizen matched against a placeholder or an array binds no rune of its own; a
          // rune it should have bound is reported at its use, as "not bound at this call".
          _ => {}
        }
      }
      // These written shapes bind no rune at a call; a rune one should have bound is reported at its
      // use, as "not bound at this call".
      ITypeST::Int(_) => {}
      ITypeST::Tuple(_) => {}
      ITypeST::Name(_) => {}
      ITypeST::WeakRef(_) => {}
      ITypeST::OwnRef(_) => {}
      ITypeST::Pack(_) => {}
      ITypeST::RuntimeSizedArray(RuntimeSizedArrayST { range, element: element_st }) => {
        match type_g {
          ITemplataG::Kind(KindTemplataG { kind: KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { name, element_type }) }) => {
            // let struct_template_id_t = Compiler::get_template(self.typing_interner, **name);
            // let citizen_denizen_s = coutputs.peek_postparsed_type(struct_template_id_t).expect("Couldn't find struct template");
            // let struct_def_templata_type =
            //     match citizen_denizen_s {
            //       ICitizenDenizenS::TopLevelStruct(struct_s) => struct_s.tyype,
            //       ICitizenDenizenS::TopLevelInterface(_) => panic!("Expected struct"),
            //     };
            // let struct_templata_g =
            //     ITemplataG::StructDefinition(
            //       bump_g.alloc(
            //         StructDefinitionTemplataG {
            //           struct_template_id: struct_template_id_t,
            //           tyype: struct_def_templata_type
            //         }));
            // self.match_types(coutputs, bump_g, **template_s, struct_templata_g, map);
            self.match_types(coutputs, bump_g, **element_st, ITemplataG::Kind(KindTemplataG { kind: *element_type }), map);
          }
          _ => panic!("Unexpected non-runtime sized array type"),
        }
      }
      ITypeST::String(_) => {}
    }
  }

  /// The caller-side churn paths a callee effect declares: one flat path per union member, with the
  /// `...` dropped (`mut(g...)` churns exactly `mut(g)`). `not(mut(..))` churns nothing.
  fn groupify_effect<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    effect_s: &'s EffectS<'s>,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
  ) -> Vec<&'g [GroupStep<'s, 't>]> {
    match effect_s {
      EffectS::Mut(group_s) => self
        .groupify_group_expr(coutputs, bump_g, rune_to_templata, &IndexMap::new(), group_s)
        .into_iter()
        .map(|(path, _)| {
          let steps: &'g [GroupStep<'s, 't>] = bump_g.alloc_slice_copy(flatten(&path).as_slice());
          steps
        })
        .collect(),
      EffectS::NotMut(_) => vec![],
    }
  }

  /// The paths a written group names, one per union member, each with the type of what lives at it.
  /// A rune is read through the rune map handed in, so a callee's rune comes out as the caller's path
  /// (with that path's steps first, and its `...` kept).
  fn groupify_group_expr<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    // NOTE: This might be keyed on caller or callee runes.
    rune_to_templata: &IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    local_to_type_g: &IndexMap<IVarNameT<'s, 't>, KindGT<'s, 't, 'g>>,
    group_s: &'s GroupS<'s>,
  ) -> Vec<(GroupPathG<'s, 't, 'g>, KindGT<'s, 't, 'g>)> {
    match group_s {
      GroupS::Rune(RuneUsage { rune, .. }) => {
        // A rune with no binding is a checker bug: every rune a callee's written type or effect names
        // must be bound at the call.
        let bound = rune_to_templata
          .get(rune)
          .unwrap_or_else(|| panic!("vfail: callee group rune {:?} not bound at this call", rune));
        match bound {
          ITemplataG::Group(GroupTemplataG { group, kind, .. }) => group.iter().map(|path| (*path, *kind)).collect(),
          other => panic!("vfail: group rune bound to a non-group: {other:?}"),
        }
      }
      GroupS::Local(imprecise_name) => {
        // TODO: perhaps key local_to_type_g on imprecise name instead, this is slow
        let (var_name_t, var_kind_gt) = local_to_type_g
          .iter()
          .find(|(v, _)| v.imprecise_name() == Some(*imprecise_name))
          .expect("Local not found");
        vec![(GroupPathG { root: GroupRootG::Local(*var_name_t), steps: &[], ellipsis: false }, *var_kind_gt)]
      }
      GroupS::Member { base, member_name } => self
        .groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, base)
        .into_iter()
        .map(|(path, type_gt)| {
          let member_type_gt = match type_gt {
            KindGT::Struct(struct_gt) => *self
              .translate_struct_members(coutputs, bump_g, *struct_gt)
              .get(member_name)
              .expect("No member in struct with that name"),
            _ => panic!("Unexpected type in group member expr"),
          };
          (Self::path_with_step(bump_g, path, GroupChildStepG::Member { member_name: *member_name }), member_type_gt)
        })
        .collect(),
      GroupS::Elements { base } => self
        .groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, base)
        .into_iter()
        .map(|(path, type_gt)| match type_gt {
          KindGT::StaticSizedArray(StaticSizedArrayGT { element_type, .. }) => {
            (Self::path_with_step(bump_g, path, GroupChildStepG::InlineElements {}), *element_type)
          }
          KindGT::RuntimeSizedArray(RuntimeSizedArrayGT { element_type, .. }) => {
            (Self::path_with_step(bump_g, path, GroupChildStepG::ChildElements {}), *element_type)
          }
          _ => panic!("Unexpected type in group elements expr"),
        })
        .collect(),
      GroupS::Ellipsis { base } => self
        .groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, base)
        .into_iter()
        .map(|(path, type_gt)| (GroupPathG { ellipsis: true, ..path }, type_gt))
        .collect(),
      GroupS::Union { members } => members
        .iter()
        .flat_map(|m| self.groupify_group_expr(coutputs, bump_g, rune_to_templata, local_to_type_g, m))
        .collect(),
    }
  }

  /// Register the group runes a parameter's written type names, walking it against the typed kind: a
  /// bare `in g` on a borrow whose rune is not yet registered becomes a group whose referent is that
  /// borrow's. Nested runes are registered first, so the referent's own groups resolve. `param_name`
  /// keys an unannotated borrow's anonymous group. Any other written shape names no rune.
  fn register_group_runes<'g>(
    &self,
    coutputs: &CompilerOutputs<'s, 't>,
    bump_g: &'g Bump,
    rune_map: &mut IndexMap<IRuneS<'s>, ITemplataG<'s, 't, 'g>>,
    param_name: Option<IVarNameT<'s, 't>>,
    type_st: ITypeST<'s>,
    kind_t: KindT<'s, 't>,
  ) {
    match (type_st, kind_t) {
      (ITypeST::BorrowRef(bst), KindT::BorrowRef(BorrowRefT { inner: inner_tt })) => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, *bst.inner, *inner_tt);
        if let RegionS::Group(GroupS::Rune(RuneUsage { rune, .. })) = bst.region {
          match rune_map.get(rune) {
            Some(ITemplataG::Group(_)) => {}
            Some(_) => panic!("Group rune {:?} was bound to a non-group templata", rune),
            None => {
              let inner_written = WrittenContext { type_s: *bst.inner, name: param_name };
              let inner_gt =
                self.groupify_type(coutputs, bump_g, &*rune_map, &IndexMap::new(), *inner_tt, Some(&inner_written));
              rune_map.insert(
                *rune,
                ITemplataG::Group(GroupTemplataG { group: Self::new_rune_group_expr(bump_g, *rune), kind: inner_gt, born_at: LocT { path: &[] } }),
              );
            }
          }
        }
      }
      (ITypeST::RuntimeSizedArray(st), KindT::RuntimeSizedArray(a)) => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, *st.element, a.element_type());
      }
      // A static-sized array is written `StaticArray<N, T>` — a `Call` whose second arg is the element.
      (ITypeST::Call(c), KindT::StaticSizedArray(a)) if c.args.len() == 2 => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, *c.args[1], a.element_type());
      }
      (ITypeST::Call(c), KindT::Struct(StructTT { id, .. }))
      | (ITypeST::Call(c), KindT::Interface(InterfaceTT { id, .. })) => {
        let template_args_t: &'t [ITemplataT<'s, 't>] =
          ICitizenNameT::try_from(id.local_name).expect("Citizen without ICitizenNameT").template_args();
        if c.args.len() == template_args_t.len() {
          for (written_arg, templata_t) in c.args.iter().zip(template_args_t.iter()) {
            if let ITemplataT::Kind(KindTemplataT { kind }) = templata_t {
              self.register_group_runes(coutputs, bump_g, rune_map, param_name, **written_arg, *kind);
            }
          }
        }
      }
      (ITypeST::OwnRef(st), KindT::OwnRef(OwnRefT { inner: inner_tt })) => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, *st.inner, *inner_tt);
      }
      (ITypeST::WeakRef(st), KindT::WeakRef(WeakRefT { inner: inner_tt })) => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, *st.inner, *inner_tt);
      }
      // A claim's written type is its bare citizen, so the payload walks against the same written type.
      (_, KindT::ShareRef(ShareRefT { inner: inner_tt })) => {
        self.register_group_runes(coutputs, bump_g, rune_map, param_name, type_st, *inner_tt);
      }
      _ => {}
    }
  }
}
