// Source-level cost, compiled tie: ensures the corpus records that a Valen caller
// loses its reference to an entity in a Rust `Vec` when it hands that reference
// to a function that writes one of the entity's fields, and must reach the
// entity again.
//
// Valen must write two `at` calls per iteration where Rust binds one reference
// before the loop. In the compiled loop the two are expected to be equal,
// provided `level` is a `noalias` parameter that is not captured. If that does
// not hold, Valen reloads the length and the buffer pointer and repeats the
// bounds check after every call: three loads and a check against Rust's one
// load.
//
// One `at` per iteration would be enough if the checker allowed it: bind `e`
// before the loop, and in the loop `strike(e); set e = at(&level.entities, i);`
// and read `e.hp`. The checker rejects that today, with the same error as the
// one quoted below, at the next iteration's `strike(e)`. Re-deriving a borrow
// with `set` works in straight-line code and fails only around a loop's back
// edge; the tests reassigned_borrow_is_usable_after_loop_back_edge and
// reassigned_accessor_borrow_is_usable_after_loop_back_edge in
// src/typing/borrow_checker/experimental/tests/caller_member_ref_tests.rs fail
// on it. So this arm calls `at` twice per iteration.
//
// This is a cost of interop, not of group borrowing. Over a Valen-native array
// the caller keeps its reference across such a call; see
// caller_keeps_member_ref_across_mut_callee in the same test file.
//
// Why: `at` is a Rust function, and the compiler cannot see whether it returned
// the `Vec`'s own element or something deeper inside it. So a borrow returned
// by `at` covers everything under the `Vec` (rust-interop-design.md, S25).
// `strike` declares `mut(g)`, which is the only way to say "I write this", and
// it stands for structural changes as well as field writes. A structural change
// under the `Vec` would invalidate the caller's borrow, so the checker ends the
// borrow at the call. With an array native to Valen the reference would be to a
// member of the group, and a member survives such a call.
//
// What would close it: a way for an imported accessor to return a borrow typed
// as a member (`in g[]` and not `in g...`) when it is known to return an
// element of the container it was handed, as `at` and `get` do; or an effect
// that promises field writes only. No proposal is filed for either.
//
// The program:
//   Level { entities Vec<Entity> }
//   strike(e &Entity in g) mut(g) takes one hp off and calls do_nothing().
//   siege(level &Level in g, i i64, n int) int mut(level.entities...)
//   loop n times: strike(at(&level.entities, i));
//                 total = total + at(&level.entities, i).hp;
//   main calls siege on two entities with different counts.
//
// The form the checker rejects, with `e = at(&level.entities, i)` bound once
// before the loop, `strike(e)` and then `e.hp` in the loop:
//   At stub:main.valen:17:12:
//       strike(e);
//              ^
//   Used a borrow after invalidated.
//   Invalidated at stub:main.valen:17:5:
//       strike(e);
//       ^^^^^^
// The first call to `strike` ends `e`, so the second iteration's `strike(e)` and
// the read of `e.hp` are both rejected.
//
// No driven fixture here builds and runs a Valen-native array, so that version
// of the program is not in this file.
//
// Arms:
// - Plain Rust: rust/caller_ref_across_mutating_callee_plain.rs
// - GhostCell:  rust/caller_ref_across_mutating_callee.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `siege` and `strike` functions.
// - `at` is inlined into `siege`.
// - `level` is a `noalias` parameter that is not captured. `strike` is handed a
//   pointer loaded from the level, not the level itself, so LLVM then knows the
//   call cannot change the `Vec`'s pointer or length. Scopes do not say so: the
//   loads inside the inlined `at` carry no scope of their own.
//
// Expected IR of `siege`, unoptimized:
// - Each iteration reaches the entity twice: two loads of the buffer pointer,
//   two of the length, two bounds checks. It calls `strike` and loads `hp`.
//
// Expected IR of `siege`, optimized:
// - The buffer pointer and length are loaded before the loop, and the bound on
//   `i` is checked once.
// - Each iteration calls `strike` and then loads `hp`. That is the Rust arm's
//   loop.
// - If the parameter is not `noalias` and uncaptured, each iteration instead
//   reloads the length and the buffer pointer and repeats the bounds check
//   after the call: three loads and a check where Rust has one load.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[ignore]
#[test]
fn caller_ref_across_mutating_callee() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Entity { hp int; }
struct Level { entities Vec<Entity, Global>; }
#inline(never)
func strike<g'>(e &Entity in g) mut(g) {
  set e.hp = __copy_prim(e.hp) - 1;
  do_nothing();
}
#inline(never)
func siege<g'>(level &Level in g, i i64, n int) int mut(level.entities...) {
  total = 0;
  k = 0;
  while k < __copy_prim(n) {
    strike(at(&level.entities, __copy_prim(i)));
    set total = total + __copy_prim(at(&level.entities, __copy_prim(i)).hp);
    set k = k + 1;
  }
  return total;
}
exported func main() int {
  level = Level(Vec.new<Entity>());
  level.entities.push(Entity(40));
  level.entities.push(Entity(20));
  first = siege(&level, 0i64, 3);
  second = siege(&level, 1i64, 2);
  return first + second;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(151),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
