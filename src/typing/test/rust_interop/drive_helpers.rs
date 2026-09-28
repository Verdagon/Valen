use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::typing::hinputs_t::HinputsT;
use crate::typing::rust_interop::{
  copy_dir_recursive, deps_dir_of, drive, get_env_rustc_and_sysroot_locations, src_dir_of,
  ValenInputs,
};
use crate::typing::test::rust_interop::cargo_mimic::mimic_compile_dependency_rust_lib;
use crate::typing::test::rust_interop::test_setup::setup_test_temp_dir;

pub(super) struct DriveRun {
  pub(super) drove_valen: bool,
  pub(super) rustc_exit: i32,
  pub(super) firings: Vec<String>,
}

pub(super) struct Scratch {
  root: TempDir,
}

impl Scratch {
  pub(super) fn new() -> Scratch {
    let root = TempDir::new().expect("could not create scratch dir");
    fs::create_dir_all(root.path().join("deps")).expect("could not create the deps dir");
    Scratch { root }
  }

  pub(super) fn out_dir(&self) -> PathBuf {
    self.root.path().join("deps")
  }
}

fn prog_rustc_args(out_dir: &Path, valen: &Path, extra: Vec<String>) -> Vec<String> {
  let (_rustc_location, sysroot) = get_env_rustc_and_sysroot_locations();
  let mut rustc_args = vec![
    "valenc-rs".to_string(),
    valen.display().to_string(),
    "--crate-type=bin".to_string(),
    "--crate-name=prog".to_string(),
    "--edition=2021".to_string(),
    format!("--sysroot={sysroot}"),
    format!("--out-dir={}", out_dir.display()),
  ];
  rustc_args.extend(extra);
  rustc_args
}

pub(super) fn drive_prog(
  out_dir: &Path,
  vale_source: &str,
  extra: Vec<String>,
  borrow_check: bool,
  after_typing: impl Fn(&str),
) -> Result<DriveRun, String> {
  let valen = out_dir.join("prog.valen");
  fs::write(&valen, vale_source).expect("could not write prog.valen");
  let rustc_args = prog_rustc_args(out_dir, &valen, extra);
  let firings: RefCell<Vec<String>> = RefCell::new(Vec::new());
  let (drove_valen, rustc_exit) = drive(
    &ValenInputs { rustc_args, borrow_check, stop_after_typing: false },
    true,
    |_importer_file_text, _hinputs, final_rust_file_text| after_typing(final_rust_file_text),
    |fired| *firings.borrow_mut() = fired.to_vec(),
  )
  .map_err(|err| err.to_string())?;
  Ok(DriveRun { drove_valen, rustc_exit, firings: firings.into_inner() })
}

pub(super) fn run_produced_binary(out_dir: &Path) -> i32 {
  let exe = out_dir.join("prog");
  let output = Command::new(&exe).output().expect("could not run the produced binary");
  output.status.code().unwrap_or(-1)
}

pub(super) fn drive_and_run_binary(
  out_dir: &Path,
  vale_source: &str,
  extra: Vec<String>,
  borrow_check: bool,
) -> i32 {
  let result = drive_prog(out_dir, vale_source, extra, borrow_check, |_| {}).expect("drive should succeed");
  assert_eq!(result.rustc_exit, 0, "firings: {:?}", result.firings);
  assert!(result.drove_valen, "a .valen crate must drive the Valen engine");
  run_produced_binary(out_dir)
}

pub fn fixtures_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/typing/test/rust_interop/fixtures")
}

struct FixtureProject {
  _temp_dir: TempDir,
  valen_project_dir: PathBuf,
  crate_names: Vec<String>,
  sysroot: String,
}

