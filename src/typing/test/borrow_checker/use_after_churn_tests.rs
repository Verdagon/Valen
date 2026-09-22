//! Rung-2 use-after-churn tests. A reference to a runtime-sized array *element* points into a child
//! group; a call that declares `mut(r)` over the group `r` the array is bound to invalidates every
//! live element reference into `r`'s child groups, so using such a reference afterward is an error.
//! A reference to the whole array, or to an inline field, is in the parent group and survives.
//!
//! Fixtures build a monomorphic RSA local, bind a *borrow* to an element (never read the value out),
//! and never push/pop — see the plan `please-plan-out-rung-quiet-kazoo.md`.

use super::util::{assert_borrow_error_renders_with_arrays, assert_compiles_clean_with_arrays};

// A group annotation on a return type (`&int in g`) compiles — the rules/solver side treats it as
// `Unspecified` (it carries no group), so a returned grouped reference no longer panics the scout.
#[test]
fn test_return_position_group_compiles() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.drop.*;
func idr<g'>(a &int in g) &int in g { return a; }
exported func main() int { return 0; }
"#);
}

// Rung 3: a reference returned by a call points into an element of the argument's group; churning
// that group afterward invalidates it, so using it is a use-after-churn.
#[test]
fn test_use_returned_reference_after_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func get<g'>(a &[]int in g, i int) &int in g[] { return &a[__copy_prim(i)]; }
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, tg'>(x &T in tg) { }
exported func main() int {
  arr = Array<int>(3);
  v = arr.get(0);
  churn(&arr);
  observe(v);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
  observe(v);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
"#,
  );
}

// Rung 3 (clean): a returned reference into a group that is never churned stays live. The callee's
// return group is mapped to the specific argument (`arr`), so churning a *different* array leaves it.
#[test]
fn test_returned_reference_into_untouched_group_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func get<g'>(a &[]int in g) &int in g[] { return &a[0]; }
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  other = Array<int>(3);
  v = get(&arr);
  churn(&other);
  observe(v);
  return 0;
}
"#);
}

// An element-path group annotation (`in g[]`) on a parameter compiles.
#[test]
fn test_param_element_group_compiles() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func peek<g'>(a &[]int in g, e &int in g[]) { }
exported func main() int { return 0; }
"#);
}

// An inline-member reference survives a churn of its parent group: `&w.val` is a `Member` step (same
// group as `w`), not a child group, so churning `w` cannot dangle it. Only child groups (`Elements`)
// die.
#[test]
fn test_inline_member_reference_survives_parent_churn() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Wrap { val int; }
func churn<r'>(w &Wrap in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  w = Wrap(3);
  f = &w.val;
  churn(&w);
  observe(f);
  return 0;
}
"#);
}

// Slice 1 (Phase 0): the pipeline reaches the (no-op) checker for an RSA-element fixture — build an
// array, borrow an element, call a `mut(r)` function, and never use the element afterward. Clean.
#[test]
fn test_rsa_element_borrow_no_use_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(arr &[]int in r) mut(r) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  return 0;
}
"#);
}

// Slice 2 (Phase A): using an element reference after a `mut(r)` churn call is rejected.
#[test]
fn test_use_element_after_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(arr &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
"#,
  );
}

// Slice 3 (Phase A): a call that borrows the array but does not declare `mut` does not churn, so an
// element reference stays live across it.
#[test]
fn test_use_element_after_readonly_call_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func touch<r'>(arr &[]int in r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  touch(&arr);
  observe(ref);
  return 0;
}
"#);
}

// Slice 4 (Phase A): churning a *different* array's group does not invalidate an element reference
// into this array — the callee only churns the group it was handed.
#[test]
fn test_churn_other_group_leaves_element_live() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  other = Array<int>(3);
  ref = &arr[0];
  churn(&other);
  observe(ref);
  return 0;
}
"#);
}

// Slice 5 (Phase A): a reference to the whole array is in the parent group, not a child group, so a
// churn does not invalidate it.
#[test]
fn test_whole_array_ref_survives_churn() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  whole = &arr;
  churn(&arr);
  observe(whole);
  return 0;
}
"#);
}

// Slice 5 (Phase A): with both a whole-array reference and an element reference live across one
// churn, only the element reference (child group) is invalidated.
#[test]
fn test_element_ref_dies_but_sibling_whole_array_ref_lives() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  whole = &arr;
  ref = &arr[0];
  churn(&arr);
  observe(whole);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
"#,
  );
}

// Slice 6 (Phase A): using an element reference *before* the churn is clean — a churn only affects
// references live across it.
#[test]
fn test_use_element_before_churn_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  observe(ref);
  churn(&arr);
  return 0;
}
"#);
}

