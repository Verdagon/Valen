use std::cell::RefCell;
use crate::typing::rust_interop::bifrost::dir_structure::final_file_path_in;
use crate::typing::rust_interop::bifrost::dir_structure::importer_file_path_in;
use crate::typing::rust_interop::bifrost::dir_structure::stubs_dir_in;
use std::path::Path;
use std::fs;
use bumpalo::Bump;
use rustc_driver::Callbacks;
use crate::code_source::{CodeSource, Source};
use crate::compile_options::GlobalOptions;
use crate::Keywords;
use crate::parse_arena::ParseArena;
use crate::postparsing::ScoutCompilation;
use crate::scout_arena::ScoutArena;
use crate::typing::hinputs_t::HinputsT;
use crate::utils::code_hierarchy::*;
use crate::typing::rust_interop::bifrost::generate_importer_file::phase0_generate_importer_source;
use crate::typing::rust_interop::bifrost::rustc_args::crate_name_from_rustc_args;
use crate::typing::rust_interop::bifrost::rustc_args::crate_type_from_rustc_args;
use crate::typing::rust_interop::bifrost::rustc_args::out_dir_from_rustc_args;
use crate::typing::rust_interop::bifrost::rustc_args::rust_crate_names_from_rustc_args;
use crate::typing::rust_interop::bifrost::callbacks::{BifrostPhase, BifrostRustcCallbacks};
use crate::typing::typing_interner::TypingInterner;
use crate::typing::rust_interop::bifrost::error::ValenError;
use std::collections::HashMap;
use crate::backend_ffi::metal_lowerer::ExternAbi;
use crate::instantiating::ast::ast::FunctionExportI;
use crate::instantiating::instantiating_interner::InstantiatingInterner;
use crate::instantiating::instantiator::InstantiatedOutputsI;
use crate::instantiating::rust_interop::BifrostState;
use crate::typing::rust_interop::source_digest;

pub struct ValenInputs {
  pub rustc_args: Vec<String>,
  pub borrow_check: bool,
}

struct NormalRustcCallbacks;
impl Callbacks for NormalRustcCallbacks {}

