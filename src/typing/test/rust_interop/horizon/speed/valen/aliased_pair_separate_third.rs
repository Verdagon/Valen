// Fence: ensures that Valen loads two values of an unrelated object once, while
// it writes two entities that may be the same entity.
//
// Disclosed workaround: in either Rust arm, `rebalance` could be split into a
// version for two different stats objects and an in-place version for one. The
// plain arm could then keep its stats as two locals, and the GhostCell arm
// would need no cells for stats. It changes nothing in `attack`, the loop this
// test measures. Splitting a function into an aliasing and a non-aliasing
// version is ruled inadmissible for every arm.
//
// Both Rust arms do the same today. This test shows no Valen advantage. It
// exists to record a case where plain Rust already matches, and where GhostCell
// matches too when it passes the unrelated object the right way.
//
// The program:
//   attack(a &Entity in g, d &Entity in g, stats &Stats in h, n int) mut(g)
//   loop n times: set a.hp = a.hp - stats.cost; set d.hp = d.hp - stats.damage;
//   main calls attack with two entities, then with one entity as both a and d.
//   rebalance(x &Stats in h, y &Stats in h) mut(h) adds y's damage to x's cost.
//   It is called with two stats and with one stats twice. It is why the
//   GhostCell arm keeps stats in cells.
//
// Mechanism: parameter `noalias`, and M2. `stats` is the only reference into
// group `h`, so Valen marks the parameter `noalias`; LLVM then knows the stores
// to `a.hp` and `d.hp` cannot change `stats.cost` or `stats.damage`. The scopes
// say the same thing a second way: the stores carry the scope of `g`, and the
// loads of `stats` are marked `!noalias` against it. Either is enough.
//
// What Valen does not claim: that `a` and `d` are different entities. Both are
// in `g`, so each store to `hp` may change the other's `hp`, and each `hp` is
// reloaded after the other's store.
//
// Both writes go to the same field, `hp`. If `a` wrote `stamina` and `d` wrote
// `hp`, the plain arm would do better than this arm: it reaches both entities
// from one buffer by index, LLVM sees that two different fields of that
// buffer's elements never overlap, and it replaces the whole loop with two
// multiplications. Valen's two pointers into one group do not carry that fact.
// That gap belongs to the test stamina_vs_components.
//
// Arms:
// - Plain Rust: rust/aliased_pair_separate_third_plain.rs
// - GhostCell:  rust/aliased_pair_separate_third.rs
//
// Assumption behind the expected IR: `#inline(never)` keeps `attack` a
// function, so `stats` stays a parameter.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of `stats.cost`, a load and a store of `a.hp`, a
//   load of `stats.damage`, and a load and a store of `d.hp`.
//
// Expected IR of `attack`, optimized:
// - The parameter `stats` carries `noalias`. `a` and `d` do not.
// - `stats.cost` and `stats.damage` are loaded once, before the loop.
// - Each iteration loads and stores `a.hp`, then loads and stores `d.hp`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn aliased_pair_separate_third() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Stats { cost int; damage int; }
struct Entity { hp int; }
func rebalance<h'>(x &Stats in h, y &Stats in h) mut(h) {
  set x.cost = __copy_prim(x.cost) + __copy_prim(y.damage);
}
#inline(never)
func attack<g', h'>(a &Entity in g, d &Entity in g, stats &Stats in h, n int) mut(g) {
  i = 0;
  while i < __copy_prim(n) {
    set a.hp = __copy_prim(a.hp) - __copy_prim(stats.cost);
    set d.hp = __copy_prim(d.hp) - __copy_prim(stats.damage);
    set i = i + 1;
  }
}
exported func main() int {
  entities = Vec.new<Entity>();
  entities.push(Entity(50));
  entities.push(Entity(60));
  calm = Stats(1, 2);
  fierce = Stats(2, 3);
  rebalance(&calm, &fierce);
  rebalance(&fierce, &fierce);
  attack(at(&entities, 0i64), at(&entities, 1i64), &calm, 3);
  attack(at(&entities, 1i64), at(&entities, 1i64), &fierce, 2);
  return __copy_prim(at(&entities, 0i64).hp) + __copy_prim(at(&entities, 1i64).hp) + __copy_prim(calm.cost) + __copy_prim(fierce.cost);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(85),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
