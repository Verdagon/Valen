// Fence: ensures that Valen reads a value once through a reference stored in a
// struct field, when a write to another object sits between two uses of it.
//
// The written entity is reached through a `noalias` parameter, so a pointer
// loaded out of a struct field cannot alias it, in either language. Valen's two
// groups say the same thing no more strongly. All three arms compile to the
// same code. This test shows no Valen advantage.
//
// What the test is for: it is the only one in the corpus where a borrow is
// stored in a struct field and carried across a function boundary.
//
// The program:
//   MoveResult<t'> { blocked_by &TileComponent in t; cost int; }
//   try_move(tiles &Vec<Tile> in t, e &Entity in g) MoveResult<t> returns a
//   result that holds a reference to a component of the tile ahead.
//   resolve(e &Entity in g, r &MoveResult<t>) mut(g):
//     set e.stamina = e.stamina - r.cost - r.blocked_by.cover;
//     set e.hp = e.hp - r.blocked_by.cover;
//   main calls try_move then resolve, twice.
//
// Mechanism: `!alias.scope` on loads and stores (M2), and parameter `noalias`.
// The store to `e.stamina` is in group `g`; the load of `r.blocked_by.cover` is
// in group `t`. So the second read of `cover` is the first read's value. That
// fact comes from the borrow checker's aliasing info, so this test runs with
// the checker on.
//
// Two variations that would make the arms differ, and why neither is a new
// claim:
// - Put the entity and the component in one group. Then Valen reloads `cover`
//   after the store too, and plain Rust, with `Cell` fields and two shared
//   references, reloads it as well. Equal again.
// - Hold the entity as a local reference, not a parameter. rustc marks only
//   parameters `noalias`. That is valen/forwarding_across_groups.rs. The stored
//   borrow adds nothing to it.
//
// Arms:
// - Plain Rust: rust/move_result_references_tile_plain.rs
// - GhostCell:  rust/move_result_references_tile.rs
//
// Size of the win: none, against either arm.
//
// Assumptions behind the expected IR:
// - `#inline(never)` keeps `resolve` and `try_move` functions, so `e` and `r`
//   stay parameters of `resolve`.
// - `at` inlines into its callers.
// - The test is built with the `borrow_checker_experimental` feature.
// - main re-derives `e` for each call, because a `mut(g)` call invalidates a
//   borrow into an imported `Vec`. The borrow checker requires it. The Rust
//   arms bind `e` once and hold it, so outside the measured function Valen does
//   one lookup per iteration that Rust does not.
//
// Expected IR of `resolve`, unoptimized:
// - Two loads of the `blocked_by` pointer, two loads of `cover`, a load and a
//   store of `stamina`, a load and a store of `hp`.
//
// Expected IR of `resolve`, optimized:
// - One load of `cover`, used by both statements. No load of `cover` follows
//   the store to `stamina`.
// - One load and one store each of `stamina` and `hp`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn move_result_references_tile() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct TileComponent { cover int; }
struct Tile { components Vec<TileComponent, Global>; }
struct Entity { hp int; stamina int; next i64; }
struct Level { tiles Vec<Tile, Global>; entities Vec<Entity, Global>; }
struct MoveResult<t'> { blocked_by &TileComponent in t; cost int; }
#inline(never)
func try_move<t', g'>(tiles &Vec<Tile, Global> in t, e &Entity in g) MoveResult<t> {
  target = at(tiles, __copy_prim(e.next));
  return MoveResult(at(&target.components, 1i64), 2);
}
#inline(never)
func resolve<g', t'>(e &Entity in g, r &MoveResult<t>) mut(g) {
  set e.stamina = __copy_prim(e.stamina) - __copy_prim(r.cost) - __copy_prim(r.blocked_by.cover);
  set e.hp = __copy_prim(e.hp) - __copy_prim(r.blocked_by.cover);
}
exported func main() int {
  level = Level(Vec.new<Tile>(), Vec.new<Entity>());
  c0 = Vec.new<TileComponent>();
  c0.push(TileComponent(0));
  level.tiles.push(Tile(^c0));
  c1 = Vec.new<TileComponent>();
  c1.push(TileComponent(0));
  c1.push(TileComponent(3));
  level.tiles.push(Tile(^c1));
  level.entities.push(Entity(30, 20, 1i64));
  n = 0;
  while n < 2 {
    e = at(&level.entities, 0i64);
    r = try_move(&level.tiles, e);
    resolve(e, &r);
    set n = n + 1;
  }
  done = at(&level.entities, 0i64);
  return __copy_prim(done.hp) + __copy_prim(done.stamina);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(34),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
