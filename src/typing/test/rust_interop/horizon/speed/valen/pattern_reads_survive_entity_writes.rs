// Ensures that a write to an entity does not force Valen to reload a value of
// the level's pattern, which no entity write can change.
//
// The program:
//   Level { pattern Pattern, entities Vec<Entity> }, Pattern { rows Vec<Row> }
//   wound(level &Level in g, i i64) mut(level.entities...)
//     e = at(&level.entities, i); set e.hp = e.hp - rows[e.kind].cost
//   tire(level &Level in g, i i64) mut(level.entities...)
//     e = at(&level.entities, i); set e.stamina = e.stamina - rows[e.kind].cost
//   tick(level &Level in g, n i64) mut(level.entities...)
//     for each entity i: wound(level, i); tire(level, i);
//   heal(a &Entity in g, b &Entity in g) mut(g) adds b's hp to a's. It is
//   called with two entities and with one entity twice. It is why the GhostCell
//   arm keeps entities in cells.
//
// Mechanism: M2. The store to `e.hp` carries the scope of `level.entities`. The
// read of `cost` goes through a borrow into `level.pattern.rows`, a sibling
// path, and is marked `!noalias` against that scope. After `wound` and `tire`
// are inlined into `tick`, LLVM sees that the store to `e.hp` cannot change
// `cost`, and uses the first read's value for the second. These marks come from
// the borrow checker's aliasing info, so this test runs with the checker on.
//
// C compilers do this by default through struct-path type-based alias analysis
// (clang 17 at -O2 loads `cost` once per entity; with -fno-strict-aliasing the
// extra load returns). Rust has no such rule, because unsafe Rust may view the
// same memory as two types. This is Valen recovering what C has and Rust gave
// up; it is not a win from group borrowing. That is true of C's default and not
// of every C codebase: much real C, the Linux kernel for one, is built with
// -fno-strict-aliasing. One compiler and version was probed; GCC was not run.
//
// The two reads of `cost` are in two functions. Neither function reads it
// twice, so there is no repeated expression for a programmer to bind to a
// local. Two functions that each consult the pattern is the natural program:
// the borrow checker rejects nothing in the Rust arms' mirror of it, and
// holding the row across both would mean passing it in, which is an unprompted
// hand-optimization.
//
// Arms:
// - Plain Rust: rust/pattern_reads_survive_entity_writes_plain.rs
// - GhostCell:  rust/pattern_reads_survive_entity_writes.rs
//
// Assumptions behind the expected IR:
// - `at`, `wound` and `tire` are inlined into `tick`.
// - `#inline(never)` keeps `tick` a function, so `level` stays a `noalias`
//   parameter that is not captured. That is what lets the buffer pointers and
//   lengths of `level.entities` and `level.pattern.rows` be loaded once. The
//   scopes do not do it for `level.entities`: `at(&level.entities, i)` reaches
//   the same scope the entity stores carry. If the parameter is not `noalias`
//   and uncaptured, the entities' pointer and length are reloaded after each
//   entity store, and so is `e.kind`.
//
// Expected IR of `tick`, unoptimized:
// - Per entity, each of `wound` and `tire` loads both buffer pointers and both
//   lengths, checks both bounds, loads `kind`, and loads `cost`. `wound` loads
//   and stores `hp`; `tire` loads and stores `stamina`.
//
// Expected IR of `tick`, optimized:
// - Both buffer pointers and both lengths are loaded once, before the loop.
// - Per entity there is one load of `kind` and one bounds check against each
//   length.
// - Per entity there is one load of `cost`. No load of `cost` sits between the
//   store to `hp` and the store to `stamina`.
// - Per entity there is a load and a store of `hp`, and a load and a store of
//   `stamina`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn pattern_reads_survive_entity_writes() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Row { cost int; }
struct Pattern { rows Vec<Row, Global>; }
struct Entity { hp int; stamina int; kind i64; }
struct Level { pattern Pattern; entities Vec<Entity, Global>; }
func heal<g'>(a &Entity in g, b &Entity in g) mut(g) {
  set a.hp = __copy_prim(a.hp) + __copy_prim(b.hp);
}
func wound<g'>(level &Level in g, i i64) mut(level.entities...) {
  e = at(&level.entities, __copy_prim(i));
  set e.hp = __copy_prim(e.hp) - __copy_prim(at(&level.pattern.rows, __copy_prim(e.kind)).cost);
}
func tire<g'>(level &Level in g, i i64) mut(level.entities...) {
  e = at(&level.entities, __copy_prim(i));
  set e.stamina = __copy_prim(e.stamina) - __copy_prim(at(&level.pattern.rows, __copy_prim(e.kind)).cost);
}
#inline(never)
func tick<g'>(level &Level in g, n i64) mut(level.entities...) {
  i = 0i64;
  while i < __copy_prim(n) {
    wound(level, __copy_prim(i));
    tire(level, __copy_prim(i));
    set i = i + 1i64;
  }
}
exported func main() int {
  level = Level(Pattern(Vec.new<Row>()), Vec.new<Entity>());
  level.pattern.rows.push(Row(3));
  level.pattern.rows.push(Row(5));
  level.entities.push(Entity(10, 20, 0i64));
  level.entities.push(Entity(40, 30, 1i64));
  heal(at(&level.entities, 0i64), at(&level.entities, 1i64));
  heal(at(&level.entities, 1i64), at(&level.entities, 1i64));
  tick(&level, 2i64);
  e0 = at(&level.entities, 0i64);
  e1 = at(&level.entities, 1i64);
  return __copy_prim(e0.hp) + __copy_prim(e0.stamina) + __copy_prim(e1.hp) + __copy_prim(e1.stamina);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(164),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
