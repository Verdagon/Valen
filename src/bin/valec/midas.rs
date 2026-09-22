use std::path::Path;

use frontend_rust::backend_ffi::{
  BackendCompileOptions, BACKEND_OPT_LEVEL_O0, BACKEND_OPT_LEVEL_O1, BACKEND_OPT_LEVEL_O2,
  BACKEND_OPT_LEVEL_O2I, BACKEND_OPT_LEVEL_O3,
};

#[allow(clippy::too_many_arguments)]
pub fn build_backend_options(
  output_dir: &Path,
  maybe_opt_level: Option<&str>,
  maybe_cpu: Option<&str>,
  _executable_name: &str,
  flares: bool,
  census: bool,
  verify: bool,
  opt_level: &str,
  llvm_ir: bool,
  asm: bool,
  pic: bool,
  print_mem_overhead: bool,
  use_atomic_rc: bool,
  include_bounds_checks: bool,
  debug: bool,
) -> BackendCompileOptions {
  let mut opts = BackendCompileOptions::default();
  opts.output_dir = output_dir.display().to_string();
  opts.verify = verify;
  opts.flares = flares;
  opts.census = census;
  opts.print_llvmir = llvm_ir;
  opts.print_asm = asm;
  opts.pic = pic;
  opts.print_mem_overhead = print_mem_overhead;
  opts.use_atomic_rc = use_atomic_rc;
  opts.include_bounds_checks = include_bounds_checks;
  opts.debug = debug;

  if let Some(cpu) = maybe_cpu {
    opts.cpu = cpu.to_string();
  }

  let level_str = maybe_opt_level.unwrap_or(opt_level);
  opts.opt_level = match level_str {
    "O0" => BACKEND_OPT_LEVEL_O0,
    "O1" => BACKEND_OPT_LEVEL_O1,
    "O2" => BACKEND_OPT_LEVEL_O2,
    "O2i" => BACKEND_OPT_LEVEL_O2I,
    "O3" => BACKEND_OPT_LEVEL_O3,
    other => panic!("Unknown opt_level: {}", other),
  };

  opts
}
