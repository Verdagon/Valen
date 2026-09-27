#![feature(rustc_private)]

use std::env;
use std::path::PathBuf;
use std::process::exit;

use frontend_rust::typing::rust_interop::valen_build_dir_of;
use frontend_rust::typing::rust_interop::{run_build, BuildInputs};

fn main() {
  let argv: Vec<String> = env::args().collect();
  if argv.get(1).map(String::as_str) != Some("build") {
    eprintln!("usage: valen build [--manifest-path <Valen.toml>] [--no-borrow-check]");
    exit(2);
  }

  let mut manifest_path = PathBuf::from("Valen.toml");
  let mut borrow_check = true;
  let mut rest = argv.iter().skip(2);
  while let Some(arg) = rest.next() {
    match arg.as_str() {
      "--manifest-path" => manifest_path = PathBuf::from(rest.next().cloned().unwrap_or_default()),
      "--no-borrow-check" => borrow_check = false,
      other => {
        eprintln!("valen: unknown argument {other}");
        exit(2);
      }
    }
  }

  let project_dir =
    manifest_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
  let build_dir = valen_build_dir_of(&project_dir);
  let valenc_rs = env::current_exe()
    .ok()
    .and_then(|exe| exe.parent().map(|dir| dir.join("valenc-rs")))
    .unwrap_or_else(|| PathBuf::from("valenc-rs"));

  match run_build(&BuildInputs {
    manifest_path,
    build_dir,
    valenc_rs,
    clear_incremental: false,
    borrow_check,
  }) {
    Ok(code) => exit(code),
    Err(e) => {
      eprintln!("valen: {e}");
      exit(1);
    }
  }
}
