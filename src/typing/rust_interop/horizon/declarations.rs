// Synthesizes ordinary Vale declarations from oracle data.
//
// This is the piece that makes the rest of the compiler stop knowing about Rust. A Rust free
// function becomes a `FunctionS` whose body kind is `IBodyS::ExternBody` — the same shape the
// postparser produces for a hand-written `extern func add_two_numbers(a int, b int) int;`. From
// there nothing is Rust-specific: the solver resolves its rules, and `make_extern_function` mints
// the concrete `PrototypeT` and registers its instantiation bounds at that point — per
// instantiation, with real arguments.
//
// @SMLRZ: a synthesized declaration must be structurally indistinguishable from what the
// postparser produces for the equivalent hand-written Vale source. If the oracle's knowledge of
// Rust's shape is visible anywhere in the `FunctionS`, Rust's rendering has been baked into the
// typing pass. The `Extern` attribute and the `ExternBody` below are exactly what `function_scout`
// attaches; nothing else about the declaration records that rustc was involved.
//
// Every rune this file mints is one of the postparser's own synthesized forms, as in bifrost's
// importer: an intermediate position is an `ImplicitRune` keyed by a `LocationInDenizen`, a
// borrowed parameter's group is an `ImplicitRegionRune` derived from the parameter's rune, and a
// group nobody wrote is `GroupS::Rune` over an `ImplicitGroupRune`. A `CodeRune` means the
// programmer wrote that name, so it appears only for a Rust generic parameter, which carries
// Rust's own identifier.

use crate::interner::StrI;
use crate::parsing::ast::ast::{IMacroInclusionP, SharednessP};
use crate::postparsing::ast::{
  AbstractBodyS, AbstractSP, ExternBodyS, ExternS, FunctionS, GenericParameterS, IBodyS,
  ICitizenAttributeS, IFunctionAttributeS, IGenericParameterTypeS, InterfaceS,
  KindGenericParameterTypeS, LocationInDenizen, LocationInDenizenBuilder, MacroCallS, ParameterS,
  SealedS, StructS,
};
use crate::postparsing::itemplatatype::{
  FunctionTemplataType, ITemplataType, KindTemplataType, TemplateTemplataType,
};
use crate::postparsing::names::{
  ArgumentRuneS, CodeNameValS, CodeRuneS, CodeVarNameS, FunctionNameS, IFunctionDeclarationNameS,
  IImpreciseNameS, IImpreciseNameValS, IRuneValS, IStructDeclarationNameS, IVarDeclarationNameS,
  ImplicitGroupRuneS, ImplicitRegionRuneValS, ImplicitRuneValS, ReturnRuneS,
  TopLevelInterfaceDeclarationNameS, TopLevelStructDeclarationNameS,
};
use crate::postparsing::rules::rules::{
  BorrowRefSR, CallSR, IRulexSR, ImplBoundS, LookupSR, RegionSR, RuneUsage,
};
use crate::postparsing::rules::types::{
  BorrowRefST, CallST, EffectS, GroupS, ITypeST, NameST, RegionS, RuneUsageST,
};
use crate::scout_arena::ScoutArena;
use crate::typing::compiler::Compiler;
use crate::typing::rust_interop::bifrost::oracle::{FuncSignatureR, PrimitiveR, TypeR};
use crate::typing::typing_interner::TypingInterner;
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::utils::range::{CodeLocationS, RangeS};

/// The synthetic code-location offset every synthesized Rust denizen shares. Negative because
/// `CodeLocationS::internal` requires it (real source offsets are non-negative). It carries no
/// identity: a denizen's identity is its template id, which is unique without help from the range.
pub const SYNTHESIZED_RANGE_OFFSET: i32 = -1;

