use crate::postparsing::names::IImpreciseNameS;
use crate::typing::env::environment::*;
use crate::typing::names::names::*;
use crate::typing::templata::templata::ITemplataT;
use crate::typing::typing_interner::MustIntern;


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SharednessT {
  Single,
  Shared,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum RegionT {
  Iso,
  // TODO: Get rid of this when we have an actual default region
  Default,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BorrowRefT<'s, 't> {
  pub inner: KindT<'s, 't>,
  // No group here, per BCHATZ.
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OwnRefT<'s, 't> {
  pub inner: KindT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ShareRefT<'s, 't> {
  pub inner: KindT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct WeakRefT<'s, 't> {
  pub inner: KindT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum KindT<'s, 't> {
  Never(NeverT),
  Void(VoidT),
  Int(IntT),
  Bool(BoolT),
  Str(StrT),
  Float(FloatT),
  USize(USizeT),
  Struct(&'t StructTT<'s, 't>),
  RawInterface(&'t RawInterfaceTT<'s, 't>),
  DynInterface(&'t DynInterfaceTT<'s, 't>),
  EnumInterface(&'t EnumInterfaceTT<'s, 't>),
  StaticSizedArray(&'t StaticSizedArrayTT<'s, 't>),
  RuntimeSizedArray(&'t RuntimeSizedArrayTT<'s, 't>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
  OverloadSet(&'t OverloadSetT<'s, 't>),
  BorrowRef(&'t BorrowRefT<'s, 't>),
  OwnRef(&'t OwnRefT<'s, 't>),
  ShareRef(&'t ShareRefT<'s, 't>),
  WeakRef(&'t WeakRefT<'s, 't>),
}

impl<'s, 't> KindT<'s, 't> {
  pub fn expect_citizen(&self) -> ICitizenTT<'s, 't> {
    match self {
      KindT::Struct(c) => ICitizenTT::Struct(c),
      KindT::RawInterface(c) => ICitizenTT::Interface(c.inner),
      _ => panic!("vfail"),
    }
  }

  pub fn expect_interface(&self) -> &'t InterfaceTT<'s, 't> {
    match self {
      KindT::RawInterface(c) => c.inner,
      _ => panic!("vfail"),
    }
  }

  // VCOORD: rename to underlying_interface perhaps
  pub fn interface_tt(&self) -> Option<&'t InterfaceTT<'s, 't>> {
    match self {
      KindT::RawInterface(c) => Some(c.inner),
      KindT::DynInterface(d) => Some(d.inner),
      KindT::EnumInterface(e) => Some(e.inner),
      _ => None,
    }
  }

  pub fn expect_struct(&self) -> &'t StructTT<'s, 't> {
    match self {
      KindT::Struct(c) => c,
      _ => panic!("vfail"),
    }
  }

  pub fn is_primitive(&self) -> bool {
    match self {
      KindT::Never(_) => true,
      KindT::Void(_) => true,
      KindT::Int(_) => true,
      KindT::Bool(_) => true,
      KindT::Str(_) => false,
      KindT::Float(_) => true,
      KindT::USize(_) => true,
      KindT::Struct(_) => false,
      KindT::RawInterface(_) => false,
      KindT::DynInterface(_) => false,
      KindT::EnumInterface(_) => false,
      KindT::StaticSizedArray(_) => false,
      KindT::RuntimeSizedArray(_) => false,
      KindT::KindPlaceholder(_) => false,
      KindT::OverloadSet(_) => true,
      KindT::BorrowRef(_) => false,
      KindT::OwnRef(_) => false,
      KindT::ShareRef(_) => false,
      KindT::WeakRef(_) => false,
    }
  }

  // A `&X -> X` read-out copies the value out of the borrow with no user-written clone,
  // via a CopyPrim intrinsic. True for primitives and for str (a share value that copies
  // like a primitive).
  // VCOORD: TODO: also a ShareRef, and a bare `share` citizen (that one needs
  // the declare_type_sharedness query, so it moves to a Compiler method then).
  // VCOORD: this helper should go away, shouldnt be hardcoding str like this.
  pub fn is_implicitly_cloneable(&self) -> bool {
    self.is_primitive() || matches!(self, KindT::Str(_))
  }
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct NeverT {
  pub from_break: bool,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct VoidT;

impl IntT {
  pub const I32: IntT = IntT { bits: 32 };
  pub const I64: IntT = IntT { bits: 64 };
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct IntT {
  pub bits: i32,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct BoolT;


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StrT;


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FloatT;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct USizeT;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticSizedArrayTT<'s, 't> {
  pub name: IdT<'s, 't>,
  pub _must_intern: MustIntern,
}

impl<'s, 't> StaticSizedArrayTT<'s, 't>
where
  's: 't,
{
  pub fn element_type(&self) -> KindT<'s, 't> {
    match self.name.local_name {
      INameT::StaticSizedArray(ssa_name) => ssa_name.arr.element_type,
      _ => panic!("vwat"),
    }
  }

  pub fn size(&self) -> ITemplataT<'s, 't> {
    match self.name.local_name {
      INameT::StaticSizedArray(ssa_name) => ssa_name.size,
      _ => panic!("vwat"),
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticSizedArrayTTValT<'s, 't> {
  pub name: IdT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuntimeSizedArrayTT<'s, 't> {
  pub name: IdT<'s, 't>,
  pub _must_intern: MustIntern,
}

impl<'s, 't> RuntimeSizedArrayTT<'s, 't>
where
  's: 't,
{
  pub fn element_type(&self) -> KindT<'s, 't> {
    match self.name.local_name {
      INameT::RuntimeSizedArray(rsa_name) => rsa_name.arr.element_type,
      _ => panic!("vwat"),
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuntimeSizedArrayTTValT<'s, 't> {
  pub name: IdT<'s, 't>,
}

fn unapply_i_citizen_tt() {
  panic!("Unimplemented: unapply_i_citizen_tt");
  // Some(self.id)
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ISubKindTT<'s, 't> {
  Struct(&'t StructTT<'s, 't>),
  Interface(&'t InterfaceTT<'s, 't>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
}

impl<'s, 't> ISubKindTT<'s, 't>
where
  's: 't,
{
  pub fn id(&self) -> IdT<'s, 't> {
    match self {
      ISubKindTT::Struct(s) => *s.id,
      ISubKindTT::Interface(i) => *i.id,
      ISubKindTT::KindPlaceholder(kp) => kp.id,
    }
  }

  pub fn expect_citizen(&self) -> ICitizenTT<'s, 't> {
    match self {
      ISubKindTT::Struct(s) => ICitizenTT::Struct(s),
      ISubKindTT::Interface(i) => ICitizenTT::Interface(i),
      ISubKindTT::KindPlaceholder(_) => panic!("vfail"),
    }
  }

  pub fn expect_interface(&self) -> &'t InterfaceTT<'s, 't> {
    match self {
      ISubKindTT::Interface(i) => i,
      _ => panic!("vfail"),
    }
  }

  pub fn expect_struct(&self) -> &'t StructTT<'s, 't> {
    match self {
      ISubKindTT::Struct(s) => s,
      _ => panic!("vfail"),
    }
  }

  pub fn is_primitive(&self) -> bool {
    match self {
      ISubKindTT::Struct(_) => false,
      ISubKindTT::Interface(_) => false,
      ISubKindTT::KindPlaceholder(_) => false,
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ISuperKindTT<'s, 't> {
  Interface(&'t InterfaceTT<'s, 't>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
}

impl<'s, 't> ISuperKindTT<'s, 't>
where
  's: 't,
{
  pub fn id(&self) -> IdT<'s, 't> {
    match self {
      ISuperKindTT::Interface(i) => *i.id,
      ISuperKindTT::KindPlaceholder(kp) => kp.id,
    }
  }

  pub fn expect_citizen(&self) -> ICitizenTT<'s, 't> {
    match self {
      ISuperKindTT::Interface(i) => ICitizenTT::Interface(i),
      ISuperKindTT::KindPlaceholder(_) => panic!("vfail"),
    }
  }

  pub fn expect_interface(&self) -> &'t InterfaceTT<'s, 't> {
    match self {
      ISuperKindTT::Interface(i) => i,
      _ => panic!("vfail"),
    }
  }

  pub fn expect_struct(&self) -> &'t StructTT<'s, 't> {
    panic!("vfail")
  }

  pub fn is_primitive(&self) -> bool {
    match self {
      ISuperKindTT::Interface(_) => false,
      ISuperKindTT::KindPlaceholder(_) => false,
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ICitizenTT<'s, 't> {
  Struct(&'t StructTT<'s, 't>),
  Interface(&'t InterfaceTT<'s, 't>),
}

impl<'s, 't> ICitizenTT<'s, 't>
where
  's: 't,
{
  pub fn id(&self) -> IdT<'s, 't> {
    match self {
      ICitizenTT::Struct(s) => *s.id,
      ICitizenTT::Interface(i) => *i.id,
    }
  }

  pub fn expect_citizen(&self) -> ICitizenTT<'s, 't> {
    *self
  }

  pub fn expect_interface(&self) -> &'t InterfaceTT<'s, 't> {
    match self {
      ICitizenTT::Interface(i) => i,
      _ => panic!("vfail"),
    }
  }

  pub fn expect_struct(&self) -> &'t StructTT<'s, 't> {
    match self {
      ICitizenTT::Struct(s) => s,
      _ => panic!("vfail"),
    }
  }

  pub fn is_primitive(&self) -> bool {
    match self {
      ICitizenTT::Struct(_) => false,
      ICitizenTT::Interface(_) => false,
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StructTT<'s, 't> {
  pub id: &'t IdT<'s, 't>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StructTTValT<'s, 't> {
  pub id: IdT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct InterfaceTT<'s, 't> {
  pub id: &'t IdT<'s, 't>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct InterfaceTTValT<'s, 't> {
  pub id: IdT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DynInterfaceTT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DynInterfaceTTValT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct EnumInterfaceTT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct EnumInterfaceTTValT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RawInterfaceTT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RawInterfaceTTValT<'s, 't> {
  pub inner: &'t InterfaceTT<'s, 't>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OverloadSetT<'s, 't> {
  pub env: IInDenizenEnvironmentT<'s, 't>,
  pub name: &'s IImpreciseNameS<'s>,
  pub _must_intern: MustIntern,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct OverloadSetTValT<'s, 't> {
  pub env: IInDenizenEnvironmentT<'s, 't>,
  pub name: &'s IImpreciseNameS<'s>,
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KindPlaceholderT<'s, 't> {
  pub id: IdT<'s, 't>,
}


#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub enum InternedKindPayloadValT<'s, 't>
where
  's: 't,
{
  StructTT(StructTTValT<'s, 't>),
  InterfaceTT(InterfaceTTValT<'s, 't>),
  RawInterfaceTT(RawInterfaceTTValT<'s, 't>),
  DynInterfaceTT(DynInterfaceTTValT<'s, 't>),
  EnumInterfaceTT(EnumInterfaceTTValT<'s, 't>),
  StaticSizedArrayTT(StaticSizedArrayTTValT<'s, 't>),
  RuntimeSizedArrayTT(RuntimeSizedArrayTTValT<'s, 't>),
  KindPlaceholder(KindPlaceholderT<'s, 't>),
  OverloadSet(OverloadSetTValT<'s, 't>),
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub enum InternedKindPayloadT<'s, 't>
where
  's: 't,
{
  StructTT(&'t StructTT<'s, 't>),
  InterfaceTT(&'t InterfaceTT<'s, 't>),
  RawInterfaceTT(&'t RawInterfaceTT<'s, 't>),
  DynInterfaceTT(&'t DynInterfaceTT<'s, 't>),
  EnumInterfaceTT(&'t EnumInterfaceTT<'s, 't>),
  StaticSizedArrayTT(&'t StaticSizedArrayTT<'s, 't>),
  RuntimeSizedArrayTT(&'t RuntimeSizedArrayTT<'s, 't>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
  OverloadSet(&'t OverloadSetT<'s, 't>),
}


impl<'s, 't> From<&'t StructTT<'s, 't>> for ICitizenTT<'s, 't> {
  fn from(x: &'t StructTT<'s, 't>) -> Self {
    ICitizenTT::Struct(x)
  }
}
impl<'s, 't> From<&'t StructTT<'s, 't>> for ISubKindTT<'s, 't> {
  fn from(x: &'t StructTT<'s, 't>) -> Self {
    ISubKindTT::Struct(x)
  }
}
impl<'s, 't> From<&'t StructTT<'s, 't>> for KindT<'s, 't> {
  fn from(x: &'t StructTT<'s, 't>) -> Self {
    KindT::Struct(x)
  }
}

impl<'s, 't> From<&'t InterfaceTT<'s, 't>> for ICitizenTT<'s, 't> {
  fn from(x: &'t InterfaceTT<'s, 't>) -> Self {
    ICitizenTT::Interface(x)
  }
}
impl<'s, 't> From<&'t InterfaceTT<'s, 't>> for ISubKindTT<'s, 't> {
  fn from(x: &'t InterfaceTT<'s, 't>) -> Self {
    ISubKindTT::Interface(x)
  }
}
impl<'s, 't> From<&'t InterfaceTT<'s, 't>> for ISuperKindTT<'s, 't> {
  fn from(x: &'t InterfaceTT<'s, 't>) -> Self {
    ISuperKindTT::Interface(x)
  }
}

impl<'s, 't> From<&'t StaticSizedArrayTT<'s, 't>> for KindT<'s, 't> {
  fn from(x: &'t StaticSizedArrayTT<'s, 't>) -> Self {
    KindT::StaticSizedArray(x)
  }
}

impl<'s, 't> From<&'t RuntimeSizedArrayTT<'s, 't>> for KindT<'s, 't> {
  fn from(x: &'t RuntimeSizedArrayTT<'s, 't>) -> Self {
    KindT::RuntimeSizedArray(x)
  }
}

impl<'s, 't> From<&'t KindPlaceholderT<'s, 't>> for ISubKindTT<'s, 't> {
  fn from(x: &'t KindPlaceholderT<'s, 't>) -> Self {
    ISubKindTT::KindPlaceholder(x)
  }
}
impl<'s, 't> From<&'t KindPlaceholderT<'s, 't>> for ISuperKindTT<'s, 't> {
  fn from(x: &'t KindPlaceholderT<'s, 't>) -> Self {
    ISuperKindTT::KindPlaceholder(x)
  }
}
impl<'s, 't> From<&'t KindPlaceholderT<'s, 't>> for KindT<'s, 't> {
  fn from(x: &'t KindPlaceholderT<'s, 't>) -> Self {
    KindT::KindPlaceholder(x)
  }
}

impl<'s, 't> From<&'t OverloadSetT<'s, 't>> for KindT<'s, 't> {
  fn from(x: &'t OverloadSetT<'s, 't>) -> Self {
    KindT::OverloadSet(x)
  }
}


impl<'s, 't> From<ICitizenTT<'s, 't>> for ISubKindTT<'s, 't> {
  fn from(c: ICitizenTT<'s, 't>) -> Self {
    match c {
      ICitizenTT::Struct(x) => ISubKindTT::Struct(x),
      ICitizenTT::Interface(x) => ISubKindTT::Interface(x),
    }
  }
}
// No `From<ICitizenTT/ISubKindTT/ISuperKindTT> for KindT`: their interface arm produces the
// `RawInterface` union kind, which must be interned. Use `TypingInterner::citizen_to_kind` /
// `sub_kind_to_kind` / `super_kind_to_kind` instead.


impl<'s, 't> TryFrom<KindT<'s, 't>> for ICitizenTT<'s, 't> {
  type Error = ();
  fn try_from(k: KindT<'s, 't>) -> Result<Self, ()> {
    match k {
      KindT::Struct(x) => Ok(ICitizenTT::Struct(x)),
      KindT::RawInterface(x) => Ok(ICitizenTT::Interface(x.inner)),
      _ => Err(()),
    }
  }
}
impl<'s, 't> TryFrom<KindT<'s, 't>> for ISubKindTT<'s, 't> {
  type Error = ();
  fn try_from(k: KindT<'s, 't>) -> Result<Self, ()> {
    match k {
      KindT::Struct(x) => Ok(ISubKindTT::Struct(x)),
      KindT::RawInterface(x) => Ok(ISubKindTT::Interface(x.inner)),
      KindT::KindPlaceholder(x) => Ok(ISubKindTT::KindPlaceholder(x)),
      _ => Err(()),
    }
  }
}
impl<'s, 't> TryFrom<KindT<'s, 't>> for ISuperKindTT<'s, 't> {
  type Error = ();
  fn try_from(k: KindT<'s, 't>) -> Result<Self, ()> {
    match k {
      KindT::RawInterface(x) => Ok(ISuperKindTT::Interface(x.inner)),
      KindT::KindPlaceholder(x) => Ok(ISuperKindTT::KindPlaceholder(x)),
      _ => Err(()),
    }
  }
}
impl<'s, 't> TryFrom<ISubKindTT<'s, 't>> for ICitizenTT<'s, 't> {
  type Error = ();
  fn try_from(s: ISubKindTT<'s, 't>) -> Result<Self, ()> {
    match s {
      ISubKindTT::Struct(x) => Ok(ICitizenTT::Struct(x)),
      ISubKindTT::Interface(x) => Ok(ICitizenTT::Interface(x)),
      _ => Err(()),
    }
  }
}
