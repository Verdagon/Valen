use super::util::assert_borrow_check_passes;

#[test]
fn test_calling_a_struct_constructor_is_clean() {
  assert_borrow_check_passes(&[], r#"
struct Ship { hp int; }
exported func main() int {
  s = Ship(7);
  return s.hp;
}
"#);
}

#[test]
fn test_calling_a_generic_struct_constructor_is_clean() {
  assert_borrow_check_passes(&[], r#"
struct Ship { hp int; }
struct Box<T> where func drop(T)void { x T; }
exported func main() int {
  b = Box<Ship>(Ship(7));
  return b.x.hp;
}
"#);
}