/// Synthesize the declaration for one importable Rust function or method.
///
/// The synthetic range is a single shared sentinel (`SYNTHESIZED_RANGE_OFFSET`), not a per-item
/// location: a denizen's identity is its template id, which is already unique by
/// `(package_coord, init_steps, human_name)` (free functions per package, methods and drops per
/// owner type).
///
/// `None` when any type in the signature has no Vale source-level name yet — see `vale_type_name`.
pub fn synthesize_extern_function<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  package_coord: &'s PackageCoordinate<'s>,
  human_name: StrI<'s>,
  sig: &FuncSignatureR<'s, 't>,
) -> Option<&'s FunctionS<'s>>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  let loc = CodeLocationS::internal(scout_arena, SYNTHESIZED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  // The item's own generic parameters, declared before anything refers to them. A generic
  // position then references its rune *directly*, with no rule at all — which is exactly what
  // the postparser emits for a hand-written `func foo<T>(x T) T`. Empty for a concrete function:
  // the degenerate case, not a separate path.
  let mut generic_params: Vec<&'s GenericParameterS<'s>> = Vec::new();
  let mut generic_runes: Vec<RuneUsage<'s>> = Vec::new();
  for name in sig.generic_param_names.iter() {
    let rune = scout_arena.intern_rune(IRuneValS::CodeRune(CodeRuneS { name: *name }));
    let usage = RuneUsage { range, rune };
    generic_runes.push(usage);
    generic_params.push(scout_arena.alloc(GenericParameterS {
      range,
      rune: usage,
      tyype: IGenericParameterTypeS::KindGenericParameterType(KindGenericParameterTypeS {}),
      default: None,
    }));
  }

  // Rules the *function* owns, which is the return type's and nothing else. A parameter's rules
  // belong to the parameter (@PFVSZ), so they are built per-parameter below.
  let mut header_rules: Vec<IRulexSR<'s>> = Vec::new();

  let mut params: Vec<ParameterS<'s>> = Vec::new();
  // A `mut(g)` effect per `&mut` parameter, mirroring Rust's mutation so the borrow checker can
  // enforce the callee's aliasing rules at the call site. Empty when nothing is borrowed mutably.
  let mut effects: Vec<EffectS<'s>> = Vec::new();
  // Each reference parameter's group rune, in order, so a borrow return can tie its group to the one
  // the input borrowed into (Rust elision). It must be the *same* rune declared on the parameter: the
  // borrow checker's `arg_rune_subst` only substitutes a return rune that a parameter also declares.
  let mut param_region_runes: Vec<RuneUsage<'s>> = Vec::new();
  // The synthesized function mints its own lid space. Every implicit rune and every parameter name
  // takes a distinct child of it.
  let mut lidb = LocationInDenizenBuilder::new(Vec::new());
  for (index, sig_type) in sig.params.iter().enumerate() {
    let own_rune = RuneUsage {
      range,
      rune: scout_arena
        .intern_rune(IRuneValS::ArgumentRune(ArgumentRuneS { arg_index: index as i32 })),
    };
    // The parameter's own bucket, built here and handed straight to `ParameterS::new`. There is
    // no shared list for it to leak into, which is what keeps @PFVSZ's split true by
    // construction rather than by remembering.
    let mut value_type_rules: Vec<IRulexSR<'s>> = Vec::new();
    // Per @PFVSZ a parameter's type splits into its outer ref wraps (the full type) and the value
    // they enclose. A `&self`/`&T` receiver is the one place a synthesized extern has an outer
    // wrap: the borrow chains the full type (a fresh rune) down to the value type (bound in the
    // value bucket). Every other position has no wrap, so full == value.
    //
    // The fourth element is the parameter's `tyype`: a `BorrowRefST` carrying the region group for
    // a borrow (which is where the borrow checker reads the group), a bare rune otherwise. It is
    // metadata alongside the binding rules (@PFVSZ), which stay the source of truth for typing.
    let (full_type_rune, value_type_rune, outer_ref_rules, tyype): (_, _, Vec<IRulexSR<'s>>, _) =
      match sig_type {
        TypeR::Borrow { inner, is_mut } => {
          // The argument binds to the *value* type: a dot-call peels the receiver's outer
          // borrow and matches the value it encloses. `bind_sig_type` returns the rune that
          // value settled to — `own_rune` for a concrete inner like `&Counter` (it binds
          // `own_rune` via Lookup/Call), or the generic's own rune for a `&C` inner (a generic
          // references its rune directly, with no rule that would bind `own_rune`). Wiring the
          // borrow onto an unbound `own_rune` would leave it unsolved.
          let full_type_rune = new_rune(scout_arena, range, &mut lidb);
          let value_rune = bind_sig_type(
            compiler,
            inner,
            own_rune,
            range,
            &generic_runes,
            &mut value_type_rules,
            &mut lidb,
          )?;
          // A region group for this borrow: the parameter borrows `in <group>`, and a `&mut` marks
          // that group `mut(g)`. One rune is shared by the `in` clause on the `tyype` and the
          // effect, so the borrow checker sees them as one group.
          //
          // Derived from the parameter's own argument rune rather than from `value_rune`: two
          // `&T` parameters share one `value_rune` (the generic's), but each gets its own group,
          // which is how Rust's elision gives each borrow its own lifetime (@ELASZ).
          //
          // The group rune is deliberately NOT a `generic_params` entry. The postparser filters
          // every region generic parameter out of a function's `generic_params`, so a region rune
          // lives only on the parameter `tyype` and the effects — never as an identifying rune.
          let region_rune = RuneUsage {
            range,
            rune: scout_arena.intern_rune(IRuneValS::ImplicitRegionRune(ImplicitRegionRuneValS {
              original_rune: own_rune.rune,
            })),
          };
          param_region_runes.push(region_rune);
          let group = scout_arena.alloc(GroupS::Rune(scout_arena.alloc(region_rune)));
          if *is_mut {
            effects.push(EffectS::Mut(group));
          }
          let outer = vec![IRulexSR::BorrowRef(BorrowRefSR {
            range,
            result_rune: full_type_rune,
            inner_rune: value_rune,
            // The rule side carries no group of its own, matching the postparser: the `in g`
            // group survives only on the `BorrowRefST` below.
            region: RegionSR::Group(unwritten_group(scout_arena, range)),
          })];
          let tyype = ITypeST::BorrowRef(scout_arena.alloc(BorrowRefST {
            range,
            inner: scout_arena
              .alloc(ITypeST::Rune(scout_arena.alloc(RuneUsageST { rune: value_rune }))),
            region: RegionS::Group(group),
          }));
          (full_type_rune, value_rune, outer, tyype)
        }
        _ => {
          let rune = bind_sig_type(
            compiler,
            sig_type,
            own_rune,
            range,
            &generic_runes,
            &mut value_type_rules,
            &mut lidb,
          )?;
          // A bare rune, exactly what the postparser hands a closure or magic param
          // (`create_lambda_param`/`create_magic_parameters`). The binding rules stay the source of
          // truth (@PFVSZ); this carries the shape alongside them.
          let tyype = ITypeST::Rune(scout_arena.alloc(RuneUsageST { rune }));
          (rune, rune, Vec::new(), tyype)
        }
      };
    params.push(ParameterS::new(
      range,
      None,
      false,
      IVarDeclarationNameS::CodeVarName(CodeVarNameS {
        imprecise_name: scout_arena
          .intern_code_name(scout_arena.intern_str(&format!("p{}", index))),
        lid: lidb.child().consume_in_arena(scout_arena),
      }),
      tyype,
      full_type_rune,
      value_type_rune,
      scout_arena.alloc_slice_from_vec(outer_ref_rules),
      scout_arena.alloc_slice_from_vec(value_type_rules),
    ));
  }

  let ret_own_rune =
    RuneUsage { range, rune: scout_arena.intern_rune(IRuneValS::ReturnRune(ReturnRuneS {})) };
  let ret_rune = bind_sig_type(
    compiler,
    &sig.ret,
    ret_own_rune,
    range,
    &generic_runes,
    &mut header_rules,
    &mut lidb,
  )?;

  // A borrow return ties, by Rust elision, to the one reference input it borrows from — and it
  // borrows somewhere *within* that input's territory, so its group is the descendant `g...` form,
  // reusing that input parameter's group rune. When a call churns that argument (`mut(g)`), the
  // returned reference is invalidated (use-after-churn). Only the single-reference-input case is
  // handled — elision rule 2, and the `&self` receiver every imported borrow-returning method has;
  // zero or several reference inputs can't be elided here, so that return is written as a borrow
  // with no group, the shape the borrow checker rejects on a Vale function.
  let maybe_return_type = match (&sig.ret, param_region_runes.as_slice()) {
    (TypeR::Borrow { inner, .. }, [input_region_rune]) => {
      let base = scout_arena.alloc(GroupS::Rune(scout_arena.alloc(*input_region_rune)));
      let descendant = scout_arena.alloc(GroupS::Ellipsis { base });
      value_position_type_st(compiler, inner, range, &generic_runes).map(|inner_st| {
        ITypeST::BorrowRef(scout_arena.alloc(BorrowRefST {
          range,
          inner: scout_arena.alloc(inner_st),
          region: RegionS::Group(descendant),
        }))
      })
    }
    // Every other return is written in value position — a citizen by its short name, a primitive by
    // its Vale name, a generic by its rune. Every non-lambda carries a written return type; the
    // borrow checker reads a callee's return groups off this declaration.
    _ => value_position_type_st(compiler, &sig.ret, range, &generic_runes),
  };

  // Emit each imported-trait bound (`where implements(P, Trait)`) the oracle surfaced. The sub is
  // the generic param's own rune; the super is the trait, bound through the same `bind_sig_type`
  // path a parameter citizen uses so its Lookup/Call rules join `header_rules` and both operands get
  // conclusions. The result rune has no rules — the post-solve pass fills it — so it is an
  // ImplicitRune kept out of every rune-usage list, exactly as the postparser mints it (rule_scout).
  // Without these bounds a rust caller `run<C: MainLoop>` carries no `C: MainLoop`, so Vale never
  // resolves the impl `MyStruct: MainLoop` nor records the concrete override the reverse callback needs.
  let mut impl_bounds: Vec<ImplBoundS<'s>> = Vec::new();
  for bound in sig.generic_param_bounds.iter() {
    let sub_rune = generic_runes[bound.sub_generic_index as usize];
    let super_own_rune = new_rune(scout_arena, range, &mut lidb);
    let super_rune = bind_sig_type(
      compiler,
      &bound.super_trait,
      super_own_rune,
      range,
      &generic_runes,
      &mut header_rules,
      &mut lidb,
    )?;
    let result_rune = new_rune(scout_arena, range, &mut lidb);
    impl_bounds.push(ImplBoundS { range, sub_rune, super_rune, result_rune });
  }

  // One template parameter per declared generic, typed as a kind. Empty for a concrete
  // function, which is what makes `make_extern_function` — which reads its template arguments
  // off the *solved* environment — work identically for both.
  let tyype = TemplateTemplataType {
    param_types: scout_arena.alloc_slice_from_vec::<ITemplataType<'s>>(
      generic_params.iter().map(|p| p.tyype.tyype()).collect(),
    ),
    return_type: scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {})),
  };

  Some(scout_arena.alloc(FunctionS::new(
    range,
    IFunctionDeclarationNameS::FunctionName(FunctionNameS {
      imprecise_name: scout_arena.intern_code_name(human_name),
      code_location: loc,
      lid: LocationInDenizen { path: &[] },
    }),
    // The same attribute `function_scout` attaches for a source-level `extern func`. It is
    // what `translate_function_attributes` turns into `IFunctionAttributeT::Extern`, and
    // downstream what marks the denizen as foreign.
    scout_arena.alloc_slice_from_vec(vec![IFunctionAttributeS::Extern(ExternS { package_coord })]),
    scout_arena.alloc_slice_from_vec(generic_params),
    tyype,
    scout_arena.alloc_slice_from_vec(params),
    Some(ret_rune),
    // The return's group-annotated type when it is a borrow tied to an input (built above); the
    // value-position type otherwise. The borrow checker reads this to derive a callee's
    // returned-borrow group at the call site.
    maybe_return_type,
    // One `mut(g)` per `&mut` parameter — Rust's mutation, mirrored so the borrow checker can hold
    // callers to the callee's aliasing rules. Empty when nothing is borrowed mutably.
    scout_arena.alloc_slice_from_vec(effects),
    scout_arena.alloc_slice_from_vec(header_rules),
    // Impl bounds (`where implements(P, Trait)`) surfaced from rustc's predicates for imported
    // traits — see the loop above. For the forward direction rustc discharges these, so this is
    // usually empty; the reverse direction relies on it.
    scout_arena.alloc_slice_from_vec(impl_bounds),
    &[],
    scout_arena.alloc(IBodyS::ExternBody(ExternBodyS {})),
  )))
}

