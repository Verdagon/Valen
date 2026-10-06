// Red by design: ensures that a write to one component does not force Valen to
// reload the pointer and length of the component list it is walking.
//
// Valen does not do this today, and the current design cannot. The expected IR
// below gives both today's shape and the intended future shape. Blocker:
// proposal S19 in src/typing/docs/architecture/borrowing-design.md.
//
// Both Rust arms are ahead of Valen on this today. Each already compiles to the
// shape Valen is waiting on S19 for: two loads and one bounds check per step,
// where Valen today has four loads and one bounds check. S19 would bring Valen
// level with them, not past them.
//
// This test replaces the retired attack_reads_defender_armor, which had the
// same program: walk the defender's components while writing one of the
// attacker's.
//
// The program:
//   Entity { components Vec<Component> }, Component { weight int, value int }
//   attack(a &Entity in g, d &Entity in g, n i64) mut(g)
//   sword = at(&a.components, 0)
//   for j in 0..n: set sword.value = sword.value - at(&d.components, j).weight
//   main calls attack with two entities, then with one entity as both a and d.
//
// Why Valen reloads today: the store to `sword.value` carries the scope of the
// components it was reached through. The pointer and length of `d.components`
// are loaded inside `at`, after it is inlined, and those loads carry only what
// the call to `at` carried: a mark against the scopes the call does not reach.
// The call reaches `d.components`, which is the same scope the store carries,
// since `a` and `d` share group `g`. So LLVM is never told that a store to a
// component leaves a component list's pointer and length alone, and it reloads
// both after every store. Parameter `noalias` does not help either: `a` and `d`
// share a group, so neither is `noalias`. S19 is the proposal to give a `Vec`'s
// own pointer and length a different scope from its elements, even when the
// access goes through an accessor such as `at`.
//
// How the Rust arms get there:
// - The GhostCell arm, with one cell per component, reaches both entities
//   through shared `&Entity` parameters, which rustc marks `noalias`; the
//   pointer and length are loaded once.
// - The plain arm, with `value` in a `Cell`, does the same.
// Each arm's header names the spelling or placement it did not use, with the
// loop that compiles to: six loads and two bounds checks per step in both.
//
// C compilers get this by default through type-based alias analysis: clang -O2
// loads both component pointers once, keeps the sword's value in a register and
// vectorizes the loop; with -fno-strict-aliasing all four loads return every
// iteration. (C has no bounds checks. With hand-written ones clang keeps most
// of the loads in the loop, because a C pointer is not known to be
// dereferenceable; a different limit, not an aliasing one.) That is true of C's
// default and not of every C codebase: much real C, the Linux kernel for one,
// is built with -fno-strict-aliasing. One compiler and version was probed
// (clang 17); GCC was not run.
//
// Arms:
// - Plain Rust: rust/element_store_keeps_spine_plain.rs
// - GhostCell:  rust/element_store_keeps_spine.rs
//
// Assumptions behind the expected IR:
// - `at` is inlined into `attack`.
// - `#inline(never)` keeps `attack` a function.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of the pointer and of the length of
//   `d.components`, a bounds check, a load of `weight`, a load of
//   `sword.value`, and a store of `sword.value`.
//
// Expected IR of `attack`, optimized, today:
// - The `at` call for `sword` sits before the loop.
// - Each iteration loads the length of `d.components`, checks the bound on `j`,
//   loads the pointer of `d.components`, loads `weight`, loads `sword.value`,
//   and stores `sword.value`.
//
// Expected IR of `attack`, optimized, intended (needs S19):
// - The pointer and the length of `d.components` are loaded once, before the
//   loop.
// - Each iteration checks the bound on `j` against that length, loads `weight`,
//   loads `sword.value`, and stores `sword.value`. (`sword.value` is loaded
//   each time because `weight` may be a field of the same component.)

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn element_store_keeps_spine() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Component { weight int; value int; }
struct Entity { components Vec<Component, Global>; }
#inline(never)
func attack<g'>(a &Entity in g, d &Entity in g, n i64) mut(g) {
  sword = at(&a.components, 0i64);
  j = 0i64;
  while j < __copy_prim(n) {
    set sword.value = __copy_prim(sword.value) - __copy_prim(at(&d.components, __copy_prim(j)).weight);
    set j = j + 1i64;
  }
}
exported func main() int {
  entities = Vec.new<Entity>();
  c0 = Vec.new<Component>();
  c0.push(Component(2, 90));
  c0.push(Component(3, 5));
  entities.push(Entity(^c0));
  c1 = Vec.new<Component>();
  c1.push(Component(4, 70));
  c1.push(Component(1, 6));
  entities.push(Entity(^c1));
  attack(at(&entities, 0i64), at(&entities, 1i64), 2i64);
  attack(at(&entities, 1i64), at(&entities, 1i64), 2i64);
  e0 = at(&entities, 0i64);
  e1 = at(&entities, 1i64);
  return __copy_prim(at(&e0.components, 0i64).value) + __copy_prim(at(&e1.components, 0i64).value);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
