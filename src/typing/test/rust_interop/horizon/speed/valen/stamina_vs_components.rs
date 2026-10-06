// Red by design: records that Valen does not yet keep one entity's stamina in a
// register while walking the components of an entity that may be the same one.
//
// The program:
//   Entity { stamina int; components Vec<EntityComponent>; }
//   attack(attacker &Entity in g, defender &Entity in g, count i64) mut(g)
//   for j in 0..count:
//     set attacker.stamina = attacker.stamina - defender.components[j].value;
//   main calls attack with two entities, then with one entity as both. The two
//   calls pass different counts, so the optimizer cannot fold `count`.
//
// Under the current design all three arms compile to the same loop; this test
// shows no win today.
//
// Blocker: field-level scopes inside one group, proposal S20 in
// src/typing/docs/architecture/borrowing-design.md.
// Today a member is in its owner's group, so `attacker.stamina` and the
// pointer and length of `defender.components` are one scope. Each store to
// `stamina` may change that pointer and length, so both reload, the bounds
// check repeats, and `stamina` itself cannot be promoted while those loads may
// alias it. With one scope per field, the argument `&defender.components`
// reaches the components and not `stamina`, and all of that goes away.
// Field-level scopes would be sound: members of one group have one type, and
// two members are either the same object or disjoint, so two different fields
// never overlap.
//
// What this would show: C compilers do this by default through type-based
// alias analysis (clang -O2; with -fno-strict-aliasing the extra loads return).
// Rust has no such rule, because unsafe Rust may view the same memory as two
// types. This is Valen recovering what C has and Rust gave up; it is not a win
// from group borrowing. C goes further than the shape intended here: it also
// moves the store of `stamina` out of the loop and vectorizes. Cautions: one
// compiler and version was probed (GCC was not run), and much real C, the
// Linux kernel for one, is built with -fno-strict-aliasing, so this is true of
// C's default and not of every C codebase.
//
// Why the test is kept: this is the `attack` function from the group-borrowing
// article, the shape a reader will try first. The corpus should say exactly
// what Valen needs in order to win it.
//
// Arms:
// - Plain Rust: rust/stamina_vs_components_plain.rs
// - GhostCell:  rust/stamina_vs_components.rs
//
// Size of the win once field-level scopes exist, per iteration: three loads and
// a bounds check (the components length, the components pointer, `stamina`),
// against both Rust arms. Both Rust arms keep those in the loop.
//
// Assumptions behind the expected IR:
// - `#inline(never)` keeps `attack` a function.
// - `at` inlines into its callers.
// - Field-level scopes exist.
// - The test is built with the `borrow_checker_experimental` feature.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of the components length, a bounds check, a load
//   of the components pointer, a load of `value`, a load of `stamina`, and a
//   store of `stamina`.
//
// Expected IR of `attack`, optimized, once field-level scopes exist:
// - The components pointer and length are loaded once, before the loop.
// - The loop contains no bounds check.
// - The loop contains no load of `stamina`.
// - The loop contains one load of `value` per element.
//
// Expected IR of `attack`, optimized, today (the same as both Rust arms):
// - The loop contains a load of the components length, a bounds check, a load
//   of the components pointer, a load of `value`, a load of `stamina`, and a
//   store of `stamina`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn stamina_vs_components() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct EntityComponent { value int; }
struct Entity { stamina int; components Vec<EntityComponent, Global>; }
struct Level { entities Vec<Entity, Global>; }
#inline(never)
func attack<g'>(attacker &Entity in g, defender &Entity in g, count i64) mut(g) {
  j = 0i64;
  while j < __copy_prim(count) {
    c = at(&defender.components, __copy_prim(j));
    set attacker.stamina = __copy_prim(attacker.stamina) - __copy_prim(c.value);
    set j = j + 1i64;
  }
}
exported func main() int {
  level = Level(Vec.new<Entity>());
  c0 = Vec.new<EntityComponent>();
  c0.push(EntityComponent(1));
  c0.push(EntityComponent(2));
  c0.push(EntityComponent(3));
  level.entities.push(Entity(50, ^c0));
  c1 = Vec.new<EntityComponent>();
  c1.push(EntityComponent(4));
  c1.push(EntityComponent(5));
  c1.push(EntityComponent(6));
  level.entities.push(Entity(50, ^c1));
  attack(at(&level.entities, 0i64), at(&level.entities, 1i64), 2i64);
  attack(at(&level.entities, 1i64), at(&level.entities, 1i64), 3i64);
  e0 = at(&level.entities, 0i64);
  e1 = at(&level.entities, 1i64);
  return __copy_prim(e0.stamina) + __copy_prim(e1.stamina);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(76),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
