// Fence: ensures that Valen reads a field once, not once per iteration, when
// the loop calls an opaque function and the field is reached through a borrow
// parameter of a function that declares no `mut`.
//
// The program:
//   total(e &Entity in g, n int) int
//   loop n times: set sum = sum + e.hp; do_nothing();
//
// Mechanism: parameter `noalias`. `e` is the only reference into group `g`, so
// Valen marks the parameter `noalias`, and LLVM knows `do_nothing()` cannot
// change `e.hp`. That fact comes from the borrow checker's aliasing info
// (`param_index_to_noalias`), so this test runs with the checker on.
// Plain Rust has the same mechanism for `&Entity`. This test shows no Valen
// advantage over plain Rust. It is the read-only twin of
// valen/rmw_across_barrier.rs: it validates the measuring method, and shows
// that the GhostCell arm (rust/read_across_barrier.rs) loses what the other two
// keep.
//
// Arms:
// - Plain Rust: rust/read_across_barrier_plain.rs
// - GhostCell:  rust/read_across_barrier.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `total` a function, so `e` stays a parameter.
// - The test is built with the `borrow_checker_experimental` feature.
//
// Expected IR of `total`, unoptimized:
// - The loop contains a load of `hp` and the call to `do_nothing`.
//
// Expected IR of `total`, optimized:
// - The parameter `e` carries `noalias`.
// - The loop contains the call to `do_nothing`.
// - The loop contains no load of `hp` and no store. `hp` is loaded at most
//   once, outside the loop.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn read_across_barrier() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.do_nothing;
struct Entity { hp int; }
#inline(never)
func total<g'>(e &Entity in g, n int) int {
  sum = 0;
  i = 0;
  while i < __copy_prim(n) {
    set sum = sum + __copy_prim(e.hp);
    do_nothing();
    set i = i + 1;
  }
  return sum;
}
exported func main() int {
  e = Entity(2);
  return total(&e, 50);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
