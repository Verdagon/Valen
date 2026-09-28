use crate::parsing::SharednessP;
use crate::parsing::IMacroInclusionP;
use crate::postparsing::names::*;
use crate::postparsing::rules::types::*;
use crate::postparsing::rules::rules::*;
use crate::utils::range::RangeS;
use crate::postparsing::ast::GenericParameterS;
use crate::interner::StrI;
use crate::typing::ast::ast::*;
use crate::postparsing::itemplatatype::*;
use crate::typing::compiler::Compiler;
use crate::scout_arena::ScoutArena;
use crate::typing::env::environment::{ImportedItemKind, ResolvedName};
use crate::typing::env::i_env_entry::{FunctionEnvEntry, IEnvEntryT, StructEnvEntry};
use crate::typing::rust_interop::bifrost::rust_method_entries::new_extern_function_name;
use crate::typing::names::names::*;
use crate::typing::compiler_error_reporter::CouldNotPostparseReason;
use crate::typing::rust_interop::bifrost::oracle::*;
use crate::typing::rust_interop::RustImportDeclarations;
use crate::typing::types::types::*;
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::utils::range::CodeLocationS;
use crate::postparsing::ast::*;
use crate::postparsing::rules::*;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::env::environment::{TemplatasStoreBuilder, TemplatasStoreT};
use crate::utils::code_hierarchy::FileCoordinateMap;
use crate::utils::fx::IndexMap;

pub const GENERATED_RANGE_OFFSET: i32 = -1;

