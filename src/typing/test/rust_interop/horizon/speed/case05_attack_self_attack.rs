use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case05_attack_self_attack() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
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
  attack((entities.get(0u)).unwrap(), (entities.get(1u)).unwrap());
  attack((entities.get(2u)).unwrap(), (entities.get(2u)).unwrap());
  e1 = (entities.get(1u)).unwrap();
  e2 = (entities.get(2u)).unwrap();
  return __copy_prim(e1.hp) + __copy_prim(e2.hp) + __copy_prim(e2.energy);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(22),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
