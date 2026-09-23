
use super::util::{group_facts_of, group_facts_of_with_arrays};
use std::collections::HashSet;

#[test]
fn shared_group_accesses_map_to_the_one_group_and_noarg_call_reaches_nothing() {
  let facts = group_facts_of(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set a.fuel = 1;
  set b.fuel = 2;
  set a.fuel = 3;
  nothing();
  set a.fuel = 4;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    "do_things",
  );
  assert_eq!(facts.group_paths, vec!["g".to_string()]);
  assert!(facts.accessed_group_sets.iter().any(|s| !s.is_empty()), "expected some accesses");
  assert!(
    facts.accessed_group_sets.iter().filter(|s| !s.is_empty()).all(|s| s == &vec![0u32]),
    "every access should be into group 0, got {:?}",
    facts.accessed_group_sets
  );
  assert_eq!(
    facts.accessed_group_sets.iter().filter(|s| s.is_empty()).count(),
    1,
    "the one no-arg call reaches nothing (exactly one empty set), got {:?}",
    facts.accessed_group_sets
  );
}

#[test]
fn whole_function_sole_reference_accesses_are_still_recorded() {
  let facts = group_facts_of(
    r#"
struct Ship { fuel int; }
func nothing() { }
func solo<g'>(a &Ship in g) mut(g) {
  set a.fuel = 1;
  nothing();
  set a.fuel = 2;
}
exported func main() int {
  ship = Ship(1);
  solo(&ship);
  return 0;
}
"#,
    "solo",
  );
  assert_eq!(facts.group_paths, vec!["g".to_string()]);
  assert!(facts.accessed_group_sets.iter().any(|s| !s.is_empty()), "expected some accesses");
  assert!(
    facts.accessed_group_sets.iter().filter(|s| !s.is_empty()).all(|s| s == &vec![0u32]),
    "every access should be into group 0, got {:?}",
    facts.accessed_group_sets
  );
}

#[test]
fn tail_without_a_call_still_records_accesses() {
  let facts = group_facts_of(
    r#"
struct Ship { fuel int; }
func do_things<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set a.fuel = 1;
  set b.fuel = 2;
  set a.fuel = 3;
  set a.fuel = 4;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    "do_things",
  );
  assert_eq!(facts.group_paths, vec!["g".to_string()]);
  assert!(!facts.accessed_group_sets.is_empty(), "expected recorded accesses");
  assert!(
    facts.accessed_group_sets.iter().all(|s| s == &vec![0u32]),
    "with no call, every instruction is an access into group 0, got {:?}",
    facts.accessed_group_sets
  );
}

#[test]
fn sibling_array_members_get_distinct_child_scopes() {
  let facts = group_facts_of_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Level { tiles []int; foes []int; }
func nothing() { }
func do_things<l'>(lvl &Level in l) {
  t = &lvl.tiles[0];
  f = &lvl.foes[0];
  x = __copy_prim(t);
  y = __copy_prim(f);
  nothing();
}
exported func main() int {
  lvl = Level(Array<int>(3), Array<int>(3));
  do_things(&lvl);
  return 0;
}
"#,
    "do_things",
  );
  assert!(facts.group_paths.iter().any(|n| n == "l.tiles[]"), "expected an l.tiles[] scope, got {:?}", facts.group_paths);
  assert!(facts.group_paths.iter().any(|n| n == "l.foes[]"), "expected an l.foes[] scope, got {:?}", facts.group_paths);
  let distinct: HashSet<u32> =
    facts.accessed_group_sets.iter().filter(|s| s.len() == 1).map(|s| s[0]).collect();
  assert!(distinct.len() >= 2, "the two element loads should land in distinct scopes, got {distinct:?}");
}

#[test]
fn direct_element_accesses_get_distinct_child_scopes() {
  let facts = group_facts_of_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Level { tiles []int; foes []int; }
func do_things<l'>(lvl &Level in l) {
  set lvl.tiles[0] = 1;
  set lvl.foes[0] = 2;
}
exported func main() int {
  lvl = Level(Array<int>(3), Array<int>(3));
  do_things(&lvl);
  return 0;
}
"#,
    "do_things",
  );
  assert!(facts.group_paths.iter().any(|n| n == "l.tiles[]"), "expected an l.tiles[] scope, got {:?}", facts.group_paths);
  assert!(facts.group_paths.iter().any(|n| n == "l.foes[]"), "expected an l.foes[] scope, got {:?}", facts.group_paths);
  let distinct: HashSet<u32> =
    facts.accessed_group_sets.iter().filter(|s| s.len() == 1).map(|s| s[0]).collect();
  assert!(distinct.len() >= 2, "the two direct element writes should land in distinct scopes, got {distinct:?}");
}

#[test]
fn unaccessed_parameter_group_is_still_counted() {
  let facts = group_facts_of(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g', h'>(a &Ship in g, b &Ship in h) mut(g) {
  set a.fuel = 1;
  nothing();
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    "do_things",
  );
  assert!(facts.group_paths.iter().any(|n| n == "g"), "expected g, got {:?}", facts.group_paths);
  assert!(facts.group_paths.iter().any(|n| n == "h"), "expected h (unaccessed param group) to still be counted, got {:?}", facts.group_paths);
}

#[test]
fn call_reaches_descendants_of_its_argument_group() {
  let facts = group_facts_of_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Level { tiles []int; foes []int; }
func reads_level<l'>(lvl &Level in l) { }
func do_things<l'>(lvl &Level in l) {
  set lvl.tiles[0] = 1;
  set lvl.foes[0] = 2;
  reads_level(lvl);
}
exported func main() int {
  lvl = Level(Array<int>(3), Array<int>(3));
  do_things(&lvl);
  return 0;
}
"#,
    "do_things",
  );
  assert!(facts.group_paths.len() >= 3, "expected l + two child groups, got {:?}", facts.group_paths);
  assert!(
    facts.accessed_group_sets.iter().any(|s| s.len() >= 3),
    "reads_level(lvl) should reach l and its two child groups (a 3-element set), got {:?}",
    facts.accessed_group_sets
  );
}

#[test]
fn static_sized_array_element_ref_folds_into_parent_group() {
  let facts = group_facts_of_with_arrays(
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func do_things() {
  arr = [#](1, 2, 3, 4, 5);
  b = &arr;
  t = &b[0];
  x = __copy_prim(t);
}
exported func main() int {
  do_things();
  return 0;
}
"#,
    "do_things",
  );
  assert!(
    !facts.group_paths.iter().any(|n| n.ends_with("[]")),
    "a static-sized array element must fold into its parent group, not get a `[]` child scope, got {:?}",
    facts.group_paths
  );
}
