use crate::end_to_end_tests::{assert_compile_and_run_dbg, assert_compile_and_run_dbg_without_borrow_check, cmd, expect, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}

#[test]
fn while_loop() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/while/while.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: while-iter' -f while.vale"),
        cmd("run"),
        expect("frame variable a", &["a = 1"]),
        cmd("continue"),
        expect("frame variable a", &["a = 2"]),
        cmd("continue"),
        expect("frame variable a", &["a = 3"]),
    ]);
}
