use crate::interner::StrI;
use crate::parsing::ast::SharednessP;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RangeL(i32, i32);

impl RangeL {
  pub fn new(begin: i32, end: i32) -> Self {
    assert!(begin == end || begin <= end);
    RangeL(begin, end)
  }

  pub fn zero() -> Self {
    RangeL(0, 0)
  }

  pub fn begin(&self) -> i32 {
    self.0
  }

  pub fn end(&self) -> i32 {
    self.1
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FileL<'p> {
  pub denizens: &'p [IDenizenL<'p>],
  pub comment_ranges: &'p [RangeL],
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum IDenizenL<'p> {
  TopLevelFunction(FunctionL<'p>),
  TopLevelStruct(StructL<'p>),
  TopLevelInterface(InterfaceL<'p>),
  TopLevelImpl(ImplL<'p>),
  TopLevelExportAs(ExportAsL<'p>),
  TopLevelImport(ImportL<'p>),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ImplL<'p> {
  pub range: RangeL,
  pub identifying_runes: Option<AngledLE<'p>>,
  pub template_rules: Option<ScrambleLE<'p>>,
  pub struct_: Option<ScrambleLE<'p>>, // Option because we can say `impl MyInterface;` inside a struct
  pub interface: ScrambleLE<'p>,
  pub attributes: &'p [IAttributeL<'p>],
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ExportAsL<'p> {
  pub range: RangeL,
  pub contents: ScrambleLE<'p>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ImportL<'p> {
  pub range: RangeL,
  pub module_name: WordLE<'p>,
  pub package_steps: &'p [WordLE<'p>],
  pub importee_name: WordLE<'p>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StructL<'p> {
  pub range: RangeL,
  pub name: WordLE<'p>,
  pub attributes: &'p [IAttributeL<'p>],
  pub sharedness: SharednessP,
  pub identifying_runes: Option<AngledLE<'p>>,
  pub template_rules: Option<ScrambleLE<'p>>,
  pub contents_range: RangeL,
  pub members: &'p [ScrambleLE<'p>],
  pub methods: &'p [FunctionL<'p>],
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct InterfaceL<'p> {
  pub range: RangeL,
  pub name: WordLE<'p>,
  pub attributes: &'p [IAttributeL<'p>],
  pub sharedness: SharednessP,
  pub maybe_identifying_runes: Option<AngledLE<'p>>,
  pub template_rules: Option<ScrambleLE<'p>>,
  pub body_range: RangeL,
  pub members: &'p [FunctionL<'p>],
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum IAttributeL<'p> {
  AbstractAttribute(RangeL),
  ExportAttribute(RangeL),
  ExternAttribute { range: RangeL, maybe_custom_name: Option<ParendLE<'p>> },
  SealedAttribute(RangeL),
  MacroCall { range: RangeL, inclusion: IMacroInclusionL, name: WordLE<'p> },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IMacroInclusionL {
  CallMacro,
  DontCallMacro,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FunctionL<'p> {
  pub range: RangeL,
  pub header: FunctionHeaderL<'p>,
  pub body: Option<FunctionBodyL<'p>>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FunctionBodyL<'p> {
  pub body: CurliedLE<'p>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FunctionHeaderL<'p> {
  pub range: RangeL,
  pub name: WordLE<'p>,
  pub attributes: &'p [IAttributeL<'p>],
  pub maybe_user_specified_identifying_runes: Option<AngledLE<'p>>,
  pub params: ParendLE<'p>,
  pub trailing_details: ScrambleLE<'p>,
}

pub trait INodeLE {
  fn range(&self) -> RangeL;
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScrambleLE<'p> {
  pub range: RangeL,
  pub elements: &'p [&'p INodeLEEnum<'p>],
}
impl INodeLE for ScrambleLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum INodeLEEnum<'p> {
  Parend(ParendLE<'p>),
  Curlied(CurliedLE<'p>),
  Squared(SquaredLE<'p>),
  Angled(AngledLE<'p>),
  Word(WordLE<'p>),
  Symbol(SymbolLE),
  String(StringLE<'p>),
  ParsedInteger(ParsedIntegerLE),
  ParsedDouble(ParsedDoubleLE),
  Scramble(ScrambleLE<'p>),
}

impl INodeLE for INodeLEEnum<'_> {
  fn range(&self) -> RangeL {
    match self {
      INodeLEEnum::Parend(x) => x.range,
      INodeLEEnum::Curlied(x) => x.range,
      INodeLEEnum::Squared(x) => x.range,
      INodeLEEnum::Angled(x) => x.range,
      INodeLEEnum::Word(x) => x.range,
      INodeLEEnum::Symbol(x) => x.range(),
      INodeLEEnum::String(x) => x.range,
      INodeLEEnum::ParsedInteger(x) => x.range,
      INodeLEEnum::ParsedDouble(x) => x.range,
      INodeLEEnum::Scramble(x) => x.range,
    }
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParendLE<'p> {
  pub range: RangeL,
  pub contents: ScrambleLE<'p>,
}
impl INodeLE for ParendLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AngledLE<'p> {
  pub range: RangeL,
  pub contents: ScrambleLE<'p>,
}
impl INodeLE for AngledLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SquaredLE<'p> {
  pub range: RangeL,
  pub contents: ScrambleLE<'p>,
}

impl INodeLE for SquaredLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CurliedLE<'p> {
  pub range: RangeL,
  pub contents: ScrambleLE<'p>,
}

impl INodeLE for CurliedLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WordLE<'p> {
  pub range: RangeL,
  pub str: StrI<'p>,
}
impl INodeLE for WordLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SymbolLE(pub RangeL, pub char);

impl SymbolLE {
  pub fn range(&self) -> RangeL {
    self.0
  }

  pub fn c(&self) -> char {
    self.1
  }
}

impl INodeLE for SymbolLE {
  fn range(&self) -> RangeL {
    self.0
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StringLE<'p> {
  pub range: RangeL,
  pub parts: &'p [StringPart<'p>],
}

impl INodeLE for StringLE<'_> {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringPart<'p> {
  Literal { range: RangeL, s: StrI<'p> },
  Expr(ScrambleLE<'p>),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParsedIntegerLE {
  pub range: RangeL,
  pub value: i64,
  pub bits: Option<i64>,
}

impl INodeLE for ParsedIntegerLE {
  fn range(&self) -> RangeL {
    self.range
  }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParsedDoubleLE {
  pub range: RangeL,
  pub value: f64,
  pub bits: Option<i64>,
}

impl INodeLE for ParsedDoubleLE {
  fn range(&self) -> RangeL {
    self.range
  }
}
