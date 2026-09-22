use super::util::{assert_borrow_error_renders, assert_borrow_error_renders_with_arrays};

#[test]
fn test_attack_element_borrow_used_after_damage_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Entity { hp int; buffs []int; }
func damage<r'>(self &Entity in r, amount int) mut(r) { }
func print_int<g'>(i &int in g) { }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  buff = &d.buffs[0];
  d.damage(5);
  print_int(buff);
}
exported func main() int {
  e = Entity(5, Array<int>(3));
  attack(&e, &e);
  return 0;
}
"#,
    r#"At test:0.vale:11:13:
  print_int(buff);
            ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:4:
  d.damage(5);
   ^^^^^^^^^^
"#,
  );
}

#[test]
fn test_array_element_borrow_used_after_churn_repro() {
  assert_borrow_error_renders(
    r#"
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T, g'>(x &T in g) { }
exported func peek<r'>(a &[]int in r) mut(r) {
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
fn test_use_after_churn_names_the_churn() {
  assert_borrow_error_renders(
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
fn test_copied_stale_element_reference_rejected() {
  assert_borrow_error_renders(
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
