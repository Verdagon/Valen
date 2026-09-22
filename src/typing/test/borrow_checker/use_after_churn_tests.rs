
use super::util::{assert_borrow_error_renders_with_arrays, assert_compiles_clean_with_arrays};

#[test]
fn test_return_position_group_compiles() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.drop.*;
func idr<g'>(a &int in g) &int in g { return a; }
exported func main() int { return 0; }
"#);
}

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
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

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

#[test]
fn test_param_element_group_compiles() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func peek<g'>(a &[]int in g, e &int in g[]) { }
exported func main() int { return 0; }
"#);
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

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
            ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

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
            ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:11:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

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
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

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
          ^^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
At test:0.vale:12:11:
  observe(second);
          ^^^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

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
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  damage(&arr);
  ^^^^^^
"#,
  );
}

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
       ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:13:
  use2(ref, churn_ret(&arr));
            ^^^^^^^^^
"#,
  );
}

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
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:11:3:
  churn_tiles(&lvl);
  ^^^^^^^^^^^
"#,
  );
}

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
  ^^^^^^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:5:3:
  churn(a);
  ^^^^^
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
      ^
Used a borrow after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
  ^^^^^
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
                     ^
Used a borrow after invalidated.
Invalidated at test:0.vale:5:3:
  churn(a);
  ^^^^^
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
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
  ^^^^^
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
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
  ^^^^^
At test:0.vale:9:11:
  observe(f);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
  ^^^^^
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
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
  ^^^^^
"#,
  );
}
