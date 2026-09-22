
use crate::end_to_end_tests::{compile_inline_debug, target_backend, Backend};

fn skip_non_native() -> bool {
    if matches!(target_backend(), Backend::Native) {
        return false;
    }
    eprintln!(
        "SKIP: debugger gate requires the Native backend (lldb/dsymutil/dwarfdump); \
         not validated under wasi."
    );
    true
}


#[test]
fn breakpoint_resolves_to_main_source_line() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug("exported func main() int { return 42; }");
    cp.lldb_check(
        &["b :main", "run", "bt"],
        &["test.vale:1", "frame #0", ":main"],
    );
}

#[test]
fn breakpoint_resolves_by_file_and_line() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug("exported func main() int { return 42; }");
    cp.lldb_check(
        &["b test.vale:1", "run", "bt"],
        &["test.vale:1", "frame #0", ":main"],
    );
}

#[test]
fn breakpoint_line_tracks_function_declaration() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "\n\
         \n\
         \n\
         exported func main() int { return 42; }\n",
    );
    cp.lldb_check(&["b :main", "run", "bt"], &["test.vale:4", ":main"]);
}

#[test]
fn distinct_functions_resolve_to_their_own_lines() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int { return helper(); }\n\
         \n\
         \n\
         func helper() int { return 7; }\n",
    );
    cp.lldb_check(
        &["image lookup -F :main", "image lookup -F helper"],
        &["test.vale:1", "test.vale:4"],
    );
}


#[test]
fn dwarf_has_compile_unit_and_named_subprogram() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug("exported func main() int { return 42; }");
    let out = cp.dwarfdump_capture(&["--debug-info"]);
    assert!(out.contains("DW_TAG_compile_unit"), "no compile unit:\n{out}");
    assert!(
        out.contains("DW_AT_producer") && out.contains("(\"Vale compiler\")"),
        "compile unit not tagged as the Vale compiler:\n{out}"
    );
    assert!(out.contains("DW_TAG_subprogram"), "no subprogram DIE:\n{out}");
    assert!(out.contains("(\":main\")"), "no :main subprogram name:\n{out}");
    assert!(out.contains("DW_AT_decl_line"), "subprogram has no decl line:\n{out}");
    assert!(
        out.contains("DW_AT_decl_file") && out.contains("test.vale"),
        "decl file isn't test.vale:\n{out}"
    );
}

#[test]
fn function_decl_line_is_declaration_not_body_statement() {
    if skip_non_native() {
        return;
    }
    // func keyword line 1; `{` line 2; `return 42;` line 3.
    let (cp, _) = compile_inline_debug(
        "exported func main() int\n\
         {\n\
         return 42;\n\
         }\n",
    );
    let out = cp.dwarfdump_capture(&["--debug-info"]);
    assert!(out.contains("(\":main\")"), "no :main subprogram:\n{out}");
    assert!(
        out.contains("DW_AT_decl_line\t(1)"),
        "expected decl_line 1 (the declaration line), not the body statement:\n{out}"
    );
}


#[test]
fn three_statements_step_to_distinct_lines() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = 7;\n\
         y = 11;\n\
         return x + y;\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &[
            "b test.vale:2",
            "run",
            "thread step-over",
            "frame info",
            "thread step-over",
            "frame info",
        ],
        &["test.vale:2", "test.vale:3", "test.vale:4"],
    );
}

#[test]
fn mutate_steps_to_assignment_line() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = 7;\n\
         set x = 11;\n\
         return x;\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &[
            "b test.vale:2",
            "run",
            "thread step-over",
            "frame info",
            "thread step-over",
            "frame info",
        ],
        &["test.vale:2", "test.vale:3", "test.vale:4"],
    );
}

#[test]
fn while_loop_steps_through_body() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = 0;\n\
         while x < 1 { set x = x + 1; }\n\
         return x;\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &[
            "b test.vale:2",
            "run",
            "thread step-over",
            "frame info",
            "thread step-over",
            "frame info",
        ],
        &["test.vale:2", "test.vale:3"],
    );
}

