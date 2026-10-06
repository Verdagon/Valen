// Ensures that Valen keeps an entity's field in a register across an opaque
// call, and writes it back and reloads it only around a call that is handed
// the level.
//
// The program:
//   Level { entities Vec<Entity> }
//   burn(level &Level in g, steps int) int mut(level.entities...)
//   e = at(&level.entities, 0)
//   loop steps times: set e.hp = e.hp - 1; do_nothing();
//                     on one step: seen = trace(level);
//   trace(level &Level in g) int reads entity 0's hp. It is the observer.
//   heal(a &Entity in g, b &Entity in g) mut(g) adds b's hp to a's. It is
//   called with two entities and with one entity twice. It is why the GhostCell
//   arm keeps entities in cells.
//
// Mechanism: M3 by argument reach, applied per call. `do_nothing()` receives no
// argument, so it reaches no group, and Valen marks that call `!noalias`
// against the scope of the store to `e.hp`. `trace(level)` is handed the level,
// so it reaches the entities and gets no such mark. That fact comes from the
// borrow checker's aliasing info, so this test runs with the checker on.
//
// The observer is what forbids keeping `hp` in a local by hand and writing it
// back at the end: `trace` must see the current value in memory. The compiler
// has to do the bookkeeping.
//
// What Valen does not claim: that `trace` leaves `e.hp` unchanged. `trace`
// declares no `mut`, but a call's mark has no way to say "reads this group and
// does not write it" (proposal S18 in
// src/typing/docs/architecture/borrowing-design.md). So `e.hp` is reloaded
// after `trace`.
//
// Arms:
// - Plain Rust: rust/accumulator_with_observer_plain.rs
// - GhostCell:  rust/accumulator_with_observer.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `burn` and `trace` functions, so the call to `trace`
//   stays in the loop and is handed the level.
// - `at` is inlined into `burn`.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, the call to
//   `do_nothing`, and, on the branch taken when `pc == 49`, the call to `trace`.
//
// Expected IR of `burn`, optimized:
// - The call to `do_nothing` carries `!noalias` naming the scope of the store
//   to `hp`. The call to `trace` does not.
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration stores `hp` once, before the call to `do_nothing`.
// - No load of `hp` follows the call to `do_nothing`.
// - The only load of `hp` in the loop follows the call to `trace`, in the block
//   that runs when `pc == 49`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn accumulator_with_observer() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Entity { hp int; }
struct Level { entities Vec<Entity, Global>; }
func heal<g'>(a &Entity in g, b &Entity in g) mut(g) {
  set a.hp = __copy_prim(a.hp) + __copy_prim(b.hp);
}
#inline(never)
func trace<g'>(level &Level in g) int {
  return __copy_prim(at(&level.entities, 0i64).hp);
}
#inline(never)
func burn<g'>(level &Level in g, steps int) int mut(level.entities...) {
  e = at(&level.entities, 0i64);
  seen = 0;
  pc = 0;
  while pc < __copy_prim(steps) {
    set e.hp = __copy_prim(e.hp) - 1;
    do_nothing();
    if pc == 49 {
      set seen = trace(level);
    }
    set pc = pc + 1;
  }
  return seen;
}
exported func main() int {
  level = Level(Vec.new<Entity>());
  level.entities.push(Entity(111));
  level.entities.push(Entity(9));
  heal(at(&level.entities, 0i64), at(&level.entities, 1i64));
  heal(at(&level.entities, 1i64), at(&level.entities, 1i64));
  seen = burn(&level, 100);
  return seen + __copy_prim(at(&level.entities, 0i64).hp);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(90),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
