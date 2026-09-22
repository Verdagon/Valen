
use super::util::assert_compiles_clean;

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
