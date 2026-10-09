
use super::util::assert_borrow_check_gives_error;
use super::util::assert_borrow_check_passes;
use super::util::param_noalias_of;

#[test]
fn test_param_element_group_shares_root_rune_with_container() {
  let noalias = param_noalias_of(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Entity { hp int; }
struct World { entities []Entity; }
func step(world &World, entity &Entity in world.entities[]) { }
"#,
    "step",
  );
  assert_eq!(noalias, vec![false, false]);
}

#[test]
fn test_member_element_ref_survives_nonmutating_whole_read() {
  assert_borrow_check_passes(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Collision { x int; }
struct Entity { hp int; }
struct World { entities []Entity; }
func advance<e'>(entity &Entity in e) mut(e) { }
func get_collision_for_entity(world &World, entity &Entity in world.entities[]) Collision { return Collision(0); }
func resolve<e'>(entity &Entity in e, collision &Collision) mut(e) { }
func step(world &World, entity &Entity in world.entities[] mut) {
  entity.advance();
  let collision = world.get_collision_for_entity(entity);
  entity.resolve(collision);
}
"#,
  );
}

#[test]
fn test_mutating_shared_borrow_param_without_mut_permission_rejected() {
  assert_borrow_check_gives_error(
    &[],
    r#"
struct Entity { hp int; }
func churn<e'>(entity &Entity in e) mut(e) { }
func step(entity &Entity) {
  entity.churn();
}
"#,
    r#"At test:0.vale:5:9:
  entity.churn();
        ^^^^^^^^
this call changes an outside group, but function does not declare a mut effect for it.
"#,
  );
}

#[test]
fn test_use_element_after_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(arr &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

#[test]
fn test_element_ref_dies_but_sibling_whole_array_ref_lives() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  whole = &arr;
  ref = &arr[0];
  churn(&arr);
  observe(whole);
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

#[test]
fn test_churn_in_one_arm_use_after_if_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
  }
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

#[test]
fn test_churn_then_use_within_arm_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  if (true) {
    churn(&arr);
    observe(ref);
  }
  return 0;
}
"#,
    r#"At test:0.vale:11:13:
    observe(ref);
            ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

#[test]
fn test_use_after_loop_with_body_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  while (false) {
    churn(&arr);
  }
  observe(ref);
  return 0;
}
"#,
    r#"At test:0.vale:12:11:
  observe(ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

#[test]
fn test_pass_invalidated_element_ref_as_arg_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func pair<T>(a int, b &T) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  churn(&arr);
  pair(7, ref);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  pair(7, ref);
          ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

#[test]
fn test_ring_ref_used_after_damage_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func damage<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  ring = &arr[0];
  damage(&arr);
  observe(ring);
  return 0;
}
"#,
    r#"At test:0.vale:10:11:
  observe(ring);
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  damage(&arr);
  ^^^^^^
"#,
  );
}

#[test]
fn test_held_element_ref_invalidated_by_sibling_arg_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn_ret<r'>(a &[]int in r) int mut(r) { return 0; }
func use2<T>(a &T, b int) { }
exported func main() int {
  arr = Array<int>(3);
  ref = &arr[0];
  use2(ref, churn_ret(&arr));
  return 0;
}
"#,
    r#"At test:0.vale:9:8:
  use2(ref, churn_ret(&arr));
       ^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:13:
  use2(ref, churn_ret(&arr));
            ^^^^^^^^^
"#,
  );
}



#[test]
fn use_after_churn_through_a_rustlike_borrow_return_is_rejected() {
  assert_borrow_check_gives_error(
    &["panic"],
    r#"
import v.builtins.panic.*;

struct Domino { }
func add_glyph<d'>(self &Domino in d) mut(d) { __vbi_panic(); }
func get_glyph<d'>(self &Domino in d) &Glyph in d... { __vbi_panic(); }
struct Glyph { }
func location<g'>(self &Glyph in g) &int in g... { __vbi_panic(); }

exported func foo<g'>(d &Domino in g) int mut(g) {
  d.add_glyph();
  d_ref = d.get_glyph();
  d.add_glyph();
  return d_ref.location();
}
"#,
    r#"At test:0.vale:14:10:
  return d_ref.location();
         ^^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:13:4:
  d.add_glyph();
   ^^^^^^^^^^^^
"#,
  );
}

#[test]
fn test_attack_element_borrow_used_after_damage_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
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
fn test_array_element_borrow_used_after_churn_repro() {
  assert_borrow_check_gives_error(
    &[],
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

#[test]
fn test_use_returned_reference_after_churn_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func get<g'>(a &[]int in g, i int) &int in g[] { return &a[__copy_prim(i)]; }
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, tg'>(x &T in tg) { }
exported func main() int {
  arr = Array<int>(3);
  v = arr.get(0);
  churn(&arr);
  observe(v);
  return 0;
}
"#,
    r#"At test:0.vale:11:11:
  observe(v);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:3:
  churn(&arr);
  ^^^^^
"#,
  );
}

#[test]
fn test_use_after_churn_names_the_churn() {
  assert_borrow_check_gives_error(
    &[],
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, h'>(x &T in h) { }
exported func peek<g'>(a &[]int in g) mut(g) {
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

#[test]
fn test_copied_stale_element_reference_rejected() {
  assert_borrow_check_gives_error(
    &[],
    r#"
func churn<g'>(a &[]int in g) mut(g) { }
func observe<T, h'>(x &T in h) { }
exported func peek<g'>(a &[]int in g) mut(g) {
  e = &a[0];
  w = e;
  churn(a);
  observe(w);
}
"#,
    r#"At test:0.vale:8:11:
  observe(w);
          ^
Used a borrow after invalidated.
Invalidated at test:0.vale:7:3:
  churn(a);
  ^^^^^
"#,
  );
}



