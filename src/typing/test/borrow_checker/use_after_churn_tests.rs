
use super::util::{assert_borrow_error_renders, assert_borrow_error_renders_with_arrays, assert_borrow_error_renders_with_panic};

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
fn use_after_churn_through_a_rustlike_borrow_return_is_rejected() {
  assert_borrow_error_renders_with_panic(
    r#"
import v.builtins.panic.*;

struct Domino { }
func add_glyph<d'>(self &Domino in d) mut(d) { __vbi_panic(); }
func get_glyph<d'>(self &Domino in d) &Glyph in d... { __vbi_panic(); }
struct Glyph { }
func location<g'>(self &Glyph in g) &int in g... { __vbi_panic(); }

exported func foo(d &Domino) int mut(d) {
  d.add_glyph();
  d_ref = d.get_glyph();
  d.add_glyph();
  return d_ref.location();
}
"#,
    r#"At test:0.vale:14:10:
  return d_ref.location();
         ^^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:13:4:
  d.add_glyph();
   ^^^^^^^^^^^^
"#,
  );
}



