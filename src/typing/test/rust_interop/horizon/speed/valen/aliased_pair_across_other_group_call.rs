// Ensures that a call handed an unrelated object does not force Valen to
// reload two component values that may belong to the same entity.
//
// The program:
//   Entity { components Vec<Component> }, Log { count int }
//   duel(a &Entity in g, d &Entity in g, log &Log in h, n int) mut(g) mut(h)
//   sword = at(&a.components, 0); armor = at(&d.components, 1)
//   loop n times: set sword.value = sword.value - armor.value; record(log);
//   record(log &Log in h) mut(h) counts the swing and calls do_nothing().
//   main calls duel with two entities, then with one entity as both a and d.
//   The two calls pass different counts, so the optimizer cannot fold `n`.
//
// Mechanism: M3 by argument reach. `record` is handed only `log`, which is in
// group `h`. Its arguments cannot reach group `g`, so Valen marks the call as
// unable to touch `g`, and LLVM need not reload `sword.value` or `armor.value`
// after it. That fact comes from the borrow checker's aliasing info, so this
// test runs with the checker on.
//
// The loop writes `sword.value` and nothing else. If it also wrote
// `armor.value`, each value would be reloaded because of the other's store, and
// no arm would show a reload that the call forced.
//
// What Valen does not claim: that `sword` and `armor` are different components.
// Both are in `g`, so the store to `sword.value` may change `armor.value`, and
// `armor.value` is reloaded after that store.
//
// Arms:
// - Plain Rust: rust/aliased_pair_across_other_group_call_plain.rs
// - GhostCell:  rust/aliased_pair_across_other_group_call.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `duel` and `record` functions, so the call to
//   `record` stays in the loop.
// - `at` is inlined into `duel`.
//
// Expected IR of `duel`, unoptimized:
// - The loop contains a load of `armor.value`, a load of `sword.value`, a store
//   of `sword.value`, and the call to `record`.
//
// Expected IR of `duel`, optimized:
// - The loop contains the call to `record`, and the call carries `!noalias`
//   naming the scope of the store to `sword.value`.
// - The loop contains one store of `sword.value` and no load of `sword.value`.
// - The loop contains one load of `armor.value`, which the store to
//   `sword.value` forces. No load is forced by the call to `record`.
// - The loop loads no `components` buffer pointer or length and has no bounds
//   check. Both `at` calls sit before the loop.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn aliased_pair_across_other_group_call() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Component { kind int; value int; }
struct Entity { components Vec<Component, Global>; }
struct Log { count int; }
#inline(never)
func record<h'>(log &Log in h) mut(h) {
  set log.count = __copy_prim(log.count) + 1;
  do_nothing();
}
#inline(never)
func duel<g', h'>(a &Entity in g, d &Entity in g, log &Log in h, n int) mut(g) mut(h) {
  sword = at(&a.components, 0i64);
  armor = at(&d.components, 1i64);
  i = 0;
  while i < __copy_prim(n) {
    set sword.value = __copy_prim(sword.value) - __copy_prim(armor.value);
    record(log);
    set i = i + 1;
  }
}
exported func main() int {
  entities = Vec.new<Entity>();
  c0 = Vec.new<Component>();
  c0.push(Component(2, 60));
  c0.push(Component(1, 5));
  entities.push(Entity(^c0));
  c1 = Vec.new<Component>();
  c1.push(Component(2, 30));
  c1.push(Component(1, 4));
  entities.push(Entity(^c1));
  log = Log(0);
  duel(at(&entities, 0i64), at(&entities, 1i64), &log, 3);
  duel(at(&entities, 1i64), at(&entities, 1i64), &log, 2);
  e0 = at(&entities, 0i64);
  e1 = at(&entities, 1i64);
  return __copy_prim(at(&e0.components, 0i64).value) + __copy_prim(at(&e1.components, 0i64).value) + __copy_prim(log.count);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(75),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
