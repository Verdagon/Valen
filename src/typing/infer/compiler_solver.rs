use crate::interner::Interner;
use crate::keywords::Keywords;
use crate::postparsing::ast::*;
use crate::postparsing::itemplatatype::ITemplataType;
use crate::postparsing::names::*;
use crate::postparsing::post_parser_error_humanizer::humanize_rune;
use crate::postparsing::rules::rules::*;
use crate::postparsing::*;
use crate::solver::simple_solver_state::*;
use crate::solver::solver::make_solver_state;
use crate::solver::solver::*;
use crate::typing::ast::ast::*;
use crate::typing::ast::citizens::*;
use crate::typing::ast::expressions::*;
use crate::typing::citizen::impl_compiler::IsParentResult;
use crate::typing::citizen::impl_compiler::IsntParent;
use crate::typing::citizen::struct_compiler::ResolveFailure;
use crate::typing::compiler::Compiler;
use crate::typing::templata_compiler::get_interface_template;
use crate::typing::compiler_outputs::*;
use crate::typing::env::environment::*;
use crate::typing::env::function_environment_t::*;
use crate::typing::env::i_env_entry::*;
use crate::typing::infer_compiler::InferEnv;
use crate::typing::rule_runes::rune_usages;
use crate::typing::names::names::*;
use crate::typing::overload_resolver::FindFunctionFailure;
use crate::typing::templata::templata::expect_integer;
use crate::typing::templata::templata::KindTemplataT;
use crate::typing::templata::templata::*;
use crate::typing::types::types::*;
use crate::typing::typing_interner::TypingInterner;
use crate::utils::fx::{HashMap, HashSet};
use crate::utils::fx::{IndexMap, IndexSet};
use crate::utils::range::RangeS;
use std::iter::once;
use std::marker::PhantomData;

