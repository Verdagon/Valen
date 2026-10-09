// Red by design: records that Valen cannot yet store, inside an entity, a
// reference to a tile owned by the same level.
//
// The program:
//   Entity<t'> { hp int; standing_on &Tile in t; }
//   Level { tiles Vec<Tile>; entities Vec<Entity<in self.tiles[]>>; }
//   tick(level &Level in g, count i64) mut(g)
//   for each of the first `count` entities: set e.hp = e.hp - e.standing_on.hazard;
//   then each tile's hazard goes up by one.
//   main calls tick with two different counts, so the optimizer cannot fold it.
//
// Blocker: a struct field whose type names a sibling field's path,
// `Vec<Entity<in self.tiles[]>, Global>`. The spelling is ruled, and the parser
// does not accept `in` inside a type yet (src/parsing/templex_parser.rs).
//
// What this would show, and what it would not: this is about holding a
// reference, not about alias information. No `!noalias` stamp or load/store
// scope is involved. The Valen arm follows a stored pointer to the tile. Rust
// cannot store that pointer, because the level would borrow its own field.
//
// Rust already matches the intended shape, with a branded index: an index type
// that can only be made by checking a number against one particular collection,
// and that then needs no bounds check. What Rust needs for that: a brand
// lifetime threaded through `Level` and `Entity`, and tiles that never shrink
// while the brand lives. What Valen needs to match it: stored sibling
// references. So even with the feature, this test is a fence.
//
// Arms:
// - Plain Rust (a `usize` index, bounds-checked): rust/entity_stores_its_tile_plain.rs
// - Branded index (in the GhostCell slot): rust/entity_stores_its_tile.rs
//
// Size of the win once the feature exists:
// - Against the branded-index arm: none.
// - Against the plain `usize` arm: one bounds check per entity.
// Both arms' loops were read from optimized IR.
//
// Assumptions behind the expected IR:
// - `#inline(never)` keeps `tick` a function.
// - `at` inlines into its callers.
// - Valen accepts the field type above.
// - The test is built with the `borrow_checker_experimental` feature.
// - The exit value 140 is verified in both Rust arms only.
//
// Expected IR of `tick`, unoptimized:
// - The loop contains a load of the `standing_on` pointer, a load of `hazard`,
//   and a load and a store of `hp`.
//
// Expected IR of `tick`, optimized, once the feature exists:
// - The loop contains a load of the `standing_on` pointer, a load of `hazard`,
//   and a load and a store of `hp`.
// - The loop contains no bounds check on the tile.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn entity_stores_its_tile() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Tile { hazard int; }
struct Entity<t'> { hp int; standing_on &Tile in t; }
struct Level { tiles Vec<Tile, Global>; entities Vec<Entity<in self.tiles[]>, Global>; }
#inline(never)
func tick<g'>(level &Level in g, count i64) mut(g) {
  i = 0i64;
  while i < __copy_prim(count) {
    e = at(&level.entities, __copy_prim(i));
    set e.hp = __copy_prim(e.hp) - __copy_prim(e.standing_on.hazard);
    set i = i + 1i64;
  }
  set at(&level.tiles, 0i64).hazard = __copy_prim(at(&level.tiles, 0i64).hazard) + 1;
  set at(&level.tiles, 1i64).hazard = __copy_prim(at(&level.tiles, 1i64).hazard) + 1;
}
exported func main() int {
  level = Level(Vec.new<Tile>(), Vec.new<Entity>());
  level.tiles.push(Tile(1));
  level.tiles.push(Tile(2));
  level.entities.push(Entity(50, at(&level.tiles, 0i64)));
  level.entities.push(Entity(50, at(&level.tiles, 1i64)));
  level.entities.push(Entity(50, at(&level.tiles, 0i64)));
  tick(&level, 2i64);
  tick(&level, 3i64);
  return __copy_prim(at(&level.entities, 0i64).hp) + __copy_prim(at(&level.entities, 1i64).hp) + __copy_prim(at(&level.entities, 2i64).hp);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(140),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
