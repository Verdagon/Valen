use crate::end_to_end_tests::{assert_compile_and_run, assert_compile_and_run_without_borrow_check, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}


#[test]
fn lambda()    { assert_compile_and_run_without_borrow_check(&p("programs/lambdas/lambda.vale"), 42); }
#[test]
#[ignore]
fn lambdamut() { assert_compile_and_run(&p("programs/lambdas/lambdamut.vale"), 42); }
