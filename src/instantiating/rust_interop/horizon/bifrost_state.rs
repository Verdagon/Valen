use std::cell::RefCell;
use std::collections::HashMap;

use crate::backend_ffi::metal_lowerer::ExternAbi;
use crate::compile_options::GlobalOptions;
use crate::instantiating::ast::ast::FunctionExportI;
use crate::instantiating::ast::citizens::StructDefinitionI;
use crate::instantiating::ast::names::IdI;
use crate::instantiating::instantiating_interner::InstantiatingInterner;
use crate::instantiating::instantiator::InstantiatedOutputsI;
use crate::interner::StrI;
use crate::keywords::Keywords;
use crate::scout_arena::ScoutArena;
use crate::typing::hinputs_t::HinputsT;
use crate::typing::typing_interner::TypingInterner;
use crate::utils::fx::IndexMap;

/// Horizon's cross-pass state: everything bifrost's `BifrostState` holds, plus the two things only
/// horizon's reverse callbacks and opaque crossings need.
///
/// It keeps bifrost's name and constructor because bifrost's `drive` builds it and bifrost's
/// callbacks read it, through the parent `rust_interop/mod.rs`'s build-time switch. Like bifrost's, it
/// is **never `&mut`-borrowed**: rustc holds `&mut Callbacks` for the whole `run_compiler`, so the
/// providers reach this state through a raw pointer (`set_bifrost_state_ptr`) at an object rustc never
/// touches, and everything that changes lives behind a `RefCell`.
pub struct BifrostState<'s, 'ctx, 't, 'i> {
  pub opts: &'ctx GlobalOptions,
  pub interner: &'ctx InstantiatingInterner<'s, 'i>,
  pub typing_interner: &'ctx TypingInterner<'s, 't>,
  pub scout_arena: &'ctx ScoutArena<'s>,
  pub keywords: &'ctx Keywords<'s>,
  pub rust_crates: &'ctx [StrI<'s>],
  pub hinputs: &'ctx RefCell<Option<HinputsT<'s, 't>>>,
  pub monouts: &'ctx RefCell<InstantiatedOutputsI<'s, 't, 'i>>,
  /// The `FunctionExportI` for each export the collector actually walked (demand-driven), retained so
  /// the emit can hand them to `assemble_hinputs`.
  pub function_exports: &'ctx RefCell<Vec<FunctionExportI<'s, 'i>>>,
  /// The rustc-mangled symbol of the `__vale_main` stub instance, captured when the collector walks it,
  /// so the backend emits the entry under the name the final file's `fn main` calls.
  pub entry_symbol: &'ctx RefCell<Option<String>>,
  /// One line per Vale item the providers fired on. Per run rather than global, so parallel driven
  /// tests never race.
  pub firings: &'ctx RefCell<Vec<String>>,
  /// Each Rust leaf's and each callback's boundary ABI, keyed by the humanized prototype name the
  /// metal lowerer uses, so the backend finds it.
  pub extern_abis: &'ctx RefCell<HashMap<String, ExternAbi>>,
  /// Rust→Vale callbacks the collector reached: a Vale trait-impl override that *Rust* calls, not a
  /// Vale export. For each, the backend emits a wrapper under the rustc-mangled symbol that adapts the
  /// Rust ABI and forwards to the internal Vale body.
  pub callbacks: RefCell<Vec<CallbackReq>>,
  /// The typeid→kind universe: every Vale struct/interface instantiated so far, keyed by the
  /// content-addressed typeid its `__ValeOpaque<typeid>` crossing carries (`opaque_typeid`). Appended
  /// after every instantiator drain and read by the `layout_of` override to size a Vale struct rustc
  /// holds by value, and by `collect_callback` to confirm a callback's opaque type args are ones Vale
  /// instantiated. A separate cell from `monouts` on purpose: the ABI queries that re-enter `layout_of`
  /// fire while the resolve loop still holds `monouts` mutably. An `IndexMap`, so iteration is in
  /// instantiation order.
  pub opaque_universe: RefCell<IndexMap<u64, OpaqueKindI<'s, 'i>>>,
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
      callbacks: RefCell::new(Vec::new()),
      opaque_universe: RefCell::new(IndexMap::default()),
    }
  }
}

/// One Rust→Vale callback the backend must emit a wrapper for: `symbol` is the rustc-mangled name
/// Rust's monomorphized call site targets (so the wrapper is the sole definition), and `vale_name` is
/// the humanized name of the internal Vale body to forward to. `vale_name` is also the key its inbound
/// ABI is stored under in `extern_abis`, matching the metal prototype name.
pub struct CallbackReq {
  pub symbol: String,
  pub vale_name: String,
}

/// What a `__ValeOpaque<typeid>` stands for, as recorded in `opaque_universe`. A struct carries its
/// definition so the `layout_of` override can size it from its members without touching `monouts`; an
/// interface has no by-value layout (it crosses only by borrow), so only its identity is kept for the
/// callback presence check.
pub enum OpaqueKindI<'s, 'i> {
  Struct(&'i StructDefinitionI<'s, 'i>),
  Interface(IdI<'s, 'i>),
}
