use super::util::assert_borrow_check_passes;

#[test]
fn test_return_group_compiles() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.drop.*;
func idr<g'>(a &int in g) &int in g { return a; }
exported func main() int { return 0; }
"#);
}

#[test]
fn generic_caller_of_generic_borrow_return() {
  assert_borrow_check_passes(&[], r#"
struct Box<E> { x E; }
func get<E, g'>(b &Box<E> in g) &E in g.x { return &b.x; }
func peek<T, h'>(b &Box<T> in h) &T in h.x { return b.get(); }
"#);
}