/// Bind `own_rune` to one signature position, emitting whatever rules that takes.
///
/// Three shapes, and the split is by *what the name resolves to*, never by argument count:
///
///   - **A generic parameter** references its declared rune directly, with no rule at all — which
///     is what the postparser emits for a hand-written `func foo<T>(x T) T`.
///   - **A primitive** is one `LookupSR`, because the builtins store holds `int` as a bare
///     `ITemplataT::Kind`.
///   - **A citizen is always two rules** — `LookupSR` binding a rune to the *template*, then
///     `CallSR` applying the argument runes to it. A Rust citizen is registered as
///     `IEnvEntryT::Struct`, so its name resolves to a `StructDefinition` template, and turning a
///     template into a kind is what `CallSR` does. **Zero arguments is the degenerate case, not a
///     special one** (@NNGZ): skipping the call for a non-generic citizen fails loudly, with the
///     parameter rune resolving to a `StructDefinition` where a `Kind` is wanted.
///
/// Recursive, so `Holder<Holder<int>>` and `Holder<T>` fall out rather than needing their own
/// cases — an argument is just another position.
///
/// `None` for anything not nameable, which drops the whole declaration rather than importing it
/// with a hole.
fn bind_sig_type<'s, 't>(
  compiler: &Compiler<'s, '_, 't>,
  sig_type: &TypeR<'s, 't>,
  own_rune: RuneUsage<'s>,
  range: RangeS<'s>,
  generic_runes: &[RuneUsage<'s>],
  rules: &mut Vec<IRulexSR<'s>>,
  lidb: &mut LocationInDenizenBuilder,
) -> Option<RuneUsage<'s>>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  match sig_type {
    TypeR::Generic(index) => {
      // A declared generic parameter *is* its rune. Two parameters of the same type therefore
      // share one rune, which is what `f<T>(a T, b T)` means.
      generic_runes.get(*index as usize).copied()
    }
    TypeR::Primitive(primitive) => {
      let name = vale_type_name(compiler, *primitive);
      rules.push(IRulexSR::Lookup(LookupSR {
        range,
        rune: own_rune,
        // One segment, deliberately. A primitive — `int`, `bool`, `void` — lives in the builtins
        // store under a bare name. Qualifying it would un-resolve it; only a citizen carries a
        // package path.
        parts: scout_arena.alloc_slice_copy(&[
          scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name }))
        ]),
      }));
      Some(own_rune)
    }
    TypeR::Citizen { name, package, args } => {
      let template_rune = new_rune(scout_arena, range, lidb);
      rules.push(IRulexSR::Lookup(LookupSR {
        range,
        rune: template_rune,
        // A multi-segment path: the citizen is named by its package coordinate followed by its
        // short name — `mycrate.Widget` — so two crates exporting the same short name are reached
        // by different paths and the ambiguity never forms. Both ends are ours: the importer
        // registers the store under this coordinate and this writes the same one, so they agree by
        // construction rather than by a key both sides have to compute identically.
        parts: package_path(scout_arena, package, *name),
      }));

      let mut arg_runes: Vec<RuneUsage<'s>> = Vec::new();
      for arg in args.iter() {
        // An argument is just another position, so it goes through the same call. A generic
        // one comes back as the declared rune, which is what lets the solver run the call
        // backwards from a concrete argument; anything else binds the fresh rune offered here.
        let fresh = new_rune(scout_arena, range, lidb);
        let arg_rune = bind_sig_type(compiler, arg, fresh, range, generic_runes, rules, lidb)?;
        arg_runes.push(arg_rune);
      }

      rules.push(IRulexSR::Call(CallSR {
        range,
        result_rune: own_rune,
        template_rune,
        args: scout_arena.alloc_slice_from_vec(arg_runes),
      }));
      Some(own_rune)
    }
    TypeR::Borrow { inner, .. } => {
      // A borrow in a non-parameter position — a return type, or nested inside a citizen's
      // arguments (`Vec<&T>`) — where the wrap belongs inline in the value rules rather than in
      // a parameter's outer-ref bucket. A parameter's *top-level* borrow is split off by the
      // caller in `synthesize_extern_function` before it reaches here, per @PFVSZ. A nested
      // borrow's mutation is not yet mirrored into a group (only a top-level parameter borrow
      // is), so `is_mut` is not read here.
      //
      // The borrow wraps the rune the inner *settled to*, not the fresh one offered: for a
      // concrete inner that is the fresh rune (bound by its Lookup/Call), but for a generic inner
      // (`&T`, the return of `at<T>(&Vec<T>, i64) -> &T`) it is the generic's own rune, and no
      // rule ever binds the fresh one.
      let offered_inner_rune = new_rune(scout_arena, range, lidb);
      let inner_rune =
        bind_sig_type(compiler, inner, offered_inner_rune, range, generic_runes, rules, lidb)?;
      rules.push(IRulexSR::BorrowRef(BorrowRefSR {
        range,
        result_rune: own_rune,
        inner_rune,
        region: RegionSR::Group(unwritten_group(scout_arena, range)),
      }));
      Some(own_rune)
    }
  }
}