// Loop over all Valen files' imports, follow them to whatever Rust thing it describes,
// and conjure up their IEnvEntry's and their StructS etc.
pub fn declare_rust_imports<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  file_to_program_s: &FileCoordinateMap<'s, ProgramS<'s>>,
) -> Result<RustImportDeclarations<'s, 't>, ICompileErrorT<'s, 't>>
where
  's: 't,
{
  // Everything (functions, structs, etc) gets an IEnvEntry
  let mut per_crate: IndexMap<
    &'s PackageCoordinate<'s>,
    Vec<(INameT<'s, 't>, IEnvEntryT<'s, 't>)>,
  > = IndexMap::default();
  // Structs also get their StructS early
  // TODO: perhaps we should lazily make these
  let mut structs: Vec<(&'t IdT<'s, 't>, &'s StructS<'s>)> = Vec::new();
  for program in file_to_program_s.file_coord_to_contents.values() {
    for import in program.imports {
      if !compiler.rust_crates.contains(&import.module_name) {
        // This is a Valen import, we don't care about those here
      } else {
        let oracle =
            compiler.oracles.rust
            .expect("missing Rust oracle");
        let name =
            match oracle.resolve_import(import) {
              Some(name) => name,
              None => {
                let mut path = format!("{}.", import.module_name.0);
                path.extend(import.package_names.iter().map(|s| format!("{}.", s.0)));
                path.push_str(import.importee_name.0);
                return Err(ICompileErrorT::UnresolvableRustImport {
                  range: compiler.typing_interner.alloc_slice_from_vec(vec![import.range]),
                  path,
                });
              }
            };
        let (local_name, entry, maybe_struct) = declare_rust_import(compiler, name);
        if let Some(struct_template_id_and_struct) = maybe_struct {
          structs.push(struct_template_id_and_struct);
        }
        per_crate.entry(name.package_coord).or_default().push((local_name, entry));
      }
    }
  }
  let mut namespaces: Vec<(&'t IdT<'s, 't>, &'t TemplatasStoreT<'s, 't>)> = Vec::new();
  for (coord, entries) in per_crate {
    let package_id = compiler.typing_interner.intern_id(IdValT {
      package_coord: coord,
      init_steps: &[],
      local_name: INameT::PackageTopLevel(
        compiler.typing_interner.intern_package_top_level_name(PackageTopLevelNameT {}),
      ),
    });
    let mut store = TemplatasStoreBuilder::new(package_id);
    store.add_entries(compiler.scout_arena, entries);
    namespaces.push((package_id, store.build_in(compiler.typing_interner)));
  }
  Ok(RustImportDeclarations {
    functions: Vec::new(),
    structs,
    interfaces: Vec::new(),
    impls: Vec::new(),
    namespaces,
  })
}

fn declare_rust_import<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  name: ResolvedName<'s>,
) -> (INameT<'s, 't>, IEnvEntryT<'s, 't>, Option<(&'t IdT<'s, 't>, &'s StructS<'s>)>)
where
  's: 't,
{
  let interner = compiler.typing_interner;
  let oracle = compiler.oracles.rust.expect("declare_rust_import called without a rust oracle");
  let item = oracle
    .resolve(None, &name)
    .unwrap_or_else(|| unimplemented!());
  let package_coord = name.package_coord;
  let package_id = interner.intern_id(IdValT {
    package_coord,
    init_steps: &[],
    local_name: INameT::PackageTopLevel(
      interner.intern_package_top_level_name(PackageTopLevelNameT {}),
    ),
  });
  let human_name = name.importee_name;

  match name.kind {
    ImportedItemKind::Type => {
      let template_name = interner.intern_struct_template_name(StructTemplateNameT { human_name });
      let struct_local_name = INameT::StructTemplate(template_name);
      let struct_s = translate_extern_struct(
        compiler,
        package_coord,
        human_name,
        oracle.type_generic_params(item, interner),
      );
      let struct_template_id = package_id.add_step(interner, struct_local_name);
      let env_entry =
          IEnvEntryT::Struct(StructEnvEntry {
            template_id: struct_template_id,
            tyype: struct_s.tyype,
          });
      (struct_local_name, env_entry, Some((struct_template_id, struct_s)))
    }
    ImportedItemKind::Function => {
      let function_local_name = new_extern_function_name(compiler, human_name);
      let function_template_id = package_id.add_step(interner, function_local_name);
      let env_entry =
          IEnvEntryT::Function(FunctionEnvEntry { template_id: function_template_id });
      (function_local_name, env_entry, None)
    }
    ImportedItemKind::Enum | ImportedItemKind::Trait => unimplemented!(),
  }
}

pub fn resolved_name_of<'s, 't>(
  package_coord: &'s PackageCoordinate<'s>,
  local_name: INameT<'s, 't>,
) -> ResolvedName<'s>
where
  's: 't,
{
  let (importee_name, kind) = match local_name {
    INameT::StructTemplate(t) => (t.human_name, ImportedItemKind::Type),
    INameT::InterfaceTemplate(_) => unimplemented!(),
    INameT::FunctionTemplate(t) => (t.human_name, ImportedItemKind::Function),
    _ => unimplemented!(),
  };
  ResolvedName { package_coord, importee_name, kind }
}

pub fn create_postparsed_function<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  template_id: &'t IdT<'s, 't>,
) -> Option<Result<&'s FunctionS<'s>, CouldNotPostparseReason>>
where
  's: 't,
{
  if !compiler.in_rust_crate(template_id) {
    return None;
  }
  let oracle = compiler.oracles.rust.unwrap(); // error unimplemented
  let interner = compiler.typing_interner;
  let function_template_name =
      match template_id.local_name {
        INameT::FunctionTemplate(r) => r,
        _ => unimplemented!(),
      };
  let human_name = function_template_name.human_name;

  // TODO: pull this out into a common helper, I feel like this will be pretty common
  let mut current_step_item: Option<RustItemId> = None;
  for step in template_id.steps() {
    let step_name = resolved_name_of(template_id.package_coord, step);
    current_step_item = match oracle.resolve(current_step_item, &step_name) { Some(v) => Some(v), None => unimplemented!() };
  }
  let function_item = match current_step_item { Some(v) => v, None => unimplemented!() };

  let sig = oracle.fn_sig(function_item, interner).unwrap();
  let package_coord = template_id.package_coord;

  let scout_arena = compiler.scout_arena;
  let loc = CodeLocationS::internal(scout_arena, GENERATED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  for name in sig.generic_param_names.iter() {
    let _ = name;
    unimplemented!()
  }

  let mut header_rules: Vec<IRulexSR<'s>> = Vec::new();

  let mut params: Vec<ParameterS<'s>> = Vec::new();
  let mut effects: Vec<EffectS<'s>> = Vec::new();
  let mut param_group_runes: Vec<RuneUsage<'s>> = Vec::new();
  let mut lidb = LocationInDenizenBuilder::new(Vec::new());
  for (index, param_type) in sig.params.iter().enumerate() {
    let own_rune = RuneUsage {
      range,
      rune: scout_arena
          .intern_rune(IRuneValS::ArgumentRune(ArgumentRuneS { arg_index: index as i32 })),
    };
    let mut value_type_rules: Vec<IRulexSR<'s>> = Vec::new();
    let (full_type_rune, value_type_rune, outer_ref_rules, tyype): (_, _, Vec<IRulexSR<'s>>, _) =
        match param_type {
          TypeR::Borrow { inner, is_mut } => {
            let full_type_rune = new_rune(scout_arena, range, &mut lidb);
            let value_rune = add_rules_for_type(
              compiler,
              inner,
              own_rune,
              range,
              &mut value_type_rules,
              &mut lidb,
            );
            let group_rune = RuneUsage {
              range,
              rune: scout_arena.intern_rune(IRuneValS::ImplicitRegionRune(ImplicitRegionRuneValS {
                original_rune: value_rune.rune,
              })),
            };
            param_group_runes.push(group_rune);
            let group = scout_arena.alloc(GroupS::Rune(scout_arena.alloc(group_rune)));
            if *is_mut {
              effects.push(EffectS::Mut(group));
            }
            let outer = vec![IRulexSR::BorrowRef(BorrowRefSR {
              range,
              result_rune: full_type_rune,
              inner_rune: value_rune,
              region: RegionSR::Group(
                scout_arena.alloc(
                  GroupS::Rune(
                    scout_arena.alloc(
                      RuneUsage {
                        range: range.clone(),
                        rune: scout_arena.intern_rune(
                          IRuneValS::ImplicitGroupRune(
                            ImplicitGroupRuneS{ range: range.clone() })),
                      }))))
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
            let rune = add_rules_for_type(
              compiler,
              param_type,
              own_rune,
              range,
              &mut value_type_rules,
              &mut lidb,
            );
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
  let ret_rune = add_rules_for_type(
    compiler,
    &sig.ret,
    ret_own_rune,
    range,
    &mut header_rules,
    &mut lidb,
  );

  let maybe_return_type =
      match &sig.ret {
        TypeR::Borrow { inner, .. } => {
          // For now, only support returning a ref into the only parameter
          let param_group_rune =
              match param_group_runes.as_slice() {
                [only] => *only,
                _ => unimplemented!(),
              };
          let ellipsis_base =
              scout_arena.alloc(GroupS::Rune(scout_arena.alloc(param_group_rune)));
          Some(ITypeST::BorrowRef(scout_arena.alloc(BorrowRefST {
            range,
            inner: scout_arena.alloc(translate_type(compiler, inner, range)),
            region: RegionS::Group(scout_arena.alloc(GroupS::Ellipsis { base: ellipsis_base })),
          })))
        }
        _ => Some(translate_type(compiler, &sig.ret, range)),
      };

  for bound in sig.generic_param_bounds.iter() {
    let _ = bound;
    unimplemented!()
  }

  let tyype = TemplateTemplataType {
    param_types: &[],
    return_type: scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {})),
  };

  Some(Ok(scout_arena.alloc(FunctionS::new(
    range,
    IFunctionDeclarationNameS::FunctionName(FunctionNameS {
      imprecise_name: scout_arena.intern_code_name(human_name),
      code_location: loc,
      lid: LocationInDenizen { path: &[] },
    }),
    scout_arena.alloc_slice_from_vec(vec![IFunctionAttributeS::Extern(ExternS { package_coord })]),
    &[],
    tyype,
    scout_arena.alloc_slice_from_vec(params),
    Some(ret_rune),
    maybe_return_type,
    scout_arena.alloc_slice_from_vec(effects),
    scout_arena.alloc_slice_from_vec(header_rules),
    &[],
    &[],
    scout_arena.alloc(IBodyS::ExternBody(ExternBodyS {})),
  ))))
}

pub fn translate_extern_struct<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  package_coord: &'s PackageCoordinate<'s>,
  human_name: StrI<'s>,
  generic_param_names: &[StrI<'s>],
) -> &'s StructS<'s>
where
    's: 't,
{
  let scout_arena = compiler.scout_arena;

  let loc = CodeLocationS::internal(scout_arena, GENERATED_RANGE_OFFSET);
  let range = RangeS::new(loc, loc);

  let mut generic_params: Vec<&'s GenericParameterS<'s>> = Vec::new();
  for name in generic_param_names.iter() {
    let rune = scout_arena.intern_rune(IRuneValS::CodeRune(CodeRuneS { name: *name }));
    generic_params.push(scout_arena.alloc(GenericParameterS {
      range,
      rune: RuneUsage { range, rune },
      tyype: IGenericParameterTypeS::KindGenericParameterType(KindGenericParameterTypeS {}),
      default: None,
    }));
  }

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
      ICitizenAttributeS::Extern(ExternS { package_coord }),
      dont_call(compiler.keywords.derive_struct_constructor),
      dont_call(compiler.keywords.derive_struct_drop),
    ]),
    scout_arena.alloc_slice_from_vec(generic_params),
    SharednessP::Single,
    tyype,
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
  ))
}

fn add_rules_for_type<'s, 't>(
  compiler: &Compiler<'s, '_, 't>,
  tyype: &TypeR<'s, 't>,
  own_rune: RuneUsage<'s>,
  range: RangeS<'s>,
  rules: &mut Vec<IRulexSR<'s>>,
  lidb: &mut LocationInDenizenBuilder,
) -> RuneUsage<'s>
where
    's: 't,
{
  let scout_arena = compiler.scout_arena;
  match tyype {
    TypeR::Generic(_index) => unimplemented!(),
    TypeR::Primitive(primitive) => {
      let name = translate_primitive(compiler, *primitive);
      rules.push(IRulexSR::Lookup(LookupSR {
        range,
        rune: own_rune,
        parts: scout_arena.alloc_slice_copy(&[
          scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name }))
        ]),
      }));
      own_rune
    }
    TypeR::Citizen { name, package, args } => {
      let template_rune = new_rune(scout_arena, range, lidb);
      rules.push(IRulexSR::Lookup(LookupSR {
        range,
        rune: template_rune,
        parts: {
          // TODO: all this goes away when we move to ITypeST
          let mut segments: Vec<StrI<'s>> = vec![package.module];
          for segment in package.packages.iter() {
            segments.push(*segment);
          }
          segments.push(*name);
          let mut parts: Vec<IImpreciseNameS<'s>> = Vec::new();
          for segment in segments {
            parts.push(
              scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name: segment })),
            );
          }
          scout_arena.alloc_slice_from_vec(parts)
        },
      }));

      let mut arg_runes: Vec<RuneUsage<'s>> = Vec::new();
      for arg in args.iter() {
        let _ = arg;
        unimplemented!()
      }

      rules.push(IRulexSR::Call(CallSR {
        range,
        result_rune: own_rune,
        template_rune,
        args: scout_arena.alloc_slice_from_vec(arg_runes),
      }));
      own_rune
    }
    TypeR::Borrow { inner, .. } => {
      // We don't yet support named lifetimes yet, we assume all Rust param lifetimes are anonymous.
      // We make one here.
      // TODO: i think the right way soon would be to read rust's own implicit anon lifetime
      let anon_group_rune = new_rune(scout_arena, range, lidb);
      let inner_type_rune = add_rules_for_type(compiler, inner, anon_group_rune, range, rules, lidb);
      rules.push(IRulexSR::BorrowRef(BorrowRefSR {
        range,
        result_rune: own_rune,
        inner_rune: inner_type_rune,
        region: RegionSR::Group(scout_arena.alloc(GroupS::Rune(scout_arena.alloc(RuneUsage {
          range,
          rune: scout_arena.intern_rune(IRuneValS::ImplicitGroupRune(ImplicitGroupRuneS { range })),
        })))),
      }));
      own_rune
    }
  }
}

