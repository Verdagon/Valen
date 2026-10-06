// Red by design: records that Valen does not yet keep tile fields in registers
// across a call that is handed the whole level but changes only its clock.
//
// The program:
//   Level { tiles HashMap<int, Tile>; clock int; }
//   tick(level &Level) changes only level.clock.
//   spread(level &Level, a &Tile in level.tiles..., b &Tile in level.tiles...,
//          n int)
//   loop n times: set a.hazard = a.hazard + b.cover; tick(level);
//   main calls spread with two tiles, then with one tile as both a and b. The
//   two calls pass different counts, so the optimizer cannot fold `n`.
//
// Blocker: S18 in src/typing/docs/architecture/borrowing-design.md. Valen
// stamps a call by what its arguments reach. `tick` is handed `level`, which
// reaches `level.tiles...`, so the call gets no `!noalias` against the tiles.
// The stamp cannot simply be added from `tick`'s `mut(level.clock)` clause:
// `tick` may still read the tiles, and LLVM's `!noalias` on a call says "does
// not read or write", with no way to say "does not write".
//
// Why this shape matters: it is the one where Rust's types cannot say what the
// callee leaves alone. A win that works today needs a callee whose arguments do
// not reach the held data, and the same program in Rust with `Cell` hands the
// callee the same arguments, so safe code cannot reach the held data there
// either. Rust's types fail only when the callee is handed something that
// reaches the held data, as here, and then Valen has no stamp either.
//
// Why the test is kept: this is the most natural shape for a game tick, and
// the one a reader will try first. The reach-limited version that Valen can
// optimize today is valen/two_tiles_across_other_collection_call.rs.
//
// Arms:
// - Plain Rust: rust/two_tiles_may_coincide_plain.rs
// - GhostCell:  rust/two_tiles_may_coincide.rs
//
// Size of the win once S18 is resolved, per iteration. Each one load comes
// from the call stamp, which is the part blocked on S18.
// - Against the plain arm (tile fields and clock as `Cell`s, two shared
//   references held): one load.
// - Against the GhostCell arm: one load.
// - Against the plain spelling not used (the level and two keys): two table
//   lookups and one load. Those lookups would come from holding references,
//   which Valen does today.
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `spread` and `tick` functions, so the call to `tick`
//   stays in the loop.
// - `get` and `unwrap` inline into their callers.
// - S18 is resolved: Valen can tell LLVM that a call does not write a group it
//   can reach.
// - The test is built with the `borrow_checker_experimental` feature.
// - Valen can compile the `HashMap` lookups. Today it cannot, for the two
//   reasons listed in valen/two_tiles_across_other_collection_call.rs.
// - Tiles are keyed by `int` until a Valen struct can be a `HashMap` key.
// - The parameter spelling `in level.tiles...` is unverified: nothing can
//   compile this program yet.
// - The exit value 36 is verified in both Rust arms only.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `b.cover`, a load of `a.hazard`, a store of
//   `a.hazard`, and the call to `tick`.
//
// Expected IR of `spread`, optimized, once S18 is resolved:
// - The loop contains the call to `tick`.
// - The store of `a.hazard` before the call stays, because `tick` may read it.
// - No load of `a.hazard` or `b.cover` follows the call.
// - The loop contains one load of `b.cover`, which follows the store to
//   `a.hazard`: both are in one scope, and `a` may be `b`.
//
// Expected IR of `spread`, optimized, today:
// - A load of `a.hazard` and a load of `b.cover` follow the call to `tick` on
//   every iteration.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn two_tiles_may_coincide() {
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
func spread(level &Level, a &Tile in level.tiles... mut, b &Tile in level.tiles..., n int) mut(level.clock) {
  i = 0;
  while i < __copy_prim(n) {
    set a.hazard = __copy_prim(a.hazard) + __copy_prim(b.cover);
    tick(level);
    set i = i + 1;
  }
}
exported func main() int {
  level = Level(HashMap.new<int, Tile>(), 0);
  level.tiles.insert(1, Tile(0, 2));
  level.tiles.insert(2, Tile(0, 3));
  k1 = 1;
  k2 = 2;
  spread(&level, (level.tiles.get(&k1)).unwrap(), (level.tiles.get(&k2)).unwrap(), 4);
  spread(&level, (level.tiles.get(&k2)).unwrap(), (level.tiles.get(&k2)).unwrap(), 5);
  t1 = (level.tiles.get(&k1)).unwrap();
  t2 = (level.tiles.get(&k2)).unwrap();
  return __copy_prim(t1.hazard) + __copy_prim(t2.hazard) + __copy_prim(level.clock);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(36),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