impl FixtureProject {
  fn new(fixture: &str) -> FixtureProject {
    let (rustc_location, sysroot) = get_env_rustc_and_sysroot_locations();
    let temp_dir = TempDir::new().expect("could not create scratch dir");
    let valen_project_dir = temp_dir.path().join("testvalenproj");
    setup_test_temp_dir(&valen_project_dir);

    let fixture_dir = fixtures_dir().join(fixture);
    let mut crate_names: Vec<String> = fs::read_dir(&fixture_dir)
      .unwrap_or_else(|e| panic!("could not read fixture {}: {e}", fixture_dir.display()))
      .map(|entry| entry.expect("could not read a fixture entry").file_name())
      .map(|name| name.into_string().expect("fixture crate names are utf8"))
      .collect();
    crate_names.sort();
    for crate_name in &crate_names {
      copy_dir_recursive(
        &fixture_dir.join(crate_name),
        &temp_dir.path().join(crate_name),
      )
      .expect("could not copy a fixture crate");
      mimic_compile_dependency_rust_lib(temp_dir.path(), &valen_project_dir, &rustc_location, crate_name);
    }
    FixtureProject { _temp_dir: temp_dir, valen_project_dir, crate_names, sysroot }
  }

  fn rustc_args(&self, input: &Path, crate_type: &str) -> Vec<String> {
    let deps_dir = deps_dir_of(&self.valen_project_dir);
    let mut rustc_args: Vec<String> = vec![
      "valec-rs".to_string(),
      input.display().to_string(),
      format!("--crate-type={crate_type}"),
      "--crate-name=stub".to_string(),
      "--edition=2021".to_string(),
      format!("--sysroot={}", self.sysroot),
      format!("-L{}", deps_dir.display()),
      format!("--out-dir={}", deps_dir.display()),
    ];
    for crate_name in &self.crate_names {
      rustc_args.push(format!(
        "--extern={crate_name}={}",
        deps_dir.join(format!("lib{crate_name}.rlib")).display()
      ));
    }
    rustc_args
  }
}

#[derive(Clone, Debug)]
pub struct CompileFailure {
  pub variant: String,
  pub detail: String,
}

impl CompileFailure {
  fn from_debug(rendered: String) -> CompileFailure {
    let variant =
      rendered.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>();
    CompileFailure { variant, detail: rendered }
  }

  pub fn is(&self, variant: &str) -> bool {
    self.variant == variant
  }
}

pub struct TypecheckOutcome<R> {
  pub compiled: Result<R, CompileFailure>,
}

impl<R> TypecheckOutcome<R> {
  pub fn expect_compiled(&self) -> &R {
    match &self.compiled {
      Ok(r) => r,
      Err(e) => panic!(
        "expected the Vale program to typecheck, but it failed with {}:\n{}",
        e.variant, e.detail
      ),
    }
  }

  pub fn expect_failure(&self) -> &CompileFailure {
    match &self.compiled {
      Err(e) => e,
      Ok(_) => panic!("expected the Vale program to fail, but it typechecked"),
    }
  }
}

fn run_typecheck<R>(
  fixture: &str,
  vale: &str,
  borrow_check: bool,
  compile_builtins: bool,
  capture: impl for<'s, 't> Fn(&HinputsT<'s, 't>, &str) -> R,
) -> TypecheckOutcome<R> {
  let project = FixtureProject::new(fixture);
  let src_dir = src_dir_of(&project.valen_project_dir);
  fs::create_dir_all(&src_dir).expect("could not create the src dir");
  let valen_source_path = src_dir.join("main.valen");
  fs::write(&valen_source_path, vale).expect("could not write the valen source");
  let rustc_args = project.rustc_args(&valen_source_path, "lib");

  let captured: RefCell<Option<R>> = RefCell::new(None);
  let result = drive(
    &ValenInputs { rustc_args, borrow_check, stop_after_typing: true },
    compile_builtins,
    |_importer_file_text, hinputs, final_rust_file_text| {
      *captured.borrow_mut() = Some(capture(hinputs, final_rust_file_text));
    },
    |_firings| {},
  );
  let compiled = match result {
    Ok(_) => Ok(captured.into_inner().expect("typing succeeded but after_typing never ran")),
    Err(err) => Err(CompileFailure::from_debug(err.to_string())),
  };
  drop(project);
  TypecheckOutcome { compiled }
}