/// The value-position `ITypeST` for a synthesized function's return type or an abstract method's
/// param or return type — what the anonymous-substruct macro's `where func __call` bound needs so
/// `resolve_citizen_bounds` can reify it. That resolver evaluates each param/return with only the
/// citizen's env + generic-substitution map — never the method's header rules — so the type must be
/// self-contained: a concrete type name as a zero-arg template `Call` (resolved by name lookup,
/// @TNLTZACZ), a generic as a bare substitution rune. This is exactly the shape the parser produces
/// for a hand-written native method's param/return, so the synthesizer mirrors it rather than
/// emitting the internal value-runes it uses for the method's own header.
///
/// `None` means "not expressible in value position", which for a realistic (non-generic) Rust trait
/// does not occur — every param/return is a primitive, void, or a Rust citizen (or a borrow of one).
/// The caller gates the whole macro off a trait if any method returns `None`, so the bound is never
/// emitted with a hole.
fn value_position_type_st<'s, 't>(
  compiler: &Compiler<'s, '_, 't>,
  sig_type: &TypeR<'s, 't>,
  range: RangeS<'s>,
  generic_runes: &[RuneUsage<'s>],
) -> Option<ITypeST<'s>>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  match sig_type {
    // A declared generic *is* its rune; a bare rune is exactly what `resolve_citizen_bounds` reifies
    // against the citizen's generic-param substitution.
    TypeR::Generic(index) => generic_runes
      .get(*index as usize)
      .map(|ru| ITypeST::Rune(scout_arena.alloc(RuneUsageST { rune: *ru }))),
    // A primitive is a bare builtin name → the value-position zero-arg Call form the parser and the
    // macro's drop bound both use, resolved by name lookup in the env.
    TypeR::Primitive(primitive) => {
      let name = vale_type_name(compiler, *primitive);
      Some(value_position_name_call(scout_arena, range, name, Vec::new()))
    }
    // A borrow wraps its inner value-position type, in a group nobody wrote. A top-level param
    // borrow's real group lives on the param's own `tyype`, added by the caller.
    TypeR::Borrow { inner, .. } => {
      let inner_st = value_position_type_st(compiler, inner, range, generic_runes)?;
      Some(ITypeST::BorrowRef(scout_arena.alloc(BorrowRefST {
        range,
        inner: scout_arena.alloc(inner_st),
        region: RegionS::Group(unwritten_group(scout_arena, range)),
      })))
    }
    // A Rust citizen resolves by its SHORT single-segment name: the importer seeds it in its
    // crate's package under its `CodeName`, and cross-package lookup is unconditional (every
    // package's store is in `global_namespaces`), so a bare `Name{NobiliaWindow}` resolves from the
    // substruct's env even without the user's `import` in scope. Generic citizens carry their arg
    // value-position types.
    TypeR::Citizen { name, args, .. } => {
      let arg_sts: Option<Vec<ITypeST<'s>>> = args
        .iter()
        .map(|a| value_position_type_st(compiler, a, range, generic_runes))
        .collect();
      Some(value_position_name_call(scout_arena, range, *name, arg_sts?))
    }
  }
}

/// A value-position type name applied to (possibly zero) args — a `Lookup`'s name wrapped in a `Call`,
/// per @TNLTZACZ (a type name never lowers to a bare name; it is always applied). Zero args is the bare
/// name case (`int`, `NobiliaWindow`); N args a generic instantiation (`Vec<int>`).
fn value_position_name_call<'s>(
  scout_arena: &ScoutArena<'s>,
  range: RangeS<'s>,
  name: StrI<'s>,
  args: Vec<ITypeST<'s>>,
) -> ITypeST<'s> {
  let imprecise =
    scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name }));
  let arg_refs: Vec<&'s ITypeST<'s>> =
    args.into_iter().map(|st| &*scout_arena.alloc(st)).collect();
  ITypeST::Call(scout_arena.alloc(CallST {
    range,
    template: scout_arena.alloc(ITypeST::Name(scout_arena.alloc(NameST { range, name: imprecise }))),
    args: scout_arena.alloc_slice_from_vec(arg_refs),
  }))
}

