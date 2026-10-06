// Ensures that Valen returns an entity's field from the value it just stored,
// when a write to a tile sits between the store and the read.
//
// The program:
//   Level { tiles Vec<Tile>, entities Vec<Entity> }
//   scorch(level &Level in g, i i64, j i64) int
//       mut(level.entities...) mut(level.tiles...)
//     e = at(&level.entities, i); t = at(&level.tiles, j);
//     set e.hp = e.hp - 3; set t.hazard = t.hazard + 1; return e.hp;
//   heal(a &Entity in g, b &Entity in g) mut(g) adds b's hp to a's. It is
//   called with two entities and with one entity twice. It is why the GhostCell
//   arm keeps entities in cells.
//
// Mechanism: M2. The store to `t.hazard` carries the scope of `level.tiles`.
// The read of `e.hp` goes through a borrow into `level.entities`, a sibling
// path, and is marked `!noalias` against that scope. LLVM sees that the tile
// store cannot change `hp`, and returns the value just stored. These marks come
// from the borrow checker's aliasing info, so this test runs with the checker
// on.
//
// C compilers do this by default through struct-path type-based alias analysis
// (clang 17 at -O2 returns the stored value for the C equivalent of this
// program written as three small functions; with -fno-strict-aliasing the extra
// load returns). Rust has no such rule, because unsafe Rust may view the same
// memory as two types. This is Valen recovering what C has and Rust gave up; it
// is not a win from group borrowing. That is true of C's default and not of
// every C codebase: much real C, the Linux kernel for one, is built with
// -fno-strict-aliasing. One compiler and version was probed; GCC was not run.
//
// Both Rust arms hold a reference to the entity across the tile store, as this
// arm does, and both reload `hp` today. A rustc that marked local references
// could close that, with no change to the language.
//
// Arms:
// - Plain Rust: rust/forwarding_across_groups_plain.rs
// - GhostCell:  rust/forwarding_across_groups.rs
//
// Assumptions behind the expected IR:
// - `at` is inlined into `scorch`.
// - `#inline(never)` keeps `scorch` a function.
//
// Expected IR of `scorch`, unoptimized:
// - A load and a store of `hp`, then a load and a store of `hazard`, then a
//   second load of `hp`, which is returned.
//
// Expected IR of `scorch`, optimized:
// - One load of `hp`, before the store to `hp`.
// - No load of `hp` after the store to `hazard`. The returned value is the
//   value stored to `hp`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn forwarding_across_groups() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Tile { hazard int; }
struct Entity { hp int; }
struct Level { tiles Vec<Tile, Global>; entities Vec<Entity, Global>; }
func heal<g'>(a &Entity in g, b &Entity in g) mut(g) {
  set a.hp = __copy_prim(a.hp) + __copy_prim(b.hp);
}
#inline(never)
func scorch<g'>(level &Level in g, i i64, j i64) int mut(level.entities...) mut(level.tiles...) {
  e = at(&level.entities, __copy_prim(i));
  t = at(&level.tiles, __copy_prim(j));
  set e.hp = __copy_prim(e.hp) - 3;
  set t.hazard = __copy_prim(t.hazard) + 1;
  return __copy_prim(e.hp);
}
exported func main() int {
  level = Level(Vec.new<Tile>(), Vec.new<Entity>());
  level.tiles.push(Tile(2));
  level.tiles.push(Tile(4));
  level.entities.push(Entity(10));
  level.entities.push(Entity(40));
  heal(at(&level.entities, 0i64), at(&level.entities, 1i64));
  heal(at(&level.entities, 1i64), at(&level.entities, 1i64));
  first = scorch(&level, 0i64, 1i64);
  second = scorch(&level, 1i64, 1i64);
  return first + second + __copy_prim(at(&level.tiles, 1i64).hazard);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(130),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