pub fn typecheck<R: Send>(
  fixture: &str,
  vale: &str,
  extract: impl for<'s, 't> Fn(&HinputsT<'s, 't>) -> R + Send,
) -> TypecheckOutcome<R> {
  run_typecheck(fixture, vale, true, true, move |hinputs, _final| extract(hinputs))
}

pub fn typecheck_without_borrow_check<R: Send>(
  fixture: &str,
  vale: &str,
  extract: impl for<'s, 't> Fn(&HinputsT<'s, 't>) -> R + Send,
) -> TypecheckOutcome<R> {
  run_typecheck(fixture, vale, false, true, move |hinputs, _final| extract(hinputs))
}

pub fn typecheck_without_builtins<R: Send>(
  fixture: &str,
  vale: &str,
  extract: impl for<'s, 't> Fn(&HinputsT<'s, 't>) -> R + Send,
) -> TypecheckOutcome<R> {
  run_typecheck(fixture, vale, true, false, move |hinputs, _final| extract(hinputs))
}

pub fn final_rust_source_without_borrow_check(
  fixture: &str,
  vale: &str,
) -> TypecheckOutcome<String> {
  run_typecheck(fixture, vale, false, true, |_hinputs, final_rust_file_text| {
    final_rust_file_text.to_string()
  })
}

pub struct DrivenRun {
  pub firings: Vec<String>,
  pub rustc_exit: i32,
  pub process_exit: Option<i32>,
}

pub fn drive_lib(fixture: &str, vale: &str) -> DrivenRun {
  drive_fixture_program(fixture, vale, "lib", false, true)
}

pub fn drive_lib_without_borrow_check(fixture: &str, vale: &str) -> DrivenRun {
  drive_fixture_program(fixture, vale, "lib", false, false)
}

pub fn drive_and_run(fixture: &str, vale: &str) -> DrivenRun {
  drive_fixture_program(fixture, vale, "bin", true, true)
}

pub fn drive_and_run_without_borrow_check(fixture: &str, vale: &str) -> DrivenRun {
  drive_fixture_program(fixture, vale, "bin", true, false)
}

fn drive_fixture_program(
  fixture: &str,
  vale: &str,
  crate_type: &str,
  run_exe: bool,
  borrow_check: bool,
) -> DrivenRun {
  let project = FixtureProject::new(fixture);
  let src_dir = src_dir_of(&project.valen_project_dir);
  fs::create_dir_all(&src_dir).expect("could not create the src dir");
  let valen_source_path = src_dir.join("main.valen");
  fs::write(&valen_source_path, vale).expect("could not write the valen source");
  let rustc_args = project.rustc_args(&valen_source_path, crate_type);

  let firings: RefCell<Vec<String>> = RefCell::new(Vec::new());
  let (_drove_valen, rustc_exit) = drive(
    &ValenInputs { rustc_args, borrow_check, stop_after_typing: false },
    true,
    |_importer_file_text, _hinputs, _final_rust_file_text| {},
    |fired| *firings.borrow_mut() = fired.to_vec(),
  )
  .unwrap_or_else(|err| {
    panic!(
      "the driven program failed to typecheck, so no Vale body was emitted:\n{}",
      err.to_string()
    )
  });

  let process_exit = if run_exe && rustc_exit == 0 {
    let exe = deps_dir_of(&project.valen_project_dir).join("stub");
    let output = Command::new(&exe)
      .output()
      .unwrap_or_else(|e| panic!("could not run the driven bin at {}: {e}", exe.display()));
    Some(output.status.code().unwrap_or(-1))
  } else {
    None
  };
  DrivenRun { firings: firings.into_inner(), rustc_exit, process_exit }
}