// Slice 7 (Phase A): a fresh element reference taken *after* the churn is live — invalidation marks
// the reference that existed across the churn, not the array.
#[test]
fn test_reborrow_after_churn_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  ref2 = &arr[0];
  observe(ref2);
  return 0;
}
"#);
}

// Slice 8 (Phase B): a churn inside one `if` arm invalidates an element reference used after the
// `if` — the may-invalidation flows to the join.
#[test]
fn test_churn_in_one_arm_use_after_if_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
  }
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
"#,
  );
}

// Slice 9 (Phase B): a churn in both arms invalidates after the `if`.
#[test]
fn test_churn_in_both_arms_use_after_if_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
  } else {
    churn(&arr);
  }
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:14:11:
  observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
"#,
  );
}

// Slice 10 (Phase B): a churn then a use *within* one arm is straight-line inside that arm.
#[test]
fn test_churn_then_use_within_arm_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
    observe(ref);
  }
  return 0;
}
"#,
    r#"At test:0.vale:11:13:
    observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
"#,
  );
}

// Slice 11 (Phase B): a churn in an arm that *diverges* (returns) never reaches the code after the
// `if`, so an element reference is still live there.
#[test]
fn test_churn_in_returning_arm_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
    return 0;
  }
  observe(ref);
  return 0;
}
"#);
}

// Slice 12 (Phase B): using an element reference inside an arm, with the churn only in a later
// statement after the `if`, is clean — the use precedes the churn on every path.
#[test]
fn test_use_in_arm_then_later_churn_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    observe(ref);
  }
  churn(&arr);
  return 0;
}
"#);
}

// Slice 13 (Phase C): a reference created before a loop, churned inside the body, and used at the
// top of the body is invalidated on the second iteration — the back-edge carries the churn.
#[test]
fn test_use_at_loop_top_after_body_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  while (false) {
    observe(ref);
    churn(&arr);
  }
  return 0;
}
"#,
    r#"At test:0.vale:10:13:
    observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:11:5:
    churn(&arr);
"#,
  );
}

// Slice 14 (Phase C): a churn inside a loop body invalidates a reference used after the loop.
#[test]
fn test_use_after_loop_with_body_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  while (false) {
    churn(&arr);
  }
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
"#,
  );
}

// Slice 15 (Phase C): a reference created fresh each iteration and used before that iteration's
// churn is live — the back-edge does not carry it, because the binding is re-taken.
#[test]
fn test_fresh_element_each_iteration_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  while (false) {
    ref = &arr[0];
    observe(ref);
    churn(&arr);
  }
  return 0;
}
"#);
}

// Slice 16 (Phase C): a loop with no churn leaves an element reference live.
#[test]
fn test_loop_without_churn_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  while (false) {
    observe(ref);
  }
  return 0;
}
"#);
}

// Slice 17 (Phase D): an element reference with no churn anywhere is freely usable — no false
// positive.
#[test]
fn test_element_used_without_any_churn_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  observe(ref);
  observe(ref);
  return 0;
}
"#);
}

// Slice 18 (Phase D): passing an invalidated element reference as a (non-first) argument is a use.
#[test]
fn test_pass_invalidated_element_ref_as_arg_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func pair<T>(a int, b &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  pair(7, ref);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  pair(7, ref);
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
"#,
  );
}

// Slice 19 (Phase D): churning one array leaves an element reference into a *different* array live.
#[test]
fn test_two_groups_churn_one_use_other_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  other = Array<int>(3);
  kept = &other[0];
  churn(&arr);
  observe(kept);
  return 0;
}
"#);
}

// Slice 20 (Phase D): one churn invalidates every element reference into the churned array; every
// subsequent use is reported, in source order.
#[test]
fn test_multiple_element_refs_all_invalidated_by_one_churn() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  first = &arr[0];
  second = &arr[1];
  churn(&arr);
  observe(first);
  observe(second);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
  observe(first);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
At test:0.vale:12:11:
  observe(second);
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
"#,
  );
}

// Slice 21 (Phase E): the grimoire's `ring_ref` scenario — an element reference used after a
// `damage` call that churns its group is rejected.
#[test]
fn test_ring_ref_used_after_damage_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func damage<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ring = &arr[0];
  damage(&arr);
  observe(ring);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  observe(ring);
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  damage(&arr);
"#,
  );
}

// Slice 22 (Phase E): the safe companion — a whole-array reference used after the same `damage`
// call is live, and a fuller clean program compiles.
#[test]
fn test_whole_array_ref_after_damage_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func damage<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  whole = &arr;
  before = &arr[0];
  observe(before);
  damage(&arr);
  observe(whole);
  after = &arr[0];
  observe(after);
  return 0;
}
"#);
}

