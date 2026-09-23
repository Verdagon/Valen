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
      KindT::RawInterface(i) => ISubKindTT::Interface(i.inner),
      KindT::DynInterface(i) => ISubKindTT::Interface(i.inner),
      KindT::EnumInterface(i) => ISubKindTT::Interface(i.inner),
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

    // VCOORD: clean this up once we get rid of rules
    let param_is_dyn = value_type_is_dyn(param.value_type_rules, param.value_type_rune.rune);
    let in_param_form = |interface: &'t InterfaceTT<'s, 't>| {
      if param_is_dyn {
        KindT::DynInterface(self.typing_interner.intern_dyn_interface_tt(DynInterfaceTTValT { inner: interface }))
      } else {
        self.typing_interner.raw_interface_kind(interface)
      }
    };

    // The arg already names the param's own interface (in either form) — this is dispatch: a
    // `dyn X` arg reaching an abstract `&X` self (or the reverse). An interface is not its own
    // parent, so this resolves here, not via get_parents; only the form is reconciled.
    if let Some(arg_interface) = peeled_arg.interface_tt() {
      if *self.get_citizen_template(arg_interface.id) == expected_template_id {
        return Some(in_param_form(arg_interface));
      }
    }

    self
      .get_parents(coutputs, call_range_t, call_location, calling_env, arg_sub_kind)
      .into_iter()
      .find(|s| {
        matches!(s, ISuperKindTT::Interface(_))
          && *self.get_citizen_template(&s.id()) == expected_template_id
      })
      .map(|s| match self.typing_interner.super_kind_to_kind(s).interface_tt() {
        Some(interface) => in_param_form(interface),
        None => self.typing_interner.super_kind_to_kind(s),
      })
    // /VCOORD
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
  // `dyn X`: the DynInterface rule produces the value type; follow its inner rune, which names the
  // interface itself (bare, or a Call for `dyn X<...>`), so the interface template is found.
  if let Some(d) = rules.iter().find_map(|r| match r {
    IRulexSR::DynInterface(d) if d.result_rune.rune == target => Some(d),
    _ => None,
  }) {
    target = d.inner_rune.rune;
  }
  if let Some(c) = rules.iter().find_map(|r| match r {
    IRulexSR::Call(c) if c.result_rune.rune == target => Some(c),
    _ => None,
  }) {
    target = c.template_rune.rune;
  }
  rules.iter().find_map(|r| match r {
    IRulexSR::Lookup(l) if l.rune.rune == target => l.parts.first().copied(),
    _ => None,
  })
}

/// True when the value type is written `dyn X` — i.e. a DynInterface rule produces value_type_rune.
// VCOORD: clean this up when we get rid of rules
fn value_type_is_dyn<'s>(rules: &[IRulexSR<'s>], value_type_rune: IRuneS<'s>) -> bool {
  rules
    .iter()
    .any(|r| matches!(r, IRulexSR::DynInterface(d) if d.result_rune.rune == value_type_rune))
}
