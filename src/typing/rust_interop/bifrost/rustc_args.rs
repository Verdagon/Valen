use std::path::{Path, PathBuf};

pub fn crate_name_from_rustc_args(args: &[String]) -> String {
  flag_value(args, "--crate-name").to_string()
}

pub fn crate_type_from_rustc_args(args: &[String]) -> String {
  flag_value(args, "--crate-type").to_string()
}

pub fn out_dir_from_rustc_args(args: &[String]) -> PathBuf {
  Path::new(flag_value(args, "--out-dir")).to_path_buf()
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> &'a str {
  let mut index = 0;
  while index < args.len() {
    let arg = &args[index];
    if let Some(value) = arg.strip_prefix(flag).and_then(|rest| rest.strip_prefix('=')) {
      return value;
    }
    if arg == flag {
      if let Some(value) = args.get(index + 1) {
        return value;
      }
    }
    index += 1;
  }
  panic!("rustc args carry no `{flag}`: {args:?}");
}

pub fn rust_crate_names_from_rustc_args(args: &[String]) -> Vec<String> {
  let mut names: Vec<String> = vec!["std".to_string(), "core".to_string()];
  let mut specs: Vec<&str> = Vec::new();
  let mut iter = args.iter();
  while let Some(arg) = iter.next() {
    if let Some(spec) = arg.strip_prefix("--extern=") {
      specs.push(spec);
    } else if arg == "--extern" {
      if let Some(spec) = iter.next() {
        specs.push(spec);
      }
    }
  }
  for spec in specs {
    let name_with_opts = spec.split_once('=').map(|(name, _path)| name).unwrap_or(spec);
    let name = name_with_opts.rsplit(':').next().unwrap_or(name_with_opts);
    if !names.iter().any(|existing| existing == name) {
      names.push(name.to_string());
    }
  }
  names
}
