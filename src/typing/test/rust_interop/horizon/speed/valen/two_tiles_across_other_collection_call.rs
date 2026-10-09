// Ensures that Valen looks two tiles up once and holds them across a loop that
// writes one, reads the other, and calls a function handed a different
// collection, when the two tiles may be the same tile.
//
// The program:
//   Level { tiles HashMap<int, Tile>; entities Vec<Entity>; }
//   spread(a &Tile in g, b &Tile in g, entities &Vec<Entity> in h, n int)
//       mut(g) mut(h)
//   loop n times: set a.hazard = a.hazard + b.cover; muster(entities);
//   muster(entities &Vec<Entity> in h) mut(h) writes entities[0].hp.
//   main looks the tiles up by key, then calls spread with two tiles, then with
//   one tile as both a and b. The two calls pass different counts, so the
//   optimizer cannot fold `n`.
//
// Mechanism: `!noalias` on a call, by argument reach (M3). `muster` is handed
// only `entities`, which is in group `h`. Its arguments cannot reach group `g`,
// so Valen marks the call as unable to touch `g`, and LLVM need not reload
// `a.hazard` or `b.cover` after it. That fact comes from the borrow checker's
// aliasing info, so this test runs with the checker on. Valen cannot compile
// this program yet (see the last two assumptions), so the test is red.
//
// What Valen does not claim: that `a` and `b` are different tiles. Both are in
// `g`, so the store to `a.hazard` may change `b.cover`, and `b.cover` is
// reloaded after that store.
//
// Size of the win, per iteration, and what causes each part:
// - Against the plain arm: one load, from the call stamp. The plain arm gives
//   the tile fields `Cell` types, holds two shared references that may be the
//   same tile, and does no lookups, but loads `a.hazard` and `b.cover` after
//   every call to `muster`.
// - Against the plain spelling not used (the map and two keys): two table
//   lookups and one load. The lookups would come from holding references, not
//   from the call stamp.
// - Against the GhostCell arm: one load, from the call stamp. It holds both
//   cell pointers and does no lookups, but loads `a.hazard` and `b.cover` after
//   every call to `muster`; Valen loads only `b.cover`.
//
// Arms:
// - Plain Rust: rust/two_tiles_across_other_collection_call_plain.rs
// - GhostCell:  rust/two_tiles_across_other_collection_call.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `spread` and `muster` functions, so the call to
//   `muster` stays in the loop.
// - `at`, `get` and `unwrap` inline into their callers.
// - Valen can write through a borrow that `HashMap.get` returned, the same way
//   it can through one that `at` returned.
// - The test is built with the `borrow_checker_experimental` feature.
// - The borrow checker can check a program that uses `HashMap`. Today the
//   experimental checker panics on this one ("callee group rune
//   ImplicitGroupRune ... not bound at this call", in `groupify_group_expr`,
//   src/typing/borrow_checker/experimental/groupify.rs).
// - Valen can call `unwrap` on the `Option<&Tile>` that `get` returns. Today,
//   with the checker off, interop lowering panics on it ("cannot lower generic
//   type argument of Rust callee `unwrap` to a rustc type", in
//   src/instantiating/rust_interop/horizon/resolve_request.rs).
// - Tiles are keyed by `int` until a Valen struct can be a `HashMap` key.
// - The exit value 36 is verified in both Rust arms only.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `b.cover`, a load of `a.hazard`, a store of
//   `a.hazard`, and the call to `muster`.
// - The loop contains no call into `HashMap`.
//
// Expected IR of `spread`, optimized:
// - The loop contains the call to `muster`, and the call carries `!noalias`
//   naming the scope of group `g`.
// - The loop contains one store of `a.hazard` and no load of `a.hazard`.
// - The loop contains one load of `b.cover`, which follows the store to
//   `a.hazard`. No load follows the call to `muster`.
// - The loop contains no hashing and no call into `HashMap`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn two_tiles_across_other_collection_call() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
import std.collections.HashMap;
import std.hash.random.RandomState;
import std.option.Option;
struct Tile { hazard int; cover int; }
struct Entity { hp int; }
struct Level { tiles HashMap<int, Tile, RandomState, Global>; entities Vec<Entity, Global>; }
#inline(never)
func muster<h'>(entities &Vec<Entity, Global> in h) mut(h) {
  e = at(entities, 0i64);
  set e.hp = __copy_prim(e.hp) + 1;
  do_nothing();
}
#inline(never)
func spread<g', h'>(a &Tile in g, b &Tile in g, entities &Vec<Entity, Global> in h, n int) mut(g) mut(h) {
  i = 0;
  while i < __copy_prim(n) {
    set a.hazard = __copy_prim(a.hazard) + __copy_prim(b.cover);
    muster(entities);
    set i = i + 1;
  }
}
exported func main() int {
  level = Level(HashMap.new<int, Tile>(), Vec.new<Entity>());
  level.tiles.insert(1, Tile(0, 2));
  level.tiles.insert(2, Tile(0, 3));
  level.entities.push(Entity(0));
  k1 = 1;
  k2 = 2;
  spread((level.tiles.get(&k1)).unwrap(), (level.tiles.get(&k2)).unwrap(), &level.entities, 4);
  spread((level.tiles.get(&k2)).unwrap(), (level.tiles.get(&k2)).unwrap(), &level.entities, 5);
  t1 = (level.tiles.get(&k1)).unwrap();
  t2 = (level.tiles.get(&k2)).unwrap();
  e0 = at(&level.entities, 0i64);
  return __copy_prim(t1.hazard) + __copy_prim(t2.hazard) + __copy_prim(e0.hp);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(36),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