pub fn drive(
  inputs: &ValenInputs,
  compile_builtins: bool,
  // Run after the typing pass. Used by tests.
  after_typing: impl Fn(&str, &HinputsT, &str),
  // Run after codegen. Used by tests.
  after_codegen: impl Fn(&[String]),
) -> Result<(bool, i32), ValenError> {
  let mut valen_source_indices: Vec<usize> = Vec::new();
  for (index, arg) in inputs.rustc_args.iter().enumerate() {
    if arg.ends_with(".valen") {
      valen_source_indices.push(index);
    }
  }
  if valen_source_indices.is_empty() {
    let mut callbacks = NormalRustcCallbacks {};
    let rustc_exit = rustc_driver::catch_with_exit_code(|| {
      rustc_driver::run_compiler(&inputs.rustc_args, &mut callbacks);
    });
    return Ok((false, rustc_exit));
  }
  if valen_source_indices.len() > 1 {
    unimplemented!();
  }
  let valen_source_index = valen_source_indices[0];
  let valen_source_path = Path::new(&inputs.rustc_args[valen_source_index]);
  let valen_source_text = fs::read_to_string(valen_source_path)
      .unwrap_or_else(|e| panic!("could not read {}: {e}", valen_source_path.display()));
  let valen_file_name =
      valen_source_path.file_name().and_then(|n| n.to_str())
          .unwrap_or_else(|| panic!("valen source path has no file name: {}", valen_source_path.display()));

  let crate_name = crate_name_from_rustc_args(&inputs.rustc_args);
  let crate_type = crate_type_from_rustc_args(&inputs.rustc_args);
  let out_dir = out_dir_from_rustc_args(&inputs.rustc_args);

  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let rust_crate_names = rust_crate_names_from_rustc_args(&inputs.rustc_args);
  let rust_crates = scout_arena.alloc_slice_from_vec(
    rust_crate_names.iter().map(|name| scout_arena.intern_str(name)).collect(),
  );
  let parser_rust_crates = parse_arena.alloc_slice_from_vec(
    rust_crate_names.iter().map(|name| parse_arena.intern_str(name)).collect(),
  );

  let package_coord =
      parse_arena.intern_package_coordinate(parse_arena.intern_str(&crate_name), &[]);

  let mut files = FileCoordinateMap::<String>::new();
  files.put(parse_arena.intern_file_coordinate(package_coord, valen_file_name), valen_source_text.clone());
  let mut sources = Vec::new();
  if compile_builtins {
    sources.push(Source::builtins(&parse_arena, &parser_keywords));
  }
  sources.push(Source::from_code_map(&files));
  let code_source = CodeSource::new(sources);

  let src_digest = source_digest(&valen_source_text);
  let stubs_dir = stubs_dir_in(&out_dir, &crate_name);
  fs::create_dir_all(&stubs_dir)
      .unwrap_or_else(|e| panic!("could not create {}: {e}", stubs_dir.display()));
  let importer_file_path = importer_file_path_in(&out_dir, &crate_name);
  let importer_source = phase0_generate_importer_source(&rust_crate_names);
  fs::write(&importer_file_path, &importer_source)
      .unwrap_or_else(|e| panic!("could not write {}: {e}", importer_file_path.display()));
  let mut pass1_args = inputs.rustc_args.clone();
  pass1_args[valen_source_index] = importer_file_path.display().to_string();

  let typing_bump = Bump::new();
  let instantiating_bump = Bump::new();
  let typing_interner = TypingInterner::new(&typing_bump);
  let instantiating_interner = InstantiatingInterner::new(&instantiating_bump);
  let hinputs_slot: RefCell<Option<HinputsT>> = RefCell::new(None);
  let typing_error_slot: RefCell<Option<String>> = RefCell::new(None);
  let monouts_slot: RefCell<InstantiatedOutputsI> = RefCell::new(InstantiatedOutputsI::new());
  let function_exports_slot: RefCell<Vec<FunctionExportI>> = RefCell::new(Vec::new());
  let entry_symbol_slot: RefCell<Option<String>> = RefCell::new(None);
  let firings_slot: RefCell<Vec<String>> = RefCell::new(Vec::new());
  let extern_abis_slot: RefCell<HashMap<String, ExternAbi>> = RefCell::new(HashMap::new());
  let final_rust_file_slot: RefCell<Option<String>> = RefCell::new(None);

  let global_options = GlobalOptions {
    sanity_check: true,
    use_overload_index: true,
    use_optimized_solver: true,
    verbose_errors: true,
    debug_output: true,
    borrow_checker_enabled: true,
  };
  let state = BifrostState {
    opts: &global_options,
    interner: &instantiating_interner,
    typing_interner: &typing_interner,
    scout_arena: &scout_arena,
    keywords: &keywords,
    rust_crates: &rust_crates,
    hinputs: &hinputs_slot,
    monouts: &monouts_slot,
    function_exports: &function_exports_slot,
    entry_symbol: &entry_symbol_slot,
    firings: &firings_slot,
    extern_abis: &extern_abis_slot,
  };
  let mut pass1_callbacks = BifrostRustcCallbacks {
    state: &state,
    compile_builtins,
    borrow_check: inputs.borrow_check,
    parser_keywords: &parser_keywords,
    parser_rust_crates: &parser_rust_crates,
    parse_arena: &parse_arena,
    package_coord,
    code_source: &code_source,
    typing_error_slot: &typing_error_slot,
    emit_pass2_stub_into: &final_rust_file_slot,
    src_digest,
    phase: BifrostPhase::Typing,
  };
  let pass1_exit = rustc_driver::catch_with_exit_code(|| {
    rustc_driver::run_compiler(&pass1_args, &mut pass1_callbacks);
  });
  fs::remove_file(&importer_file_path)
      .unwrap_or_else(|e| panic!("could not delete {}: {e}", importer_file_path.display()));
  if let Some(typing_error) = typing_error_slot.borrow().as_ref() {
    return Err(ValenError::TypingFailed(typing_error.clone()));
  }
  if pass1_exit != 0 {
    return Ok((true, pass1_exit));
  }

  let final_rust_file_text =
      final_rust_file_slot.borrow_mut().take().expect("pass 1 produced no final rust file");
  {
    let hinputs = hinputs_slot.borrow();
    after_typing(
      &importer_source,
      hinputs.as_ref().expect("pass 1 produced no hinputs"),
      &final_rust_file_text);
  }
  let final_file_path = final_file_path_in(&out_dir, &crate_name, &crate_type);
  fs::write(&final_file_path, final_rust_file_text)
      .unwrap_or_else(|e| panic!("could not write {}: {e}", final_file_path.display()));
  let mut pass2_args = inputs.rustc_args.clone();
  pass2_args[valen_source_index] = final_file_path.display().to_string();

  let mut pass2_callbacks = BifrostRustcCallbacks {
    state: &state,
    compile_builtins,
    borrow_check: inputs.borrow_check,
    parser_keywords: &parser_keywords,
    parser_rust_crates: &parser_rust_crates,
    parse_arena: &parse_arena,
    package_coord,
    code_source: &code_source,
    typing_error_slot: &typing_error_slot,
    emit_pass2_stub_into: &final_rust_file_slot,
    src_digest,
    phase: BifrostPhase::InstantiatingAndBackend,
  };
  let pass2_exit = rustc_driver::catch_with_exit_code(|| {
    rustc_driver::run_compiler(&pass2_args, &mut pass2_callbacks);
  });
  after_codegen(&firings_slot.borrow());
  Ok((true, pass2_exit))
}
