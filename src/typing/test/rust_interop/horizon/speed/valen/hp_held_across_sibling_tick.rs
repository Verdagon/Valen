// Red by design: ensures that a call handed the whole level, which changes only
// the level's clock, does not force Valen to reload an entity's field.
//
// Valen does not do this today, and the current design cannot. The expected IR
// below is the intended future shape. Blocker: proposal S18 in
// src/typing/docs/architecture/borrowing-design.md.
//
// The program:
//   Level { entities Vec<Entity>, clock int }
//   advance_clock(level &Level in g) mut(level.clock)
//     adds one to the clock and calls do_nothing().
//   burn(level &Level in g, i i64, n int) mut(level.entities...) mut(level.clock)
//   e = at(&level.entities, i)
//   loop n times: set e.hp = e.hp - 1; advance_clock(level);
//   heal(a &Entity in g, b &Entity in g) mut(g) adds b's hp to a's. It is
//   called with two entities and with one entity twice. It is why the GhostCell
//   arm keeps entities in cells.
//
// This is the shape of most real code: a method that takes the whole object and
// changes one part of it. The held reference may be several containers deep;
// the saving is then the whole path to it, not one load.
//
// Why Valen reloads today: a call is marked by what its arguments reach, never
// by what the callee declares it mutates. `advance_clock` is handed `level`, so
// it reaches every group under the level, the entities included, and gets no
// `!noalias` mark for them. Its `mut(level.clock)` clause says it does not
// write the entities, but it may still read them, and the mark LLVM offers for
// a call means "touches none of this memory"; it has no form for "reads it and
// does not write it". S18 is the proposal to tell LLVM that.
//
// Why no test that passes today can be the strongest kind of evidence: Valen
// marks a call by what its arguments reach, so a win that works today needs a
// callee whose arguments do not reach the held data. The same program in Rust
// with `Cell` hands the callee the same arguments, so safe code cannot reach
// the held data there either. Rust's types fail to say it only when the callee
// is handed something that reaches the held data, as here, and then Valen has
// no mark either. That is what S18 would change.
//
// What the intended shape is, and is not: with S18 the load of `e.hp` after
// the call goes away. The store before the call stays, because the callee may
// read the entity. So `hp` is not simply kept in a register across the call.
//
// Arms:
// - Plain Rust: rust/hp_held_across_sibling_tick_plain.rs
// - GhostCell:  rust/hp_held_across_sibling_tick.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `burn` and `advance_clock` functions, so the call
//   stays in the loop and is handed the level.
// - `at` is inlined into `burn`.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, and the call to
//   `advance_clock`.
//
// Expected IR of `burn`, optimized, today:
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `hp`, stores `hp`, and calls `advance_clock`. The load
//   follows the previous iteration's call.
//
// Expected IR of `burn`, optimized, intended (needs S18):
// - Each iteration stores `hp` and calls `advance_clock`.
// - The loop contains no load of `hp`. The one load of `hp` sits before the
//   loop.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn hp_held_across_sibling_tick() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Entity { hp int; }
struct Level { entities Vec<Entity, Global>; clock int; }
func heal<g'>(a &Entity in g, b &Entity in g) mut(g) {
  set a.hp = __copy_prim(a.hp) + __copy_prim(b.hp);
}
#inline(never)
func advance_clock<g'>(level &Level in g) mut(level.clock) {
  set level.clock = __copy_prim(level.clock) + 1;
  do_nothing();
}
#inline(never)
func burn<g'>(level &Level in g, i i64, n int) mut(level.entities...) mut(level.clock) {
  e = at(&level.entities, __copy_prim(i));
  k = 0;
  while k < __copy_prim(n) {
    set e.hp = __copy_prim(e.hp) - 1;
    advance_clock(level);
    set k = k + 1;
  }
}
exported func main() int {
  level = Level(Vec.new<Entity>(), 0);
  level.entities.push(Entity(111));
  level.entities.push(Entity(9));
  heal(at(&level.entities, 0i64), at(&level.entities, 1i64));
  heal(at(&level.entities, 1i64), at(&level.entities, 1i64));
  burn(&level, 0i64, 50);
  return __copy_prim(at(&level.entities, 0i64).hp) + __copy_prim(level.clock);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(120),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
