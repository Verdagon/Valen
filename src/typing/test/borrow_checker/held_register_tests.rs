
use super::util::{assert_borrow_error_renders_with_arrays, assert_compiles_clean_with_arrays};

#[test]
fn test_held_call_result_invalidated_by_sibling_arg_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func get<g'>(a &[]int in g) &int in g[] { return &a[0]; }
func churn_ret<g'>(a &[]int in g) int mut(g) { return 0; }
func use2<T>(a &T, b int) { }
exported func main() int {
  arr = Array<int>(3);
  use2(get(&arr), churn_ret(&arr));
  return 0;
}
"#,
    r#"At test:0.vale:9:8:
  use2(get(&arr), churn_ret(&arr));
       ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:19:
  use2(get(&arr), churn_ret(&arr));
                  ^^^^^^^^^
"#,
  );
}

#[test]
fn test_held_call_result_into_untouched_group_is_clean() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func get<g'>(a &[]int in g) &int in g[] { return &a[0]; }
func churn_ret<g'>(a &[]int in g) int mut(g) { return 0; }
func use2<T>(a &T, b int) { }
exported func main() int {
  arr = Array<int>(3);
  other = Array<int>(3);
  use2(get(&arr), churn_ret(&other));
  return 0;
}
"#);
}