/// Whether every param and the return of a synthesized trait method is expressible in value position,
/// so the anonymous-substruct macro can build a `where func` bound the resolver can reify. Used to gate
/// the macro off a trait whose signatures it cannot yet project (the method still projects for the
/// hand-written-forwarder path; only the auto-substruct is withheld).
fn sig_is_anon_representable<'s, 't>(
  compiler: &Compiler<'s, '_, 't>,
  sig: &FuncSignatureR<'s, 't>,
) -> bool
where
  's: 't,
{
  let range = RangeS::new(
    CodeLocationS::internal(compiler.scout_arena, SYNTHESIZED_RANGE_OFFSET),
    CodeLocationS::internal(compiler.scout_arena, SYNTHESIZED_RANGE_OFFSET),
  );
  let generic_runes: Vec<RuneUsage<'s>> = Vec::new();
  // `&mut` borrows are anon-eligible: the final-file generator reads the abstract method's `mut(g)`
  // effects to render `&mut` (mutability lives on the effect clause + region, not the type, @BCHATZ).
  let representable =
    |t: &TypeR<'s, 't>| value_position_type_st(compiler, t, range, &generic_runes).is_some();
  sig.params.iter().all(representable) && representable(&sig.ret)
}

/// A fresh rune for one of the intermediate positions a signature needs: an `ImplicitRune` at the
/// next child of the declaration's lid space, the form the postparser mints for a position nobody
/// named.
fn new_rune<'s>(
  scout_arena: &ScoutArena<'s>,
  range: RangeS<'s>,
  lidb: &mut LocationInDenizenBuilder,
) -> RuneUsage<'s> {
  RuneUsage {
    range,
    rune: scout_arena
      .intern_rune(IRuneValS::ImplicitRune(ImplicitRuneValS::new(lidb.child().borrow_val()))),
  }
}

/// The group a borrow carries when nobody wrote one: `GroupS::Rune` over an `ImplicitGroupRune`,
/// the shape `templex_scout.rs` builds for an unwritten group.
fn unwritten_group<'s>(scout_arena: &ScoutArena<'s>, range: RangeS<'s>) -> &'s GroupS<'s> {
  scout_arena.alloc(GroupS::Rune(scout_arena.alloc(RuneUsage {
    range,
    rune: scout_arena.intern_rune(IRuneValS::ImplicitGroupRune(ImplicitGroupRuneS { range })),
  })))
}

/// Synthesize the declaration for one importable Rust type.
///
/// The counterpart to `synthesize_extern_function`, and the same idea: hand the ordinary machinery
/// a *declaration* and let it produce the definition. `precompile_struct` and `compile_struct` then
/// do the `declare_type` / `add_struct` / environment work.
///
/// **`generic_params` is the payload.** It is what makes `Holder` a *template* rather than a
/// finished type — and therefore what gives a `CallSR` something to apply `[int]` to. Without it,
/// `Holder<i32>` and `Holder<bool>` intern to one argument-less kind.
///
/// Three fields say something worth stating:
///
///   - **`members: &[]`** — zero members is the truth, not a stub. Vale is an external consumer of
///     a Rust type; its layout is opaque and its private fields are none of Vale's business.
///   - **`internal_methods: &[]`** — a Rust method is an ordinary declaration in the type's outer
///     environment whose first parameter is the receiver, not something declared inside the
///     citizen's braces.
///   - **both derive macros suppressed** — the derived constructor would claim a layout Vale does
///     not have, and the derived drop destructures zero members, so it would be an empty
///     destructor that never reaches rustc. The oracle's synthesized `drop` has an `ExternBody`
///     that becomes `__vale_drop<T>` → `drop_in_place::<T>` at codegen instead, letting rustc
///     resolve its own drop glue.
pub fn synthesize_extern_struct<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  package_coord: &'s PackageCoordinate<'s>,
  human_name: StrI<'s>,
  generic_param_names: &[StrI<'s>],
) -> &'s StructS<'s>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  let loc = CodeLocationS::internal(scout_arena, SYNTHESIZED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  let generic_params = kind_generic_params(scout_arena, range, generic_param_names);

  let tyype = TemplateTemplataType {
    param_types: scout_arena.alloc_slice_from_vec::<ITemplataType<'s>>(
      generic_params.iter().map(|p| p.tyype.tyype()).collect(),
    ),
    return_type: scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {})),
  };

  let dont_call = |macro_name: StrI<'s>| {
    ICitizenAttributeS::MacroCall(MacroCallS {
      range,
      include: IMacroInclusionP::DontCallMacro,
      macro_name,
    })
  };

  scout_arena.alloc(StructS::new(
    range,
    IStructDeclarationNameS::TopLevelStructDeclarationName(TopLevelStructDeclarationNameS {
      name: human_name,
      range,
    }),
    scout_arena.alloc_slice_from_vec(vec![
      // The same attribute the postparser attaches for a hand-written `extern struct`.
      ICitizenAttributeS::Extern(ExternS { package_coord }),
      dont_call(compiler.keywords.derive_struct_constructor),
      dont_call(compiler.keywords.derive_struct_drop),
    ]),
    scout_arena.alloc_slice_from_vec(generic_params),
    // Rust will never support sharedness variants, so Single is permanent rather than provisional.
    SharednessP::Single,
    tyype,
    // No bounds: rustc discharges a Rust type's own obligations, and we read no predicates.
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
  ))
}

