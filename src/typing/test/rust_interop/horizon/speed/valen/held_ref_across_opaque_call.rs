// Fence: ensures that Valen keeps a field in a register across an opaque call
// when the field is reached through a reference bound once from a `Vec`.
//
// The program:
//   bump(ships &Vec<Ship, Global> in g, n int) mut(g)
//   s = at(ships, 0)
//   loop n times: set s.fuel = s.fuel + 1; do_nothing();
//   refuel(a &Ship in g, b &Ship in g) mut(g) adds b's fuel to a's. It is called
//   with two ships and with one ship twice. It is why the GhostCell arm keeps
//   ships in cells.
//
// Mechanism: M3 by argument reach. `do_nothing()` receives no argument, so it
// reaches no group, and Valen marks the call `!noalias` against the scope that
// the store to `s.fuel` carries. Parameter `noalias` does not help here: `s`
// points into the `Vec`'s buffer, which is reached through a pointer loaded from
// `ships`, not through `ships` itself. The `!noalias` comes from the borrow
// checker's aliasing info, so this test runs with the checker on.
//
// This is the one situation where rustc could catch up without a language
// change: both Rust arms hold a reference across the call, as this arm does.
// The test exists so the cases where Rust cannot catch up can be seen not to be
// this one.
//
// Arms:
// - Plain Rust: rust/held_ref_across_opaque_call_plain.rs
// - GhostCell:  rust/held_ref_across_opaque_call.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `at` is inlined into `bump`.
// - `#inline(never)` keeps `bump` a function.
//
// Expected IR of `bump`, unoptimized:
// - The loop contains a load of `fuel`, a store of `fuel`, and the call to
//   `do_nothing`.
//
// Expected IR of `bump`, optimized:
// - The call to `do_nothing` carries `!noalias` naming the scope of the store to
//   `fuel`.
// - The loop contains the call to `do_nothing` and one store of `fuel`.
// - The loop contains no load of `fuel`. The one load of `fuel` sits before the
//   loop.
// - The `Vec`'s buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn held_ref_across_opaque_call() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Ship { fuel int; }
func refuel<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set a.fuel = __copy_prim(a.fuel) + __copy_prim(b.fuel);
}
#inline(never)
func bump<g'>(ships &Vec<Ship, Global> in g, n int) mut(g) {
  s = at(ships, 0i64);
  i = 0;
  while i < __copy_prim(n) {
    set s.fuel = __copy_prim(s.fuel) + 1;
    do_nothing();
    set i = i + 1;
  }
}
exported func main() int {
  ships = Vec.new<Ship>();
  ships.push(Ship(7));
  ships.push(Ship(9));
  refuel(at(&ships, 0i64), at(&ships, 1i64));
  refuel(at(&ships, 1i64), at(&ships, 1i64));
  bump(&ships, 50);
  return __copy_prim(at(&ships, 0i64).fuel) + __copy_prim(at(&ships, 1i64).fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(84),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