#[test]
fn step_over_function_call() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = helper();\n\
         return x;\n\
         }\n\
         func helper() int { return 42; }\n",
    );
    cp.lldb_check_ordered(
        &["b test.vale:2", "run", "thread step-over", "frame info"],
        &["test.vale:2", "test.vale:3"],
    );
}

#[test]
fn if_branch_steps_to_branch_body() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = 7;\n\
         if true { return x; }\n\
         return 0;\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &[
            "b test.vale:2",
            "run",
            "thread step-over",
            "frame info",
            "thread step-over",
            "frame info",
        ],
        &["test.vale:2", "test.vale:3"],
    );
}

#[test]
fn member_access_steps_to_distinct_lines() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct S { x int; }\n\
         exported func main() int {\n\
         s = S(7);\n\
         return s.x;\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &["b test.vale:3", "run", "thread step-over", "frame info"],
        &["test.vale:3", "test.vale:4"],
    );
}


#[test]
fn backtrace_resolves_caller_and_callee_frames() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         return helper();\n\
         }\n\
         func helper() int {\n\
         return 42;\n\
         }\n",
    );
    let out = cp.lldb_capture(&["b test.vale:5", "run", "bt"]);
    assert!(out.contains("helper"), "no helper frame in bt:\n{out}");
    assert!(out.contains("test.vale:5"), "callee frame not at line 5:\n{out}");
    assert!(out.contains(":main"), "no :main frame in bt:\n{out}");
    assert!(out.contains("test.vale:2"), "caller frame not at call site line 2:\n{out}");
}

#[test]
fn array_program_steps_by_line() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         a = [#](23, 31, 42);\n\
         return __copy_prim(a.2);\n\
         }\n",
    );
    cp.lldb_check_ordered(
        &["b test.vale:2", "run", "thread step-over", "frame info"],
        &["test.vale:2", "test.vale:3"],
    );
}


#[test]
fn local_variable_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x = 7;\n\
         return x;\n\
         }\n",
    );
    cp.lldb_check(&["b test.vale:3", "run", "frame variable x"], &["x = 7"]);
}

#[test]
fn function_argument_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int { return helper(7); }\n\
         func helper(a int) int {\n\
         return a;\n\
         }\n",
    );
    cp.lldb_check(&["b test.vale:3", "run", "frame variable a"], &["a = 7"]);
}

#[test]
fn bool_and_float_locals_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         b = true;\n\
         f = 1.5;\n\
         if b { return 1; }\n\
         return 0;\n\
         }\n",
    );
    let out = cp.lldb_capture(&[
        "b test.vale:4",
        "run",
        "frame variable b",
        "frame variable f",
    ]);
    assert!(out.contains("b = true"), "missing `b = true`:\n{out}");
    assert!(out.contains("f = 1.5"), "missing `f = 1.5`:\n{out}");
}


#[test]
fn struct_local_field_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct S { x int; }\n\
         exported func main() int {\n\
         s = S(7);\n\
         return s.x;\n\
         }\n",
    );
    cp.lldb_check(
        &["b test.vale:4", "run", "frame variable -P 1 s"],
        &["x = 7"],
    );
}

#[test]
fn struct_local_multifield_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct S { a int; b bool; }\n\
         exported func main() int {\n\
         s = S(7, true);\n\
         return s.a;\n\
         }\n",
    );
    let out = cp.lldb_capture(&["b test.vale:4", "run", "frame variable -P 1 s"]);
    assert!(out.contains("a = 7"), "missing `a = 7`:\n{out}");
    assert!(out.contains("b = true"), "missing `b = true`:\n{out}");
}

#[test]
fn dwarf_dies_for_struct_have_user_members() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct S { a int; b bool; }\n\
         exported func main() int {\n\
         s = S(7, true);\n\
         return s.a;\n\
         }\n",
    );
    let out = cp.dwarfdump_capture(&["--debug-info", "--name=S", "--show-children"]);
    assert!(
        out.contains("DW_TAG_structure_type"),
        "no DW_TAG_structure_type for S:\n{out}"
    );
    assert!(out.contains("(\"a\")"), "missing member `a`:\n{out}");
    assert!(out.contains("(\"b\")"), "missing member `b`:\n{out}");
}

