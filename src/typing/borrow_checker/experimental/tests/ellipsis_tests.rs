
use super::util::{assert_borrow_error_renders_with_arrays, assert_compiles_clean_with_arrays};

#[test]
fn test_use_ellipsis_return_after_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func peek<r'>(a &[]int in r) &int in r... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = peek(&arr);
  churn(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
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
fn test_ellipsis_ref_into_untouched_group_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func peek<r'>(a &[]int in r) &int in r... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  other = Array<int>(3);
  ref = peek(&arr);
  churn(&other);
  observe(ref);
  return 0;
}
"#);
}

#[test]
fn test_ellipsis_ref_invalidated_by_element_churn() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn_elems<r'>(a &[]int in r) mut(r[]) { }
func peek<r'>(a &[]int in r) &int in r... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = peek(&arr);
  churn_elems(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn_elems(&arr);
  ^^^^^^^^^^^
"#,
  );
}

#[test]
fn test_ellipsis_effect_invalidates_child_element() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn_ellipsis<r'>(a &[]int in r) mut(r...) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn_ellipsis(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn_ellipsis(&arr);
  ^^^^^^^^^^^^^^
"#,
  );
}

#[test]
fn test_ellipsis_effect_spares_whole_array() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn_ellipsis<r'>(a &[]int in r) mut(r...) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  whole = &arr;
  churn_ellipsis(&arr);
  observe(whole);
  return 0;
}
"#);
}

#[test]
fn test_ancestor_churn_invalidates_nested_ellipsis() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func peek_deep<r'>(a &[]int in r) &int in r[]... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = peek_deep(&arr);
  churn(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}
