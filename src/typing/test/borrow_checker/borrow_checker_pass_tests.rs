use super::util::{
  assert_compiles_clean, assert_compiles_clean_with_arith, assert_compiles_clean_with_arrays,
  assert_param_noalias,
};

#[test]
fn test_common_group_attack_aliasing_call_is_safe() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) { }
exported func main() int {
  e = Entity(5);
  attack(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_disjoint_fields_attack_is_safe() {
  assert_compiles_clean(r#"
struct Ship { fuel int; }
struct Fleet { flagship Ship; escort Ship; }
func attack2<r', s'>(a &Ship in r, d &Ship in s) mut(r) mut(s) { }
exported func main() int {
  fleet = Fleet(Ship(1), Ship(2));
  attack2(&fleet.flagship, &fleet.escort);
  return 0;
}
"#);
}

#[test]
fn test_method_call_attack_distinct_entities() {
  assert_compiles_clean_with_arith(r#"
import v.builtins.arith.*;
struct Entity { hp int; energy int; }
func calculate_attack_power<r'>(self &Entity in r) int { return 5; }
func calculate_attack_cost<r'>(self &Entity in r, d &Entity in r) int { return 3; }
func calculate_defense<r'>(self &Entity in r) int { return 2; }
func calculate_defend_cost<r'>(self &Entity in r, a &Entity in r) int { return 1; }
func use_energy<r'>(self &Entity in r, cost int) mut(r) { }
func damage<r'>(self &Entity in r, amount int) mut(r) { }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  a_power = a.calculate_attack_power();
  a_energy_cost = a.calculate_attack_cost(d);
  d_armor = d.calculate_defense();
  d_energy_cost = d.calculate_defend_cost(a);
  a.use_energy(a_energy_cost);
  d.use_energy(d_energy_cost);
  d.damage(a_power - d_armor);
}
exported func main() int {
  e = Entity(5, 100);
  e2 = Entity(6, 100);
  attack(&e, &e2);
  return 0;
}
"#);
}

#[test]
fn test_method_call_attack_self_attack() {
  assert_compiles_clean_with_arith(r#"
import v.builtins.arith.*;
struct Entity { hp int; energy int; }
func calculate_attack_power<r'>(self &Entity in r) int { return 5; }
func calculate_attack_cost<r'>(self &Entity in r, d &Entity in r) int { return 3; }
func calculate_defense<r'>(self &Entity in r) int { return 2; }
func calculate_defend_cost<r'>(self &Entity in r, a &Entity in r) int { return 1; }
func use_energy<r'>(self &Entity in r, cost int) mut(r) { }
func damage<r'>(self &Entity in r, amount int) mut(r) { }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  a_power = a.calculate_attack_power();
  a_energy_cost = a.calculate_attack_cost(d);
  d_armor = d.calculate_defense();
  d_energy_cost = d.calculate_defend_cost(a);
  a.use_energy(a_energy_cost);
  d.use_energy(d_energy_cost);
  d.damage(a_power - d_armor);
}
exported func main() int {
  e = Entity(5, 100);
  attack(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_borrow_struct_member_minimal_repro() {
  assert_compiles_clean(r#"
struct Ship { fuel int; }
func peek<r'>(s &Ship in r) {
  f = &s.fuel;
}
exported func main() int {
  ship = Ship(5);
  peek(&ship);
  return 0;
}
"#);
}

#[test]
fn test_borrow_into_other_local_with_move_is_clean() {
  assert_compiles_clean(r#"
struct Holder { n int; }
func consume<g'>(a &Holder in g, b Holder) { }
exported func main() int {
  h = Holder(1);
  y = Holder(2);
  consume(&y, ^h);
  return 0;
}
"#);
}

#[test]
fn test_alias_into_distinct_groups_without_mut_is_clean() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func purepair<r', s'>(a &Entity in r, d &Entity in s) { }
exported func main() int {
  e = Entity(5);
  purepair(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_common_group_aliasing_is_clean() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func heal<g'>(a &Entity in g, d &Entity in g) mut(g) { }
exported func main() int {
  e = Entity(5);
  heal(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_distinct_locals_into_distinct_mut_groups_clean() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func badpair<r', s'>(a &Entity in r, d &Entity in s) mut(r) { }
exported func main() int {
  e1 = Entity(5);
  e2 = Entity(6);
  badpair(&e1, &e2);
  return 0;
}
"#);
}

#[test]
fn test_sibling_fields_are_disjoint_clean() {
  assert_compiles_clean(r#"
struct Ship { fuel int; }
struct Fleet { flagship Ship; escort Ship; }
func badships<r', s'>(a &Ship in r, d &Ship in s) mut(r) { }
exported func main() int {
  f = Fleet(Ship(1), Ship(2));
  badships(&f.flagship, &f.escort);
  return 0;
}
"#);
}

#[test]
fn same_group_params_are_not_noalias() {
  assert_param_noalias(
    r#"
struct Ship { fuel int; }
func pair<g'>(a &Ship in g, b &Ship in g) { }
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  pair(&s1, &s2);
  return 0;
}
"#,
    "pair",
    &[false, false],
  );
}

#[test]
fn test_mixed_group_and_plain_params_no_false_positive() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func mixed<r'>(a &Entity in r, b int) mut(r) { }
exported func main() int {
  e = Entity(5);
  mixed(&e, 7);
  return 0;
}
"#);
}

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
fn test_return_position_group_compiles() {
  assert_compiles_clean_with_arrays(r#"
import v.builtins.drop.*;
func idr<g'>(a &int in g) &int in g { return a; }
exported func main() int { return 0; }
"#);
}

#[test]
fn multiple_mutable_aliases_to_one_object_are_legal() {
  assert_compiles_clean(r#"
struct Slot { value int; }
func mutate<g'>(self &Slot in g, v int) mut(g) { }
func get<g'>(self &Slot in g) int { return self.value; }
exported func main() int {
  slot = Slot(0);
  ref_a = &slot;
  ref_b = &slot;
  ref_a.mutate(42);
  ref_b.mutate(73);
  return ref_a.get();
}
"#);
}
