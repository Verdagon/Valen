use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case07_vec_indirection_with_observer() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
struct Ship { fuel int; }
func observe<g'>(ships &Vec<Ship, Global> in g, i i64) int {
  return __copy_prim((ships.get(usize(i))).unwrap().fuel);
}
func bump<g'>(ships &Vec<Ship, Global> in g, i i64) int mut(g) {
  seen = 0;
  n = 0;
  while n < 100 {
    s = (ships.get(usize(i))).unwrap();
    set s.fuel = __copy_prim(s.fuel) + 1;
    do_nothing();
    if n == 49 {
      set seen = observe(ships, __copy_prim(i));
    }
    set n = n + 1;
  }
  return seen;
}
exported func main() int {
  ships = Vec.new<Ship>();
  ships.push(Ship(0));
  ships.push(Ship(0));
  ships.push(Ship(0));
  seen = bump(&ships, 1i64);
  return seen + __copy_prim((ships.get(1u)).unwrap().fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
