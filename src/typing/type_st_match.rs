use crate::utils::range::RangeS;

use crate::typing::compiler::Compiler;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::infer_compiler::InferEnv;

use crate::typing::env::environment::{IEnvironmentT, IInDenizenEnvironmentT};
use crate::typing::names::names::*;
use crate::typing::templata::templata::*;
use crate::typing::types::types::*;

use crate::postparsing::ast::{LocationInDenizen, ParameterS};
use crate::postparsing::names::{IImpreciseNameS, IRuneS};
use crate::postparsing::rules::rules::IRulexSR;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't>
where
  's: 't,
{
  pub fn compute_upcast_coerced_arg(
    &self,
    coutputs: &mut CompilerOutputs<'s, 't>,
    calling_env: IInDenizenEnvironmentT<'s, 't>,
    call_range_t: &'t [RangeS<'s>],
    call_location: LocationInDenizen<'s>,
    context_region: RegionT,
    peeled_arg: KindT<'s, 't>,
    param: &ParameterS<'s>,
  ) -> Option<KindT<'s, 't>> {
    let arg_sub_kind = match peeled_arg {
      KindT::Struct(s) => ISubKindTT::Struct(s),
      KindT::Interface(i) => ISubKindTT::Interface(i),
      KindT::KindPlaceholder(kp) => ISubKindTT::KindPlaceholder(kp),
      _ => return None,
    };

    let expected_template_id = self.param_expected_value_type_template(
      coutputs,
      calling_env,
      call_range_t,
      call_location,
      context_region,
      param,
    )?;

    if let KindT::Struct(_) | KindT::Interface(_) = peeled_arg {
      if *self.get_citizen_template(&arg_sub_kind.id()) == expected_template_id {
        return None;
      }
    }

    self
      .get_parents(coutputs, call_range_t, call_location, calling_env, arg_sub_kind)
      .into_iter()
      .find(|s| {
        matches!(s, ISuperKindTT::Interface(_))
          && *self.get_citizen_template(&s.id()) == expected_template_id
      })
      .map(KindT::from)
  }

  fn param_expected_value_type_template(
    &self,
    coutputs: &mut CompilerOutputs<'s, 't>,
    calling_env: IInDenizenEnvironmentT<'s, 't>,
    call_range_t: &'t [RangeS<'s>],
    call_location: LocationInDenizen<'s>,
    context_region: RegionT,
    param: &ParameterS<'s>,
  ) -> Option<IdT<'s, 't>> {
    let name = value_type_root_name(param.value_type_rules, param.value_type_rune.rune)?;
    let envs = InferEnv {
      original_calling_env: calling_env,
      parent_ranges: call_range_t,
      call_location,
      self_env: IEnvironmentT::from(calling_env),
      context_region,
    };
    match self.lookup_templata_imprecise(envs, coutputs, call_range_t, name)? {
      ITemplataT::StructDefinition(sd) => Some(*sd.struct_template_id),
      ITemplataT::InterfaceDefinition(idf) => Some(*idf.interface_template_id),
      _ => None,
    }
  }
}

fn value_type_root_name<'s>(
  rules: &[IRulexSR<'s>],
  value_type_rune: IRuneS<'s>,
) -> Option<IImpreciseNameS<'s>> {
  let mut target = value_type_rune;
  if let Some(c) = rules.iter().find_map(|r| match r {
    IRulexSR::Call(c) if c.result_rune.rune == value_type_rune => Some(c),
    _ => None,
  }) {
    target = c.template_rune.rune;
  }
  rules.iter().find_map(|r| match r {
    IRulexSR::Lookup(l) if l.rune.rune == target => l.parts.first().copied(),
    _ => None,
  })
}
