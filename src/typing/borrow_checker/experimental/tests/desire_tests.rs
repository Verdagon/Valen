use super::util::{assert_borrow_check_gives_error, assert_borrow_check_passes};

const PREAMBLE: &str = r#"
import v.builtins.arrays.*;
import v.builtins.arith.*;
import v.builtins.drop.*;
struct Weapon { dmg int; }
struct Entity { hp int; weapon Weapon; }
#!DeriveStructDrop
struct World { entities []Entity; }
func spawn<w'>(world &World in w) mut(w) { }
func spawn_count<w'>(world &World in w) int mut(w) { return 0; }

sealed interface Desire<w', s'> {
  func desire_strength(virtual self &Desire<w, s>) int;
  func enact(virtual self &Desire<w, s>) mut(w) mut(s);
}
struct AttackDesire<w', s'> { strength int; victim &Entity in w; weapon &Weapon in s; }
impl<w', s'> Desire<w, s> for AttackDesire<w, s>;
func desire_strength<w', s'>(self &AttackDesire<w, s>) int { return self.strength; }
func enact<w', s'>(self &AttackDesire<w, s>) mut(w) mut(s) {
  set self.victim.hp = 1;
  set self.weapon.dmg = 0;
}
func want_attack<w', s'>(me &Entity in s, victim &Entity in w) Desire<w, s> {
  return AttackDesire(10, victim, &me.weapon);
}
func observe<T>(x &T) { }
"#;

fn program(body: &str) -> String {
  format!("{}{}", PREAMBLE, body)
}

// Ignored: virtual dispatch on the group-parametrized `Desire<w', s'>` interface panics upstream in
// the typing pass (get_placeholder_templata_id on a Group templata, edge_compiler.rs), so the borrow
// check never runs. Blocked on the edge_compiler/instantiator Group-in-override-dispatch decision.
#[ignore]
#[test]
fn test_desire_used_after_world_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    &program(r#"
func act<w', s'>(me &Entity in s, world &World in w) mut(w) mut(s) {
  d = want_attack(me, &world.entities[0]);
  world.spawn();
  d.enact();
}
"#),
    r#"At test:0.vale:31:3:
  d.enact();
  ^
Used a borrow after invalidated.
Invalidated at test:0.vale:30:8:
  world.spawn();
       ^^^^^^^^
"#,
  );
}

// Ignored: same Group-in-override-dispatch panic as above (edge_compiler.rs), before the borrow
// check runs. (Also a loop test — the back-edge fix governs its loop reasoning once unblocked.)
#[ignore]
#[test]
fn test_desire_used_after_world_churn_in_loop_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    &program(r#"
func act<w', s'>(me &Entity in s, world &World in w) mut(w) mut(s) {
  d = want_attack(me, &world.entities[0]);
  i = 0;
  while (i < 3) {
    world.spawn();
    set i = i + 1;
  }
  d.enact();
}
"#),
    r#"At test:0.vale:35:3:
  d.enact();
  ^
Used a borrow after invalidated.
Invalidated at test:0.vale:32:10:
    world.spawn();
         ^^^^^^^^
"#,
  );
}

// Ignored: same Group-in-override-dispatch panic as above (edge_compiler.rs), before the borrow
// check runs.
#[ignore]
#[test]
fn test_desire_spoiled_by_sibling_argument_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    &program(r#"
func use2<T>(a &T, b int) { }
func act<w', s'>(me &Entity in s, world &World in w) mut(w) mut(s) {
  d = want_attack(me, &world.entities[0]);
  use2(&d, world.spawn_count());
}
"#),
    r#"At test:0.vale:31:9:
  use2(&d, world.spawn_count());
        ^
Used a borrow after invalidated.
Invalidated at test:0.vale:31:17:
  use2(&d, world.spawn_count());
                ^^^^^^^^^^^^^^
"#,
  );
}

