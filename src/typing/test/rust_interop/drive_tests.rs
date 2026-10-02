use crate::typing::rust_interop::source_digest;
use crate::typing::rust_interop::deps_dir_of;
use crate::typing::rust_interop::{BifrostPhase, BifrostRustcCallbacks};
use std::fs::{copy, create_dir_all, read_dir, read_to_string, write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::cell::RefCell;
use bumpalo::Bump;
use tempfile::TempDir;
use crate::{collect_only_tnode, collect_where_tnode, Keywords};
use crate::typing::rust_interop::importer_file_path;
use crate::typing::rust_interop::src_dir_of;
use crate::typing::rust_interop::stubs_dir_of;
use crate::typing::rust_interop::{drive, ValenInputs};
use crate::typing::rust_interop::rust_crate_names_from_rustc_args;
use crate::interner::StrI;
use crate::typing::rust_interop::get_env_rustc_and_sysroot_locations;
use crate::typing::ast::expressions::FunctionCallTE;
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::compile_options::GlobalOptions;
use crate::instantiating::instantiating_interner::InstantiatingInterner;
use crate::instantiating::instantiator::InstantiatedOutputsI;
use crate::parse_arena::ParseArena;
use crate::postparsing::ScoutCompilation;
use crate::scout_arena::ScoutArena;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::hinputs_t::HinputsT;
use crate::typing::names::names::{FunctionNameT, FunctionTemplateNameT, INameT, IdT};
use crate::typing::templata::templata::ITemplataT;
use crate::typing::test::traverse::NodeRefT;
use crate::typing::types::types::KindT;
use crate::typing::typing_interner::TypingInterner;
use crate::typing::{TypingPassCompilation, TypingPassOptions};
use crate::typing::oracles::Oracles;
use crate::utils::code_hierarchy::FileCoordinateMap;
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler as RustcCompiler;
use rustc_middle::ty::{Ty, TyCtxt, TyKind};
use crate::code_source::{CodeSource, Source};
use crate::typing::test::rust_interop::cargo_mimic::*;
use crate::typing::test::rust_interop::test_setup::*;
use crate::typing::test::rust_interop::drive_helpers::{drive_and_run_binary, Scratch};
use crate::instantiating::rust_interop::BifrostState;
use crate::instantiating::ast::ast::FunctionExportI;
use crate::backend_ffi::metal_lowerer::ExternAbi;
use std::collections::HashMap;

fn assemble_rustc_args(
  sysroot_location: &str,
  valen_project_dir: &Path,
  dependency_rust_lib_names: &[&str],
  crate_type: &str,
) -> Vec<String> {
  let mut rustc_args: Vec<String> = vec![
    "valec-rs".to_string(),
    importer_file_path(valen_project_dir).display().to_string(),
    format!("--crate-type={crate_type}"),
    "--crate-name=stub".to_string(),
    "--edition=2021".to_string(),
    format!("--sysroot={}", sysroot_location),
    format!("-L{}", deps_dir_of(valen_project_dir).display()),
    format!("--out-dir={}", deps_dir_of(valen_project_dir).display()),
  ];
  for crate_name in dependency_rust_lib_names {
    rustc_args.push(format!(
      "--extern={crate_name}={}",
      deps_dir_of(valen_project_dir).join(format!("lib{crate_name}.rlib")).display()
    ));
  }
  return rustc_args;
}

pub(crate) fn test_code_source<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  parser_keywords: &'ctx Keywords<'a>,
  files: &FileCoordinateMap<'a, String>,
  compile_builtins: bool,
) -> CodeSource<'a>
where
    'a: 'ctx,
{
  let mut sources = Vec::new();
  if compile_builtins {
    sources.push(Source::builtins(parse_arena, parser_keywords));
  }
  sources.push(Source::from_code_map(files));
  CodeSource::new(sources)
}

