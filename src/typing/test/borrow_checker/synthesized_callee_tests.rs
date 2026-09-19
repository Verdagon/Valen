//! A borrow-checked body may call functions the typing pass synthesizes rather than the user writes:
//! struct constructors and the drops inserted at scope end. The checker reads a callee's return groups
//! off its written return type and refuses a non-lambda callee without one, so each synthesized
//! producer has to write its return. These pin that through the only interface that observes it: the
//! program compiles clean.

use super::util::assert_compiles_clean;

// A struct constructor call in a checked body: the constructor is a synthesized function whose written
// return is the struct itself.
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

// A generic struct's constructor, written as the struct applied to its own runes.
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

// The drops inserted where `s` and `sec` leave scope call the struct's synthesized drop and the sealed
// interface's abstract drop, each written `void`.
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
