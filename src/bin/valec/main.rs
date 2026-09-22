#[cfg(not(feature = "no_backend"))]
mod build;
#[cfg(not(feature = "no_backend"))]
mod frontend;
#[cfg(not(feature = "no_backend"))]
mod midas;

#[cfg(not(feature = "no_backend"))]
use std::env;
#[cfg(not(feature = "no_backend"))]
use std::path::PathBuf;
#[cfg(not(feature = "no_backend"))]
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "valec",
    version,
    about = "The Vale compiler.",
    long_about = None,
)]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  #[cfg(not(feature = "no_backend"))]
  Build(build::BuildArgs),
  Version,
}

fn main() {
  let cli = Cli::parse();

  #[cfg(not(feature = "no_backend"))]
  let compiler_dir: PathBuf = env::current_exe()
    .and_then(|p| p.canonicalize())
    .ok()
    .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    .unwrap_or_else(|| {
      eprintln!("Could not determine compiler install directory.");
      process::exit(1);
    });

  match cli.command {
    #[cfg(not(feature = "no_backend"))]
    Command::Build(args) => {
      build::build_stuff(&compiler_dir, args);
    }
    Command::Version => {
      println!("valec {}", env!("CARGO_PKG_VERSION"));
    }
  }
}
