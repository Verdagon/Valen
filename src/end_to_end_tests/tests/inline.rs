
use crate::end_to_end_tests::{
    assert_inline_compile_and_run, assert_inline_compile_and_run_dbg, assert_inline_compile_and_run_dbg_without_borrow_check, cmd, expect,
};

#[test]
fn pass_manager_main_builds_simple_program_end_to_end() {
    assert_inline_compile_and_run_dbg_without_borrow_check(
        "exported func main() int { return 3; }",
        3,
        &[
            cmd("b :main"),
            cmd("run"),
            expect("bt", &["test.vale:1", ":main"]),
        ],
    );
}

#[test]
fn pass_manager_main_builds_program_using_builtin_some() {
    assert_inline_compile_and_run_dbg_without_borrow_check(
        "exported func main() int { x = Some<int>(3); return 0; }",
        0,
        &[
            cmd("b :main"),
            cmd("run"),
            expect("bt", &["test.vale:1", ":main"]),
        ],
    );
}

#[test]
fn basic_function_call() {
    assert_inline_compile_and_run_dbg_without_borrow_check(
        "func helper() int { return 42; }\nexported func main() int { return helper(); }",
        42,
        &[
            cmd("b helper"),
            cmd("run"),
            expect("bt", &["helper at test.vale", ":main"]),
        ],
    );
}


#[test]
#[ignore]
fn string_len() {
    assert_inline_compile_and_run(
        "exported func main() int { return (&\"hello\").len(); }",
        5,
    );
}
