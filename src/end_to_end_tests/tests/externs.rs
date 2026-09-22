use crate::end_to_end_tests::{
    assert_compile_and_run_with_c, assert_compile_and_run_with_c_without_borrow_check,
    assert_inline_compile_and_run, compile_program, programs_dir,
};

fn run(dir_rel: &str, expected: i32) {
    let dir = programs_dir().join(dir_rel);
    assert_compile_and_run_with_c(&dir, &[], expected);
}

fn run_without_borrow_check(dir_rel: &str, expected: i32) {
    let dir = programs_dir().join(dir_rel);
    assert_compile_and_run_with_c_without_borrow_check(&dir, &[], expected);
}

#[test]
#[ignore]
fn zst_struct_exported_by_value() {
    assert_inline_compile_and_run(
        r#"
exported struct Zst { }
exported func makeZst() Zst { Zst() }
exported func takeZst(z Zst) int { 7 }
exported func main() int { 7 }
"#,
        7,
    );
}

#[test]
#[ignore]
fn interfacemutreturnexport() { run("programs/externs/interfacemutreturnexport", 42); }

#[test]
#[ignore]
fn interfacemutparamexport()  { run("programs/externs/interfacemutparamexport", 42); }

#[test]
#[ignore]
fn structmutreturnexport()    { run("programs/externs/structmutreturnexport", 42); }

#[test]
fn structmutparamexport()     { run_without_borrow_check("programs/externs/structmutparamexport", 42); }
#[test]
#[ignore]
fn rsamutparamexport()        { run("programs/externs/rsamutparamexport", 10); }

#[test]
#[ignore]
fn rsamutreturnexport()       { run("programs/externs/rsamutreturnexport", 42); }

#[test]
fn ssamutparamexport()        { run_without_borrow_check("programs/externs/ssamutparamexport", 10); }
#[test]
#[ignore]
fn ssamutreturnexport()       { run("programs/externs/ssamutreturnexport", 42); }

#[test]
fn simpleexternreturn()        { run_without_borrow_check("programs/externs/simpleexternreturn", 42); }

#[test]
fn simpleexternparam()         { run_without_borrow_check("programs/externs/simpleexternparam", 42); }
#[test]
#[ignore]
fn structimmreturnextern()     { run("programs/externs/structimmreturnextern", 42); }

#[test]
#[ignore]
fn structimmreturnexport()     { run("programs/externs/structimmreturnexport", 42); }

#[test]
#[ignore]
fn structimmparamextern()      { run("programs/externs/structimmparamextern", 42); }

#[test]
#[ignore]
fn structimmparamexport()      { run("programs/externs/structimmparamexport", 42); }

#[test]
#[ignore]
fn structimmparamdeepextern()  { run("programs/externs/structimmparamdeepextern", 42); }

#[test]
#[ignore]
fn structimmparamdeepexport()  { run("programs/externs/structimmparamdeepexport", 42); }

#[test]
#[ignore]
fn strreturnexport()           { run("programs/externs/strreturnexport", 6); }

#[test]
#[ignore]
fn strlenextern()              { run("programs/externs/strlenextern", 11); }

#[test]
#[ignore]
fn interfaceimmparamextern_vale_dispatch()     { run("programs/externs/interfaceimmparamextern_vale_dispatch", 42); }

#[test]
#[ignore]
fn interfaceimmparamextern()                   { run("programs/externs/interfaceimmparamextern", 42); }

#[test]
#[ignore]
fn interfaceimmparamextern_owned()             { run("programs/externs/interfaceimmparamextern_owned", 42); }

#[test]
#[ignore]
fn interfaceimmparamdeepextern_vale_dispatch() { run("programs/externs/interfaceimmparamdeepextern_vale_dispatch", 42); }

#[test]
#[ignore]
fn interfaceimmparamdeepextern()               { run("programs/externs/interfaceimmparamdeepextern", 42); }

#[test]
#[ignore]
fn interfaceimmparamexport()                   { run("programs/externs/interfaceimmparamexport", 42); }

#[test]
#[ignore]
fn interfaceimmparamdeepexport()               { run("programs/externs/interfaceimmparamdeepexport", 42); }

#[test]
#[ignore]
fn interfaceimmreturnextern()                  { run("programs/externs/interfaceimmreturnextern", 42); }

#[test]
#[ignore]
fn interfaceimmreturnexport()                  { run("programs/externs/interfaceimmreturnexport", 42); }

#[test]
#[ignore]
fn feature_alias_dealias()      { run("programs/externs/feature_alias_dealias", 42); }

#[test]
#[ignore]
fn feature_ref_eq()             { run("programs/externs/feature_ref_eq", 42); }

#[test]
#[ignore]
fn feature_field_getters()      { run("programs/externs/feature_field_getters", 42); }

#[test]
#[ignore]
fn feature_interface_dispatch() { run("programs/externs/feature_interface_dispatch", 42); }

#[test]
#[ignore]
fn feature_str_read()           { run("programs/externs/feature_str_read", 42); }

#[test]
#[ignore]
fn feature_arr_read_rsa()       { run("programs/externs/feature_arr_read_rsa", 42); }

#[test]
#[ignore]
fn feature_arr_read_ssa()       { run("programs/externs/feature_arr_read_ssa", 42); }

#[test]
#[ignore]
fn structimm_roundtrip()          { run("programs/externs/structimm_roundtrip", 42); }

#[test]
#[ignore]
fn structimm_alias()              { run("programs/externs/structimm_alias", 42); }

#[test]
#[ignore]
fn str_empty()                    { run("programs/externs/str_empty", 42); }

#[test]
#[ignore]
fn interfaceimm_single_variant()  { run("programs/externs/interfaceimm_single_variant", 42); }

#[test]
#[ignore]
fn stradd_fromextern()      { run("programs/externs/stradd_fromextern", 4); }

#[test]
#[ignore]
fn substring_fromextern()   { run("programs/externs/substring_fromextern", 1); }

#[test]
#[ignore]
fn casti32str_fromextern()  { run("programs/externs/casti32str_fromextern", 12); }

#[test]
#[ignore]
fn structimm_with_str_return()  { run("programs/externs/structimm_with_str_return", 47); }

#[test]
#[ignore]
fn structimm_with_str_return_twice()  { run("programs/externs/structimm_with_str_return_twice", 13); }

#[test]
#[ignore]
fn getmainarg_basic() {
    let dir = programs_dir().join("programs/externs/getmainarg_basic");
    let cp = compile_program(&dir, &[], |_| {});
    let r = cp.run(&["hello"]);
    assert_eq!(r.exit_code, 5, "stdout={:?} stderr={:?}", r.stdout, r.stderr);
}
