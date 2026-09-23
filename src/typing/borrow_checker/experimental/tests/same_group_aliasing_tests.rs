
use super::util::{assert_borrow_error_renders_with_arrays, assert_compiles_clean_with_arrays};

const PREAMBLE: &str = r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Entity { id int; hp int; }
#!DeriveStructDrop
struct Vec<T> { data []T; }
func drop<T>(v Vec<T>) where func drop(T)void {
  [data] = ^v;
  drop(^data);
}
func grow<r'>(vec &Vec<Entity> in r) mut(r) { }
func observe<T>(x &T) { }
"#;

fn program(body: &str) -> String {
  format!("{}{}", PREAMBLE, body)
}

#[test]
fn test_churn_sibling_param_in_same_group_rejected() {
  assert_borrow_error_renders_with_arrays(
    &program(r#"
func attack<r'>(a &Vec<Entity> in r, t &Vec<Entity> in r) mut(r) {
  e = &a.data[0];
  grow(t);
  observe(e);
}
exported func main() int {
  v = Vec<Entity>(Array<Entity>(3));
  attack(&v, &v);
  return 0;
}
"#),
    r#"At test:0.vale:17:11:
  observe(e);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:16:3:
  grow(t);
  ^^^^
"#,
  );
}

#[test]
fn test_churn_fresh_local_group_is_accepted() {
  assert_compiles_clean_with_arrays(&program(r#"
func attack<r'>(a &Vec<Entity> in r, t &Vec<Entity> in r) {
  nv = Vec<Entity>(Array<Entity>(0));
  e = &a.data[0];
  grow(&nv);
  observe(e);
}
exported func main() int {
  v = Vec<Entity>(Array<Entity>(3));
  attack(&v, &v);
  return 0;
}
"#));
}

#[test]
fn test_churn_same_param_rejected() {
  assert_borrow_error_renders_with_arrays(
    &program(r#"
func attack<r'>(a &Vec<Entity> in r, t &Vec<Entity> in r) mut(r) {
  e = &a.data[0];
  grow(a);
  observe(e);
}
exported func main() int {
  v = Vec<Entity>(Array<Entity>(3));
  attack(&v, &v);
  return 0;
}
"#),
    r#"At test:0.vale:17:11:
  observe(e);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:16:3:
  grow(a);
  ^^^^
"#,
  );
}