#[test]
fn multiple_mutable_aliases_to_one_rust_object_are_legal() {
  let (rustc_location, sysroot_location) = get_env_rustc_and_sysroot_locations();
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  add_dependency_rust_lib(temp_dir.path(), "simple_dep_rust_lib");
  let valen_project_dir = temp_dir.path().join("testvalenproj");
  setup_test_temp_dir(&valen_project_dir);

  mimic_compile_dependency_rust_lib(
    temp_dir.path(),
    &valen_project_dir,
    &rustc_location,
    "simple_dep_rust_lib");
  let code = r#"
import simple_dep_rust_lib.Slot;
exported func main() i64 {
  slot = Slot.new();
  ref_a = &slot;
  ref_b = &slot;
  ref_a.mutate(42i64);
  ref_b.mutate(73i64);
  return ref_a.get();
}
"#;
  let valen_source_path = src_dir_of(&valen_project_dir).join("main.valen");
  create_dir_all(src_dir_of(&valen_project_dir)).expect("could not create the src dir");
  write(&valen_source_path, code).expect("could not write the valen source");

  let mut rustc_args =
      assemble_rustc_args(
        &sysroot_location, &valen_project_dir, &["simple_dep_rust_lib"], "bin");
  rustc_args[1] = valen_source_path.display().to_string();

  let (drove_valen, rustc_exit) = drive(
    &ValenInputs { rustc_args, borrow_check: true, stop_after_typing: false },
    false,
    |importer_file_text, hinputs, final_rust_file_text| {
      let main = hinputs.lookup_function_by_str("main");
      let mutates = collect_where_tnode!(
          NodeRefT::FunctionDefinition(main),
          NodeRefT::FunctionCall(FunctionCallTE {
              callable: PrototypeT { id: IdT {
                  package_coord: PackageCoordinate { module: StrI("simple_dep_rust_lib"), .. },
                  local_name: INameT::Function(FunctionNameT {
                      template: FunctionTemplateNameT { human_name: StrI("mutate"), .. }, ..
                  }), ..
              }, .. }, ..
          }) => Some(())
      );
      assert_eq!(mutates.len(), 2, "both aliases should mutate the rust-backed slot");

      collect_only_tnode!(
          NodeRefT::FunctionDefinition(main),
          NodeRefT::FunctionCall(FunctionCallTE {
              callable: PrototypeT { id: IdT {
                  package_coord: PackageCoordinate { module: StrI("simple_dep_rust_lib"), .. },
                  local_name: INameT::Function(FunctionNameT {
                      template: FunctionTemplateNameT { human_name: StrI("get"), .. }, ..
                  }), ..
              }, .. }, ..
          }) => Some(())
      );

      let expected_importer_file = r#"extern crate std;
extern crate core;
extern crate alloc;
extern crate simple_dep_rust_lib;
"#;
      assert_eq!(importer_file_text, expected_importer_file, "importer file drifted from the golden");
      let expected_final_rust_file = r#"#![feature(register_tool)]
#![register_tool(vale)]

extern crate std;
extern crate core;
extern crate alloc;
extern crate simple_dep_rust_lib;

use std::process::exit;

pub const __VALE_STUBS_MARKER: () = ();

pub struct __ValeOpaque<const T: u64>(::core::cell::UnsafeCell<()>, ::std::marker::PhantomData<*mut ()>, ::std::marker::PhantomPinned);

#[vale::emit_consumer_body(digest = "76e557e4623137ea")]
pub fn __vale_main() -> i32 {
    unreachable!()
}

fn main() {
    exit(__vale_main());
}

#[inline(never)]
pub unsafe fn __vale_drop<T>(x: *mut T) {
    core::ptr::drop_in_place(x)
}
"#;
      assert_eq!(
        final_rust_file_text, expected_final_rust_file, "final rust file drifted from the golden");
    },
    |firings| assert!(!firings.is_empty(), "per_instance_mir never fired"))
    .unwrap_or_else(|e| panic!("drive failed: {}", e.to_string()));
  assert!(drove_valen);
  assert_eq!(rustc_exit, 0);

  let exe = deps_dir_of(&valen_project_dir).join("stub");
  let output = Command::new(&exe).output().expect("could not run the driven bin");
  assert_eq!(
    output.status.code(),
    Some(73),
    "both aliases mutate one Slot, so the second mutate (73) is what get() reads back");
}

#[test]
fn chest_gem_accepted() {
  let (rustc_location, sysroot_location) = get_env_rustc_and_sysroot_locations();
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  add_dependency_rust_lib(temp_dir.path(), "boxed_dep_rust_lib");
  let valen_project_dir = temp_dir.path().join("testvalenproj");
  setup_test_temp_dir(&valen_project_dir);

  mimic_compile_dependency_rust_lib(
    temp_dir.path(),
    &valen_project_dir,
    &rustc_location,
    "boxed_dep_rust_lib");
  let code = r#"
  import boxed_dep_rust_lib.Chest;
  import boxed_dep_rust_lib.Gem;
  exported func main() i64 {
    chest = Chest.new();
    chest.replace(8i64);
    gem = chest.gem();
    return gem.get();
  }
  "#;
  let valen_source_path = src_dir_of(&valen_project_dir).join("main.valen");
  create_dir_all(src_dir_of(&valen_project_dir)).expect("could not create the src dir");
  write(&valen_source_path, code).expect("could not write the valen source");

  let mut rustc_args =
      assemble_rustc_args(
        &sysroot_location, &valen_project_dir, &["boxed_dep_rust_lib"], "bin");
  rustc_args[1] = valen_source_path.display().to_string();

  let (drove_valen, rustc_exit) =
      match drive(
        &ValenInputs { rustc_args, borrow_check: true, stop_after_typing: false },
        false,
        // No typing checks needed
        |_, _, _| {},
        // No firings checks needed
        |_| {})
      {
        Err(err) => panic!("didn't compile!"),
        Ok((drove_valen, rustc_exit)) => (drove_valen, rustc_exit),
      };

  assert!(drove_valen);
  assert_eq!(rustc_exit, 0);
  let exe = deps_dir_of(&valen_project_dir).join("stub");
  let output = Command::new(&exe).output().expect("could not run the driven bin");
  assert_eq!(output.status.code(), Some(8));
}

