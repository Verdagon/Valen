use super::util::{
  assert_borrow_error_renders, assert_borrow_error_renders_with_arrays, assert_compiles_clean,
  assert_compiles_clean_with_arith, assert_compiles_clean_with_arrays,
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
fn test_full_attack_program_is_safe() {
  assert_compiles_clean(r#"
struct Entity { hp int; }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  set a.hp = 1;
  set d.hp = 2;
}
exported func main() int {
  e = Entity(5);
  e2 = Entity(6);
  attack(&e, &e2);
  attack(&e, &e);
  return 0;
}
"#);
}

#[test]
fn test_attack_element_borrow_used_after_damage_churn_rejected() {
  assert_borrow_error_renders_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Entity { hp int; buffs []int; }
func damage<r'>(self &Entity in r, amount int) mut(r) { }
func print_int<g'>(i &int in g) { }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  buff = &d.buffs[0];
  d.damage(5);
  print_int(buff);
}
exported func main() int {
  e = Entity(5, Array<int>(3));
  attack(&e, &e);
  return 0;
}
"#,
    r#"At test:0.vale:11:13:
  print_int(buff);
            ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:4:
  d.damage(5);
   ^^^^^^^^^^
"#,
  );
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
fn test_array_element_borrow_used_after_churn_repro() {
  assert_borrow_error_renders(
    r#"
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T, g'>(x &T in g) { }
exported func peek<r'>(a &[]int in r) mut(r) {
  e = &a[0];
  churn(a);
  observe(e);
}
"#,
    r#"At test:0.vale:7:11:
  observe(e);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
  ^^^^^
"#,
  );
}
