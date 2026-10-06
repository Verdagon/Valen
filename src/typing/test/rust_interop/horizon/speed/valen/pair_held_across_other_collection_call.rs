// Second claim: ensures that a call handed a different collection does not
// force Valen to reload the fields of two entities that may be the same entity.
//
// Why second claim: the tier follows the best-informed Rust arm, which is the
// plain arm. It mirrors this program line for line with `Cell` fields, `drill`
// is handed only `reserves` there too, and it loses only because Rust allows
// unsafe code. The GhostCell arm loses for a stronger reason (one brand is
// forced, and `drill` takes the token), but that is a weakness of GhostCell
// which plain Rust with `Cell` does not share.
//
// Why no test of this kind can do better today: Valen stamps a call by what its
// arguments reach, so a win that works today needs a callee whose arguments do
// not reach the held data. The same program in Rust with `Cell` hands the
// callee the same arguments, so safe code cannot reach the held data there
// either. Rust's types fail to say it only when the callee is handed something
// that reaches the held data, and then Valen has no stamp either (S18).
//
// Disclosed workaround: if `swap_gear` were split into two functions, one for a
// pair drawn from one collection and one for a pair drawn from two, the
// GhostCell arm could give `entities` and `reserves` separate brands, and
// `drill` would not take the token of the entities `hot` reads. That arm's loop
// would not change (it still loads `hp` and `power` after every call), but it
// too would lose only for the "trust" reason. Splitting a function into an
// aliasing version and a non-aliasing version is ruled inadmissible for every
// arm. Size of the change in loads: none.
//
// The program:
//   Level { entities Vec<Entity>; reserves Vec<Entity>; }
//   hot(a &Entity in g, b &Entity in g, reserves &Vec<Entity> in h, n int)
//       mut(g) mut(h)
//   loop n times: set a.hp = a.hp - b.power; drill(reserves);
//   drill(reserves &Vec<Entity> in h) mut(h) writes reserves[0].hp.
//   main calls hot with two entities, then with one entity as both a and b.
//   The two calls pass different counts, so the optimizer cannot fold `n`.
//
// Mechanism: `!noalias` on a call, by argument reach (M3). `drill` is handed
// only `reserves`, which is in group `h`. Its arguments cannot reach group `g`,
// so Valen marks the call as unable to touch `g`, and LLVM need not reload
// `a.hp` or `b.power` after it. That fact comes from the borrow checker's
// aliasing info, so this test runs with the checker on.
//
// What Valen does not claim: that `a` and `b` are different entities. Both are
// in `g`, so the store to `a.hp` may change `b.power`, and `b.power` is
// reloaded after that store.
//
// Size of the win: one load per iteration. Both Rust arms load `a.hp` and
// `b.power` after every call to `drill`; Valen loads only `b.power`. The win
// is the same one load against the plain arm rewritten with `Cell` fields, and
// against the GhostCell arm with a cell on each field (both observed).
// Without the call, the plain arm would be ahead of Valen here: LLVM knows
// `entities[a].hp` and `entities[b].power` never overlap, while Valen reloads
// `power` after the `hp` store (see valen/aliased_pair_two_fields.rs).
// Cause: all of it is the call stamp. Holding the two references saves Valen
// nothing here: the GhostCell arm holds two cell pointers, and in the plain
// arm rustc computes the two element addresses once, before the loop.
//
// Why `swap_gear` is in the program: it takes two entities that may be the
// same, and main calls it with one entity from each collection, with two
// entities of one collection, and with one entity twice. In Valen the pair
// shares a group only for the duration of each call. In the GhostCell arm it
// puts every entity of both collections under one token, which `drill` must
// then take.
//
// Arms:
// - Plain Rust: rust/pair_held_across_other_collection_call_plain.rs
// - GhostCell:  rust/pair_held_across_other_collection_call.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `hot` and `drill` functions, so the call to `drill`
//   stays in the loop.
// - `at` inlines into its callers.
// - The test is built with the `borrow_checker_experimental` feature. The
//   default checker panics on a field read through a borrow that `at` returned.
// - A function may bind one group parameter to two arguments from different
//   collections at a call site (valen-design-1.md, "Temporary call-site unions
//   do not persist"). `swap_gear(entity, reserve)` needs it.
//
// Expected IR of `hot`, unoptimized:
// - The loop contains a load of `b.power`, a load of `a.hp`, a store of `a.hp`,
//   and the call to `drill`.
//
// Expected IR of `hot`, optimized:
// - The loop contains the call to `drill`, and the call carries `!noalias`
//   naming the scope of group `g`.
// - The loop contains one store of `a.hp` and no load of `a.hp`.
// - The loop contains one load of `b.power`, which follows the store to `a.hp`.
//   No load follows the call to `drill`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn pair_held_across_other_collection_call() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Entity { hp int; power int; }
struct Level { entities Vec<Entity, Global>; reserves Vec<Entity, Global>; }
func swap_gear<g'>(x &Entity in g, y &Entity in g) mut(g) {
  tmp = __copy_prim(x.power);
  set x.power = __copy_prim(y.power);
  set y.power = tmp;
}
#inline(never)
func drill<h'>(reserves &Vec<Entity, Global> in h) mut(h) {
  r = at(reserves, 0i64);
  set r.hp = __copy_prim(r.hp) + 1;
  do_nothing();
}
#inline(never)
func hot<g', h'>(a &Entity in g, b &Entity in g, reserves &Vec<Entity, Global> in h, n int) mut(g) mut(h) {
  i = 0;
  while i < __copy_prim(n) {
    set a.hp = __copy_prim(a.hp) - __copy_prim(b.power);
    drill(reserves);
    set i = i + 1;
  }
}
exported func main() int {
  level = Level(Vec.new<Entity>(), Vec.new<Entity>());
  level.entities.push(Entity(40, 1));
  level.entities.push(Entity(40, 2));
  level.reserves.push(Entity(0, 5));
  level.reserves.push(Entity(0, 7));
  swap_gear(at(&level.entities, 0i64), at(&level.reserves, 0i64));
  swap_gear(at(&level.entities, 0i64), at(&level.entities, 1i64));
  swap_gear(at(&level.entities, 1i64), at(&level.entities, 1i64));
  hot(at(&level.entities, 0i64), at(&level.entities, 1i64), &level.reserves, 3);
  hot(at(&level.entities, 1i64), at(&level.entities, 1i64), &level.reserves, 4);
  e0 = at(&level.entities, 0i64);
  e1 = at(&level.entities, 1i64);
  r0 = at(&level.reserves, 0i64);
  return __copy_prim(e0.hp) + __copy_prim(e1.hp) + __copy_prim(r0.hp) + __copy_prim(e0.power) + __copy_prim(r0.power);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(55),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
