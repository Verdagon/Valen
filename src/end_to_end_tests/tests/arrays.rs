use crate::end_to_end_tests::{assert_compile_and_run, assert_compile_and_run_dbg, assert_compile_and_run_dbg_without_borrow_check, cmd, expect, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}

#[test]
fn ssamutfromcallable() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/arrays/ssamutfromcallable.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: ssamutfromcallable-ready' -f ssamutfromcallable.vale"),
        cmd("run"),
        expect("frame variable -P 1 a", &["[0] = 0", "[1] = 42", "[2] = 84", "[3] = 126", "[4] = 168"]),
    ]);
}
#[test]
fn ssamutfromvalues() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/arrays/ssamutfromvalues.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: ssamutfromvalues-ready' -f ssamutfromvalues.vale"),
        cmd("run"),
        expect("frame variable -P 1 a", &["[0] = 23", "[1] = 31", "[2] = 37", "[3] = 42", "[4] = 49"]),
    ]);
}

#[test]
#[ignore]
fn rsaimm()                    { assert_compile_and_run(&p("programs/arrays/rsaimm.vale"), 3); }

#[test]
#[ignore]
fn rsamut()                    { assert_compile_and_run(&p("programs/arrays/rsamut.vale"), 3); }

#[test]
#[ignore]
fn rsamutdestroyintocallable() { assert_compile_and_run(&p("programs/arrays/rsamutdestroyintocallable.vale"), 42); }

#[test]
#[ignore]
fn ssamutdestroyintocallable() { assert_compile_and_run(&p("programs/arrays/ssamutdestroyintocallable.vale"), 42); }

#[test]
#[ignore]
fn rsamutlen()                 { assert_compile_and_run(&p("programs/arrays/rsamutlen.vale"), 5); }

#[test]
#[ignore]
fn rsamutcapacity()            { assert_compile_and_run(&p("programs/arrays/rsamutcapacity.vale"), 42); }

#[test]
#[ignore]
fn swaprsamutdestroy()         { assert_compile_and_run(&p("programs/arrays/swaprsamutdestroy.vale"), 42); }