#[derive(Copy, Clone, Debug)]
pub enum ITypingPassSolverError<'s, 't> {
  KindIsNotConcrete {
    kind: KindT<'s, 't>,
  },
  KindIsNotInterface {
    kind: KindT<'s, 't>,
  },
  KindIsNotStruct {
    kind: KindT<'s, 't>,
  },
  KindIsNotBorrowRef {
    kind: KindT<'s, 't>,
  },
  KindIsNotWeakRef {
    kind: KindT<'s, 't>,
  },
  KindIsNotOwnRef {
    kind: KindT<'s, 't>,
  },
  KindIsNotFromATemplate {
    kind: KindT<'s, 't>,
  },
  CouldntFindFunction {
    range: &'t [RangeS<'s>],
    fff: FindFunctionFailure<'s, 't>,
  },
  CouldntFindImpl {
    range: &'t [RangeS<'s>],
    fail: &'t IsntParent<'s, 't>,
  },
  CouldntResolveKind {
    rf: &'t ResolveFailure<'s, 't, KindT<'s, 't>>,
  },
  CantShareMutable {
    kind: KindT<'s, 't>,
  },
  CantSharePlaceholder {
    kind: KindT<'s, 't>,
  },
  BadIsaSubKind {
    kind: KindT<'s, 't>,
  },
  BadIsaSuperKind {
    kind: KindT<'s, 't>,
  },
  SendingNonCitizen {
    kind: KindT<'s, 't>,
  },
  CantCheckPlaceholder {
    range: &'t [RangeS<'s>],
  },
  ReceivingDifferentOwnerships {
    params: &'t [(IRuneS<'s>, KindT<'s, 't>)],
  },
  SendingNonIdenticalKinds {
    send_coord: KindT<'s, 't>,
    receive_coord: KindT<'s, 't>,
  },
  NoCommonAncestors {
    params: &'t [(IRuneS<'s>, KindT<'s, 't>)],
  },
  LookupFailed {
    path: &'s [IImpreciseNameS<'s>],
  },
  NoAncestorsSatisfyCall {
    params: &'t [(IRuneS<'s>, KindT<'s, 't>)],
  },
  CantDetermineNarrowestKind {
    kinds: &'t [KindT<'s, 't>],
  },
  CallResultWasntExpectedType {
    expected: ITemplataT<'s, 't>,
    actual: ITemplataT<'s, 't>,
  },
  CallResultIsntCallable {
    result: ITemplataT<'s, 't>,
  },
  IsaFailed {
    sub: KindT<'s, 't>,
    suuper: KindT<'s, 't>,
  },
  WrongNumberOfTemplateArgs {
    expected_min_num_args: i32,
    expected_max_num_args: i32,
  },
  FunctionDoesntHaveName {
    range: &'t [RangeS<'s>],
    name: IFunctionNameT<'s, 't>,
  },
  CantGetComponentsOfPlaceholderPrototype {
    range: &'t [RangeS<'s>],
  },
  ReturnTypeConflict {
    range: &'t [RangeS<'s>],
    expected_return_type: KindT<'s, 't>,
    actual: PrototypeT<'s, 't>,
  },
  InternalSolverError {
    range: &'t [RangeS<'s>],
    err: &'t ISolverError<IRuneS<'s>, ITemplataT<'s, 't>, ITypingPassSolverError<'s, 't>>,
  },
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't>
where
  's: 't,
{
  pub fn get_runes(&self, rule: IRulexSR<'s>) -> Vec<IRuneS<'s>> {
    rune_usages(&rule).iter().map(|ru| ru.rune).collect()
  }
}

pub fn get_puzzles<'s>(rule: IRulexSR<'s>) -> Vec<Vec<IRuneS<'s>>> {
  match rule {
    IRulexSR::Lookup(_) => vec![vec![]],
    IRulexSR::RuneParentEnvLookup(_) => vec![vec![]],
    IRulexSR::Call(r) => {
      let mut first = vec![r.template_rune.rune];
      first.extend(r.args.iter().map(|a| a.rune));
      vec![first, vec![r.result_rune.rune, r.template_rune.rune]]
    }
    IRulexSR::CallSiteFunc(r) => vec![vec![r.prototype_rune.rune]],
    IRulexSR::DefinitionFunc(r) => vec![vec![r.params_list_rune.rune, r.return_rune.rune]],
    IRulexSR::Resolve(r) => {
      vec![vec![r.params_list_rune.rune, r.return_rune.rune], vec![r.params_list_rune.rune]]
    }
    IRulexSR::Equals(r) => vec![vec![r.left.rune], vec![r.right.rune]],
    IRulexSR::Literal(_) => vec![vec![]],
    IRulexSR::BorrowRef(r) => vec![vec![r.inner_rune.rune], vec![r.result_rune.rune]],
    IRulexSR::WeakRef(r) => vec![vec![r.inner_rune.rune], vec![r.result_rune.rune]],
    IRulexSR::OwnRef(r) => vec![vec![r.inner_rune.rune], vec![r.result_rune.rune]],
    IRulexSR::KindList(r) => {
      vec![vec![r.result_rune.rune], r.members.iter().map(|m| m.rune).collect()]
    }
    other => panic!("get_puzzles: unhandled rule {:?}", other),
  }
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't>
where
  's: 't,
{
  pub fn make_solver_state_solver(
    &self,
    _range: Vec<RangeS<'s>>,
    env: InferEnv<'s, 't>,
    state: &mut CompilerOutputs<'s, 't>,
    rules: Vec<IRulexSR<'s>>,
    rune_to_type: IndexMap<IRuneS<'s>, ITemplataType<'s>>,
    initially_known_rune_to_templata: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>>,
  ) -> SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>> {
    for rule in &rules {
      for rune_usage in rune_usages(rule) {
        assert!(
          rune_to_type.contains_key(&rune_usage.rune),
          "rune {} is used by a rule but has no type: {:?}",
          humanize_rune(rune_usage.rune),
          rule
        );
      }
    }

    // These two shouldn't both be in the rules, see SROACSD.
    assert!(
      rules.iter().all(|r| !matches!(r, IRulexSR::CallSiteFunc(_)))
        || rules.iter().all(|r| !matches!(r, IRulexSR::DefinitionFunc(_)))
    );

    for (rune, templata) in &initially_known_rune_to_templata {
      if self.opts.global_options.sanity_check {
        self.sanity_check_conclusion(&env, state, *rune, *templata);
      }
      assert_eq!(templata.tyype(self.scout_arena), *rune_to_type.get(rune).unwrap());
    }

    let all_runes: Vec<IRuneS<'s>> = rune_to_type.keys().copied().collect();

    let rule_to_puzzles: Box<dyn Fn(&IRulexSR<'s>) -> Vec<Vec<IRuneS<'s>>>> =
      Box::new(|rule| get_puzzles(*rule));
    let rule_to_runes: &dyn Fn(&IRulexSR<'s>) -> Vec<IRuneS<'s>> = &|rule| self.get_runes(*rule);

    make_solver_state(
      self.opts.global_options.sanity_check,
      self.opts.global_options.use_optimized_solver,
      rule_to_puzzles,
      rule_to_runes,
      rules,
      initially_known_rune_to_templata,
      all_runes,
    )
  }

  pub fn advance_infer(
    &self,
    env: InferEnv<'s, 't>,
    state: &mut CompilerOutputs<'s, 't>,
    solver_state: &mut SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>>,
  ) -> Result<
    bool,
    FailedSolve<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>, ITypingPassSolverError<'s, 't>>,
  > {
    solver_state.sanity_check();
    let solving_rule_index = match solver_state.get_next_solvable() {
      None => return Ok(false),
      Some(s) => s,
    };
    let rule = *solver_state.get_rule(solving_rule_index);
    let steps_before = solver_state.get_steps().len();
    match self.solve(state, env, solver_state, solving_rule_index, rule) {
      Ok(()) => {}
      Err(e) => {
        return Err(FailedSolve {
          steps: solver_state.get_steps(),
          conclusions: solver_state.get_conclusions().into_iter().collect(),
          unsolved_rules: solver_state.get_unsolved_rules(),
          unsolved_runes: solver_state.get_unsolved_runes(),
          error: e,
        })
      }
    }
    let steps_after = solver_state.get_steps().len();
    assert!(steps_after == steps_before + 1);
    // Per @CSCDSRZ, only true after simple solve.
    assert!(solver_state.rule_is_solved(solving_rule_index));
    solver_state.sanity_check();
    Ok(true)
  }

  pub fn continue_solver(
    &self,
    env: InferEnv<'s, 't>,
    state: &mut CompilerOutputs<'s, 't>,
    solver_state: &mut SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>>,
  ) -> Result<
    (),
    FailedSolve<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>, ITypingPassSolverError<'s, 't>>,
  > {
    while {
      self.advance_infer(env, state, solver_state)?
    } {}
    Ok(())
  }
}

pub fn sanity_check_conclusion<'s, 't>(
  env: InferEnv<'s, 't>,
  state: CompilerOutputs<'s, 't>,
  rune: IRuneS<'s>,
  conclusion: ITemplataT<'s, 't>,
) {
  panic!("Unimplemented: sanity_check_conclusion");
  // delegate.sanityCheckConclusion(env, state, rune, conclusion)
}

// fn solve_receives<'s, 'ctx, 't>(
//   compiler: &Compiler<'s, 'ctx, 't>,
//   typing_interner: &TypingInterner<'s, 't>,
//   state: &mut CompilerOutputs<'s, 't>,
//   env: InferEnv<'s, 't>,
//   senders: Vec<(IRuneS<'s>, KindT<'s, 't>)>,
//   call_templates: Vec<ITemplataT<'s, 't>>,
//   all_senders_known: bool,
//   all_calls_known: bool,
// ) -> Result<Option<KindT<'s, 't>>, ITypingPassSolverError<'s, 't>>
// where 's: 't,
// {
//     let sender_kinds: Vec<KindT<'s, 't>> = senders.iter().map(|(_, coord)| coord.kind).collect();
//     if sender_kinds.is_empty() {
//         return Ok(None);
//     }
//     let sender_ancestor_lists: Vec<HashSet<KindT<'s, 't>>> =
//         sender_kinds.iter().map(|kind| compiler.get_ancestors(env, state, *kind, true)).collect();
//     let common_ancestors: HashSet<KindT<'s, 't>> =
//         sender_ancestor_lists.into_iter().reduce(|a, b| a.intersection(&b).copied().collect())
//             .unwrap_or_default();
//     if common_ancestors.is_empty() {
//         let params = typing_interner.alloc_slice_from_vec(senders);
//         return Err(ITypingPassSolverError::NoCommonAncestors { params });
//     }
//     let common_ancestors_call_constrained: HashSet<KindT<'s, 't>> =
//         if call_templates.is_empty() {
//             common_ancestors
//         } else {
//             common_ancestors.into_iter().filter(|ancestor| {
//                 call_templates.iter().any(|template| compiler.kind_is_from_template(state, *ancestor, *template))
//             }).collect()
//         };
//     let narrowed_common_ancestor =
//         if common_ancestors_call_constrained.is_empty() {
//             let params = typing_interner.alloc_slice_from_vec(senders);
//             return Err(ITypingPassSolverError::NoAncestorsSatisfyCall { params });
//         } else if common_ancestors_call_constrained.len() == 1 {
//             *common_ancestors_call_constrained.iter().next().unwrap()
//         } else {
//             if !all_senders_known {
//                 return Ok(None);
//             }
//             if !all_calls_known {
//                 return Ok(None);
//             }
//             match narrow(compiler, typing_interner, env, state, common_ancestors_call_constrained) {
//                 Ok(x) => x,
//                 Err(e) => return Err(e),
//             }
//         };
//     Ok(Some(narrowed_common_ancestor))
// }

fn narrow<'s, 'ctx, 't, 'a>(
  compiler: &'a Compiler<'s, 'ctx, 't>,
  typing_interner: &'a TypingInterner<'s, 't>,
  env: InferEnv<'s, 't>,
  state: &mut CompilerOutputs<'s, 't>,
  kinds: HashSet<KindT<'s, 't>>,
) -> Result<KindT<'s, 't>, ITypingPassSolverError<'s, 't>>
where
  's: 't,
{
  assert!(kinds.len() > 1);
  let mut narrowed_ancestors: HashSet<KindT<'s, 't>> = kinds.iter().copied().collect();
  for kind in kinds.iter() {
    let ancestors = compiler.get_ancestors(env, state, *kind, false);
    for ancestor in ancestors {
      narrowed_ancestors.remove(&ancestor);
    }
  }
  if narrowed_ancestors.is_empty() {
    panic!("vwat: narrowed_ancestors empty in narrow");
  } else if narrowed_ancestors.len() == 1 {
    Ok(*narrowed_ancestors.iter().next().unwrap())
  } else {
    let kinds_slice =
      typing_interner.alloc_slice_from_vec(narrowed_ancestors.into_iter().collect());
    Err(ITypingPassSolverError::CantDetermineNarrowestKind { kinds: kinds_slice })
  }
}

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't>
where
  's: 't,
{
  fn solve(
    &self,
    state: &mut CompilerOutputs<'s, 't>,
    env: InferEnv<'s, 't>,
    solver_state: &mut SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>>,
    rule_index: i32,
    rule: IRulexSR<'s>,
  ) -> Result<(), ISolverError<IRuneS<'s>, ITemplataT<'s, 't>, ITypingPassSolverError<'s, 't>>> {
    //   solveRule(delegate, state, env, ruleIndex, rule, solverState) match {
    //     case Ok(x) => Ok(x)
    //     case Err(e) => Err(RuleError(e))
    //   }
    match self.solve_rule(state, env, rule_index, rule, solver_state) {
      Ok(x) => Ok(x),
      Err(e) => Err(ISolverError::RuleError(RuleError { err: e, _phantom: PhantomData })),
    }
  }

  fn solve_rule(
    &self,
    state: &mut CompilerOutputs<'s, 't>,
    env: InferEnv<'s, 't>,
    rule_index: i32,
    rule: IRulexSR<'s>,
    solver_state: &mut SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>>,
  ) -> Result<(), ITypingPassSolverError<'s, 't>> {
    //   rule match {
    match rule {
            //     case KindComponentsSR(...) =>
            //     case KindComponentsSR(range, kindRune, mutabilityRune) => {
            // IRulexSR::KindComponents(kc) => {
                // VCOORD: retire this
                // match solver_state.get_conclusion(&kc.kind_rune.rune).expect("kind rune not solved in KindComponentsSR") {
                    // ITemplataT::Kind(_) => {}
                    // _ => panic!("Expected KindTemplataT in KindComponentsSR"),
                // };
                // match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], IndexMap::default(), vec![], IndexSet::default()) {
                    // Ok(_) => Ok(()),
                    // Err(e) => {
                        // let ranges = once(kc.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        // let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        // let error = self.typing_interner.alloc(e);
                        // Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                    // }
                // }
            // }
            //     case CoordComponentsSR(range, resultRune, ownershipRune, kindRune) => {
            // IRulexSR::CoordComponents(cc) => {
                // match solver_state.get_conclusion(&cc.result_rune.rune) {
                    // None => {
                        // let ownership = match solver_state.get_conclusion(&cc.ownership_rune.rune).expect("ownership rune not solved in CoordComponentsSR") {
                            // ITemplataT::Ownership(ot) => ot.ownership,
                            // _ => panic!("Expected OwnershipTemplataT in CoordComponentsSR"),
                        // };
                        // let kind = match solver_state.get_conclusion(&cc.kind_rune.rune).expect("kind rune not solved in CoordComponentsSR") {
                            // ITemplataT::Kind(kt) => kt.kind,
                            // _ => panic!("Expected KindTemplataT in CoordComponentsSR"),
                        // };
                        // VCOORD: this should go away probably?
                        // let new_coord = match self.get_sharedness(state, kind) {
                            // SharednessT::Shared => CoordT::new(OwnershipT::Share, RegionT { region: IRegionT::Default }, kind),
                            // SharednessT::Single => CoordT::new(ownership, RegionT { region: IRegionT::Default }, kind),
                        // };
                        // let new_templata = ITemplataT::Kind(self.typing_interner.alloc(CoordTemplataT { coord: new_coord }));
                        // let mut conclusions = IndexMap::default();
                        // conclusions.insert(cc.result_rune.rune, new_templata);
                        // match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            // Ok(_) => Ok(()),
                            // Err(e) => {
                                // let ranges = once(cc.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                // let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                // let error = self.typing_interner.alloc(e);
                                // Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            // }
                        // }
                    // }
                    // Some(coord_templata) => {
                        // let coord = match coord_templata {
                            // ITemplataT::Kind(ct) => ct.coord,
                            // _ => panic!("Expected CoordTemplataT in CoordComponentsSR result"),
                        // };
                        // let mut conclusions = IndexMap::default();
                        // conclusions.insert(cc.ownership_rune.rune, ITemplataT::Ownership(OwnershipTemplataT { ownership: coord.ownership }));
                        // conclusions.insert(cc.kind_rune.rune, ITemplataT::Kind(KindTemplataT { kind: coord.kind }));
                        // match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            // Ok(_) => Ok(()),
                            // Err(e) => {
                                // let ranges = once(cc.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                // let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                // let error = self.typing_interner.alloc(e);
                                // Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            // }
                        // }
                    // }
                // }
            // }
            //     case PrototypeComponentsSR(...) =>
            // IRulexSR::PrototypeComponents(_) => {
                // panic!("Unimplemented: solve_rule PrototypeComponents");
                // val PrototypeTemplataT(prototype) = vassertSome(solverState.getConclusion(resultRune.rune))
                // solverState.commitStep[ITypingPassSolverError](false, Vector(ruleIndex), Map(ownershipRune.rune -> CoordListTemplataT(prototype.paramTypes), kindRune.rune -> CoordTemplataT(prototype.returnType)), Vector(), Set.empty) match { case Ok(_) => Ok(()) case Err(e) => Err(InternalSolverError(range :: env.parentRanges, e)) }
            // }
            //     case ResolveSR(range, resultRune, name, paramListRune, returnRune) => {
            IRulexSR::Resolve(resolve) => {
                // If we're here, then we're resolving a prototype.
                // This happens at the call-site.
                // The function (or struct) can either supply a default resolve rule (usually
                // via the `func moo(int)void` syntax) or let the caller pass it in.
                let param_coords = match solver_state.get_conclusion(&resolve.params_list_rune.rune).expect("paramListRune not solved in ResolveSR") {
                    ITemplataT::CoordList(cl) => cl.kinds,
                    _ => panic!("Expected CoordListTemplataT in ResolveSR paramListRune"),
                };
                //       solverState.getConclusion(returnRune.rune) match {
                //         case Some(CoordTemplataT(returnCoord)) => {
                match solver_state.get_conclusion(&resolve.return_rune.rune) {
                    Some(ITemplataT::Kind(ct)) => {
                        let return_coord = ct.kind; // VCOORD: rename all variables like _coord to _type
                        let prototype_templata = self.predict_function(env, state, resolve.range, resolve.name, param_coords, return_coord);
                        let new_templata = ITemplataT::Prototype(self.typing_interner.alloc(prototype_templata));
                        let mut conclusions = IndexMap::default();
                        conclusions.insert(resolve.result_rune.rune, new_templata);
                        match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                let ranges = once(resolve.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                let error = self.typing_interner.alloc(e);
                                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            }
                        }
                    }
                    Some(_) => panic!("Expected KindTemplataT in ResolveSR returnRune"),
                    //         case None => {
                    None => {
                        let ranges = once(resolve.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        let function_name = self.scout_arena.intern_imprecise_name(
                            IImpreciseNameValS::CodeName(CodeNameValS { name: resolve.name }));
                        let explicit_template_arg_rules_s = &[];
                        let positional_explicit_template_arg_runes_s = &[];
                        let receiving_rune_to_explicit_template_arg_rune = &[];

                        // VCOORD: add arcana for this.
                        let mut bound_runes: Vec<IRuneS<'s>> = Vec::new();
                        for param_type in resolve.params_types.iter() {
                            param_type.collect_rune_mentions(&mut bound_runes);
                        }
                        let mut rune_value_envs: Vec<IInDenizenEnvironmentT<'s, 't>> = Vec::new();
                        for rune in bound_runes {
                            if let Some(ITemplataT::Kind(kt)) = solver_state.get_conclusion(&rune) {
                                match kt.kind {
                                    KindT::Struct(sr) => rune_value_envs.push(
                                        state.get_outer_env_for_type(self.get_struct_template(*sr.id))),
                                    KindT::Interface(ir) => rune_value_envs.push(
                                        state.get_outer_env_for_type(get_interface_template(self.typing_interner, *ir.id))),
                                    KindT::KindPlaceholder(kp) => rune_value_envs.push(
                                        state.get_outer_env_for_type(*self.get_placeholder_template(&kp.id))),
                                    _ => {}
                                }
                            }
                        }
                        let potential_banner = self.find_function(
                            env.original_calling_env,
                            state,
                            ranges_slice,
                            env.call_location,
                            function_name,
                            explicit_template_arg_rules_s,
                            positional_explicit_template_arg_runes_s,
                            receiving_rune_to_explicit_template_arg_rune,
                            env.context_region,
                            param_coords,
                            &rune_value_envs,
                            true,
                            false).expect("CompileErrorExceptionT propagation");
                        match potential_banner {
                            Ok(stamp_result) => {
                                let return_type = stamp_result.prototype.return_type;
                                let mut conclusions = IndexMap::default();
                                conclusions.insert(resolve.result_rune.rune, ITemplataT::Prototype(self.typing_interner.alloc(PrototypeTemplataT { prototype: stamp_result.prototype })));
                                conclusions.insert(resolve.return_rune.rune, ITemplataT::Kind(KindTemplataT { kind: return_type }));
                                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                                    Ok(_) => Ok(()),
                                    Err(e) => {
                                        let error = self.typing_interner.alloc(e);
                                        Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                                    }
                                }
                            }
                            Err(fff) => Err(ITypingPassSolverError::CouldntFindFunction { range: ranges_slice, fff }),
                        }
                    }
                }
            }
            //     case CallSiteFuncSR(range, prototypeRune, name, paramListRune, returnRune) => {
            IRulexSR::CallSiteFunc(csf) => {
                // If we're here, then we're solving in the callsite, not the definition.
                // This should look up a function with that name and param list, and make sure
                // its return matches.
                match solver_state.get_conclusion(&csf.prototype_rune.rune).expect("prototypeRune not solved in CallSiteFuncSR") {
                    ITemplataT::Prototype(proto_templata) => {
                        let prototype = proto_templata.prototype;
                        let mut conclusions = IndexMap::default();
                        conclusions.insert(csf.params_list_rune.rune, ITemplataT::CoordList(self.typing_interner.alloc(KindListTemplataT { kinds: prototype.param_types() })));
                        conclusions.insert(csf.return_rune.rune, ITemplataT::Kind(KindTemplataT { kind: prototype.return_type }));
                        match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                let ranges = once(csf.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                let error = self.typing_interner.alloc(e);
                                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            }
                        }
                    }
                    _ => {
                        let ranges = once(csf.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        Err(ITypingPassSolverError::CantCheckPlaceholder { range: ranges_slice })
                    }
                }
            }
            //     case DefinitionFuncSR(range, resultRune, name, paramListRune, returnRune) => {
            IRulexSR::DefinitionFunc(def_func) => {
                let param_coords = match solver_state.get_conclusion(&def_func.params_list_rune.rune).expect("DefinitionFunc paramListRune has no conclusion") {
                    ITemplataT::CoordList(cl) => cl.kinds,
                    _ => unreachable!("DefinitionFunc: paramListRune is statically typed CoordList"),
                };
                let return_type = match solver_state.get_conclusion(&def_func.return_rune.rune).expect("DefinitionFunc returnRune has no conclusion") {
                    ITemplataT::Kind(ct) => ct.kind,
                    _ => unreachable!("DefinitionFunc: returnRune is statically typed Coord"),
                };
                let new_prototype = self.assemble_prototype(env, state, def_func.range, def_func.name, param_coords, return_type);
                let new_templata = ITemplataT::Prototype(self.typing_interner.alloc(PrototypeTemplataT { prototype: new_prototype }));
                let mut conclusions = IndexMap::default();
                conclusions.insert(def_func.result_rune.rune, new_templata);
                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let ranges = once(def_func.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        let error = self.typing_interner.alloc(e);
                        Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                    }
                }
            }
            IRulexSR::Equals(equals) => {
                match solver_state.get_conclusion(&equals.left.rune) {
                    None => {
                        let right = solver_state.get_conclusion(&equals.right.rune).expect("Neither left nor right rune solved in EqualsSR");
                        let mut conclusions = IndexMap::default();
                        conclusions.insert(equals.left.rune, right.clone());
                        match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                let ranges = once(equals.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                let error = self.typing_interner.alloc(e);
                                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            }
                        }
                    }
                    Some(left) => {
                        let left = left.clone();
                        let mut conclusions = IndexMap::default();
                        conclusions.insert(equals.right.rune, left);
                        match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                let ranges = once(equals.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                                let error = self.typing_interner.alloc(e);
                                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                            }
                        }
                    }
                }
            }
            IRulexSR::Literal(r) => {
                let templata = self.literal_to_templata(r.literal);
                let mut conclusions = IndexMap::default();
                conclusions.insert(r.rune.rune, templata);
                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let ranges = once(r.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        let error = self.typing_interner.alloc(e);
                        Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                    }
                }
            }
            //     case LookupSR(...) =>
            IRulexSR::Lookup(r) => {
                let ranges: Vec<RangeS<'s>> = once(r.range).chain(env.parent_ranges.iter().copied()).collect();
                let mut lookup_filter = HashSet::default();
                lookup_filter.insert(ILookupContext::TemplataLookupContext);
                let found = lookup_nearest_with_path(
                    env.self_env, r.parts, lookup_filter, self.typing_interner);
                let result = match found {
                    None => return Err(ITypingPassSolverError::LookupFailed { path: r.parts }),
                    Some(x) => x,
                };
                let mut conclusions = IndexMap::default();
                conclusions.insert(r.rune.rune, result);
                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        let error = self.typing_interner.alloc(e);
                        Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                    }
                }
            }
            //     case RuneParentEnvLookupSR(...) =>
            IRulexSR::RuneParentEnvLookup(r) => {
                panic!("vwat: RuneParentEnvLookupSR should have been MKRFA-preprocessed before reaching the solver: {:?}", r.rune)
            }
            IRulexSR::Call(r) => {
                self.solve_call_rule(state, &env, solver_state, rule_index, r.range, r.result_rune, r.template_rune, r.args)
            }
              IRulexSR::BorrowRef(r) => {
                  let mut conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> = IndexMap::default();

                    match (solver_state.get_conclusion(&r.result_rune.rune), solver_state.get_conclusion(&r.inner_rune.rune)) {
                        (Some(ITemplataT::Kind(KindTemplataT { kind: result_kind })), _) => {
                            match result_kind {
                                KindT::BorrowRef(BorrowRefT { inner: result_inner_rune }) => {
                                    conclusions.insert(r.inner_rune.rune, ITemplataT::Kind(KindTemplataT{ kind: *result_inner_rune}));
                                }
                                _ => return Err(ITypingPassSolverError::KindIsNotBorrowRef { kind: result_kind }),
                            }
                        },
                        (_, Some(ITemplataT::Kind(KindTemplataT { kind: inner }))) => {
                            let wrap = KindT::BorrowRef(self.typing_interner.alloc(BorrowRefT { inner }));
                            conclusions.insert(r.result_rune.rune, ITemplataT::Kind(KindTemplataT { kind: wrap }));
                        },
                        _ => panic!("Neither result nor inner rune solved in BorrowRef"),
                    }
                  match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                      Ok(_) => Ok(()),
                      Err(e) => {
                          let ranges = once(r.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                          let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                          let error = self.typing_interner.alloc(e);
                          Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                      }
                  }
              }
              IRulexSR::WeakRef(r) => {
                  let mut conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> = IndexMap::default();
                  match (solver_state.get_conclusion(&r.result_rune.rune), solver_state.get_conclusion(&r.inner_rune.rune)) {
                      (Some(ITemplataT::Kind(KindTemplataT { kind: result_kind })), _) => {
                          match result_kind {
                              KindT::WeakRef(WeakRefT { inner: result_inner }) => {
                                  conclusions.insert(r.inner_rune.rune, ITemplataT::Kind(KindTemplataT { kind: *result_inner }));
                              }
                              _ => return Err(ITypingPassSolverError::KindIsNotWeakRef { kind: result_kind }),
                          }
                      },
                      (_, Some(ITemplataT::Kind(KindTemplataT { kind: inner }))) => {
                          let wrap = KindT::WeakRef(self.typing_interner.alloc(WeakRefT { inner }));
                          conclusions.insert(r.result_rune.rune, ITemplataT::Kind(KindTemplataT { kind: wrap }));
                      },
                      _ => panic!("Neither result nor inner rune solved in WeakRef"),
                  }
                  match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                      Ok(_) => Ok(()),
                      Err(e) => {
                          let ranges = once(r.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                          let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                          let error = self.typing_interner.alloc(e);
                          Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                      }
                  }
              }
              IRulexSR::OwnRef(r) => {
                  let mut conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> = IndexMap::default();
                  match (solver_state.get_conclusion(&r.result_rune.rune), solver_state.get_conclusion(&r.inner_rune.rune)) {
                      (Some(ITemplataT::Kind(KindTemplataT { kind: result_kind })), _) => {
                          match result_kind {
                              KindT::OwnRef(OwnRefT { inner: result_inner }) => {
                                  conclusions.insert(r.inner_rune.rune, ITemplataT::Kind(KindTemplataT { kind: *result_inner }));
                              }
                              _ => return Err(ITypingPassSolverError::KindIsNotOwnRef { kind: result_kind }),
                          }
                      },
                      (_, Some(ITemplataT::Kind(KindTemplataT { kind: inner }))) => {
                          let wrap = KindT::OwnRef(self.typing_interner.alloc(OwnRefT { inner }));
                          conclusions.insert(r.result_rune.rune, ITemplataT::Kind(KindTemplataT { kind: wrap }));
                      },
                      _ => panic!("Neither result nor inner rune solved in OwnRef"),
                  }
                  match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                      Ok(_) => Ok(()),
                      Err(e) => {
                          let ranges = once(r.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                          let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                          let error = self.typing_interner.alloc(e);
                          Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                      }
                  }
              }
            IRulexSR::KindList(r) => {
                let conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> =
                    match solver_state.get_conclusion(&r.result_rune.rune) {
                        Some(ITemplataT::CoordList(list)) => {
                            assert_eq!(
                                list.kinds.len(), r.members.len(),
                                "KindList: the solved list has a different length than the rule's members");
                            r.members.iter().zip(list.kinds.iter())
                                .map(|(member_rune, kind)| (
                                    member_rune.rune,
                                    ITemplataT::Kind(KindTemplataT { kind: *kind })))
                                .collect()
                        }
                        Some(other) => panic!("KindList result concluded {:?}, expected a kind list", other),
                        None => {
                            let members: Vec<KindT<'s, 't>> =
                                r.members.iter()
                                    .map(|member_rune| match solver_state.get_conclusion(&member_rune.rune) {
                                        Some(ITemplataT::Kind(KindTemplataT { kind })) => kind,
                                        Some(other) => panic!("KindList member concluded {:?}, expected a kind", other),
                                        None => panic!("Neither the list nor all its members are solved in KindList"),
                                    })
                                    .collect();
                            let kinds = self.typing_interner.alloc_slice_from_vec(members);
                            let mut conclusions = IndexMap::default();
                            conclusions.insert(
                                r.result_rune.rune,
                                ITemplataT::CoordList(self.typing_interner.alloc(KindListTemplataT { kinds })));
                            conclusions
                        }
                    };
                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let ranges = once(r.range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                        let error = self.typing_interner.alloc(e);
                        Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
                    }
                }
            }
            other => unreachable!("solve_rule: {:?}", other),
        }
  }

  fn solve_call_rule(
    &self,
    state: &mut CompilerOutputs<'s, 't>,
    env: &InferEnv<'s, 't>,
    solver_state: &mut SimpleSolverState<IRulexSR<'s>, IRuneS<'s>, ITemplataT<'s, 't>>,
    rule_index: i32,
    range: RangeS<'s>,
    result_rune: RuneUsage<'s>,
    template_rune: RuneUsage<'s>,
    arg_runes: &[RuneUsage<'s>],
  ) -> Result<(), ITypingPassSolverError<'s, 't>> {
    match solver_state.get_conclusion(&result_rune.rune) {
      Some(result) => {
        let ranges: Vec<RangeS<'s>> =
          once(range).chain(env.parent_ranges.iter().copied()).collect();
        let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
        match result {
                    ITemplataT::Kind(kt) => {
                        match kt.kind {
                            KindT::Struct(struct_tt) => {
                                let struct_name = IStructNameT::try_from(struct_tt.id.local_name).unwrap_or_else(|_| panic!("solve_call_rule Some StructTT: local_name is not IStructNameT"));
                                let template_def = solver_state.get_conclusion(&template_rune.rune).unwrap_or_else(|| panic!("solve_call_rule Some StructTT: template_rune not solved"));
                                match template_def {
                                    ITemplataT::StructDefinition(it) => {
                                        if !self.citizen_is_from_template(state,ICitizenTT::Struct(struct_tt), template_def) {
                                            return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: template_def, actual: result });
                                        }
                                        let conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> =
                                            struct_name.template_args().iter().zip(arg_runes.iter())
                                                .map(|(template_arg, arg_rune)| (arg_rune.rune, *template_arg))
                                                .collect();
                                        match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                                            Ok(_) => return Ok(()),
                                            Err(e) => {
                                                let error = self.typing_interner.alloc(e);
                                                return Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error });
                                            }
                                        }
                                    }
                                    other => return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: other, actual: result }),
                                }
                            }
                            KindT::KindPlaceholder(_) => return Err(ITypingPassSolverError::CallResultIsntCallable { result }),
                            KindT::Str(_) | KindT::Int(_) | KindT::Bool(_) | KindT::Float(_) | KindT::Void(_) => {
                                return Err(ITypingPassSolverError::CallResultIsntCallable { result });
                            }
                            KindT::Interface(interface_tt) => {
                                let interface_inner_name = match interface_tt.id.local_name {
                                    INameT::Interface(r) => r,
                                    other => panic!("solve_call_rule Some InterfaceTT: local_name is not IInterfaceNameT: {:?}", other),
                                };
                                let template_def = solver_state.get_conclusion(&template_rune.rune).unwrap_or_else(|| panic!("solve_call_rule Some InterfaceTT: template_rune not solved"));
                                match template_def {
                                    ITemplataT::InterfaceDefinition(_it) => {
                                        if !self.citizen_is_from_template(state,ICitizenTT::Interface(interface_tt), template_def) {
                                            return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: template_def, actual: result });
                                        }
                                    }
                                    other => return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: other, actual: result }),
                                }
                                let conclusions: IndexMap<IRuneS<'s>, ITemplataT<'s, 't>> =
                                    arg_runes.iter().zip(interface_inner_name.template_args.iter())
                                        .map(|(arg_rune, template_arg)| (arg_rune.rune, *template_arg))
                                        .collect();
                                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                                    Ok(_) => return Ok(()),
                                    Err(e) => {
                                        let error = self.typing_interner.alloc(e);
                                        return Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error });
                                    }
                                }
                            }
                            KindT::RuntimeSizedArray(rsa_tt) => {
                                if arg_runes.len() != 1 {
                                    return Err(ITypingPassSolverError::WrongNumberOfTemplateArgs { expected_min_num_args: 1, expected_max_num_args: 1 });
                                }
                                let template_def = solver_state.get_conclusion(&template_rune.rune).expect("vassertSome: template_rune not solved in RuntimeSizedArray arm");
                                match template_def {
                                    ITemplataT::RuntimeSizedArrayTemplate(_) => {
                                        if !self.kind_is_from_template(state, KindT::RuntimeSizedArray(rsa_tt), template_def) {
                                            return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: template_def, actual: result });
                                        }
                                    }
                                    other => return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: other, actual: result }),
                                }
                                let element_rune = arg_runes[0];
                                let mut conclusions = IndexMap::default();
                                conclusions.insert(element_rune.rune, ITemplataT::Kind(KindTemplataT { kind: rsa_tt.element_type() }));
                                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                                    Ok(_) => return Ok(()),
                                    Err(e) => {
                                        let error = self.typing_interner.alloc(e);
                                        return Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error });
                                    }
                                }
                            }
                            KindT::StaticSizedArray(ssa_tt) => {
                                if arg_runes.len() != 2 {
                                    return Err(ITypingPassSolverError::WrongNumberOfTemplateArgs { expected_min_num_args: 2, expected_max_num_args: 2 });
                                }
                                let template_def = solver_state.get_conclusion(&template_rune.rune).expect("vassertSome: template_rune not solved in StaticSizedArray arm");
                                match template_def {
                                    ITemplataT::StaticSizedArrayTemplate(_) => {
                                        if !self.kind_is_from_template(state, KindT::StaticSizedArray(ssa_tt), template_def) {
                                            return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: template_def, actual: result });
                                        }
                                    }
                                    other => return Err(ITypingPassSolverError::CallResultWasntExpectedType { expected: other, actual: result }),
                                }
                                // We don't take in the region rune here because there's no syntactical way to specify it.
                                let size_rune = arg_runes[0];
                                let element_rune = arg_runes[1];
                                let mut conclusions = IndexMap::default();
                                conclusions.insert(size_rune.rune, ssa_tt.size());
                                conclusions.insert(element_rune.rune, ITemplataT::Kind(KindTemplataT { kind: ssa_tt.element_type() }));
                                match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(false, vec![rule_index], conclusions, vec![], IndexSet::default()) {
                                    Ok(_) => return Ok(()),
                                    Err(e) => {
                                        let error = self.typing_interner.alloc(e);
                                        return Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error });
                                    }
                                }
                            }
                            other => return Err(ITypingPassSolverError::KindIsNotFromATemplate { kind: other }),
                        }
                    }
                    _ => unreachable!("solve_call_rule Some branch handles only Kind result; other ITemplataT variants deferred"),
                }
      }
      None => {
        let template = solver_state
          .get_conclusion(&template_rune.rune)
          .expect("vassertSome: template_rune not solved in solve_call_rule None branch");
        match template {
          ITemplataT::RuntimeSizedArrayTemplate(_) => {
            let args: Vec<ITemplataT<'s, 't>> = arg_runes
              .iter()
              .map(|arg_rune| {
                solver_state.get_conclusion(&arg_rune.rune).expect(
                  "vassertSome: arg_rune not solved in solve_call_rule RuntimeSizedArrayTemplate",
                )
              })
              .collect();
            let coord = match args[0] {
              ITemplataT::Kind(ct) => ct.kind,
              _ => panic!(
                "Expected KindTemplataT as first arg in solve_call_rule RuntimeSizedArrayTemplate"
              ),
            };
            let context_region = RegionT::Default;
            let rsa_kind =
              self.predict_runtime_sized_array_kind(*env, state, coord, context_region);
            let mut conclusions = IndexMap::default();
            conclusions.insert(
              result_rune.rune,
              ITemplataT::Kind(KindTemplataT {
                kind:
                  KindT::RuntimeSizedArray(
                    self.typing_interner.intern_runtime_sized_array_tt(RuntimeSizedArrayTTValT {
                      name: rsa_kind.name,
                    }),
                  ),
              }),
            );
            match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(
              false,
              vec![rule_index],
              conclusions,
              vec![],
              IndexSet::default(),
            ) {
              Ok(_) => Ok(()),
              Err(e) => {
                let ranges =
                  once(range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                let error = self.typing_interner.alloc(e);
                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
              }
            }
          }
          ITemplataT::StaticSizedArrayTemplate(_) => {
            let args: Vec<ITemplataT<'s, 't>> = arg_runes
              .iter()
              .map(|arg_rune| {
                solver_state.get_conclusion(&arg_rune.rune).expect(
                  "vassertSome: arg_rune not solved in solve_call_rule StaticSizedArrayTemplate",
                )
              })
              .collect();
            let s = args[0];
            let coord = match args[1] {
              ITemplataT::Kind(ct) => ct.kind,
              _ => panic!(
                "Expected KindTemplataT as second arg in solve_call_rule StaticSizedArrayTemplate"
              ),
            };
            let context_region = RegionT::Default;
            let size = expect_integer(s);
            let ssa_kind =
              self.predict_static_sized_array_kind(*env, state, size, coord, context_region);
            let mut conclusions = IndexMap::default();
            conclusions.insert(
              result_rune.rune,
              ITemplataT::Kind(
                KindTemplataT {
                  kind:
                    KindT::StaticSizedArray(
                      self.typing_interner.intern_static_sized_array_tt(StaticSizedArrayTTValT {
                        name: ssa_kind.name,
                      }),
                    ),
                },
              ),
            );
            let ranges: Vec<RangeS<'s>> =
              once(range).chain(env.parent_ranges.iter().copied()).collect();
            let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
            match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(
              false,
              vec![rule_index],
              conclusions,
              vec![],
              IndexSet::default(),
            ) {
              Ok(_) => Ok(()),
              Err(e) => {
                let error = self.typing_interner.alloc(e);
                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
              }
            }
          }
          ITemplataT::StructDefinition(it) => {
            let args: Vec<ITemplataT<'s, 't>> = arg_runes
              .iter()
              .map(|arg_rune| {
                solver_state
                  .get_conclusion(&arg_rune.rune)
                  .expect("vassertSome: arg_rune not solved in solve_call_rule")
              })
              .collect();
            let kind = self.predict_struct(
              state,
              env.original_calling_env,
              env.parent_ranges,
              env.call_location,
              *it,
              &args,
            );
            let mut conclusions = IndexMap::default();
            conclusions.insert(
              result_rune.rune,
              ITemplataT::Kind(KindTemplataT {
                kind: KindT::Struct(
                  self.typing_interner.intern_struct_tt(StructTTValT { id: *kind.id }),
                ),
              }),
            );
            match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(
              false,
              vec![rule_index],
              conclusions,
              vec![],
              IndexSet::default(),
            ) {
              Ok(_) => Ok(()),
              Err(e) => {
                let ranges =
                  once(range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                let error = self.typing_interner.alloc(e);
                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
              }
            }
          }
          ITemplataT::InterfaceDefinition(it) => {
            let args: Vec<ITemplataT<'s, 't>> = arg_runes
              .iter()
              .map(|arg_rune| {
                solver_state
                  .get_conclusion(&arg_rune.rune)
                  .expect("vassertSome: arg_rune not solved in solve_call_rule")
              })
              .collect();
            // See SFWPRL for why we're calling predict_interface instead of resolve_interface
            let kind = self.predict_interface(
              state,
              env.original_calling_env,
              env.parent_ranges,
              env.call_location,
              *it,
              &args,
            );
            let mut conclusions = IndexMap::default();
            conclusions.insert(
              result_rune.rune,
              ITemplataT::Kind(KindTemplataT {
                kind: KindT::Interface(
                  self.typing_interner.intern_interface_tt(InterfaceTTValT { id: *kind.id }),
                ),
              }),
            );
            match solver_state.commit_step::<ITypingPassSolverError<'s, 't>>(
              false,
              vec![rule_index],
              conclusions,
              vec![],
              IndexSet::default(),
            ) {
              Ok(_) => Ok(()),
              Err(e) => {
                let ranges =
                  once(range).chain(env.parent_ranges.iter().copied()).collect::<Vec<_>>();
                let ranges_slice = self.typing_interner.alloc_slice_from_vec(ranges);
                let error = self.typing_interner.alloc(e);
                Err(ITypingPassSolverError::InternalSolverError { range: ranges_slice, err: error })
              }
            }
          }
          ITemplataT::Kind(kt) => {
            match solver_state.commit_step(
              false,
              vec![rule_index],
              [(result_rune.rune, ITemplataT::Kind(kt))].into_iter().collect(),
              vec![],
              IndexSet::default(),
            ) {
              Ok(_) => return Ok(()),
              Err(e) => {
                let error = self.typing_interner.alloc(e);
                return Err(ITypingPassSolverError::InternalSolverError {
                  range: env.parent_ranges,
                  err: error,
                });
              }
            }
          }
          other => panic!("vimpl: solve_call_rule None {:?}", other),
        }
      }
    }
  }

  fn literal_to_templata(&self, literal: ILiteralSL<'s>) -> ITemplataT<'s, 't> {
    match literal {
            // ILiteralSL::OwnershipLiteral(o) => ITemplataT::Ownership(OwnershipTemplataT { ownership: evaluate_ownership(o.ownership) }),
            ILiteralSL::StringLiteral(s) => ITemplataT::String(s.value),
            ILiteralSL::IntLiteral(i) => ITemplataT::Integer(i.value),
            ILiteralSL::BoolLiteral(_) => unreachable!("literalToTemplata: BoolLiteral constructed by TemplexScout but never reaches solver in practice"),
            // ILiteralSL::LocationLiteral(_) => unreachable!("literalToTemplata: LocationLiteral constructed by TemplexScout but never reaches solver in practice"),
        }
  }
}
