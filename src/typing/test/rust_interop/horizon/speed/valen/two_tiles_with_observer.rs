// Red by design: records that Valen does not yet keep tile fields in registers
// across calls that are handed the whole level, where one call changes only the
// clock and another only reads a tile.
//
// The program: valen/two_tiles_may_coincide.rs plus an observer.
//   tick(level &Level) changes only level.clock.
//   observe(level &Level, k int) int reads level.tiles[k].hazard.
//   spread(level &Level, a &Tile in level.tiles..., b &Tile in level.tiles...,
//          n int, watch int) int
//   loop n times: set a.hazard = a.hazard + b.cover; tick(level);
//                 on the third iteration, seen = observe(level, watch).
//   main calls spread with two tiles, then with one tile as both a and b, with
//   different counts, each time watching the tile being written.
//
// What the observer adds: it makes copying `a.hazard` into a local for the
// whole loop incorrect, in any language. `observe` reads the tile from memory
// part-way through, so the current value must be there. Only a compiler that
// knows which calls read the tile can keep the value in a register and still
// have it in memory when it is read.
//
// Blocker: S18 in src/typing/docs/architecture/borrowing-design.md. Both
// callees are handed `level`, which reaches `level.tiles...`, so neither call
// gets a `!noalias` against the tiles. LLVM's `!noalias` on a call says "does
// not read or write", with no way to say "does not write", which is what is
// true of both `tick` and `observe`.
//
// Why this shape matters: it is the one where Rust's types cannot say what the
// callee leaves alone. A win that works today needs a callee whose arguments do
// not reach the held data, and the same program in Rust with `Cell` hands the
// callee the same arguments, so safe code cannot reach the held data there
// either. Rust's types fail only when the callee is handed something that
// reaches the held data, as here, and then Valen has no stamp either.
//
// Arms:
// - Plain Rust: rust/two_tiles_with_observer_plain.rs
// - GhostCell:  rust/two_tiles_with_observer.rs
//
// Size of the win once S18 is resolved, per iteration: the same as
// valen/two_tiles_may_coincide.rs. The observer costs Valen nothing more: the
// store before each call already stays. Each one load comes from the call
// stamp, which is the part blocked on S18.
// - Against the plain arm (tile fields and clock as `Cell`s, two shared
//   references held): one load.
// - Against the GhostCell arm: one load.
// - Against the plain spelling not used (the level and two keys): two table
//   lookups and one load. Those lookups would come from holding references,
//   which Valen does today.
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `spread`, `tick` and `observe` functions, so both
//   calls stay in the loop.
// - `get` and `unwrap` inline into their callers.
// - S18 is resolved: Valen can tell LLVM that a call does not write a group it
//   can reach.
// - The test is built with the `borrow_checker_experimental` feature.
// - Valen can compile the `HashMap` lookups. Today it cannot, for the two
//   reasons listed in valen/two_tiles_across_other_collection_call.rs.
// - Tiles are keyed by `int` until a Valen struct can be a `HashMap` key.
// - The parameter spelling `in level.tiles...` is unverified: nothing can
//   compile this program yet.
// - The exit value 54 is verified in both Rust arms only.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `b.cover`, a load of `a.hazard`, a store of
//   `a.hazard`, the call to `tick`, and the conditional call to `observe`.
//
// Expected IR of `spread`, optimized, once S18 is resolved:
// - The loop contains the call to `tick` and the conditional call to `observe`.
// - The store of `a.hazard` before the calls stays, because both may read it.
// - No load of `a.hazard` or `b.cover` follows either call.
// - The loop contains one load of `b.cover`, which follows the store to
//   `a.hazard`: both are in one scope, and `a` may be `b`.
//
// Expected IR of `spread`, optimized, today:
// - A load of `a.hazard` and a load of `b.cover` follow the call to `tick` on
//   every iteration.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn two_tiles_with_observer() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.do_nothing;
import std.alloc.Global;
import std.collections.HashMap;
import std.hash.random.RandomState;
import std.option.Option;
struct Tile { hazard int; cover int; }
struct Level { tiles HashMap<int, Tile, RandomState, Global>; clock int; }
#inline(never)
func tick<l'>(level &Level in l) mut(l.clock) {
  set level.clock = __copy_prim(level.clock) + 1;
  do_nothing();
}
#inline(never)
func observe<l'>(level &Level in l, k int) int {
  return __copy_prim((level.tiles.get(&k)).unwrap().hazard);
}
#inline(never)
func spread(level &Level, a &Tile in level.tiles... mut, b &Tile in level.tiles..., n int, watch int) int mut(level.clock) {
  seen = 0;
  i = 0;
  while i < __copy_prim(n) {
    set a.hazard = __copy_prim(a.hazard) + __copy_prim(b.cover);
    tick(level);
    if i == 2 {
      set seen = observe(level, __copy_prim(watch));
    }
    set i = i + 1;
  }
  return seen;
}
exported func main() int {
  level = Level(HashMap.new<int, Tile>(), 0);
  level.tiles.insert(1, Tile(0, 2));
  level.tiles.insert(2, Tile(0, 3));
  k1 = 1;
  k2 = 2;
  s1 = spread(&level, (level.tiles.get(&k1)).unwrap(), (level.tiles.get(&k2)).unwrap(), 4, 1);
  s2 = spread(&level, (level.tiles.get(&k2)).unwrap(), (level.tiles.get(&k2)).unwrap(), 5, 2);
  t1 = (level.tiles.get(&k1)).unwrap();
  t2 = (level.tiles.get(&k2)).unwrap();
  return s1 + s2 + __copy_prim(t1.hazard) + __copy_prim(t2.hazard) + __copy_prim(level.clock);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(54),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
