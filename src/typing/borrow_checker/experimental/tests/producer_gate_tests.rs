
use super::util::{assert_borrow_check_gives_error, assert_borrow_check_passes};

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
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
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
  ^^^^
this call changes an outside group, but function does not declare a mut effect for it.
"#,
  );
}

#[test]
fn test_declared_param_churn_is_accepted() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], &program(r#"
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
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], &program(r#"
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
