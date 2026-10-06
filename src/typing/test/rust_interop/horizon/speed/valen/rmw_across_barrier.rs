// Fence: ensures that Valen keeps a field in a register across an opaque call
// when the field is reached through a borrow parameter.
//
// The program:
//   bump(e &Entity in g, n int) mut(g)
//   loop n times: set e.hp = e.hp - 1; do_nothing();
//
// Mechanism: parameter `noalias`. `e` is the only reference into group `g`, so
// Valen marks the parameter `noalias`, and LLVM knows `do_nothing()` cannot
// change `e.hp`. That fact comes from the borrow checker's aliasing info
// (`param_index_to_noalias`), so this test runs with the checker on.
// Plain Rust has the same mechanism for `&mut Entity`. This test
// shows no Valen advantage over plain Rust. It exists to validate the measuring
// method on the simplest shape, and to show that the GhostCell arm
// (rust/rmw_across_barrier.rs) loses what the other two keep.
//
// Arms:
// - Plain Rust: rust/rmw_across_barrier_plain.rs
// - GhostCell:  rust/rmw_across_barrier.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `bump` a function, so `e` stays a parameter.
// - The test is built with the `borrow_checker_experimental` feature.
//
// Expected IR of `bump`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, and the call to
//   `do_nothing`.
//
// Expected IR of `bump`, optimized:
// - The parameter `e` carries `noalias`.
// - The loop contains the call to `do_nothing` and one store of `hp`.
// - The loop contains no load of `hp`. The one load of `hp` sits before the
//   loop.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn rmw_across_barrier() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.do_nothing;
struct Entity { hp int; }
#inline(never)
func bump<g'>(e &Entity in g, n int) mut(g) {
  i = 0;
  while i < __copy_prim(n) {
    set e.hp = __copy_prim(e.hp) - 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  e = Entity(150);
  bump(&e, 50);
  return __copy_prim(e.hp);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
