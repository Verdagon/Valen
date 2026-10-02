use super::util::param_noalias_of;

#[test]
fn same_group_params_are_not_noalias() {
  let noalias = param_noalias_of(
    &[],
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
  );
  assert_eq!(noalias, vec![false, false]);
}

#[test]
fn sole_group_param_is_noalias() {
  let noalias = param_noalias_of(
    &[],
    r#"
struct Ship { fuel int; }
func solo<g'>(a &Ship in g) { }
exported func main() int {
  s1 = Ship(1);
  solo(&s1);
  return 0;
}
"#,
    "solo",
  );
  assert_eq!(noalias, vec![true]);
}

#[test]
fn distinct_group_params_are_both_noalias() {
  let noalias = param_noalias_of(
    &[],
    r#"
struct Ship { fuel int; }
func two<gl', gr'>(a &Ship in gl, b &Ship in gr) { }
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  two(&s1, &s2);
  return 0;
}
"#,
    "two",
  );
  assert_eq!(noalias, vec![true, true]);
}

#[test]
fn mut_param_is_noalias() {
  let noalias = param_noalias_of(
    &[],
    r#"
struct Entity { hp int; }
func heal(e &Entity mut) { set e.hp = 5; }
exported func main() int {
  en = Entity(1);
  heal(&en);
  return 0;
}
"#,
    "heal",
  );
  assert_eq!(noalias, vec![true]);
}
