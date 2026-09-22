use crate::end_to_end_tests::{assert_compile_and_run, assert_compile_and_run_dbg, cmd, expect, programs_dir};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}

#[test]
fn structmutfield() {
    assert_compile_and_run_dbg(&p("programs/structs/structmutfield.vale"), 5, &[
        cmd("br s -p 'lldb breakpoint: structmutfield-at' -f structmutfield.vale"),
        cmd("run"),
        expect("bt", &["structmutfield.vale", ":main"]),
    ]);
}

#[test]
#[ignore]
fn memberrefcount()      { assert_compile_and_run(&p("programs/structs/memberrefcount.vale"), 5); }
#[test]
fn bigstructmutfield() {
    assert_compile_and_run_dbg(&p("programs/structs/bigstructmutfield.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: bigstructmutfield-at' -f bigstructmutfield.vale"),
        cmd("run"),
        expect("bt", &["bigstructmutfield.vale", ":main"]),
    ]);
}
#[test]
fn structmut() {
    assert_compile_and_run_dbg(&p("programs/structs/structmut.vale"), 8, &[
        cmd("br s -p 'lldb breakpoint: structmut-at' -f structmut.vale"),
        cmd("run"),
        expect("bt", &["structmut.vale", ":main"]),
    ]);
}

#[test]
fn structmutstore() {
    assert_compile_and_run_dbg(&p("programs/structs/structmutstore.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: structmutstore-post' -f structmutstore.vale"),
        cmd("run"),
        expect("frame variable -P 1 c", &["hp = 400", "interceptors = 42"]),
    ]);
}
// A nested aggregate walks recursively (Outer -> Inner -> x), showing the nested mutation.
#[test]
fn structmutstoreinner() {
    assert_compile_and_run_dbg(&p("programs/structs/structmutstoreinner.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: structmutstoreinner-post' -f structmutstoreinner.vale"),
        cmd("run"),
        expect("frame variable o.inner.x", &["= 42"]),
    ]);
}
