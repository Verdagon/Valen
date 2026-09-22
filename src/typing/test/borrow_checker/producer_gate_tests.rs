
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
"#;

fn program(body: &str) -> String {
  format!("{}{}", PREAMBLE, body)
}

#[test]
fn test_undeclared_param_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    &program(r#"
func churner<g'>(v &Vec<Entity> in g) {
  grow(v);
}
exported func main() int {
  vec = Vec<Entity>(Array<Entity>(3));
  churner(&vec);
  return 0;
}
"#),
    r#"At test:0.vale:14:3:
  grow(v);
this call churns a group reached through a parameter, but the enclosing function does not declare a mut effect for it.
"#,
  );
}

#[test]
fn test_declared_param_churn_is_accepted() {
  assert_compiles_clean_with_arrays(&program(r#"
func churner<g'>(v &Vec<Entity> in g) mut(g) {
  grow(v);
}
exported func main() int {
  vec = Vec<Entity>(Array<Entity>(3));
  churner(&vec);
  return 0;
}
"#));
}

#[test]
fn test_local_churn_needs_no_declaration() {
  assert_compiles_clean_with_arrays(&program(r#"
func churner<g'>(v &Vec<Entity> in g) {
  nv = Vec<Entity>(Array<Entity>(0));
  grow(&nv);
}
exported func main() int {
  vec = Vec<Entity>(Array<Entity>(3));
  churner(&vec);
  return 0;
}
"#));
}
