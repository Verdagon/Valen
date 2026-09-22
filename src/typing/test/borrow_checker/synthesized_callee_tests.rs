
use super::util::assert_compiles_clean;

#[test]
fn test_calling_a_struct_constructor_is_clean() {
  assert_compiles_clean(r#"
struct Ship { hp int; }
exported func main() int {
  s = Ship(7);
  return s.hp;
}
"#);
}

#[test]
fn test_calling_a_generic_struct_constructor_is_clean() {
  assert_compiles_clean(r#"
struct Ship { hp int; }
struct Box<T> where func drop(T)void { x T; }
exported func main() int {
  b = Box<Ship>(Ship(7));
  return b.x.hp;
}
"#);
}

#[test]
fn test_scope_end_drops_of_a_struct_and_an_interface_are_clean() {
  assert_compiles_clean(r#"
sealed interface Section {}
struct Ship { hp int; }
impl Section for Ship;
exported func main() int {
  s = Ship(7);
  sec Section = Ship(8);
  return s.hp;
}
"#);
}
