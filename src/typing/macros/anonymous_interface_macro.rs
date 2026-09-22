use crate::typing::compiler_outputs::CompilerOutputs;
use crate::postparsing::ast::*;
use crate::postparsing::expressions::{BlockSE, BodySE, DotSE, FunctionCallSE, IExpressionSE, IVariableUseCertainty, LocalLoadSE, LocalS, OwnershippedSE, UnletSE};
use crate::postparsing::itemplatatype::{FunctionTemplataType, ITemplataType, KindTemplataType, PackTemplataType, PrototypeTemplataType, TemplateTemplataType};
use crate::postparsing::names::{
    AnonymousSubstructDropBoundParamsListRuneS,
    AnonymousSubstructDropBoundPrototypeRuneS,
    AnonymousSubstructFunctionBoundParamsListRuneS,
    AnonymousSubstructFunctionBoundPrototypeRuneS,
    AnonymousSubstructFunctionInterfaceKindRuneS,
    AnonymousSubstructFunctionInterfaceTemplateRuneS,
    AnonymousSubstructImplDeclarationNameS,
    AnonymousSubstructKindRuneS,
    AnonymousSubstructMemberRuneS,
    AnonymousSubstructMethodInheritedRuneValS,
    AnonymousSubstructMethodSelfBorrowKindRuneS,
    AnonymousSubstructParentInterfaceKindRuneS,
    AnonymousSubstructParentInterfaceTemplateRuneS,
    AnonymousSubstructTemplateImpreciseNameValS,
    AnonymousSubstructTemplateNameS,
    AnonymousSubstructTemplateRuneS,
    AnonymousSubstructVoidKindRuneS,
    CodeNameValS,
    CodeVarNameS,
    DesugaredParamNameDeclarationS,
    ForwarderFunctionDeclarationNameS,
    FunctionNameS,
    ForwarderFunctionImpreciseNameValS,
    IFunctionDeclarationNameS,
    IFunctionImpreciseNameS,
    IFunctionImpreciseNameValS,
    IImplDeclarationNameS,
    IImpreciseNameValS,
    INameS,
    IRuneS,
    IRuneValS,
    IStructDeclarationNameS,
    IVarDeclarationNameS,
    SelfFullTypeRuneS,
    SelfKindRuneS,
    SelfKindTemplateRuneS,
};
use crate::postparsing::patterns::patterns::{AtomSP, CaptureS};
use crate::postparsing::rules::rules::{BorrowRefSR, CallSR, CallSiteFuncSR, DefinitionFuncSR, EqualsSR, IRulexSR, KindListSR, LiteralSR, LookupSR, OwnRefSR, RegionSR, ResolveSR, RuneParentEnvLookupSR, RuneUsage, WeakRefSR};
use crate::parsing::ast::ast::LoadAsP;
use crate::postparsing::rules::templex_scout::map_runes_in_type_st;
use crate::postparsing::rules::types::{BorrowRefST, CallST, ITypeST, NameST, RegionS, RuneUsageST};
use crate::typing::compiler::Compiler;
use crate::typing::macros::macros::GeneratedAhtDenizen;
use crate::typing::names::names::*;
use crate::utils::arena_index_map::ArenaIndexMap;
use crate::utils::range::RangeS;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't>
where 's: 't,
{
    pub fn get_interface_sibling_entries_anonymous_interface(
        &self,
        interface_name: IdT<'s, 't>,
        interface_a: &'s InterfaceS<'s>,
    ) -> Vec<GeneratedAhtDenizen<'s, 't>> {

        if interface_a.attributes.iter().any(|a| matches!(a, ICitizenAttributeS::Sealed(_))) {
            return vec![];
        }

        let member_runes: Vec<RuneUsage<'s>> =
            interface_a.internal_methods.iter().enumerate().map(|(_index, method)| {
                let rune = self.scout_arena.intern_rune(
                    IRuneValS::AnonymousSubstructMemberRune(AnonymousSubstructMemberRuneS {
                        interface: *interface_a.name,
                        method: method.name,
                    }));
                RuneUsage { range: RangeS::new(method.range.begin, method.range.begin), rune }
            }).collect();
        let members: Vec<NormalStructMemberS<'s>> =
            interface_a.internal_methods.iter().zip(member_runes.iter()).enumerate().map(|(index, (method, rune))| {
                NormalStructMemberS {
                    range: method.range,
                    name: self.scout_arena.intern_str(&index.to_string()),
                    lid: LocationInDenizen { path: &[] },
                    type_rune: *rune,
                    tyype: ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: *rune })),
                    value_type_rune: *rune,
                    type_outer_ref_rules: &[],
                    value_type_rules: &[],
                }
            }).collect();

        let struct_name_s = AnonymousSubstructTemplateNameS { interface_name: *interface_a.name };
        let struct_name_s_ref = self.scout_arena.alloc(struct_name_s);
        let struct_local_name = self.translate_name_step(INameS::AnonymousSubstructTemplateName(struct_name_s_ref));
        let struct_name_t_steps = interface_name.init_steps.to_vec();
        let struct_name_t_ref = self.typing_interner.intern_id(IdValT {
            package_coord: interface_name.package_coord,
            init_steps: &struct_name_t_steps,
            local_name: struct_local_name,
        });
        let struct_name_t = *struct_name_t_ref;

        let struct_a = self.make_struct_anonymous_interface(
            interface_a,
            &member_runes,
            &members,
            struct_name_s,
        );

        let mut generated_aht_denizens: Vec<GeneratedAhtDenizen<'s, 't>> =
            self.get_struct_sibling_entries_struct_constructor(struct_name_t, struct_a);
        generated_aht_denizens.extend(self.get_struct_sibling_entries_struct_drop(struct_name_t, struct_a));

        for (method_index, (method, _rune)) in interface_a.internal_methods.iter().zip(member_runes.iter()).enumerate() {
            let local_name: INameT<'s, 't> = self.translate_generic_function_name(method.name).into();
            let name_ref = self.typing_interner.intern_id(IdValT {
                package_coord: struct_name_t.package_coord,
                init_steps: struct_name_t.init_steps,
                local_name,
            });
            let forwarder = self.make_forwarder_function_anonymous_interface(
                struct_name_s, interface_a, struct_a, *method, method_index as i32);
            generated_aht_denizens.push(GeneratedAhtDenizen::Function(name_ref, forwarder));
        }

        let anon_template_rune = self.scout_arena.intern_rune(
            IRuneValS::AnonymousSubstructTemplateRune(AnonymousSubstructTemplateRuneS {})
        );
        let anon_kind_rune = self.scout_arena.intern_rune(
            IRuneValS::AnonymousSubstructKindRune(AnonymousSubstructKindRuneS {})
        );
        let parent_interface_template_rune = self.scout_arena.intern_rune(
            IRuneValS::AnonymousSubstructParentInterfaceTemplateRune(AnonymousSubstructParentInterfaceTemplateRuneS {})
        );
        let parent_interface_kind_rune = self.scout_arena.intern_rune(
            IRuneValS::AnonymousSubstructParentInterfaceKindRune(AnonymousSubstructParentInterfaceKindRuneS {})
        );

        let struct_imprecise_name = struct_a.name.get_imprecise_name(self.scout_arena);
        let interface_imprecise_name = interface_a.name.get_imprecise_name(self.scout_arena);

        let rules: Vec<IRulexSR<'s>> = vec![
            IRulexSR::Lookup(LookupSR {
                range: struct_a.range,
                rune: RuneUsage { range: struct_a.range, rune: anon_template_rune },
                parts: self.scout_arena.alloc_slice_copy(&[struct_imprecise_name]),
            }),
            IRulexSR::Call(CallSR {
                range: struct_a.range,
                result_rune: RuneUsage { range: struct_a.range, rune: anon_kind_rune },
                template_rune: RuneUsage { range: struct_a.range, rune: anon_template_rune },
                args: self.scout_arena.alloc_slice_from_vec(
                    struct_a.generic_params.iter().map(|gp| gp.rune).collect()
                ),
            }),
            IRulexSR::Lookup(LookupSR {
                range: interface_a.range,
                rune: RuneUsage { range: interface_a.range, rune: parent_interface_template_rune },
                parts: self.scout_arena.alloc_slice_copy(&[interface_imprecise_name]),
            }),
            IRulexSR::Call(CallSR {
                range: interface_a.range,
                result_rune: RuneUsage { range: interface_a.range, rune: parent_interface_kind_rune },
                template_rune: RuneUsage { range: interface_a.range, rune: parent_interface_template_rune },
                args: self.scout_arena.alloc_slice_from_vec(
                    interface_a.generic_params.iter().map(|gp| gp.rune).collect()
                ),
            }),
        ];

        let struct_kind_rune_s = RuneUsage { range: interface_a.range, rune: anon_kind_rune };
        let interface_kind_rune_s = RuneUsage { range: interface_a.range, rune: parent_interface_kind_rune };

        let impl_name_s = IImplDeclarationNameS::AnonymousSubstructImplDeclarationName(
            AnonymousSubstructImplDeclarationNameS { interface: *interface_a.name }
        );

        let impl_param_types_vec: Vec<ITemplataType<'s>> = struct_a.generic_params
            .iter()
            .map(|gp| gp.tyype.tyype())
            .collect();
        let impl_tyype = ITemplataType::TemplateTemplataType(TemplateTemplataType {
            param_types: self.scout_arena.alloc_slice_copy(&impl_param_types_vec),
            return_type: self.scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {})),
        });

        let rules_slice = self.scout_arena.alloc_slice_from_vec(rules);
        let impl_a = self.scout_arena.alloc(ImplS::new(
            interface_a.range,
            impl_name_s,
            struct_a.generic_params,
            rules_slice,
            impl_tyype,
            struct_kind_rune_s,
            struct_imprecise_name,
            ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: struct_kind_rune_s })),
            interface_kind_rune_s,
            interface_imprecise_name,
            ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: interface_kind_rune_s })),
            &[],
        ));

        let impl_template_name = self.translate_impl_name(
            impl_a.name, impl_a.sub_citizen_imprecise_name, impl_a.super_interface_imprecise_name);
        let impl_local_name: INameT<'s, 't> = INameT::from(impl_template_name);
        let impl_name_t_steps = struct_name_t.init_steps.to_vec();
        let impl_name_t_ref = self.typing_interner.intern_id(IdValT {
            package_coord: struct_name_t.package_coord,
            init_steps: &impl_name_t_steps,
            local_name: impl_local_name,
        });
        let impl_name_t = *impl_name_t_ref;

        generated_aht_denizens.push(GeneratedAhtDenizen::Struct(struct_name_t_ref, struct_a));
        generated_aht_denizens.push(GeneratedAhtDenizen::Impl(impl_name_t_ref, impl_a));
        generated_aht_denizens
    }

    pub fn map_runes_anonymous_interface(
        &self,
        rule: IRulexSR<'s>,
        func: impl Fn(IRuneS<'s>) -> IRuneS<'s>,
    ) -> IRulexSR<'s> {
        match rule {
            IRulexSR::Lookup(x) => IRulexSR::Lookup(LookupSR {
                range: x.range,
                rune: RuneUsage { range: x.rune.range, rune: func(x.rune.rune) },
                parts: x.parts,
            }),
            // IRulexSR::MaybeCoercingLookup(_) => {
                // panic!("implement: map_runes_anonymous_interface MaybeCoercingLookup");
                // LookupSR(range, RuneUsage(a, func(rune)), name)
            // }
            IRulexSR::RuneParentEnvLookup(x) => IRulexSR::RuneParentEnvLookup(RuneParentEnvLookupSR {
                range: x.range,
                rune: RuneUsage { range: x.rune.range, rune: func(x.rune.rune) },
            }),
            IRulexSR::Equals(x) => IRulexSR::Equals(EqualsSR {
                range: x.range,
                left: RuneUsage { range: x.left.range, rune: func(x.left.rune) },
                right: RuneUsage { range: x.right.range, rune: func(x.right.rune) },
            }),
            // IRulexSR::DefinitionCoordIsa(_) => {
                // panic!("implement: map_runes_anonymous_interface DefinitionCoordIsa");
                // DefinitionCoordIsaSR(range, RuneUsage(z, func(result)), RuneUsage(a, func(sub)), RuneUsage(b, func(suuper)))
            // }
            // IRulexSR::CallSiteCoordIsa(_) => {
                // panic!("implement: map_runes_anonymous_interface CallSiteCoordIsa");
                // CallSiteCoordIsaSR(range, maybeResult.map(r => RuneUsage(r.rune.range, func(r.rune))), RuneUsage(a, func(sub)), RuneUsage(b, func(suuper)))
            // }
            // IRulexSR::KindComponents(_) => {
                // panic!("implement: map_runes_anonymous_interface KindComponents");
                // KindComponentsSR(range, RuneUsage(a, func(resultRune)), RuneUsage(b, func(mutabilityRune)))
            // }
            // IRulexSR::CoordComponents(_) => {
                // panic!("implement: map_runes_anonymous_interface CoordComponents");
                // CoordComponentsSR(range, RuneUsage(a, func(resultRune)), RuneUsage(b, func(ownershipRune)), RuneUsage(c, func(kindRune)))
            // }
            // IRulexSR::PrototypeComponents(_) => {
                // panic!("implement: map_runes_anonymous_interface PrototypeComponents");
                // PrototypeComponentsSR(range, RuneUsage(a, func(resultRune)), RuneUsage(b, func(paramsRune)), RuneUsage(c, func(returnRune)))
            // }
            IRulexSR::Resolve(x) => {
                let mut mentioned: Vec<IRuneS<'s>> = Vec::new();
                for pt in x.params_types.iter() {
                    pt.collect_rune_mentions(&mut mentioned);
                }
                x.return_type.collect_rune_mentions(&mut mentioned);
                for rune in &mentioned {
                    assert!(func(*rune).ptr_eq(rune));
                }
                IRulexSR::Resolve(ResolveSR {
                    range: x.range,
                    result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                    name: x.name,
                    params_list_rune: RuneUsage { range: x.params_list_rune.range, rune: func(x.params_list_rune.rune) },
                    params_types: x.params_types,
                    return_rune: RuneUsage { range: x.return_rune.range, rune: func(x.return_rune.rune) },
                    return_type: x.return_type,
                })
            }
            IRulexSR::CallSiteFunc(x) => IRulexSR::CallSiteFunc(CallSiteFuncSR {
                range: x.range,
                prototype_rune: RuneUsage { range: x.prototype_rune.range, rune: func(x.prototype_rune.rune) },
                name: x.name,
                params_list_rune: RuneUsage { range: x.params_list_rune.range, rune: func(x.params_list_rune.rune) },
                return_rune: RuneUsage { range: x.return_rune.range, rune: func(x.return_rune.rune) },
            }),
            IRulexSR::DefinitionFunc(x) => IRulexSR::DefinitionFunc(DefinitionFuncSR {
                range: x.range,
                result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                name: x.name,
                params_list_rune: RuneUsage { range: x.params_list_rune.range, rune: func(x.params_list_rune.rune) },
                return_rune: RuneUsage { range: x.return_rune.range, rune: func(x.return_rune.rune) },
            }),
            // IRulexSR::OneOf(_) => {
                // panic!("implement: map_runes_anonymous_interface OneOf");
                // OneOfSR(range, RuneUsage(a, func(rune)), literals)
            // }
            // IRulexSR::IsConcrete(_) => {
                // panic!("implement: map_runes_anonymous_interface IsConcrete");
                // IsConcreteSR(range, RuneUsage(a, func(rune)))
            // }
            // IRulexSR::IsInterface(_) => {
                // panic!("implement: map_runes_anonymous_interface IsInterface");
                // IsInterfaceSR(range, RuneUsage(a, func(rune)))
            // }
            // IRulexSR::IsStruct(_) => {
                // panic!("implement: map_runes_anonymous_interface IsStruct");
                // IsStructSR(range, RuneUsage(a, func(rune)))
            // }
            IRulexSR::Literal(x) => IRulexSR::Literal(LiteralSR {
                range: x.range,
                rune: RuneUsage { range: x.rune.range, rune: func(x.rune.rune) },
                literal: x.literal,
            }),
            // IRulexSR::Augment(x) => {
                // IRulexSR::Augment(AugmentSR {
                    // range: x.range,
                    // result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                    // ownership: x.ownership,
                    // inner_rune: RuneUsage { range: x.inner_rune.range, rune: func(x.inner_rune.rune) },
                // })
            // }
            // IRulexSR::MaybeCoercingCall(_) => {
                // panic!("implement: map_runes_anonymous_interface MaybeCoercingCall");
                // MaybeCoercingCallSR(range, RuneUsage(a, func(resultRune)), RuneUsage(b, func(templateRune)), args.map({ case RuneUsage(c, rune) => RuneUsage(c, func(rune)) }))
            // }
            IRulexSR::Call(x) => {
                let new_args: Vec<RuneUsage<'s>> = x.args.iter()
                    .map(|ru| RuneUsage { range: ru.range, rune: func(ru.rune) })
                    .collect();
                IRulexSR::Call(CallSR {
                    range: x.range,
                    result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                    template_rune: RuneUsage { range: x.template_rune.range, rune: func(x.template_rune.rune) },
                    args: self.scout_arena.alloc_slice_from_vec(new_args),
                })
            }
            // IRulexSR::Pack(_) => {
                // panic!("implement: map_runes_anonymous_interface Pack");
                // KindListSR(range, RuneUsage(a, resultRune), members.map({ case RuneUsage(c, rune) => RuneUsage(c, func(rune)) }))
            // }
            // IRulexSR::RefListCompoundMutability(_) => {
                // panic!("implement: map_runes_anonymous_interface RefListCompoundMutability");
                // RefListCompoundMutabilitySR(range, RuneUsage(a, func(resultRune)), RuneUsage(b, func(coordListRune)))
            // }
            IRulexSR::BorrowRef(x) => IRulexSR::BorrowRef(BorrowRefSR {
                range: x.range,
                result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                inner_rune: RuneUsage { range: x.inner_rune.range, rune: func(x.inner_rune.rune) },
                region: match x.region {
                    RegionSR::Rune(r) => RegionSR::Rune(RuneUsage { range: r.range, rune: func(r.rune) }),
                    other_region => other_region,
                },
            }),
            IRulexSR::WeakRef(x) => IRulexSR::WeakRef(WeakRefSR {
                range: x.range,
                result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                inner_rune: RuneUsage { range: x.inner_rune.range, rune: func(x.inner_rune.rune) },
            }),
            IRulexSR::OwnRef(x) => IRulexSR::OwnRef(OwnRefSR {
                range: x.range,
                result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                inner_rune: RuneUsage { range: x.inner_rune.range, rune: func(x.inner_rune.rune) },
            }),
            IRulexSR::KindList(x) => {
                let new_members: Vec<RuneUsage<'s>> = x.members.iter()
                    .map(|ru| RuneUsage { range: ru.range, rune: func(ru.rune) })
                    .collect();
                IRulexSR::KindList(KindListSR {
                    range: x.range,
                    result_rune: RuneUsage { range: x.result_rune.range, rune: func(x.result_rune.rune) },
                    members: self.scout_arena.alloc_slice_from_vec(new_members),
                })
            }
        }
    }

    pub fn inherited_method_rune_anonymous_interface(
        &self,
        interface_a: &'s InterfaceS<'s>,
        method: &'s FunctionS<'s>,
        rune: IRuneS<'s>,
    ) -> IRuneS<'s> {
        self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructMethodInheritedRune(
            AnonymousSubstructMethodInheritedRuneValS {
                interface: *interface_a.name,
                method: method.name,
                inner: rune,
            }))
    }

    pub fn make_struct_anonymous_interface(
        &self,
        interface_a: &'s InterfaceS<'s>,
        member_runes: &[RuneUsage<'s>],
        members: &[NormalStructMemberS<'s>],
        struct_template_name_s: AnonymousSubstructTemplateNameS<'s>,
    ) -> &'s StructS<'s> {

        let range = |n: i32| RangeS::internal(self.scout_arena, n);
        let use_rune = |n: i32, rune: IRuneS<'s>| RuneUsage { range: range(n), rune };

        let mut rules_builder: Vec<IRulexSR<'s>> = Vec::new();

        for rule in interface_a.rules.iter() {
            rules_builder.push(*rule);
        }

        let void_kind_rune = self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructVoidKindRune(AnonymousSubstructVoidKindRuneS {}));
        let void_imprecise_name = self.scout_arena.intern_imprecise_name(IImpreciseNameValS::CodeName(CodeNameValS { name: self.keywords.void }));
        rules_builder.push(IRulexSR::Lookup(LookupSR {
            range: range(-1672147),
            rune: use_rune(-64002, void_kind_rune),
            parts: self.scout_arena.alloc_slice_copy(&[void_imprecise_name]),
        }));

        let void_kind_rune = self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructVoidKindRune(AnonymousSubstructVoidKindRuneS {}));

        let mut struct_generic_params: Vec<&'s GenericParameterS<'s>> = Vec::new();
        for gp in interface_a.generic_params.iter() {
            struct_generic_params.push(*gp);
        }
        for mr in member_runes.iter() {
            let gp = self.scout_arena.alloc(GenericParameterS {
                range: mr.range,
                rune: *mr,
                tyype: IGenericParameterTypeS::KindGenericParameterType(KindGenericParameterTypeS {}),
                default: None,
            });
            struct_generic_params.push(gp);
        }

        let iface_gp_runes: Vec<IRuneS<'s>> =
            interface_a.generic_params.iter().map(|gp| gp.rune.rune).collect();

        let mut struct_func_bounds: Vec<(RuneUsage<'s>, FunctionS<'s>)> = Vec::new();

        for ((internal_method, member_rune), _method_index) in
            interface_a.internal_methods.iter().zip(member_runes.iter()).zip(0i32..) {
            let internal_method = *internal_method;
            let inherit = |method_rune: IRuneS<'s>| -> IRuneS<'s> {
                if iface_gp_runes.iter().any(|r| r.ptr_eq(&method_rune)) {
                    method_rune
                } else {
                    self.inherited_method_rune_anonymous_interface(interface_a, internal_method, method_rune)
                }
            };
            for rule in internal_method.header_rules.iter() {
                let mapped = self.map_runes_anonymous_interface(*rule, &inherit);
                rules_builder.push(mapped);
            }

            let original_ret_rune = internal_method.maybe_ret_kind_rune.unwrap();
            let return_rune = RuneUsage {
                range: original_ret_rune.range,
                rune: inherit(original_ret_rune.rune),
            };

            {
                let self_borrow_kind_rune_s = self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructMethodSelfBorrowKindRune(
                    AnonymousSubstructMethodSelfBorrowKindRuneS {
                        interface: *interface_a.name,
                        method: internal_method.name,
                    }));
                rules_builder.push(IRulexSR::BorrowRef(BorrowRefSR {
                    range: internal_method.range,
                    result_rune: RuneUsage { range: internal_method.range, rune: self_borrow_kind_rune_s },
                    inner_rune: *member_rune,
                    region: RegionSR::Unspecified,
                }));

                let mut param_runes: Vec<RuneUsage<'s>> = Vec::new();
                for param in internal_method.params.iter() {
                    match param.virtuality {
                        None => {
                            for rule in param.type_outer_ref_rules.iter().chain(param.value_type_rules.iter()) {
                                rules_builder.push(self.map_runes_anonymous_interface(*rule, &inherit));
                            }
                            param_runes.push(RuneUsage {
                                range: param.range,
                                rune: inherit(param.full_type_rune.rune),
                            });
                        }
                        Some(_) => {
                            param_runes.push(RuneUsage {
                                range: param.range,
                                rune: self_borrow_kind_rune_s,
                            });
                        }
                    }
                }
                let method_params_list_rune = RuneUsage {
                    range: internal_method.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructFunctionBoundParamsListRune(
                        AnonymousSubstructFunctionBoundParamsListRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };
                let param_runes_slice = self.scout_arena.alloc_slice_from_vec(param_runes);
                rules_builder.push(IRulexSR::KindList(KindListSR {
                    range: internal_method.range,
                    result_rune: method_params_list_rune,
                    members: param_runes_slice,
                }));
                let coord_type_ref = self.scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {}));

                let interface_params: Vec<&'s ParameterS<'s>> = internal_method.params.iter()
                    .filter(|p| p.virtuality.is_some())
                    .collect();
                assert_eq!(interface_params.len(), 1, "vassertOne");
                let interface_param = interface_params[0];

                let method_interface_template_rune = RuneUsage {
                    range: interface_param.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructFunctionInterfaceTemplateRune(
                        AnonymousSubstructFunctionInterfaceTemplateRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };

                let method_interface_kind_rune = RuneUsage {
                    range: interface_param.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructFunctionInterfaceKindRune(
                        AnonymousSubstructFunctionInterfaceKindRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };

                rules_builder.push(IRulexSR::Lookup(LookupSR {
                    range: interface_param.range,
                    rune: method_interface_template_rune,
                    parts: self.scout_arena.alloc_slice_copy(&[
                        interface_a.name.get_imprecise_name(self.scout_arena),
                    ]),
                }));
                let generic_param_runes: Vec<RuneUsage<'s>> = interface_a.generic_params.iter().map(|gp| gp.rune).collect();
                let generic_param_runes_slice = self.scout_arena.alloc_slice_from_vec(generic_param_runes);
                rules_builder.push(IRulexSR::Call(CallSR {
                    range: interface_param.range,
                    result_rune: method_interface_kind_rune,
                    template_rune: method_interface_template_rune,
                    args: generic_param_runes_slice,
                }));

                let method_prototype_rune = RuneUsage {
                    range: internal_method.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructFunctionBoundPrototypeRune(
                        AnonymousSubstructFunctionBoundPrototypeRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };
                rules_builder.push(IRulexSR::DefinitionFunc(DefinitionFuncSR {
                    range: internal_method.range,
                    result_rune: method_prototype_rune,
                    name: self.keywords.underscores_call,
                    params_list_rune: method_params_list_rune,
                    return_rune,
                }));
                rules_builder.push(IRulexSR::CallSiteFunc(CallSiteFuncSR {
                    range: internal_method.range,
                    prototype_rune: method_prototype_rune,
                    name: self.keywords.underscores_call,
                    params_list_rune: method_params_list_rune,
                    return_rune,
                }));
                let call_bound_params_types: Vec<ITypeST<'s>> = internal_method.params.iter().map(|param| {
                    match param.virtuality {
                        Some(_) => ITypeST::BorrowRef(self.scout_arena.alloc(BorrowRefST {
                            range: internal_method.range,
                            inner: self.scout_arena.alloc(ITypeST::Rune(
                                self.scout_arena.alloc(RuneUsageST { rune: *member_rune }))),
                            region: RegionS::Unspecified,
                        })),
                        None => ITypeST::Rune(self.scout_arena.alloc(RuneUsageST {
                            rune: RuneUsage {
                                range: param.value_type_rune.range,
                                rune: inherit(param.value_type_rune.rune),
                            },
                        })),
                    }
                }).collect();
                rules_builder.push(IRulexSR::Resolve(ResolveSR {
                    range: internal_method.range,
                    result_rune: method_prototype_rune,
                    name: self.keywords.underscores_call,
                    params_list_rune: method_params_list_rune,
                    params_types: self.scout_arena.alloc_slice_from_vec(call_bound_params_types),
                    return_rune,
                    return_type: ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: return_rune })),
                }));

                let bound_params: Vec<ParameterS<'s>> = internal_method.params.iter().map(|param| {
                    let bound_param_name = IVarDeclarationNameS::DesugaredParamName(DesugaredParamNameDeclarationS {
                        imprecise_name: self.scout_arena.intern_desugared_param_name(internal_method.range.begin),
                        lid: LocationInDenizen { path: &[] },
                    });
                    match param.virtuality {
                        Some(_) => {
                            let member_type = self.scout_arena.alloc(ITypeST::Rune(
                                self.scout_arena.alloc(RuneUsageST { rune: *member_rune })));
                            ParameterS::new(
                                internal_method.range,
                                None,
                                false,
                                bound_param_name,
                                ITypeST::BorrowRef(self.scout_arena.alloc(BorrowRefST {
                                    range: internal_method.range,
                                    inner: member_type,
                                    region: RegionS::Unspecified,
                                })),
                                RuneUsage { range: internal_method.range, rune: self_borrow_kind_rune_s },
                                *member_rune,
                                self.scout_arena.alloc_slice_from_vec(vec![IRulexSR::BorrowRef(BorrowRefSR {
                                    range: internal_method.range,
                                    result_rune: RuneUsage { range: internal_method.range, rune: self_borrow_kind_rune_s },
                                    inner_rune: *member_rune,
                                    region: RegionSR::Unspecified,
                                })]),
                                &[],
                            )
                        }
                        None => {
                            ParameterS::new(
                                param.range,
                                None,
                                param.pre_checked,
                                bound_param_name,
                                param.tyype,
                                RuneUsage { range: param.full_type_rune.range, rune: inherit(param.full_type_rune.rune) },
                                RuneUsage { range: param.value_type_rune.range, rune: inherit(param.value_type_rune.rune) },
                                self.scout_arena.alloc_slice_from_vec(
                                    param.type_outer_ref_rules.iter()
                                        .map(|r| self.map_runes_anonymous_interface(*r, &inherit))
                                        .collect::<Vec<_>>()),
                                self.scout_arena.alloc_slice_from_vec(
                                    param.value_type_rules.iter()
                                        .map(|r| self.map_runes_anonymous_interface(*r, &inherit))
                                        .collect::<Vec<_>>()),
                            )
                        }
                    }
                }).collect();
                let call_bound_fn = FunctionS::new(
                    internal_method.range,
                    IFunctionDeclarationNameS::FunctionName(FunctionNameS {
                        imprecise_name: self.scout_arena.intern_code_name(self.keywords.underscores_call),
                        code_location: internal_method.range.begin,
                        lid: LocationInDenizen { path: &[] },
                    }),
                    &[],
                    &[],
                    TemplateTemplataType {
                        param_types: &[],
                        return_type: self.scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {})),
                    },
                    self.scout_arena.alloc_slice_from_vec(bound_params),
                    Some(return_rune),
                    internal_method.maybe_return_type,
                    &[],
                    &[],
                    &[],
                    &[],
                    self.scout_arena.alloc(IBodyS::AbstractBody(AbstractBodyS {})),
                );
                struct_func_bounds.push((method_prototype_rune, call_bound_fn));
            }

            {
                let drop_params_list_rune = RuneUsage {
                    range: internal_method.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructDropBoundParamsListRune(
                        AnonymousSubstructDropBoundParamsListRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };
                let drop_params_slice = self.scout_arena.alloc_slice_from_vec(vec![RuneUsage {
                    range: internal_method.range,
                    rune: member_rune.rune,
                }]);
                rules_builder.push(IRulexSR::KindList(KindListSR {
                    range: internal_method.range,
                    result_rune: drop_params_list_rune,
                    members: drop_params_slice,
                }));
                let coord_type_ref2 = self.scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {}));

                let drop_prototype_rune = RuneUsage {
                    range: internal_method.range,
                    rune: self.scout_arena.intern_rune(IRuneValS::AnonymousSubstructDropBoundPrototypeRune(
                        AnonymousSubstructDropBoundPrototypeRuneS {
                            interface: *interface_a.name,
                            method: internal_method.name,
                        })),
                };
                let void_coord_ru = RuneUsage { range: internal_method.range, rune: void_kind_rune };
                rules_builder.push(IRulexSR::DefinitionFunc(DefinitionFuncSR {
                    range: internal_method.range,
                    result_rune: drop_prototype_rune,
                    name: self.keywords.drop,
                    params_list_rune: drop_params_list_rune,
                    return_rune: void_coord_ru,
                }));
                rules_builder.push(IRulexSR::CallSiteFunc(CallSiteFuncSR {
                    range: internal_method.range,
                    prototype_rune: drop_prototype_rune,
                    name: self.keywords.drop,
                    params_list_rune: drop_params_list_rune,
                    return_rune: void_coord_ru,
                }));
                rules_builder.push(IRulexSR::Resolve(ResolveSR {
                    range: internal_method.range,
                    result_rune: drop_prototype_rune,
                    name: self.keywords.drop,
                    params_list_rune: drop_params_list_rune,
                    params_types: self.scout_arena.alloc_slice_from_vec(
                        drop_params_slice.iter()
                            .map(|ru| ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: *ru })))
                            .collect::<Vec<_>>()),
                    return_rune: void_coord_ru,
                    return_type: ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: void_coord_ru })),
                }));

                let drop_bound_param = ParameterS::new(
                    internal_method.range,
                    None,
                    false,
                    IVarDeclarationNameS::DesugaredParamName(DesugaredParamNameDeclarationS {
                        imprecise_name: self.scout_arena.intern_desugared_param_name(internal_method.range.begin),
                        lid: LocationInDenizen { path: &[] },
                    }),
                    ITypeST::Rune(self.scout_arena.alloc(RuneUsageST { rune: *member_rune })),
                    *member_rune,
                    *member_rune,
                    &[],
                    &[],
                );
                let drop_bound_fn = FunctionS::new(
                    internal_method.range,
                    IFunctionDeclarationNameS::FunctionName(FunctionNameS {
                        imprecise_name: self.scout_arena.intern_code_name(self.keywords.drop),
                        code_location: internal_method.range.begin,
                        lid: LocationInDenizen { path: &[] },
                    }),
                    &[],
                    &[],
                    TemplateTemplataType {
                        param_types: &[],
                        return_type: self.scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {})),
                    },
                    self.scout_arena.alloc_slice_from_vec(vec![drop_bound_param]),
                    Some(void_coord_ru),
                    Some(ITypeST::Call(self.scout_arena.alloc(CallST {
                        range: internal_method.range,
                        template: self.scout_arena.alloc(ITypeST::Name(self.scout_arena.alloc(NameST {
                            range: internal_method.range,
                            name: void_imprecise_name,
                        }))),
                        args: self.scout_arena.alloc_slice_from_vec(Vec::new()),
                    }))),
                    &[],
                    &[],
                    &[],
                    &[],
                    self.scout_arena.alloc(IBodyS::AbstractBody(AbstractBodyS {})),
                );
                struct_func_bounds.push((drop_prototype_rune, drop_bound_fn));
            }
        }

        for (result_rune, bound_fn) in interface_a.func_bounds.iter() {
            let rebuilt = FunctionS::new(
                bound_fn.range,
                bound_fn.name,
                bound_fn.attributes,
                bound_fn.generic_params,
                bound_fn.tyype,
                bound_fn.params,
                bound_fn.maybe_ret_kind_rune,
                bound_fn.maybe_return_type,
                bound_fn.effects,
                bound_fn.header_rules,
                bound_fn.impl_bounds,
                bound_fn.func_bounds,
                bound_fn.body,
            );
            struct_func_bounds.push((*result_rune, rebuilt));
        }

        let member_coord_types: Vec<ITemplataType<'s>> = member_runes.iter()
            .map(|_mr| ITemplataType::KindTemplataType(KindTemplataType {}))
            .collect();
        let mut param_types: Vec<ITemplataType<'s>> = interface_a.tyype.param_types.to_vec();
        param_types.extend(member_coord_types);
        let param_types_slice = self.scout_arena.alloc_slice_from_vec(param_types);
        let kind_type = self.scout_arena.alloc(ITemplataType::KindTemplataType(KindTemplataType {}));
        let tyype = TemplateTemplataType {
            param_types: param_types_slice,
            return_type: kind_type,
        };

        let header_rules_slice = self.scout_arena.alloc_slice_from_vec(rules_builder);
        let member_rules_slice: &'s [IRulexSR<'s>] = self.scout_arena.alloc_slice_from_vec(vec![]);
        let generic_params_slice = self.scout_arena.alloc_slice_from_vec(struct_generic_params);
        let attributes_slice: &'s [ICitizenAttributeS<'s>] = self.scout_arena.alloc_slice_from_vec(vec![]);
        let members_slice: &'s [IStructMemberS<'s>] = self.scout_arena.alloc_slice_from_vec(
            members.iter().map(|m| IStructMemberS::NormalStructMember(*m)).collect::<Vec<_>>());

        let struct_a = StructS::new(
          interface_a.range,
          IStructDeclarationNameS::AnonymousSubstructTemplateName(
                *self.scout_arena.alloc(struct_template_name_s)),
          attributes_slice,
          generic_params_slice,
          interface_a.sharedness,
          tyype,
          header_rules_slice,
          member_rules_slice,
          members_slice,
          &[],
          &[],
          self.scout_arena.alloc_slice_from_vec(struct_func_bounds),
        );
        self.scout_arena.alloc(struct_a)
    }

    pub fn make_forwarder_function_anonymous_interface(
        &self,
        struct_name_s: AnonymousSubstructTemplateNameS<'s>,
        interface: &'s InterfaceS<'s>,
        struct_: &'s StructS<'s>,
        method: &'s FunctionS<'s>,
        method_index: i32,
    ) -> &'s FunctionS<'s> {

        let struct_type = struct_.tyype;
        let method_range = method.range;
        let attributes = method.attributes;
        let method_original_type = method.tyype;
        let method_original_identifying_runes: &'s [&'s GenericParameterS<'s>] = method.generic_params;
        let original_params = method.params;
        let method_original_rules = method.header_rules;

        // vassert(struct.genericParameters.map(_.rune).startsWith(methodOriginalIdentifyingRunes.map(_.rune)))
        let starts_with = struct_.generic_params.len() >= method_original_identifying_runes.len()
            && struct_.generic_params.iter().zip(method_original_identifying_runes.iter())
                .all(|(a, b)| a.rune.rune.ptr_eq(&b.rune.rune));
        assert!(starts_with, "vassert: struct.genericParameters.startsWith(methodOriginalIdentifyingRunes)");

        let inherit = |rune: IRuneS<'s>| -> IRuneS<'s> {
            self.inherited_method_rune_anonymous_interface(interface, method, rune)
        };

        let mut generic_params_vec: Vec<&'s GenericParameterS<'s>> = Vec::new();
        for gp in struct_.generic_params.iter() {
            let new_rune = inherit(gp.rune.rune);
            generic_params_vec.push(self.scout_arena.alloc(GenericParameterS {
                range: gp.range,
                rune: RuneUsage { range: gp.rune.range, rune: new_rune },
                tyype: gp.tyype,
                default: gp.default,
            }));
        }

        let mut rules: Vec<IRulexSR<'s>> = Vec::new();

        for rule in method_original_rules.iter() {
            let mapped = self.map_runes_anonymous_interface(*rule, &inherit);
            rules.push(mapped);
        }
        let original_ret_rune = method.maybe_ret_kind_rune.unwrap();
        let inherited_return_rune = RuneUsage {
            range: original_ret_rune.range,
            rune: inherit(original_ret_rune.rune),
        };

        let self_kind_rune = self.scout_arena.intern_rune(IRuneValS::SelfKindRune(SelfKindRuneS {}));
        let self_full_type_rune = self.scout_arena.intern_rune(IRuneValS::SelfFullTypeRune(SelfFullTypeRuneS {}));
        let self_kind_template_rune = self.scout_arena.intern_rune(IRuneValS::SelfKindTemplateRune(SelfKindTemplateRuneS { loc: struct_.range.begin }));

        let mut abstract_param_index: i32 = -1;
        for (i, param) in original_params.iter().enumerate() {
            let is_abstract = match param.virtuality {
                Some(AbstractSP { .. }) => true,
                None => false,
            };
            if is_abstract {
                abstract_param_index = i as i32;
                break;
            }
        }
        assert!(abstract_param_index >= 0, "vassert: abstractParamIndex >= 0");
        let abstract_param = &original_params[abstract_param_index as usize];
        let abstract_param_range = abstract_param.range;
        let abstract_param_kind_rune = RuneUsage {
            range: abstract_param_range,
            rune: inherit(abstract_param.value_type_rune.rune),
        };

        let inherited_abstract_full_rune = inherit(abstract_param.full_type_rune.rune);
        let inherited_abstract_value_rune = inherit(abstract_param.value_type_rune.rune);
        let self_outer_ref_rules_vec: Vec<IRulexSR<'s>> = abstract_param.type_outer_ref_rules.iter()
            .map(|rule| self.map_runes_anonymous_interface(*rule, |rune| {
                let inherited = inherit(rune);
                if inherited.ptr_eq(&inherited_abstract_full_rune) { self_full_type_rune }
                else if inherited.ptr_eq(&inherited_abstract_value_rune) { self_kind_rune }
                else { inherited }
            }))
            .collect();
        let self_full_rune =
            if self_outer_ref_rules_vec.is_empty() { self_kind_rune } else { self_full_type_rune };
        let self_outer_ref_rules = self.scout_arena.alloc_slice_from_vec(self_outer_ref_rules_vec);

        let struct_interface_imprecise = struct_name_s.interface_name.get_imprecise_name(self.scout_arena);
        let lookup_struct_template_rule = IRulexSR::Lookup(LookupSR {
            range: abstract_param_range,
            rune: RuneUsage { range: abstract_param_range, rune: self_kind_template_rune },
            parts: self.scout_arena.alloc_slice_copy(&[
                self.scout_arena.intern_imprecise_name(IImpreciseNameValS::AnonymousSubstructTemplateImpreciseName(
                    AnonymousSubstructTemplateImpreciseNameValS { interface_imprecise_name: struct_interface_imprecise })),
            ]),
        });
        rules.push(lookup_struct_template_rule);

        let gp_runes_vec: Vec<RuneUsage<'s>> = generic_params_vec.iter().map(|gp| gp.rune).collect();
        let gp_runes_slice = self.scout_arena.alloc_slice_from_vec(gp_runes_vec);
        let lookup_struct_rule = IRulexSR::Call(CallSR {
            range: abstract_param_range,
            result_rune: RuneUsage { range: abstract_param_range, rune: self_kind_rune },
            template_rune: RuneUsage { range: abstract_param_range, rune: self_kind_template_rune },
            args: gp_runes_slice,
        });
        rules.push(lookup_struct_rule);

        let self_name = IVarDeclarationNameS::CodeVarName(CodeVarNameS {
            imprecise_name: self.scout_arena.intern_code_name(self.keywords.self_),
            lid: LocationInDenizen { path: &[] },
        });
        let mut new_params_vec: Vec<ParameterS<'s>> = Vec::new();
        for param in original_params.iter() {
            match param.virtuality {
                Some(_) => {
                    new_params_vec.push(ParameterS::new(
                        abstract_param_range,
                        None,
                        false,
                        self_name.clone(),
                        ITypeST::Rune(self.scout_arena.alloc(RuneUsageST {
                            rune: RuneUsage { range: abstract_param_kind_rune.range, rune: self_full_rune },
                        })),
                        RuneUsage { range: abstract_param_kind_rune.range, rune: self_full_rune },
                        RuneUsage { range: abstract_param_kind_rune.range, rune: self_kind_rune },
                        self_outer_ref_rules,
                        self.scout_arena.alloc_slice_from_vec::<IRulexSR<'s>>(Vec::new()),
                    ));
                }
                None => {
                    let remap = |rune_usage: RuneUsage<'s>| RuneUsage {
                        range: rune_usage.range,
                        rune: inherit(rune_usage.rune),
                    };
                    let remap_rules = |rules: &'s [IRulexSR<'s>]| {
                        let mapped: Vec<IRulexSR<'s>> = rules.iter()
                            .map(|r| self.map_runes_anonymous_interface(*r, &inherit))
                            .collect();
                        self.scout_arena.alloc_slice_from_vec(mapped)
                    };
                    new_params_vec.push(ParameterS::new(
                        param.range,
                        param.virtuality,
                        param.pre_checked,
                        param.name,
                        param.tyype,
                        remap(param.full_type_rune),
                        remap(param.value_type_rune),
                        remap_rules(param.type_outer_ref_rules),
                        remap_rules(param.value_type_rules),
                    ));
                }
            }
        }

        let self_local_load = self.scout_arena.alloc(IExpressionSE::LocalLoad(LocalLoadSE {
            range: method_range,
            name: self_name.imprecise_name(self.scout_arena),
        }));
        let dot_member = self.scout_arena.intern_str(&method_index.to_string());
        let dot_expr = self.scout_arena.alloc(IExpressionSE::Dot(DotSE {
            range: method_range,
            left: self_local_load,
            member: dot_member,
            borrow_container: false,
        }));
        let callable_expr = self.scout_arena.alloc(IExpressionSE::Ownershipped(OwnershippedSE {
            range: method_range,
            inner_expr: dot_expr,
            target_ownership: LoadAsP::LoadAsBorrow,
        }));

        let mut call_args: Vec<&'s IExpressionSE<'s>> = Vec::new();
        for (i, param) in new_params_vec.iter().enumerate() {
            if (i as i32) == abstract_param_index { continue; }
            let nm = param.name.imprecise_name(self.scout_arena);
            call_args.push(self.scout_arena.alloc(IExpressionSE::Unlet(UnletSE {
                range: method_range,
                name: nm,
            })));
        }
        let call_args_slice = self.scout_arena.alloc_slice_from_vec(call_args);

        let new_body_expr = self.scout_arena.alloc(IExpressionSE::FunctionCall(FunctionCallSE {
            range: method_range,
            location: LocationInDenizen { path: &[] },
            callable_expr,
            arg_exprs: call_args_slice,
        }));

        let locals_vec: Vec<LocalS<'s>> = new_params_vec.iter().map(|p| {
            let nm = p.name;
            LocalS {
                var_name: nm,
                self_borrowed: IVariableUseCertainty::NotUsed,
                self_moved: IVariableUseCertainty::Used,
                self_mutated: IVariableUseCertainty::NotUsed,
                child_borrowed: IVariableUseCertainty::NotUsed,
                child_moved: IVariableUseCertainty::NotUsed,
                child_mutated: IVariableUseCertainty::NotUsed,
            }
        }).collect();
        let locals_slice = self.scout_arena.alloc_slice_from_vec(locals_vec);
        let block_se = self.scout_arena.alloc(BlockSE {
            range: method_range,
            locals: locals_slice,
            expr: new_body_expr,
        });
        let body_se = self.scout_arena.alloc(BodySE {
            range: method_range,
            closured_names: self.scout_arena.alloc_slice_from_vec::<IVarDeclarationNameS<'s>>(vec![]),
            block: block_se,
        });
        let body = self.scout_arena.alloc(IBodyS::CodeBody(CodeBodyS { body: body_se }));

        let forwarder_imprecise = match self.scout_arena.intern_function_imprecise_name(
            IFunctionImpreciseNameValS::ForwarderFunctionDeclarationName(ForwarderFunctionImpreciseNameValS {
                inner: method.name.imprecise_name(),
                index: method_index,
            })) {
            IFunctionImpreciseNameS::ForwarderFunctionDeclarationName(r) => r,
            _ => panic!("vwat: intern_function_imprecise_name returned non-Forwarder"),
        };
        let forwarder_name = IFunctionDeclarationNameS::ForwarderFunctionDeclarationName(
            self.scout_arena.alloc(ForwarderFunctionDeclarationNameS {
                inner: method.name,
                index: method_index,
                imprecise_name: forwarder_imprecise,
                lid: LocationInDenizen { path: &[] },
            }));

        // Tyype: param_types ++ struct.genericParameters.map(_ => KindTemplataType()), return FunctionTemplataType
        let mut new_param_types: Vec<ITemplataType<'s>> = method_original_type.param_types.to_vec();
        for _ in struct_.generic_params.iter() {
            new_param_types.push(ITemplataType::KindTemplataType(KindTemplataType {}));
        }
        let new_param_types_slice = self.scout_arena.alloc_slice_from_vec(new_param_types);
        let return_type_ref = self.scout_arena.alloc(ITemplataType::FunctionTemplataType(FunctionTemplataType {}));
        let new_tyype = TemplateTemplataType { param_types: new_param_types_slice, return_type: return_type_ref };

        let new_params_slice = self.scout_arena.alloc_slice_from_vec(new_params_vec);
        let rules_slice = self.scout_arena.alloc_slice_from_vec(rules);
        let generic_params_slice = self.scout_arena.alloc_slice_from_vec(generic_params_vec);

        self.scout_arena.alloc(FunctionS::new(
            method_range,
            forwarder_name,
            attributes,
            generic_params_slice,
            new_tyype,
            new_params_slice,
            Some(inherited_return_rune),
            method
                .maybe_return_type
                .map(|written| map_runes_in_type_st(self.scout_arena, &inherit, &written)),
            &[],
            rules_slice,
            &[],
            &[],
            body,
        ))
    }

}
