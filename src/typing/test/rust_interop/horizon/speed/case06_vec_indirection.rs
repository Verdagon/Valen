use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case06_vec_indirection() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.at;
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
struct Ship { fuel int; }
func bump<g'>(ships &Vec<Ship, Global> in g, i i64) mut(g) {
  n = 0;
  while n < 100 {
    s = at(ships, __copy_prim(i));
    set s.fuel = __copy_prim(s.fuel) + 1;
    do_nothing();
    set n = n + 1;
  }
}
exported func main() int {
  ships = Vec.new<Ship>();
  ships.push(Ship(0));
  ships.push(Ship(0));
  ships.push(Ship(0));
  bump(&ships, 1i64);
  return __copy_prim(at(&ships, 1i64).fuel);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
