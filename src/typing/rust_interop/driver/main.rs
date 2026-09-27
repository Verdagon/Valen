#![feature(rustc_private)]

use std::process::exit;
use frontend_rust::typing::rust_interop::{get_env_borrow_check, get_env_rustc_and_sysroot_locations};

use frontend_rust::typing::rust_interop::{
  drive, ValenInputs,
};

fn main() {
  let borrow_check =
      match get_env_borrow_check() {
        Ok(on) => on,
        Err(e) => {
          eprintln!("valenc-rs: {e}");
          exit(2);
        }
      };
  let (_rustc_location, sysroot_location) =
      get_env_rustc_and_sysroot_locations();

  let mut rustc_args: Vec<String> = std::env::args().collect();
  assert!(!rustc_args.is_empty()); // rustc_args.push("valenc-rs".to_string());
  rustc_args.remove(1); // Remove the valenc-rs binary path arg at the beginning

  if !rustc_args.iter().any(|a| a == "--sysroot" || a.starts_with("--sysroot=")) {
    rustc_args.push(format!("--sysroot={}", sysroot_location));
  }

  let valen_inputs = ValenInputs { rustc_args, borrow_check };
  match drive(&valen_inputs, true, |_, _, _| {}, |_| {}) {
    Ok((_drove_valen, rustc_exit)) => exit(rustc_exit),
    Err(e) => {
      eprintln!("valenc-rs: {}", e.to_string());
      exit(1);
    }
  }
}