#[test]
fn destructured_locals_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct Pair { a int; b int; }\n\
         exported func main() int {\n\
         [x, y] = Pair(7, 11);\n\
         return x + y;\n\
         }\n",
    );
    let out = cp.lldb_capture(&[
        "b test.vale:4",
        "run",
        "frame variable x",
        "frame variable y",
    ]);
    assert!(out.contains("x = 7"), "missing `x = 7`:\n{out}");
    assert!(out.contains("y = 11"), "missing `y = 11`:\n{out}");
}


#[test]
fn borrow_ref_struct_fields_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct Carrier { hp int; interceptors int; }\n\
         exported func main() int {\n\
         carrier = Carrier(400, 8);\n\
         ref = &carrier;\n\
         return __copy_prim(ref.interceptors);\n\
         }\n",
    );
    let out = cp.lldb_capture(&["b test.vale:5", "run", "frame variable -P 1 ref"]);
    assert!(out.contains("hp = 400"), "missing `hp = 400`:\n{out}");
    assert!(
        out.contains("interceptors = 8"),
        "missing `interceptors = 8`:\n{out}"
    );
}

#[test]
fn dwarf_dies_for_borrow_ref_are_pointer_to_struct() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "struct Carrier { hp int; interceptors int; }\n\
         exported func main() int {\n\
         carrier = Carrier(400, 8);\n\
         ref = &carrier;\n\
         return __copy_prim(ref.interceptors);\n\
         }\n",
    );
    let out = cp.dwarfdump_capture(&["--debug-info"]);
    assert!(
        out.contains("DW_TAG_pointer_type"),
        "no DW_TAG_pointer_type for the borrow ref:\n{out}"
    );
    assert!(
        out.contains("DW_TAG_structure_type"),
        "no DW_TAG_structure_type for Carrier:\n{out}"
    );
    assert!(
        out.contains("(\"interceptors\")"),
        "missing member `interceptors`:\n{out}"
    );
}


#[test]
fn array_local_elements_visible_in_lldb() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         a = [#](23, 31, 42);\n\
         return __copy_prim(a.2);\n\
         }\n",
    );
    let out = cp.lldb_capture(&["b test.vale:3", "run", "frame variable -P 1 a"]);
    assert!(out.contains("23"), "missing element `23`:\n{out}");
    assert!(out.contains("42"), "missing element `42`:\n{out}");
}

#[test]
fn dwarf_dies_for_array_are_array_type() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         a = [#](23, 31, 42);\n\
         return __copy_prim(a.2);\n\
         }\n",
    );
    let out = cp.dwarfdump_capture(&["--debug-info"]);
    assert!(
        out.contains("DW_TAG_array_type"),
        "no DW_TAG_array_type for the array local:\n{out}"
    );
    assert!(
        out.contains("DW_TAG_subrange_type"),
        "no DW_TAG_subrange_type for the array local:\n{out}"
    );
}


#[test]
fn sentinel_breakpoint_binds() {
    if skip_non_native() {
        return;
    }
    let (cp, _) = compile_inline_debug(
        "exported func main() int {\n\
         x int = 73;\n\
         0; // lldb breakpoint: probe-before\n\
         set x = 42;\n\
         0; // lldb breakpoint: probe-after\n\
         return x;\n\
         }\n",
    );
    let out = cp.lldb_capture(&[
        "br s -p 'lldb breakpoint: probe-before' -f test.vale",
        "br s -p 'lldb breakpoint: probe-after' -f test.vale",
        "run",
        "frame variable x",
        "continue",
        "frame variable x",
    ]);
    assert!(
        !out.contains("Unable to resolve breakpoint"),
        "a sentinel breakpoint didn't bind to any location:\n{out}"
    );
    assert!(out.contains("(int) x = 73"), "no `x = 73` at probe-before:\n{out}");
    assert!(out.contains("(int) x = 42"), "no `x = 42` at probe-after:\n{out}");
    let before = out.find("(int) x = 73").unwrap();
    let after = out.find("(int) x = 42").unwrap();
    assert!(before < after);
}