// A held element reference is invalidated by a *sibling* argument's churn in the same call:
// evaluating `churn_ret(&arr)` for the second argument churns `arr` while `ref` waits in a register
// for the first, so `use2` consumes a dangling reference.
#[test]
fn test_held_element_ref_invalidated_by_sibling_arg_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn_ret<r'>(a &[]int in r) int mut(r) { return 0; }
func use2<T>(a &T, b int) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  use2(ref, churn_ret(&arr));
  return 0;
}
"#,
    r#"At test:0.vale:9:8:
  use2(ref, churn_ret(&arr));
Used a borrow after invalidated.
Invalidated at test:0.vale:9:13:
  use2(ref, churn_ret(&arr));
"#,
  );
}

// A nested member-element path: `get_tile` returns a reference into `lvl.tiles`'s elements
// (`&int in l.tiles[]`), and `churn_tiles` churns that member group (`mut(l.tiles)`), so using the
// returned reference afterward is a use-after-churn — the churn path `[Local(lvl), Member(tiles)]` is
// a prefix of the reference's `[Local(lvl), Member(tiles), Elements]`.
#[test]
fn test_nested_member_element_path_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Level { tiles []int; }
func get_tile<l'>(lvl &Level in l) &int in l.tiles[] { return &lvl.tiles[0]; }
func churn_tiles<l'>(lvl &Level in l) mut(l.tiles) { }
func observe<T>(x &T) { }
exported func main() int {
  lvl = Level(Array<int>(3));
  t = get_tile(&lvl);
  churn_tiles(&lvl);
  observe(t);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(t);
Used a borrow after invalidated.
Invalidated at test:0.vale:11:3:
  churn_tiles(&lvl);
"#,
  );
}

// A callee's return group rune that no parameter binds has no meaning at the call site. The checker
// treats it as a bug rather than letting the callee's rune leak into the caller's frame.
#[test]
#[should_panic(expected = "not bound at this call")]
fn test_return_group_rune_bound_by_no_parameter_panics() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func leak<g', h'>(a &[]int in g) &int in h { return &a[0]; }
exported func main() int {
  arr = Array<int>(3);
  v = leak(&arr);
  return 0;
}
"#);
}

#[test]
fn test_return_stale_element_reference_rejected() {
  super::util::assert_borrow_error_renders(
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
exported func leak<g'>(a &[]int in g) &int in g[] mut(g) {
  e = &a[0];
  churn(a);
  return e;
}
"#,
    r#"At test:0.vale:6:3:
  return e;
Used a borrow after invalidated.
Invalidated at test:0.vale:5:3:
  churn(a);
"#,
  );
}

#[test]
fn test_set_through_stale_element_reference_rejected() {
  super::util::assert_borrow_error_renders(
    r#"
struct Ship { fuel int; }
func churn<g'>(a &[]Ship in g) mut(g) { }
exported func scorch<g'>(a &[]Ship in g) mut(g) {
  s = &a[0];
  churn(a);
  set s.fuel = 1;
}
"#,
    r#"At test:0.vale:7:7:
  set s.fuel = 1;
Used a borrow after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
"#,
  );
}

#[test]
fn test_read_through_stale_element_reference_rejected() {
  super::util::assert_borrow_error_renders(
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
exported func peek<g'>(a &[]int in g) int mut(g) {
  e = &a[0];
  churn(a);
  return __copy_prim(e);
}
"#,
    r#"At test:0.vale:6:22:
  return __copy_prim(e);
Used a borrow after invalidated.
Invalidated at test:0.vale:5:3:
  churn(a);
"#,
  );
}

#[test]
fn test_use_after_churn_names_the_churn() {
  super::util::assert_borrow_error_renders(
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, h'>(x &T in h) { }
exported func peek<g'>(a &[]int in g) mut(g) {
  e = &a[0];
  churn(a);
  observe(e);
}
"#,
    r#"At test:0.vale:7:11:
  observe(e);
Used a borrow after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
"#,
  );
}

#[test]
fn test_two_stale_references_both_reported() {
  super::util::assert_borrow_error_renders(
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, h'>(x &T in h) { }
exported func peek<g'>(a &[]int in g) mut(g) {
  e = &a[0];
  f = &a[1];
  churn(a);
  observe(e);
  observe(f);
}
"#,
    r#"At test:0.vale:8:11:
  observe(e);
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
At test:0.vale:9:11:
  observe(f);
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
"#,
  );
}

#[test]
fn test_copied_stale_element_reference_rejected() {
  super::util::assert_borrow_error_renders(
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, h'>(x &T in h) { }
exported func peek<g'>(a &[]int in g) mut(g) {
  e = &a[0];
  w = e;
  churn(a);
  observe(w);
}
"#,
    r#"At test:0.vale:8:11:
  observe(w);
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
"#,
  );
}