#[test]
fn chest_gem_use_after_churn_detected() {
  let (rustc_location, sysroot_location) = get_env_rustc_and_sysroot_locations();
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  add_dependency_rust_lib(temp_dir.path(), "boxed_dep_rust_lib");
  let valen_project_dir = temp_dir.path().join("testvalenproj");
  setup_test_temp_dir(&valen_project_dir);

  mimic_compile_dependency_rust_lib(
    temp_dir.path(),
    &valen_project_dir,
    &rustc_location,
    "boxed_dep_rust_lib");
  let code = r#"
  import boxed_dep_rust_lib.Chest;
  import boxed_dep_rust_lib.Gem;
  exported func main() i64 {
    chest = Chest.new();
    gem = chest.gem();
    chest.replace(8i64);
    return gem.get();
  }
  "#;
  let valen_source_path = src_dir_of(&valen_project_dir).join("main.valen");
  create_dir_all(src_dir_of(&valen_project_dir)).expect("could not create the src dir");
  write(&valen_source_path, code).expect("could not write the valen source");

  let mut rustc_args =
      assemble_rustc_args(
        &sysroot_location, &valen_project_dir, &["boxed_dep_rust_lib"], "bin");
  rustc_args[1] = valen_source_path.display().to_string();

  let err =
      match drive(
        &ValenInputs { rustc_args, borrow_check: true, stop_after_typing: false },
        false,
        // No typing checks needed
        |_, _, _| {},
        // No firings checks needed
        |_| {})
      {
        Err(err) => err,
        Ok((drove_valen, rustc_exit)) => panic!("use-after-churn wasn't detected"),
      };

  let message = err.to_string();
  assert!(message.contains("Used a borrow after invalidated"), "err:\n{message}");
}

#[test]
fn chest_gem_mut_and_aliasing_peek_accepted() {
  let (rustc_location, sysroot_location) = get_env_rustc_and_sysroot_locations();
  let temp_dir = TempDir::new().expect("could not create scratch dir");
  add_dependency_rust_lib(temp_dir.path(), "boxed_mut_dep_rust_lib");
  let valen_project_dir = temp_dir.path().join("testvalenproj");
  setup_test_temp_dir(&valen_project_dir);

  mimic_compile_dependency_rust_lib(
    temp_dir.path(),
    &valen_project_dir,
    &rustc_location,
    "boxed_mut_dep_rust_lib");
  let code = r#"
  import boxed_mut_dep_rust_lib.Chest;
  import boxed_mut_dep_rust_lib.Gem;
  exported func main() i64 {
    chest = Chest.new();
    chest.replace(42i64);
    ref_a = &chest;
    ref_b = &chest;
    g = ref_a.gem_mut();
    x = ref_b.peek();
    return g.get() + x;
  }
  "#;
  let valen_source_path = src_dir_of(&valen_project_dir).join("main.valen");
  create_dir_all(src_dir_of(&valen_project_dir)).expect("could not create the src dir");
  write(&valen_source_path, code).expect("could not write the valen source");

  let mut rustc_args =
      assemble_rustc_args(
        &sysroot_location, &valen_project_dir, &["boxed_mut_dep_rust_lib"], "bin");
  rustc_args[1] = valen_source_path.display().to_string();

  let (drove_valen, rustc_exit) =
      match drive(
        &ValenInputs { rustc_args, borrow_check: true, stop_after_typing: false },
        true,
        |_, _, _| {},
        |_| {})
      {
        Err(err) => panic!("didn't compile!\n{err:?}"),
        Ok((drove_valen, rustc_exit)) => (drove_valen, rustc_exit),
      };

  assert!(drove_valen);
  assert_eq!(rustc_exit, 0);
  let exe = deps_dir_of(&valen_project_dir).join("stub");
  let output = Command::new(&exe).output().unwrap();
  assert_eq!(output.status.code(), Some(84));
}

#[test]
fn drive_links_int_operators_to_exit_seven() {
  let scratch = Scratch::new();
  let exit = drive_and_run_binary(
    &scratch.out_dir(),
    "exported func main() int { sum = 3 + 4; if sum == 7 { 7 } else { 0 } }",
    vec![],
    /*borrow_check=*/ true,
  );
  assert_eq!(exit, 7);
}

#[test]
fn drive_links_int_not_equal_to_exit_seven() {
  let scratch = Scratch::new();
  let exit = drive_and_run_binary(
    &scratch.out_dir(),
    "exported func main() int { x = 3; y = 4; if x != y { 7 } else { 0 } }",
    vec![],
    /*borrow_check=*/ true,
  );
  assert_eq!(exit, 7);
}