// Ignored: same Group-in-override-dispatch panic as above (edge_compiler.rs), before the borrow
// check runs.
#[ignore]
#[test]
fn test_desires_borrow_world_and_own_gear_and_the_strongest_is_enacted() {
  assert_borrow_check_passes(&["arith", "drop", "implicit_clone"], r#"
import v.builtins.arith.*;
import v.builtins.drop.*;
struct Weapon { dmg int; }
struct HealStaff { power int; }
struct HealthPotion { heal int; }
struct Entity { hp int; weapon Weapon; staff HealStaff; potion HealthPotion; }
struct Tile { x int; }

sealed interface Desire<w', s'> {
  func desire_strength(virtual self &Desire<w, s>) int;
  func enact(virtual self &Desire<w, s>) mut(w) mut(s);
}

struct AttackDesire<w', s'> { strength int; victim &Entity in w; weapon &Weapon in s; }
impl<w', s'> Desire<w, s> for AttackDesire<w, s>;
func desire_strength<w', s'>(self &AttackDesire<w, s>) int { return self.strength; }
func enact<w', s'>(self &AttackDesire<w, s>) mut(w) mut(s) {
  set self.victim.hp = 1;
  set self.weapon.dmg = 0;
}

struct HealDesire<w', s'> { strength int; target &Entity in w; staff &HealStaff in s; }
impl<w', s'> Desire<w, s> for HealDesire<w, s>;
func desire_strength<w', s'>(self &HealDesire<w, s>) int { return self.strength; }
func enact<w', s'>(self &HealDesire<w, s>) mut(w) mut(s) {
  set self.target.hp = 20;
  set self.staff.power = 0;
}

struct MoveDesire<w', s'> { strength int; tile &Tile in w; }
impl<w', s'> Desire<w, s> for MoveDesire<w, s>;
func desire_strength<w', s'>(self &MoveDesire<w, s>) int { return self.strength; }
func enact<w', s'>(self &MoveDesire<w, s>) mut(w) mut(s) {
  set self.tile.x = 2;
}

struct UseItemDesire<w', s'> { strength int; potion &HealthPotion in s; }
impl<w', s'> Desire<w, s> for UseItemDesire<w, s>;
func desire_strength<w', s'>(self &UseItemDesire<w, s>) int { return self.strength; }
func enact<w', s'>(self &UseItemDesire<w, s>) mut(w) mut(s) {
  set self.potion.heal = 0;
}

func want_attack<w', s'>(me &Entity in s, victim &Entity in w) Desire<w, s> {
  return AttackDesire(10, victim, &me.weapon);
}
func want_heal<w', s'>(me &Entity in s, ally &Entity in w) Desire<w, s> {
  return HealDesire(5, ally, &me.staff);
}
func want_move<w', s'>(me &Entity in s, tile &Tile in w) Desire<w, s> {
  return MoveDesire(3, tile);
}
func want_item<w', s'>(me &Entity in s, tile &Tile in w) Desire<w, s> {
  return UseItemDesire(7, &me.potion);
}

func act<w', s'>(me &Entity in s, victim &Entity in w, ally &Entity in w, tile &Tile in w) mut(w) mut(s) {
  a = want_attack(me, victim);
  b = want_heal(me, ally);
  c = want_move(me, tile);
  d = want_item(me, tile);
  best = &a;
  if (b.desire_strength() > best.desire_strength()) { set best = &b; }
  if (c.desire_strength() > best.desire_strength()) { set best = &c; }
  if (d.desire_strength() > best.desire_strength()) { set best = &d; }
  best.enact();
}

exported func main() int {
  me = Entity(10, Weapon(3), HealStaff(4), HealthPotion(2));
  other = Entity(10, Weapon(3), HealStaff(4), HealthPotion(2));
  tile = Tile(0);
  act(&me, &other, &other, &tile);
  return other.hp;
}
"#);
}

#[test]
fn test_entity_passed_as_both_world_and_self_is_rejected() {
  assert_borrow_check_gives_error(&["arith", "drop", "implicit_clone"], r#"
import v.builtins.arith.*;
import v.builtins.drop.*;
struct Entity { hp int; }
func act<w', s'>(me &Entity in s, victim &Entity in w) mut(w) mut(s) {
  set victim.hp = 1;
  set me.hp = 2;
}
exported func main() int {
  me = Entity(10);
  act(&me, &me);
  return me.hp;
}
"#,
    r#"At test:0.vale:11:8:
  act(&me, &me);
       ^^
Arguments 0 and 1 both borrow into me, but their parameters are in disjoint mutated groups s and w, which the callee may treat as non-aliasing.
"#);
}