/// The interface analog of `synthesize_extern_struct`: a Rust **enum** becomes an opaque, **sealed**
/// `InterfaceS` with zero variants (Vale is an external consumer; no variant is projected yet),
/// carrying the `Extern` attribute so it lowers to an extern denizen. Its methods and drop attach
/// lazily in the type's outer env exactly like a struct's.
pub fn synthesize_extern_interface<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  package_coord: &'s PackageCoordinate<'s>,
  human_name: StrI<'s>,
  generic_param_names: &[StrI<'s>],
) -> &'s InterfaceS<'s>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  let loc = CodeLocationS::internal(scout_arena, SYNTHESIZED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  let generic_params = kind_generic_params(scout_arena, range, generic_param_names);

  let tyype = TemplateTemplataType {
    param_types: scout_arena.alloc_slice_from_vec::<ITemplataType<'s>>(
      generic_params.iter().map(|p| p.tyype.tyype()).collect(),
    ),
    return_type: scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {})),
  };

  scout_arena.alloc(InterfaceS::new(
    range,
    scout_arena.alloc(TopLevelInterfaceDeclarationNameS { name: human_name, range }),
    scout_arena.alloc_slice_from_vec(vec![
      // The `extern` attribute the postparser attaches, plus `sealed`: a Rust enum is a closed sum.
      ICitizenAttributeS::Extern(ExternS { package_coord }),
      ICitizenAttributeS::Sealed(SealedS),
    ]),
    scout_arena.alloc_slice_from_vec(generic_params),
    SharednessP::Single,
    tyype,
    &[], // rules
    &[], // internal_methods
    &[], // impl_bounds
    &[], // func_bounds
  ))
}

/// One kind-typed generic parameter per Rust generic parameter name, each a `CodeRune` carrying
/// Rust's own identifier.
fn kind_generic_params<'s>(
  scout_arena: &ScoutArena<'s>,
  range: RangeS<'s>,
  generic_param_names: &[StrI<'s>],
) -> Vec<&'s GenericParameterS<'s>> {
  generic_param_names
    .iter()
    .map(|name| {
      let rune = scout_arena.intern_rune(IRuneValS::CodeRune(CodeRuneS { name: *name }));
      &*scout_arena.alloc(GenericParameterS {
        range,
        rune: RuneUsage { range, rune },
        tyype: IGenericParameterTypeS::KindGenericParameterType(KindGenericParameterTypeS {}),
        default: None,
      })
    })
    .collect()
}

/// Rewrite a trait method's signature so the trait's implicit `Self` (generic index 0) becomes the
/// interface itself. A trait method's `&self` receiver reads as `&Self`; the abstract interface
/// method's receiver must be `&Callback`, so every `Generic(0)` position is replaced by the interface
/// citizen. Only the simple case is handled — a method whose sole generic is `Self` — so any other
/// `Generic` here is an unsupported own-generic and is left as-is (its declaration will decline).
fn map_self_to_interface<'s, 't>(
  interner: &TypingInterner<'s, 't>,
  sig_type: &TypeR<'s, 't>,
  interface_name: StrI<'s>,
  package: &'s PackageCoordinate<'s>,
) -> TypeR<'s, 't>
where
  's: 't,
{
  match sig_type {
    TypeR::Generic(0) => TypeR::Citizen { package, name: interface_name, args: &[] },
    TypeR::Generic(i) => TypeR::Generic(*i),
    TypeR::Primitive(p) => TypeR::Primitive(*p),
    TypeR::Citizen { package: p, name, args } => {
      let mapped: Vec<TypeR<'s, 't>> =
        args.iter().map(|a| map_self_to_interface(interner, a, interface_name, package)).collect();
      TypeR::Citizen { package: p, name: *name, args: interner.alloc_slice_from_vec(mapped) }
    }
    TypeR::Borrow { inner, is_mut } => TypeR::Borrow {
      inner: interner.alloc(map_self_to_interface(interner, inner, interface_name, package)),
      is_mut: *is_mut,
    },
  }
}

