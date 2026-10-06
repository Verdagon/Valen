// Fence: ensures that Valen keeps a tile's field in a register across a call
// that structurally changes a sibling collection.
//
// A callee that structurally changes a sibling collection costs Valen nothing.
// It costs plain Rust nothing either when the two are passed apart: the tile
// and the entities are two disjoint `&mut` at the call site, and the held tile
// is a `noalias` parameter. All three arms compile to the same loop. This test
// shows no Valen advantage.
//
// The program:
//   Level { tiles Vec<Tile>; entities Vec<Entity>; }
//   cleanup(entities &Vec<Entity> in h) mut(h) clears the entities and pushes a
//   fresh one.
//   wave(t &Tile in g, entities &Vec<Entity> in h, n int) int mut(g) mut(h)
//   loop n times: the first entity loses t.hazard hp; cleanup(entities);
//                 set t.hazard = t.hazard + 1.
//   main calls wave with two different tiles and two different counts, so the
//   optimizer cannot fold `n`.
//
// Mechanism: `!noalias` on a call, by argument reach (M3), and parameter
// `noalias`. `cleanup` is handed only `entities`, in group `h`, so it cannot
// reach the tile in group `g`. That fact comes from the borrow checker's
// aliasing info, so this test runs with the checker on.
//
// The structural change does not matter for alias information. Clearing and
// pushing changes which entities exist, which matters to the borrow checker (a
// reference to an entity would die). It tells the optimizer nothing more about
// the tile than a field write would. That is why this test is a fence: plain
// Rust says the same thing with `&mut Tile` beside `&mut Vec<Entity>`, two
// disjoint fields borrowed at the call.
//
// Where the difference used to be claimed: an earlier form of this test made
// `cleanup` a method on the level. In Valen that is `cleanup(level)
// mut(level.entities)`. The callee is then handed the level, reaches the tiles,
// and gets no stamp. That is blocked on S18
// (src/typing/docs/architecture/borrowing-design.md), the same as
// valen/hp_held_across_sibling_tick.rs.
//
// Arms:
// - Plain Rust: rust/tile_refs_across_cleanup_plain.rs
// - GhostCell:  rust/tile_refs_across_cleanup.rs
//
// Size of the win: none, against either arm.
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `wave` and `cleanup` functions, so the call to
//   `cleanup` stays in the loop and `t` stays a parameter.
// - `at`, `clear` and `push` inline into their callers.
// - The test is built with the `borrow_checker_experimental` feature.
//
// Expected IR of `wave`, unoptimized:
// - The loop contains a load of `t.hazard` (twice), a store of `t.hazard`, a
//   load and a store of the entity's `hp`, and the call to `cleanup`.
//
// Expected IR of `wave`, optimized:
// - `t.hazard` is loaded once, before the loop.
// - The loop contains the call to `cleanup` and one store of `t.hazard`.
// - The loop contains no load of `t.hazard`.
// - The entities' pointer, length and first element are loaded again after
//   every call to `cleanup`, which changes them.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn tile_refs_across_cleanup() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Tile { hazard int; }
struct Entity { hp int; }
struct Level { tiles Vec<Tile, Global>; entities Vec<Entity, Global>; }
#inline(never)
func cleanup<h'>(entities &Vec<Entity, Global> in h) mut(h) {
  entities.clear();
  entities.push(Entity(10));
  do_nothing();
}
#inline(never)
func wave<g', h'>(t &Tile in g, entities &Vec<Entity, Global> in h, n int) int mut(g) mut(h) {
  total = 0;
  i = 0;
  while i < __copy_prim(n) {
    e = at(entities, 0i64);
    set e.hp = __copy_prim(e.hp) - __copy_prim(t.hazard);
    set total = total + (10 - __copy_prim(e.hp));
    cleanup(entities);
    set t.hazard = __copy_prim(t.hazard) + 1;
    set i = i + 1;
  }
  return total;
}
exported func main() int {
  level = Level(Vec.new<Tile>(), Vec.new<Entity>());
  level.tiles.push(Tile(1));
  level.tiles.push(Tile(2));
  level.entities.push(Entity(10));
  s1 = wave(at(&level.tiles, 0i64), &level.entities, 3);
  s2 = wave(at(&level.tiles, 1i64), &level.entities, 4);
  t0 = at(&level.tiles, 0i64);
  t1 = at(&level.tiles, 1i64);
  return s1 + s2 + __copy_prim(t0.hazard) + __copy_prim(t1.hazard);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(30),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