fn new_rune<'s>(
  scout_arena: &ScoutArena<'s>,
  range: RangeS<'s>,
  lidb: &mut LocationInDenizenBuilder,
) -> RuneUsage<'s> {
  RuneUsage {
    range,
    rune: scout_arena.intern_rune(IRuneValS::ImplicitRune(ImplicitRuneValS::new(lidb.child().borrow_val()))),
  }
}

fn translate_type<'s, 't>(
  compiler: &Compiler<'s, '_, 't>,
  tyype: &TypeR<'s, 't>,
  range: RangeS<'s>,
) -> ITypeST<'s>
where
    's: 't,
{
  let scout_arena = compiler.scout_arena;
  match tyype {
    TypeR::Generic(_index) => unimplemented!(),
    TypeR::Primitive(primitive) => {
      let name = translate_primitive(compiler, *primitive);
      assemble_call(scout_arena, range, name, Vec::new())
    }
    TypeR::Borrow { .. } => unimplemented!(),
    TypeR::Citizen { name, args, .. } => {
      let mut arg_sts: Vec<ITypeST<'s>> = Vec::new();
      for arg in args.iter() {
        arg_sts.push(translate_type(compiler, arg, range));
      }
      assemble_call(scout_arena, range, *name, arg_sts)
    }
  }
}

fn translate_primitive<'s, 't>(compiler: &Compiler<'s, '_, 't>, primitive: PrimitiveR) -> StrI<'s>
where
    's: 't,
{
  match primitive {
    PrimitiveR::Int32 => compiler.keywords.int,
    PrimitiveR::Int64 => compiler.keywords.i64,
    PrimitiveR::Bool => unimplemented!(),
    PrimitiveR::Void => compiler.keywords.void,
    PrimitiveR::USize => unimplemented!(),
  }
}

fn assemble_call<'s>(
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
