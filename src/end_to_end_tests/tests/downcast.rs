#![allow(non_snake_case)]

use crate::end_to_end_tests::{assert_compile_and_run, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}


#[test]
#[ignore]
fn downcastBorrowSuccessful() { assert_compile_and_run(&p("programs/downcast/downcastBorrowSuccessful.vale"), 42); }

#[test]
#[ignore]
fn downcastBorrowFailed()     { assert_compile_and_run(&p("programs/downcast/downcastBorrowFailed.vale"), 42); }

#[test]
#[ignore]
fn downcastOwningSuccessful() { assert_compile_and_run(&p("programs/downcast/downcastOwningSuccessful.vale"), 42); }

#[test]
#[ignore]
fn downcastOwningFailed()     { assert_compile_and_run(&p("programs/downcast/downcastOwningFailed.vale"), 42); }
