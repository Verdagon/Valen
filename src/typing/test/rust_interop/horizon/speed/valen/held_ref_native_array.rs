// Probe: tests whether Valen's own array can stand in for Rust's `Vec` in this
// corpus, on the program of held_ref_across_opaque_call.
//
// Result: not yet. The program parses, typechecks and passes the experimental
// borrow checker; the test below pins that. Driven end to end it aborts in code
// generation:
//   Assertion failed: (result), function peel_all_references, file types.cpp,
//   line 20.
// That is Backend/src/metal/types.cpp: the backend strips the references off a
// kind and asserts that what is left is a value kind. Its callers are
// Backend/src/globalstate.cpp:107 and :145 and Backend/src/vale.cpp:220, :302,
// :483, :498 and :885. The message has no stack, so which caller and which kind
// are not known.
//
// The end-to-end test is not in this file. This aborts the whole test process
// with SIGABRT, so it is not registered; re-add it when the backend handles a
// runtime-sized array in the interop lane. To turn the test below into the
// end-to-end one, call `drive_and_run("horizon/main", PROGRAM)` in place of the
// `typecheck` helper and assert that `process_exit` is `Some(86)`. A copy of
// that test is saved as tmp/held_ref_native_array_run.md.
//
// The program:
//   bump(ships &[]Ship in g, n int) mut(g)
//   s = &ships[0]
//   loop n times: set s.fuel = s.fuel + 1; do_nothing();
//   main builds the array with Array<Ship>(2) and two pushes, and calls bump
//   twice with different counts.
//
// No `#inline(never)` and no Rust arms yet: this file is a probe, and what
// follows it is the architect's decision.

use crate::typing::test::rust_interop::drive_helpers::typecheck;

const PROGRAM: &str = r#"
import mycrate.do_nothing;
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Ship { fuel int; }
func bump<g'>(ships &[]Ship in g, n int) mut(g) {
  s = &ships[0];
  i = 0;
  while i < __copy_prim(n) {
    set s.fuel = __copy_prim(s.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  ships = Array<Ship>(2);
  ships.push(Ship(7));
  ships.push(Ship(9));
  bump(&ships, 50);
  bump(&ships, 20);
  return __copy_prim(ships[0].fuel) + __copy_prim(ships[1].fuel);
}
"#;

#[test]
fn held_ref_native_array_typechecks() {
  typecheck("horizon/main", PROGRAM, |_hinputs| ()).expect_compiled();
}
