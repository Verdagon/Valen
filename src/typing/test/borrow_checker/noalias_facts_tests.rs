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
