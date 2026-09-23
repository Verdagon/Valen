use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-05-attack-self-attack",
  vale: r#"
import rust.mycrate.at;
import rust.alloc.vec.Vec;
import rust.alloc.alloc.Global;
struct Entity { energy int; hp int; }
func attack<r'>(a &Entity in r, d &Entity in r) mut(r) {
  cost = __copy_prim(d.hp) - 8;
  set a.energy = __copy_prim(a.energy) - cost;
  set d.hp = __copy_prim(d.hp) - 3;
}
exported func main() int {
  entities = Vec.new<Entity>();
  entities.push(Entity(10, 10));
  entities.push(Entity(10, 10));
  entities.push(Entity(10, 10));
  attack(at(&entities, 0i64), at(&entities, 1i64));
  attack(at(&entities, 2i64), at(&entities, 2i64));
  e1 = at(&entities, 1i64);
  e2 = at(&entities, 2i64);
  return __copy_prim(e1.hp) + __copy_prim(e2.hp) + __copy_prim(e2.energy);
}
"#,
  expect: Expect::Returns(22),
};

#[test]
fn case05_attack_self_attack() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(22),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
