// Rust wins: ensures the corpus records that a Valen caller holding an entity
// of a Rust `HashMap` must look the entity up again after every call that
// writes one of its fields, where Rust looks it up once.
//
// The size of it: at least one table lookup per iteration in Valen, against
// none in Rust. Rust holds one reference for the whole loop. A lookup, unlike
// an index into a `Vec`, does not optimize away. Three spellings compiled in
// Rust:
// - Reference held (the Rust arms): no lookup in the loop.
// - Two lookups per iteration, as this arm is written: the hashing of the key
//   moves out of the loop, and two inlined probes of the table stay in it.
// - One lookup per iteration: the lookup is not inlined, so each iteration
//   makes one call into the map, hashing included.
//
// One `get` per iteration would be enough if the checker allowed it: bind `e`
// before the loop, and in the loop `strike(e); set e = get(..);` and read
// `e.hp`. On the `Vec` twin of this test the checker rejects that form today
// ("Used a borrow after invalidated", at the next iteration's `strike(e)`).
// Re-deriving a borrow with `set` works in straight-line code and fails only
// around a loop's back edge; the tests
// reassigned_borrow_is_usable_after_loop_back_edge and
// reassigned_accessor_borrow_is_usable_after_loop_back_edge in
// src/typing/borrow_checker/experimental/tests/caller_member_ref_tests.rs fail
// on it. So this arm calls `get` twice per iteration.
//
// This is a cost of interop, not of group borrowing.
//
// This is the mirror image of the claim earlier work on this corpus led with,
// that Rust must look a key up again where Valen holds a reference. With a Rust
// container, and a callee that declares `mut` on the held entry's group, the
// repeated lookup is on Valen's side.
//
// Why: `get` is a Rust function, and the compiler cannot see whether it
// returned the map's own entry or something deeper inside it. So a borrow
// returned by `get` covers everything under the map (rust-interop-design.md,
// S25). `strike` declares `mut(g)`, which is the only way to say "I write
// this", and it stands for structural changes as well as field writes. So the
// checker ends the caller's borrow at the call. Over a Valen-native array the
// caller keeps its reference; see the test
// caller_keeps_member_ref_across_mut_callee in
// src/typing/borrow_checker/experimental/tests/caller_member_ref_tests.rs.
//
// What would close it: a way for an imported accessor to return a borrow typed
// as a member (`in g[]` and not `in g...`) when it is known to return an
// element of the container it was handed, as `at` and `get` do; or an effect
// that promises field writes only. No proposal is filed for either.
//
// The program:
//   Level { entities HashMap<int, Entity> }
//   strike(e &Entity in g) mut(g) takes one hp off and calls do_nothing().
//   siege(level &Level in g, key int, n int) int mut(level.entities...)
//   loop n times: strike(level.entities.get(&k).unwrap());
//                 total = total + level.entities.get(&k).unwrap().hp;
//   main calls siege on two entities with different counts.
//
// The same program over a `Vec` is caller_ref_across_mutating_callee. There the
// second reach compiles away and the loops tie.
//
// Arms:
// - Plain Rust: rust/keyed_ref_across_mutating_callee_plain.rs
// - GhostCell:  rust/keyed_ref_across_mutating_callee.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `siege` and `strike` functions.
// - `get` and `unwrap` inline into `siege`.
// - `level` is a `noalias` parameter that is not captured, so the map's table
//   pointer, its mask and its hasher keys can be loaded once and the hashing of
//   the key can move out of the loop, as in Rust. If not, the hashing stays in
//   the loop too.
// - Valen can write through a borrow that `HashMap.get` returned, the same way
//   it can through one that `at` returned.
// - The test is built with the `borrow_checker_experimental` feature.
// - The borrow checker can check a program that uses `HashMap`. Today the
//   experimental checker panics on such a program ("callee group rune
//   ImplicitGroupRune ... not bound at this call", in `groupify_group_expr`,
//   src/typing/borrow_checker/experimental/groupify.rs).
// - Valen can call `unwrap` on the `Option<&Entity>` that `get` returns. Today,
//   with the checker off, interop lowering panics on it ("cannot lower generic
//   type argument of Rust callee `unwrap` to a rustc type", in
//   src/instantiating/rust_interop/horizon/resolve_request.rs).
// - Entities are keyed by `int`.
// - The exit value 151 is verified in both Rust arms only. Run with the
//   annotations removed, this arm stops at the checker panic named above. That
//   one was observed on this program; the `unwrap` gap was not reached and is
//   named from another fixture.
//
// Expected IR of `siege`, unoptimized:
// - Each iteration hashes the key twice and probes the table twice, calls
//   `strike`, and loads `hp`.
//
// Expected IR of `siege`, optimized:
// - The key is hashed once, before the loop.
// - Each iteration probes the table, calls `strike`, probes the table again,
//   and loads `hp`. Each probe loads at least one group of control bytes and
//   one stored key, and compares them.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

// Ignored: the lexer does not yet parse the `#inline(never)` attribute on a function, so this
// program (like every valen/ speed test) dies with UnrecognizedDenizenError before any checking.
// Documented in notes/docs/handoffs/speed-benchmarks-handoff.md. Un-ignore once `#inline` parses.
#[ignore]
#[test]
fn keyed_ref_across_mutating_callee() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.do_nothing;
import std.collections.HashMap;
import std.hash.random.RandomState;
import std.option.Option;
import std.alloc.Global;
struct Entity { hp int; }
struct Level { entities HashMap<int, Entity, RandomState, Global>; }
#inline(never)
func strike<g'>(e &Entity in g) mut(g) {
  set e.hp = __copy_prim(e.hp) - 1;
  do_nothing();
}
#inline(never)
func siege<g'>(level &Level in g, key int, n int) int mut(level.entities...) {
  k = __copy_prim(key);
  total = 0;
  i = 0;
  while i < __copy_prim(n) {
    strike((level.entities.get(&k)).unwrap());
    set total = total + __copy_prim((level.entities.get(&k)).unwrap().hp);
    set i = i + 1;
  }
  return total;
}
exported func main() int {
  level = Level(HashMap.new<int, Entity>());
  level.entities.insert(1, Entity(40));
  level.entities.insert(2, Entity(20));
  first = siege(&level, 1, 3);
  second = siege(&level, 2, 2);
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
