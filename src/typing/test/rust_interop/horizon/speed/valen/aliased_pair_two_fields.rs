// Rust wins: ensures the corpus records a loop that both Rust arms compile
// better than Valen does, today and under the current design.
//
// Disclosed workaround: in either Rust arm, `rebalance` could be split into a
// version for two different stats objects and an in-place version for one. It
// changes nothing in `attack`, the loop this test measures. Splitting a
// function into an aliasing and a non-aliasing version is ruled inadmissible
// for every arm.
//
// The program:
//   Entity { hp int, stamina int }
//   attack(a &Entity in g, d &Entity in g, stats &Stats in h, n int) mut(g)
//   loop n times: set a.stamina = a.stamina - stats.cost;
//                 set d.hp = d.hp - stats.damage;
//   main calls attack with two entities, then with one entity as both a and d.
//   rebalance(x &Stats in h, y &Stats in h) mut(h) is why the GhostCell arm
//   keeps stats in cells.
//
// What Rust does: both Rust arms reach the two entities by index from one
// buffer. LLVM sees one base pointer, an index scaled by the size of an entity,
// and two different field offsets, and concludes that `stamina` of one entity
// and `hp` of another never overlap, whether or not the two entities are the
// same. It removes the loop: two loads, two multiplications by `n`, two stores.
//
// What Valen does: `a` and `d` are two references in one group. A reference
// points at a whole entity, so the two are the same entity or separate
// entities, and `a.stamina` never overlaps `d.hp`. Valen does not tell LLVM
// that: a field has the scope of its group, so both stores carry the scope of
// `g`, and each forces the other field to be reloaded. The loop runs `n` times
// with two loads and two stores per iteration.
//
// Blocker for Valen to match: scopes per field inside one group, proposal S20
// in src/typing/docs/architecture/borrowing-design.md. It is the same missing
// piece as in stamina_vs_components.
//
// C compilers get this through two pointers by default: clang -O2 removes the
// loop, because struct-path type-based alias analysis knows two different
// fields of the same struct type never overlap; with -fno-strict-aliasing the
// loop returns. That is true of C's default and not of every C codebase: much
// real C, the Linux kernel for one, is built with -fno-strict-aliasing. One
// compiler and version was probed (clang 17); GCC was not run. Rust has no such
// rule, and gets the same result here only because the indices show LLVM the
// layout.
//
// The bottom line, for two references that may be the same object, writing two
// different fields:
// - C, with two pointers: no loop.
// - Plain Rust, with indices: no loop.
// - GhostCell: no loop with indices; this arm's loop with two cell pointers.
// - Valen: this arm's loop, until it has scopes per field (S20).
// Valen is the slowest of the four on this shape today.
//
// What Valen does give LLVM here: `stats` is the only reference into group `h`,
// so the parameter is `noalias` and `stats.cost` and `stats.damage` are loaded
// once.
//
// Arms:
// - Plain Rust: rust/aliased_pair_two_fields_plain.rs
// - GhostCell:  rust/aliased_pair_two_fields.rs
//
// Assumption behind the expected IR: `#inline(never)` keeps `attack` a
// function, so `stats` stays a parameter.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of `stats.cost`, a load and a store of
//   `a.stamina`, a load of `stats.damage`, and a load and a store of `d.hp`.
//
// Expected IR of `attack`, optimized, today:
// - `stats.cost` and `stats.damage` are loaded once, before the loop.
// - Each iteration loads and stores `a.stamina`, then loads and stores `d.hp`.
//
// Expected IR of `attack`, optimized, intended (needs S20):
// - There is no loop.
// - `stats.cost` and `stats.damage` are each loaded once and multiplied by `n`.
// - `a.stamina` and `d.hp` are each loaded once and stored once.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn aliased_pair_two_fields() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Stats { cost int; damage int; }
struct Entity { hp int; stamina int; }
func rebalance<h'>(x &Stats in h, y &Stats in h) mut(h) {
  set x.cost = __copy_prim(x.cost) + __copy_prim(y.damage);
}
#inline(never)
func attack<g', h'>(a &Entity in g, d &Entity in g, stats &Stats in h, n int) mut(g) {
  i = 0;
  while i < __copy_prim(n) {
    set a.stamina = __copy_prim(a.stamina) - __copy_prim(stats.cost);
    set d.hp = __copy_prim(d.hp) - __copy_prim(stats.damage);
    set i = i + 1;
  }
}
exported func main() int {
  entities = Vec.new<Entity>();
  entities.push(Entity(50, 30));
  entities.push(Entity(60, 40));
  calm = Stats(1, 2);
  fierce = Stats(2, 3);
  rebalance(&calm, &fierce);
  rebalance(&fierce, &fierce);
  attack(at(&entities, 0i64), at(&entities, 1i64), &calm, 3);
  attack(at(&entities, 1i64), at(&entities, 1i64), &fierce, 2);
  e0 = at(&entities, 0i64);
  e1 = at(&entities, 1i64);
  return __copy_prim(e0.stamina) + __copy_prim(e1.stamina) + __copy_prim(e1.hp) + __copy_prim(calm.cost) + __copy_prim(fierce.cost);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(105),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
