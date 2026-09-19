use crate::postparsing::names::IRuneS;
use crate::StrI;
use crate::typing::names::names::IVarNameT;

// An expression for expressing the group(s) a function might mutate or a ref might point at (as
// opposed to GroupPathG which is a specific mutation to a specific group).
// Unions are flattened into multiple GroupPathG's.
pub type GroupExprG<'s, 't, 'g> = &'g [GroupPathG<'s, 't, 'g>];

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct GroupPathG<'s, 't, 'g> {
  pub root: GroupRootG<'s, 't>,
  pub steps: &'g [GroupChildStepG<'s>],
  pub ellipsis: bool, // the `...` part of `x...`
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GroupRootG<'s, 't> {
  Rune(IRuneS<'s>), // a group param, e.g. <g'>, resolved to its id
  ParamAnonymousGroup(IVarNameT<'s, 't>), // A param's group if it doesn't come from a rune or another param. The StrI is the parameter's name
  Local(IVarNameT<'s, 't>), // A local's implicitly declared group.
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GroupChildStepG<'s> {
  Member { member_name: StrI<'s> }, // `x.items`
  ChildElements { }, // the `[]` part of `x.items[]` if items is a Box/Vec/RSA
  InlineElements { }, // the `[]` part of `x.items[]` if items is a SSA.
  Variant { variant_name: StrI<'s> }, // an enum's variant, the `WarpEngine` part of `my_ship.engine_enum.WarpEngine`
}