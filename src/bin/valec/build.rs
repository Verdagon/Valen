use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::Args;

use crate::frontend::{
  ProjectDirectoryDeclaration, ProjectNonValeInputDeclaration, ProjectValeInputDeclaration,
};
use crate::midas;

#[derive(Args, Debug)]
pub struct BuildArgs {
  #[arg(long, default_value = "build")]
  output_dir: PathBuf,

  #[arg(short = 'o', default_value = "main")]
  executable_name: String,

  #[arg(long)]
  builtins_dir_override: Option<PathBuf>,

  #[arg(long)]
  clang_override: Option<String>,

  #[arg(long)]
  libc_override: Option<String>,

  #[arg(long, default_value = "O0")]
  opt_level: String,

  #[arg(long)]
  cpu: Option<String>,

  #[arg(long)]
  gen_size: Option<String>,

  #[arg(long, default_value_t = false)]
  benchmark: bool,

  #[arg(long, default_value_t = false)]
  verbose: bool,

  #[arg(long, default_value_t = false)]
  debug_output: bool,

  #[arg(long, default_value_t = true)]
  include_builtins: bool,

  #[arg(long, default_value_t = true)]
  output_vast: bool,

  #[arg(long, default_value_t = false)]
  reuse_vast: bool,

  #[arg(long, default_value_t = true)]
  run_backend: bool,

  #[arg(long, default_value_t = true)]
  run_clang: bool,

  #[arg(long, default_value_t = true)]
  sanity_check: bool,

  #[arg(long, default_value_t = true)]
  borrow_check: bool,

  #[arg(long, default_value_t = false)]
  no_std: bool,

  #[arg(long, default_value_t = false)]
  flares: bool,

  #[arg(long, default_value_t = false)]
  census: bool,

  #[arg(long, default_value_t = false)]
  asan: bool,

  #[arg(long, default_value_t = false)]
  verify: bool,

  #[arg(short = 'g', default_value_t = false)]
  debug_symbols: bool,

  #[arg(long, default_value_t = false)]
  llvm_ir: bool,

  #[arg(long, default_value_t = true)]
  pic: bool,

  #[arg(long, default_value_t = true)]
  pie: bool,

  #[arg(long)]
  target_triple: Option<String>,

  #[arg(long)]
  sysroot: Option<PathBuf>,

  #[arg(long, default_value_t = true)]
  asm: bool,

  #[arg(long, default_value_t = false)]
  print_mem_overhead: bool,

  #[arg(long, default_value_t = false)]
  use_atomic_rc: bool,

  #[arg(long, default_value_t = true)]
  include_bounds_checks: bool,

  #[arg(trailing_var_arg = true)]
  inputs: Vec<String>,
}

pub fn build_stuff(compiler_dir: &Path, args: BuildArgs) {
  let windows = cfg!(windows);

  let builtins_dir = if let Some(override_path) = args.builtins_dir_override.as_ref() {
    if !override_path.is_dir() {
      eprintln!(
        "Error: --builtins-dir-override's value ({}) is not a directory.",
        override_path.display()
      );
      process::exit(1);
    }
    override_path.clone()
  } else {
    compiler_dir.join("builtins")
  };

  let mut project_directory_declarations = Vec::new();
  let mut project_vale_input_declarations = Vec::new();
  let mut project_non_vale_input_declarations = Vec::new();

  if !args.no_std {
    project_directory_declarations.push(ProjectDirectoryDeclaration {
      project_name: "stdlib".to_string(),
      path: compiler_dir.join("stdlib").join("src"),
    });
  }

  // Parse positional inputs: name=path entries become project declarations
  for input in &args.inputs {
    let Some((project_name, path_str)) = input.split_once('=') else {
      eprintln!("Unrecognized input: {}", input);
      process::exit(1);
    };
    let path = PathBuf::from(path_str);
    let resolved_path = path.canonicalize().unwrap_or_else(|_| path.clone());

    if resolved_path.is_dir() {
      project_directory_declarations.push(ProjectDirectoryDeclaration {
        project_name: project_name.to_string(),
        path: resolved_path,
      });
    } else if resolved_path
      .file_name()
      .and_then(|n| n.to_str())
      .map(|n| n.ends_with(".vale"))
      .unwrap_or(false)
    {
      project_vale_input_declarations.push(ProjectValeInputDeclaration {
        project_name: project_name.to_string(),
        path: resolved_path,
      });
    } else {
      project_non_vale_input_declarations
        .push(ProjectNonValeInputDeclaration { path: resolved_path });
    }
  }

  if !builtins_dir.exists() {
    eprintln!("Cannot find builtins directory: {}", builtins_dir.display());
    process::exit(1);
  }

  if args.verbose {
    println!("Invoking Frontend...");
  }

  // VCOORD: lets remove this soon
  if args.reuse_vast {
    eprintln!("Error: --reuse-vast is no longer supported");
    process::exit(1);
  }

  if args.output_dir.exists() {
    println!("Deleting existing {}.", args.output_dir.display());
    if let Err(e) = fs::remove_dir_all(&args.output_dir) {
      eprintln!("Error removing old dir: {}", e);
      process::exit(1);
    }
    assert!(!args.output_dir.exists(), "Removing old dir {} failed!", args.output_dir.display());
  }

  if let Err(e) = fs::create_dir_all(&args.output_dir) {
    eprintln!("Error creating output directory: {}", e);
    process::exit(1);
  }

  let backend_opts = midas::build_backend_options(
    &args.output_dir,
    Some(args.opt_level.as_str()),
    args.cpu.as_deref(),
    &args.executable_name,
    args.flares,
    args.census,
    args.verify,
    &args.opt_level,
    args.llvm_ir,
    args.asm,
    args.pic,
    args.print_mem_overhead,
    args.use_atomic_rc,
    args.include_bounds_checks,
    args.debug_symbols,
  );

  let extra_inputs: Vec<PathBuf> =
    project_non_vale_input_declarations.iter().map(|d| d.path.clone()).collect();

  let clang_cfg = frontend_rust::pass_manager::pass_manager::ClangConfig {
    builtins_dir: builtins_dir.clone(),
    extra_inputs,
    clang_path: args.clang_override.clone(),
    libc_path: args.libc_override.clone(),
    executable_name: args.executable_name.clone(),
    asan: args.asan,
    debug_symbols: args.debug_symbols,
    pic: args.pic,
    pie: args.pie,
    windows,
    target_triple: args.target_triple.clone(),
    sysroot: args.sysroot.clone(),
  };

  if !args.run_backend {
    println!(
      "Not running backend, stopping here. (Note: --run-backend=false now also skips clang.)"
    );
    return;
  }

  println!("Running frontend + backend + clang in-process...");
  let bp = match crate::frontend::compile_in_process(
    &project_directory_declarations,
    &project_vale_input_declarations,
    &project_non_vale_input_declarations,
    args.benchmark,
    args.sanity_check,
    args.borrow_check,
    args.verbose,
    args.debug_output,
    args.include_builtins,
    &args.output_dir,
    backend_opts,
    clang_cfg,
  ) {
    Ok(result) => result,
    Err(e) => {
      eprintln!("Compilation error: {}", e);
      process::exit(1);
    }
  };

  if bp.rc != 0 {
    eprintln!("Compilation returned error code {}, aborting.", bp.rc);
    process::exit(bp.rc);
  }

  if args.verbose {
    println!("Done! Stems: {:?}", bp.package_stems);
  }
}