/// One abstract method of a synthesized trait-interface — the AHT `FunctionS` `function_scout`
/// produces for a native `func on_call(virtual self &Callback) int;`. It is `synthesize_extern_function`
/// with exactly three differences: the receiver (parameter 0) is the virtual dispatch parameter, the
/// body is `AbstractBody`, and it carries no `Extern` attribute (an interface method's abstractness is
/// the parent interface plus the virtual receiver, not an attribute). `sig` must already have `Self`
/// mapped to the interface and carry no generic parameters (they must equal the interface's, which is
/// non-generic here). A borrowed receiver is the `@PFVSZ` outer-ref split, identical to a `&self`
/// extern param — the borrow lives in the parameter's `type_outer_ref_rules` as a `BorrowRefSR`.
fn synthesize_abstract_interface_method<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  human_name: StrI<'s>,
  sig: &FuncSignatureR<'s, 't>,
) -> Option<&'s FunctionS<'s>>
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  let loc = CodeLocationS::internal(scout_arena, SYNTHESIZED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  // `InterfaceS::new` asserts each internal method's generic params equal the interface's. The
  // interface is non-generic (Self filtered, generic trait methods unsupported), so the abstract
  // method carries none and there are no generic runes to reference.
  if !sig.generic_param_names.is_empty() {
    return None;
  }
  let generic_runes: Vec<RuneUsage<'s>> = Vec::new();

  let mut header_rules: Vec<IRulexSR<'s>> = Vec::new();
  // A `mut(g)` effect per `&mut` borrow, mirroring `synthesize_extern_function`'s forward behavior —
  // the faithful representation of Rust's `&mut` on the reverse-direction abstract method. It is not
  // itself borrow-checked (an abstract method is never groupified), so it is inert until something
  // enforces `mut`-parity between the abstract method and its override. Empty when nothing is
  // borrowed mutably.
  let mut effects: Vec<EffectS<'s>> = Vec::new();
  let mut params: Vec<ParameterS<'s>> = Vec::new();
  let mut lidb = LocationInDenizenBuilder::new(Vec::new());
  for (index, sig_type) in sig.params.iter().enumerate() {
    let own_rune = RuneUsage {
      range,
      rune: scout_arena
        .intern_rune(IRuneValS::ArgumentRune(ArgumentRuneS { arg_index: index as i32 })),
    };
    let mut value_type_rules: Vec<IRulexSR<'s>> = Vec::new();
    let (full_type_rune, value_type_rune, outer_ref_rules, tyype): (_, _, Vec<IRulexSR<'s>>, _) =
      match sig_type {
        // Mirror `synthesize_extern_function`'s borrow arm: a borrow parameter (including the `&mut self`
        // receiver) carries a region group on its `tyype`, and a `&mut` marks that group `mut(g)`.
        TypeR::Borrow { inner, is_mut } => {
          let full_type_rune = new_rune(scout_arena, range, &mut lidb);
          let value_rune = bind_sig_type(
            compiler,
            inner,
            own_rune,
            range,
            &generic_runes,
            &mut value_type_rules,
            &mut lidb,
          )?;
          let region_rune = RuneUsage {
            range,
            rune: scout_arena.intern_rune(IRuneValS::ImplicitRegionRune(ImplicitRegionRuneValS {
              original_rune: own_rune.rune,
            })),
          };
          let group = scout_arena.alloc(GroupS::Rune(scout_arena.alloc(region_rune)));
          if *is_mut {
            effects.push(EffectS::Mut(group));
          }
          let outer = vec![IRulexSR::BorrowRef(BorrowRefSR {
            range,
            result_rune: full_type_rune,
            inner_rune: value_rune,
            region: RegionSR::Group(unwritten_group(scout_arena, range)),
          })];
          // The `tyype` is what the anon-substruct macro copies into its `where func __call` bound, and
          // `resolve_citizen_bounds` reifies it with no access to these header rules — so it must be the
          // self-contained value-position form (a name-`Call`, mirroring the parser), NOT the internal
          // `value_rune`. If the inner type isn't value-position-expressible (does not occur for a
          // representable trait — the caller gates the macro off such traits), fall back to the rune so
          // the method still compiles for the hand-written-forwarder path.
          let inner_st = value_position_type_st(compiler, inner, range, &generic_runes)
            .unwrap_or_else(|| ITypeST::Rune(scout_arena.alloc(RuneUsageST { rune: value_rune })));
          let tyype = ITypeST::BorrowRef(scout_arena.alloc(BorrowRefST {
            range,
            inner: scout_arena.alloc(inner_st),
            region: RegionS::Group(group),
          }));
          (full_type_rune, value_rune, outer, tyype)
        }
        _ => {
          let rune = bind_sig_type(
            compiler,
            sig_type,
            own_rune,
            range,
            &generic_runes,
            &mut value_type_rules,
            &mut lidb,
          )?;
          // Value-position form for the bound (see the borrow arm); rune fallback for the method's own
          // compile if not expressible.
          let tyype = value_position_type_st(compiler, sig_type, range, &generic_runes)
            .unwrap_or_else(|| ITypeST::Rune(scout_arena.alloc(RuneUsageST { rune })));
          (rune, rune, Vec::new(), tyype)
        }
      };
    // The receiver (parameter 0) is virtual — the interface-compile reads its virtual slot.
    // `is_internal_method` is true because the method lives inside the interface citizen.
    let virtuality =
      if index == 0 { Some(AbstractSP { range, is_internal_method: true }) } else { None };
    params.push(ParameterS::new(
      range,
      virtuality,
      false,
      IVarDeclarationNameS::CodeVarName(CodeVarNameS {
        imprecise_name: scout_arena
          .intern_code_name(scout_arena.intern_str(&format!("p{}", index))),
        lid: lidb.child().consume_in_arena(scout_arena),
      }),
      tyype,
      full_type_rune,
      value_type_rune,
      scout_arena.alloc_slice_from_vec(outer_ref_rules),
      scout_arena.alloc_slice_from_vec(value_type_rules),
    ));
  }

  let ret_own_rune =
    RuneUsage { range, rune: scout_arena.intern_rune(IRuneValS::ReturnRune(ReturnRuneS {})) };
  let ret_rune = bind_sig_type(
    compiler,
    &sig.ret,
    ret_own_rune,
    range,
    &generic_runes,
    &mut header_rules,
    &mut lidb,
  )?;
  // The value-position return type the anon-substruct macro's `where func __call` bound needs (a
  // native abstract method gets it from the parser; a synthesized one must build it, or the bound
  // panics with `maybe_return_type: None`).
  let maybe_return_type_st = value_position_type_st(compiler, &sig.ret, range, &generic_runes);

  let template_type = TemplateTemplataType {
    param_types: scout_arena.alloc_slice_from_vec::<ITemplataType<'s>>(Vec::new()),
    return_type: scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {})),
  };

  Some(scout_arena.alloc(FunctionS::new(
    range,
    IFunctionDeclarationNameS::FunctionName(FunctionNameS {
      imprecise_name: scout_arena.intern_code_name(human_name),
      code_location: loc,
      lid: LocationInDenizen { path: &[] },
    }),
    // No attributes: an interface method's abstractness is its parent interface plus the virtual
    // receiver, not an attribute (`function_scout` rejects a redundant `abstract` here).
    scout_arena.alloc_slice_from_vec(Vec::new()),
    // Generic params must equal the interface's — empty here.
    scout_arena.alloc_slice_from_vec(Vec::new()),
    template_type,
    scout_arena.alloc_slice_from_vec(params),
    Some(ret_rune),
    maybe_return_type_st,
    scout_arena.alloc_slice_from_vec(effects),
    scout_arena.alloc_slice_from_vec(header_rules),
    &[],
    &[],
    scout_arena.alloc(IBodyS::AbstractBody(AbstractBodyS {})),
  )))
}

