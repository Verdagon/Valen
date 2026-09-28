use crate::typing::rust_interop::RealRustcOracle;
use crate::typing::rust_interop::generate_final_rust_file_source;
use std::cell::RefCell;
use std::sync::Arc;
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler as RustcCompiler;
use rustc_middle::ty::TyCtxt;

use crate::Keywords;
use crate::code_source::CodeSource;
use crate::instantiating::rust_interop::{
  set_bifrost_state_ptr, consumer_fill_modules, vale_override_queries, BifrostState,
};
use crate::parse_arena::ParseArena;
use crate::postparsing::ScoutCompilation;
use crate::interner::StrI;
use crate::typing::compiler::Compiler;
use crate::typing::oracles::Oracles;
use crate::typing::TypingPassOptions;
use crate::utils::code_hierarchy::PackageCoordinate;

#[derive(Clone, Copy)]
pub enum BifrostPhase {
  Typing,
  InstantiatingAndBackend
}

pub struct BifrostRustcCallbacks<'ctx, 's, 't, 'i, 'p> {
  pub state: &'ctx BifrostState<'s, 'ctx, 't, 'i>,

  pub compile_builtins: bool,
  pub borrow_check: bool,

  pub parser_keywords: &'ctx Keywords<'p>,
  pub parser_rust_crates: &'ctx [StrI<'p>],
  pub parse_arena: &'ctx ParseArena<'p>,
  pub package_coord: &'p PackageCoordinate<'p>,
  pub code_source: &'ctx CodeSource<'p>,
  pub typing_error_slot: &'ctx RefCell<Option<String>>,
  pub emit_pass2_stub_into: &'ctx RefCell<Option<String>>,
  pub src_digest: u64, // TODO: revisit

  // Which phase we should be running in this particular run of rustc.
  pub phase: BifrostPhase,
}

// `run_compiler` moves the callbacks onto a thread it spawns but joins that thread before
// returning. It's like structured concurrency.
unsafe impl<'ctx, 's, 't, 'i, 'p> Send for BifrostRustcCallbacks<'ctx, 's, 't, 'i, 'p> {}

impl<'ctx, 's, 't, 'i, 'p> Callbacks for BifrostRustcCallbacks<'ctx, 's, 't, 'i, 'p> {
  fn config(&mut self, config: &mut rustc_interface::Config) {
    config.override_queries = Some(vale_override_queries);
    // Set the hook that lets us smuggle LLVM modules into rustc's optimization/linking pipeline
    rustc_codegen_llvm::set_fill_extra_modules_hook(consumer_fill_modules);
  }

  fn after_expansion<'tcx>(&mut self, _compiler: &RustcCompiler, tcx: TyCtxt<'tcx>) -> Compilation {
    match self.phase {
      BifrostPhase::Typing => {
        // Parsing + Postparsing
        let mut package_coords_for_scout = Vec::new();
        if self.compile_builtins {
          package_coords_for_scout.push(PackageCoordinate::builtin(self.parse_arena, self.parser_keywords));
        }
        package_coords_for_scout.push(self.package_coord);
        let mut postparser = ScoutCompilation::new(
          self.state.scout_arena,
          self.state.keywords,
          self.parser_keywords,
          self.parser_rust_crates,
          self.parse_arena,
          package_coords_for_scout,
          self.code_source,
          (*self.state.opts).clone(),
        );
        // TODO: Lazily resolve imports and add RustItem to the oracle, which means we don't have
        // to do this collect_rust_import_paths here.
        let mut rust_imports = Vec::new();
        if let Ok(scoutput) = postparser.get_scoutput() {
          for program in scoutput.file_coord_to_contents.values() {
            for imp in program.imports {
              if self.state.rust_crates.contains(&imp.module_name) {
                rust_imports.push(*imp);
              }
            }
          }
        }

        // Typechecking
        let real =
            RealRustcOracle::new(tcx, self.state.scout_arena, self.state.keywords, &rust_imports);
        let mut global_options = (*self.state.opts).clone();
        global_options.borrow_checker_enabled = self.borrow_check;
        let options = TypingPassOptions {
          global_options,
          debug_out: Arc::new(|x: &str| println!("{}", x)),
          tree_shaking_enabled: true,
        };
        let code_map = postparser.get_code_map().expect("getCodeMap failed");
        let postparseds = postparser.expect_scoutput();
        let compiler = Compiler::new(
          self.state.scout_arena,
          &self.state.typing_interner,
          self.state.keywords,
          self.state.rust_crates,
          &options,
          Oracles::with_rust(&real),
        );

        let coutputs = match compiler.evaluate(&code_map, postparseds) {
          Ok((hinputs, coutputs)) => {
            *self.state.hinputs.borrow_mut() = Some(hinputs);
            coutputs
          }
          Err(err) => {
            *self.typing_error_slot.borrow_mut() = Some(format!("{err:?}"));
            return Compilation::Stop;
          }
        };

        let borrowed = self.state.hinputs.borrow();
        let hinputs = borrowed.as_ref().expect("missing hinputs");
        let final_rust_file_source =
            match generate_final_rust_file_source(
              hinputs,
              &coutputs,
              self.state.typing_interner,
              self.state.rust_crates,
              &real,
              self.src_digest,
            ) {
              Ok(stub) => stub,
              Err(e) => {
                *self.typing_error_slot.borrow_mut() =
                    Some(format!("pass-2 stub generation failed: {:?}", e));
                return Compilation::Stop;
              }
            };
        *self.emit_pass2_stub_into.borrow_mut() = Some(final_rust_file_source);
        // Stop the rustc invocation here, so we can figure out the final rust source and re-invoke
        // rustc for the InstantiatingAndBackend phase.
        Compilation::Stop
      }
      BifrostPhase::InstantiatingAndBackend => {
        // We set this here because set_bifrost_state_ptr must be called on the same thread as those
        // accessing the driver state.
        set_bifrost_state_ptr(self.state as *const _ as *const ());
        // Continue on with the instantiator and backend
        Compilation::Continue
      }
    }
  }
}
