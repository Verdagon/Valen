use crate::end_to_end_tests::{assert_compile_and_run_with_c_without_borrow_check, programs_dir};

#[test]
fn native_walker_reached_package_included() {
    let dir = programs_dir().join("programs/native_walker/walks_reached");
    assert_compile_and_run_with_c_without_borrow_check(&dir, &[],7);
}

#[test]
fn native_walker_unreached_package_excluded() {
    let dir = programs_dir().join("programs/native_walker/skips_unreached");
    assert_compile_and_run_with_c_without_borrow_check(&dir, &[],5);
}
