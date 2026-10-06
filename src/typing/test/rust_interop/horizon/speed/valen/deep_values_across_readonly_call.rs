// Red by design: ensures that a call handed the whole level, which changes
// nothing, does not force Valen to reload a value two pointers deep in the
// level.
//
// Valen does not do this today, and the current design cannot. The expected IR
// below gives both today's shape and the intended future shape. Blocker:
// proposal S18 in src/typing/docs/architecture/borrowing-design.md.
//
// The program:
//   Level { tiles Vec<Tile>, clock int }, Tile { components Vec<TileComponent> }
//   census(level &Level in g) int calls do_nothing() and returns the clock. It
//   declares no `mut`.
//   survey(level &Level in g, n int) int, which also declares no `mut`:
//   c = at(&at(&level.tiles, 0).components, 1)
//   loop n times: total = total + c.value + census(level);
//   trade(a &TileComponent in g, b &TileComponent in g) mut(g) adds b's value
//   to a's. It is called with two components and with one component twice. It
//   is why the GhostCell arm keeps components in cells.
//
// Why Valen reloads today: a call is marked by what its arguments reach, never
// by what the callee declares it mutates. `census` is handed `level`, so it
// reaches every group under the level, the tile components included, and gets
// no `!noalias` mark for them. `census` declares no `mut`, so it writes nothing,
// but the mark LLVM offers for a call means "touches none of this memory"; it
// has no form for "reads it and does not write it". S18 is the proposal to tell
// LLVM that.
//
// What the intended shape is: with S18, `c.value` is loaded once, before the
// loop. Nothing in `survey` writes it either, so there is no store to keep.
//
// Arms:
// - Plain Rust: rust/deep_values_across_readonly_call_plain.rs
// - GhostCell:  rust/deep_values_across_readonly_call.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `survey` and `census` functions, so the call stays in
//   the loop and is handed the level.
// - `at` is inlined into `survey`.
// - `main` reaches each component afresh for each call to `trade`. A borrow of
//   the tile cannot be held across `trade`: an imported `Vec` is opaque, so a
//   borrow into it covers everything under it, and `trade`'s `mut(g)` on the
//   components invalidates it.
//
// Expected IR of `survey`, unoptimized:
// - The loop contains a load of `c.value` and the call to `census`.
//
// Expected IR of `survey`, optimized, today:
// - Both `at` calls sit before the loop: the loop loads no buffer pointer or
//   length and has no bounds check.
// - Each iteration loads `c.value` and calls `census`.
//
// Expected IR of `survey`, optimized, intended (needs S18):
// - Each iteration calls `census`.
// - The loop contains no load of `c.value`. The one load of `c.value` sits
//   before the loop.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn deep_values_across_readonly_call() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct TileComponent { kind int; value int; }
struct Tile { components Vec<TileComponent, Global>; }
struct Level { tiles Vec<Tile, Global>; clock int; }
func trade<g'>(a &TileComponent in g, b &TileComponent in g) mut(g) {
  set a.value = __copy_prim(a.value) + __copy_prim(b.value);
}
#inline(never)
func census<g'>(level &Level in g) int {
  do_nothing();
  return __copy_prim(level.clock);
}
#inline(never)
func survey<g'>(level &Level in g, n int) int {
  c = at(&at(&level.tiles, 0i64).components, 1i64);
  total = 0;
  i = 0;
  while i < __copy_prim(n) {
    set total = total + __copy_prim(c.value) + census(level);
    set i = i + 1;
  }
  return total;
}
exported func main() int {
  level = Level(Vec.new<Tile>(), 2);
  comps = Vec.new<TileComponent>();
  comps.push(TileComponent(0, 1));
  comps.push(TileComponent(1, 3));
  level.tiles.push(Tile(^comps));
  trade(at(&at(&level.tiles, 0i64).components, 1i64), at(&at(&level.tiles, 0i64).components, 0i64));
  trade(at(&at(&level.tiles, 0i64).components, 1i64), at(&at(&level.tiles, 0i64).components, 1i64));
  return survey(&level, 10);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
