use crate::end_to_end_tests::{assert_compile_and_run, assert_compile_and_run_dbg, assert_compile_and_run_dbg_without_borrow_check, cmd, expect, reject, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}

#[test]
fn ifelse() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/if/if.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: if-then' -f if.vale"),
        cmd("br s -p 'lldb breakpoint: if-else' -f if.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &["exited with status = 42"], &["stop reason = breakpoint 2"]),
    ]);
}

#[test]
#[ignore]
fn upcastif() { assert_compile_and_run(&p("programs/if/upcastif.vale"), 42); }

#[test]
fn ifnevers() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/if/ifnevers.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: ifnevers-then' -f ifnevers.vale"),
        cmd("br s -p 'lldb breakpoint: ifnevers-else' -f ifnevers.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &["exited with status = 42"], &["stop reason = breakpoint 2"]),
    ]);
}
#[test]
fn nestedif() {
    assert_compile_and_run_dbg_without_borrow_check(&p("programs/if/nestedif.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: nestedif-mid' -f nestedif.vale"),
        cmd("br s -p 'lldb breakpoint: nestedif-else' -f nestedif.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &["exited with status = 42"], &["stop reason = breakpoint 2"]),
    ]);
}