/// A Rust **trait** becomes an interface carrying its abstract methods, so a Vale struct can `impl`
/// it and Rust can call back in. Like `synthesize_extern_interface` (the enum analog) it is an
/// opaque `Extern` interface; unlike it, each trait method is projected into `internal_methods` as an
/// abstract method whose virtual receiver is the interface itself, so an `impl Callback for MyCb`
/// resolves its `on_call` through the ordinary override machinery. Non-generic only for now (Self is
/// filtered and generic trait methods are unsupported).
///
/// Also returns whether the anonymous-substruct macro can project the trait: every method must
/// project AND its (mapped) params + return must be expressible in value position.
pub fn synthesize_extern_trait<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  package_coord: &'s PackageCoordinate<'s>,
  human_name: StrI<'s>,
  methods: &[(StrI<'s>, FuncSignatureR<'s, 't>)],
) -> (&'s InterfaceS<'s>, bool)
where
  's: 't,
{
  let scout_arena = compiler.scout_arena;
  let interner = compiler.typing_interner;
  let loc = CodeLocationS::internal(scout_arena, SYNTHESIZED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  let tyype = TemplateTemplataType {
    param_types: scout_arena.alloc_slice_from_vec::<ITemplataType<'s>>(Vec::new()),
    return_type: scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {})),
  };

  let mut internal_methods: Vec<&'s FunctionS<'s>> = Vec::new();
  // Checked on the MAPPED sig — after Self becomes the interface citizen — because that is what the
  // method's params are actually built from (the raw sig's `self` is `&Generic(Self)`, which has no
  // value-position form and would spuriously mark the trait ineligible).
  let mut anon_eligible = true;
  for (method_name, sig) in methods {
    let mapped_params: Vec<TypeR<'s, 't>> = sig
      .params
      .iter()
      .map(|p| map_self_to_interface(interner, p, human_name, package_coord))
      .collect();
    let mapped_ret = map_self_to_interface(interner, &sig.ret, human_name, package_coord);
    let mapped_sig = FuncSignatureR {
      generic_param_names: &[],
      generic_param_bounds: &[],
      params: interner.alloc_slice_from_vec(mapped_params),
      ret: mapped_ret,
    };
    // A method that fails to project (an unsupported generic) is skipped; an override for it then
    // fails to resolve, surfacing the gap at the impl rather than as a silent hole here. A skipped or
    // non-value-position-representable method also makes the trait anon-ineligible, so the macro is
    // withheld and the hand-written path stands.
    match synthesize_abstract_interface_method(compiler, *method_name, &mapped_sig) {
      Some(m) => {
        internal_methods.push(m);
        if !sig_is_anon_representable(compiler, &mapped_sig) {
          anon_eligible = false;
        }
      }
      None => anon_eligible = false,
    }
  }

  let interface = scout_arena.alloc(InterfaceS::new(
    range,
    scout_arena.alloc(TopLevelInterfaceDeclarationNameS { name: human_name, range }),
    // NOT `Sealed`, unlike the enum analog (`synthesize_extern_interface`). Sealing would make the
    // anonymous-substruct macro bail (it early-returns on a sealed interface), and we want that macro
    // to fire so `Callback((x) => {…})` synthesizes a forwarder. De-sealing is safe: the only reader
    // of the sealed flag is the "an open interface can't have externally-defined abstract methods"
    // check, which is gated behind `!is_internal_method` — and a synthesized trait's abstract
    // methods are all `is_internal_method: true`, so the check never fires for them.
    scout_arena.alloc_slice_from_vec(vec![ICitizenAttributeS::Extern(ExternS { package_coord })]),
    &[], // no generic parameters
    SharednessP::Single,
    tyype,
    &[], // rules
    scout_arena.alloc_slice_from_vec(internal_methods),
    &[], // impl_bounds
    &[], // func_bounds
  ));
  (interface, anon_eligible)
}

/// A citizen's `LookupSR` path: its package coordinate's segments, then its short name.
///
/// The coordinate is `{ module, packages }`, so `mycrate.[inner]` yields `[mycrate, inner, Widget]`
/// — module first, exactly the order `GlobalEnvironmentT::find_package_store` matches against.
/// The two must stay in step; they are the two ends of the same handshake.
fn package_path<'s>(
  scout_arena: &ScoutArena<'s>,
  package: &'s PackageCoordinate<'s>,
  name: StrI<'s>,
) -> &'s [IImpreciseNameS<'s>] {
  let mut parts: Vec<IImpreciseNameS<'s>> = Vec::new();
  let mut push = |segment: StrI<'s>| {
    parts.push(
      scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name: segment })),
    );
  };
  push(package.module);
  for segment in package.packages.iter() {
    push(*segment);
  }
  push(name);
  scout_arena.alloc_slice_from_vec(parts)
}

/// The Vale keyword naming a primitive, for the one-segment `LookupSR` a primitive needs.
fn vale_type_name<'s, 't>(compiler: &Compiler<'s, '_, 't>, primitive: PrimitiveR) -> StrI<'s>
where
  's: 't,
{
  match primitive {
    PrimitiveR::Int32 => compiler.keywords.int,
    PrimitiveR::Int64 => compiler.keywords.i64,
    PrimitiveR::Bool => compiler.keywords.bool,
    PrimitiveR::Void => compiler.keywords.void,
    PrimitiveR::USize => compiler.keywords.usize,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::compile_options::GlobalOptions;
  use crate::keywords::Keywords;
  use crate::typing::oracles::Oracles;
  use crate::typing::TypingPassOptions;
  use bumpalo::Bump;
  use std::sync::Arc;

  fn test_opts() -> TypingPassOptions {
    TypingPassOptions {
      global_options: GlobalOptions {
        sanity_check: true,
        use_overload_index: true,
        use_optimized_solver: true,
        verbose_errors: true,
        debug_output: false,
        borrow_checker_enabled: true,
      },
      debug_out: Arc::new(|_: &str| {}),
      tree_shaking_enabled: true,
    }
  }

  // The reverse-direction abstract method mirrors Rust `&mut` into `mut(g)` on its synthesized
  // `FunctionS` — the faithful representation a future `mut`-parity check will read. White-box by
  // necessity: an abstract method's effects are behaviorally inert until then (nothing groupifies or
  // matches on them), so the synthesized `FunctionS` is the only observation seam. The two `&mut`
  // borrows (the `&mut self` receiver and a `&mut` parameter) each yield one `EffectS::Mut`; the
  // shared `&` parameter yields none — two effects, mirroring `synthesize_extern_function`.
  #[test]
  fn abstract_method_mirrors_mut_borrow_params_to_mut_effects() {
    let scout_bump = Bump::new();
    let typing_bump = Bump::new();
    let scout_arena = ScoutArena::new(&scout_bump);
    let typing_interner = TypingInterner::new(&typing_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let opts = test_opts();
    let compiler =
      Compiler::new(&scout_arena, &typing_interner, &keywords, &[], &opts, Oracles::none());

    // `on_tick(&mut self, w &mut _, input &_)` reduced to borrows of a primitive, so the sig needs no
    // imported package — only the `&mut` vs `&` distinction matters for this lock.
    let borrow = |is_mut| TypeR::Borrow {
      inner: typing_interner.alloc(TypeR::Primitive(PrimitiveR::Void)),
      is_mut,
    };
    let params =
      typing_interner.alloc_slice_from_vec(vec![borrow(true), borrow(true), borrow(false)]);
    let sig = FuncSignatureR {
      generic_param_names: &[],
      generic_param_bounds: &[],
      params,
      ret: TypeR::Primitive(PrimitiveR::Void),
    };

    let name = scout_arena.intern_str("on_tick");
    let f = synthesize_abstract_interface_method(&compiler, name, &sig).unwrap();

    let mut_effect_count = f.effects.iter().filter(|e| matches!(e, EffectS::Mut(_))).count();
    assert_eq!(mut_effect_count, 2, "expected two mut(g) effects, got {:?}", f.effects);
  }
}
