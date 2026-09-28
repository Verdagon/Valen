use std::cell::RefCell;
use std::collections::HashMap;

use crate::backend_ffi::metal_lowerer::ExternAbi;
use crate::compile_options::GlobalOptions;
use crate::instantiating::ast::ast::FunctionExportI;
use crate::instantiating::instantiating_interner::InstantiatingInterner;
use crate::instantiating::instantiator::InstantiatedOutputsI;
use crate::interner::StrI;
use crate::keywords::Keywords;
use crate::scout_arena::ScoutArena;
use crate::typing::hinputs_t::HinputsT;
use crate::typing::typing_interner::TypingInterner;

pub struct BifrostState<'s, 'ctx, 't, 'i> {
  pub opts: &'ctx GlobalOptions,
  pub interner: &'ctx InstantiatingInterner<'s, 'i>,
  pub typing_interner: &'ctx TypingInterner<'s, 't>,
  pub scout_arena: &'ctx ScoutArena<'s>,
  pub keywords: &'ctx Keywords<'s>,
  pub rust_crates: &'ctx [StrI<'s>],
  pub hinputs: &'ctx RefCell<Option<HinputsT<'s, 't>>>,
  pub monouts: &'ctx RefCell<InstantiatedOutputsI<'s, 't, 'i>>,
  pub function_exports: &'ctx RefCell<Vec<FunctionExportI<'s, 'i>>>,
  pub entry_symbol: &'ctx RefCell<Option<String>>,
  pub firings: &'ctx RefCell<Vec<String>>,
  pub extern_abis: &'ctx RefCell<HashMap<String, ExternAbi>>,
}

impl<'s, 'ctx, 't, 'i> BifrostState<'s, 'ctx, 't, 'i> {
  pub fn new(
    opts: &'ctx GlobalOptions,
    interner: &'ctx InstantiatingInterner<'s, 'i>,
    typing_interner: &'ctx TypingInterner<'s, 't>,
    scout_arena: &'ctx ScoutArena<'s>,
    keywords: &'ctx Keywords<'s>,
    rust_crates: &'ctx [StrI<'s>],
    hinputs: &'ctx RefCell<Option<HinputsT<'s, 't>>>,
    monouts: &'ctx RefCell<InstantiatedOutputsI<'s, 't, 'i>>,
    function_exports: &'ctx RefCell<Vec<FunctionExportI<'s, 'i>>>,
    entry_symbol: &'ctx RefCell<Option<String>>,
    firings: &'ctx RefCell<Vec<String>>,
    extern_abis: &'ctx RefCell<HashMap<String, ExternAbi>>,
  ) -> Self {
    BifrostState {
      opts,
      interner,
      typing_interner,
      scout_arena,
      keywords,
      rust_crates,
      hinputs,
      monouts,
      function_exports,
      entry_symbol,
      firings,
      extern_abis,
    }
  }
}
