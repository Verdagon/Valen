use super::util::assert_borrow_check_passes;

#[test]
fn test_common_group_attack_aliasing_call_is_safe() {
  assert_borrow_check_passes(&[], r#"
struct Entity { hp int; }
func attack(a &Entity in r, d &Entity in r) mut(r) { }
exported func main() int {
  e = Entity(5);
  attack(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_disjoint_fields_attack_is_safe() {
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&["arith", "implicit_clone"], r#"
import v.builtins.arith.*;
struct Entity { hp int; energy int; }
func calculate_attack_power<r'>(self &Entity in r) int { return 5; }
func calculate_attack_cost<r'>(self &Entity in r, d &Entity in r) int { return 3; }
func calculate_defense<r'>(self &Entity in r) int { return 2; }
func calculate_defend_cost<r'>(self &Entity in r, a &Entity in r) int { return 1; }
func use_energy<r'>(self &Entity in r, cost int) mut(r) { }
func damage<r'>(self &Entity in r, amount int) mut(r) { }
func attack(a &Entity in r, d &Entity in r) mut(r) {
  let a_power = a.calculate_attack_power();
  let a_energy_cost = a.calculate_attack_cost(d);
  let d_armor = d.calculate_defense();
  let d_energy_cost = d.calculate_defend_cost(a);
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
  assert_borrow_check_passes(&["arith", "implicit_clone"], r#"
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
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&[], r#"
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
  assert_borrow_check_passes(&[], r#"
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
fn test_mixed_group_and_plain_params_no_false_positive() {
  assert_borrow_check_passes(&[], r#"
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
fn multiple_mutable_aliases_to_one_object_are_legal() {
  assert_borrow_check_passes(&[], r#"
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

#[test]
fn mut_param_is_accepted() {
  assert_borrow_check_passes(&[], r#"
struct Entity { hp int; }
func heal(e &Entity mut) { set e.hp = 5; }
exported func main() int {
  x = Entity(3);
  heal(&x);
  return 0;
}
"#);
}
