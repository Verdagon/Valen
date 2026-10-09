// Control: ensures that Valen does reload two values that may be the same
// value, after each store to the other.
//
// Both Rust arms compile to the same loop today. This test shows no Valen
// advantage. It exists to show that the marks Valen gives LLVM are selective:
// two references in one group may alias, and Valen says nothing that would let
// LLVM assume they do not.
//
// The program:
//   Entity { components Vec<Component> }
//   duel(attacker &Entity in g, defender &Entity in g, swings int) mut(g)
//   sword = at(&attacker.components, 0); armor = at(&defender.components, 1)
//   loop swings times: set sword.value = sword.value - armor.value;
//                      set armor.value = armor.value - 1;
//   main calls duel with two entities, then with one entity as both.
//   The two calls pass different counts, so the optimizer cannot fold `swings`.
//
// Mechanism: none. `sword` and `armor` are both reached through components of
// entities in group `g`, so the store to `sword.value` and the store to
// `armor.value` carry one scope. Each store may change the other value, and
// LLVM reloads it. Neither parameter is `noalias`, since they share a group.
//
// What would be a bug: an optimized loop with no load of `sword.value` or no
// load of `armor.value`. That would mean Valen told LLVM two same-group
// references cannot alias. With `attacker` and `defender` the same entity and a
// sword that was also the armor, the program would then compute a wrong value.
//
// Arms:
// - Plain Rust: rust/sword_vs_armor_interleaved_plain.rs
// - GhostCell:  rust/sword_vs_armor_interleaved.rs
//
// Assumptions behind the expected IR:
// - `at` is inlined into `duel`.
// - `#inline(never)` keeps `duel` a function.
//
// Expected IR of `duel`, unoptimized:
// - The loop contains a load of `armor.value`, a load and a store of
//   `sword.value`, and a load and a store of `armor.value`.
//
// Expected IR of `duel`, optimized:
// - Neither parameter carries `noalias`.
// - Both `at` calls sit before the loop: the loop loads no `components` pointer
//   or length and has no bounds check.
// - Each iteration loads `sword.value` and stores it, then loads `armor.value`
//   and stores it. The load of `sword.value` follows the previous iteration's
//   store to `armor.value`, and the load of `armor.value` follows the store to
//   `sword.value`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn sword_vs_armor_interleaved() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct EntityComponent { kind int; value int; }
struct Entity { components Vec<EntityComponent, Global>; }
#inline(never)
func duel<g'>(attacker &Entity in g, defender &Entity in g, swings int) mut(g) {
  sword = at(&attacker.components, 0i64);
  armor = at(&defender.components, 1i64);
  n = 0;
  while n < __copy_prim(swings) {
    set sword.value = __copy_prim(sword.value) - __copy_prim(armor.value);
    set armor.value = __copy_prim(armor.value) - 1;
    set n = n + 1;
  }
}
exported func main() int {
  entities = Vec.new<Entity>();
  c0 = Vec.new<EntityComponent>();
  c0.push(EntityComponent(2, 20));
  c0.push(EntityComponent(1, 5));
  entities.push(Entity(^c0));
  c1 = Vec.new<EntityComponent>();
  c1.push(EntityComponent(2, 9));
  c1.push(EntityComponent(1, 4));
  entities.push(Entity(^c1));
  duel(at(&entities, 0i64), at(&entities, 1i64), 3);
  duel(at(&entities, 1i64), at(&entities, 1i64), 2);
  e0 = at(&entities, 0i64);
  e1 = at(&entities, 1i64);
  return __copy_prim(at(&e0.components, 0i64).value) + __copy_prim(at(&e1.components, 0i64).value) + __copy_prim(at(&e1.components, 1i64).value) + 10;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(28),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
