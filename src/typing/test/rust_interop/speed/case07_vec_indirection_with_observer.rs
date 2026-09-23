use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-07-vec-indirection-with-observer",
  vale: r#"
import rust.mycrate.at;
import rust.mycrate.do_nothing;
import rust.alloc.vec.Vec;
import rust.alloc.alloc.Global;
struct Ship { fuel int; }
func observe<g'>(ships &Vec<Ship, Global> in g, i i64) int {
  return __copy_prim(at(ships, __copy_prim(i)).fuel);
}
func bump<g'>(ships &Vec<Ship, Global> in g, i i64) int mut(g) {
  seen = 0;
  n = 0;
  while n < 100 {
    s = at(ships, __copy_prim(i));
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
  return seen + __copy_prim(at(&ships, 1i64).fuel);
}
"#,
  expect: Expect::Returns(150),
};

#[test]
fn case07_vec_indirection_with_observer() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
