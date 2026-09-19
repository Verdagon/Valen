use crate::postparsing::itemplatatype::{ITemplataType, TemplateTemplataType};
use crate::StrI;
use crate::typing::ast::ast::{FunctionHeaderT, PrototypeT};
use crate::typing::borrow_checker::group_expr::GroupExprG;
use crate::typing::borrow_checker::kind_g::KindGT;
use crate::typing::names::names::IdT;
use crate::typing::templata::templata::{ITemplataT, KindTemplataT};
use crate::typing::types::types::KindT;
use crate::utils::range::RangeS;

/// Polyvalue (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ITemplataG<'s, 't, 'g> {
  Kind(KindTemplataG<'s, 't, 'g>),
  Placeholder(&'g PlaceholderTemplataG<'s, 't>),
  Integer(i64),
  Boolean(bool),
  String(StrI<'s>),
  Prototype(&'g PrototypeTemplataG<'s, 't>),
  Isa(&'g IsaTemplataG<'s, 't>),
  CoordList(&'g KindListTemplataG<'s, 't, 'g>),
  RuntimeSizedArrayTemplate(RuntimeSizedArrayTemplateTemplataG),
  StaticSizedArrayTemplate(StaticSizedArrayTemplateTemplataG),
  /// The ceremonial value of a group generic param. Uniform with type/int params so arity/index
  /// invariants hold, but never enters a `KindT` and is never read — the borrow checker reads groups
  /// off the declaration-side `GroupS`, not off this. See @GROUPS-are-declaration-side.
  /// VGB: arcana for this
  Group(GroupTemplataG<'s, 't, 'g>),
  Function(&'g FunctionTemplataG<'s, 't>),
  StructDefinition(&'g StructDefinitionTemplataG<'s, 't>),
  InterfaceDefinition(&'g InterfaceDefinitionTemplataG<'s, 't>),
  ImplDefinition(&'g ImplDefinitionTemplataG<'s, 't>),
  ExternFunction(&'g ExternFunctionTemplataG<'s, 't>),
}


/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PlaceholderTemplataG<'s, 't> {
  pub id: IdT<'s, 't>,
  pub tyype: ITemplataType<'s>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindTemplataG<'s, 't, 'g> {
  pub kind: KindGT<'s, 't, 'g>,
}

/// Value-type (see @TFITCX).
/// The ceremonial group-param constant; never read, so it carries no
/// payload (a `GroupB` would be the real algebra, but this is only the uniform param's value).
/// VGB: arcana
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct GroupTemplataG<'s, 't, 'g> {
  pub group: GroupExprG<'s, 't, 'g>,
  pub kind: KindGT<'s, 't, 'g>, // this could be redundant since the type is on the group
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuntimeSizedArrayTemplateTemplataG {}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticSizedArrayTemplateTemplataG {}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionTemplataG<'s, 't>
where
    's: 't,
{
  // pub outer_env: IEnvironmentT<'s, 't>,
  pub function_template_id: &'t IdT<'s, 't>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CitizenDefinitionTemplataG<'s, 't, 'g> {
  Struct(&'g StructDefinitionTemplataG<'s, 't>),
  Interface(&'g InterfaceDefinitionTemplataG<'s, 't>),
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructDefinitionTemplataG<'s, 't>
where
    's: 't,
{
  // pub declaring_env: IEnvironmentT<'s, 't>,
  pub struct_template_id: &'t IdT<'s, 't>,
  pub tyype: TemplateTemplataType<'s>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterfaceDefinitionTemplataG<'s, 't>
where
    's: 't,
{
  // pub declaring_env: IEnvironmentT<'s, 't>,
  pub interface_template_id: &'t IdT<'s, 't>,
  pub tyype: TemplateTemplataType<'s>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImplDefinitionTemplataG<'s, 't>
where
    's: 't,
{
  // pub env: IEnvironmentT<'s, 't>,
  pub impl_template_id: &'t IdT<'s, 't>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BooleanTemplataG {
  pub value: bool,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct IntegerTemplataG {
  pub value: i64,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StringTemplataG<'s> {
  pub value: StrI<'s>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PrototypeTemplataG<'s, 't> {
  pub prototype: &'t PrototypeT<'s, 't>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct IsaTemplataG<'s, 't> {
  pub declaration_range: RangeS<'s>,
  pub impl_name: IdT<'s, 't>,
  pub sub_kind: KindT<'s, 't>,
  pub super_kind: KindT<'s, 't>,
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindListTemplataG<'s, 't, 'g> {
  pub kinds: &'g [KindGT<'s, 't, 'g>],
}

/// Value-type (see @TFITCX)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ExternFunctionTemplataG<'s, 't> {
  pub header: &'t FunctionHeaderT<'s, 't>,
}

pub fn expect_kind_templata_g<'s, 't, 'g>(templata: ITemplataG<'s, 't, 'g>) -> KindTemplataG<'s, 't, 'g> {
  match templata {
    ITemplataG::Kind(t) => t,
    other => panic!("vfail: {:?}", other),
  }
}
