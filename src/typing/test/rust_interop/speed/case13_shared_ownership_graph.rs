use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-13-shared-ownership-graph",
  vale: r#"
import rust.mycrate.at;
import rust.mycrate.do_nothing;
import rust.alloc.vec.Vec;
import rust.alloc.alloc.Global;
struct Node { value int; peer i64; }
func walk<g'>(nodes &Vec<Node, Global> in g, start i64, hops int) mut(g) {
  cur = __copy_prim(start);
  i = 0;
  while i < __copy_prim(hops) {
    n = at(nodes, __copy_prim(cur));
    set n.value = __copy_prim(n.value) + 1;
    do_nothing();
    set cur = __copy_prim(n.peer);
    set i = i + 1;
  }
}
exported func main() int {
  nodes = Vec.new<Node>();
  nodes.push(Node(0, 1i64));
  nodes.push(Node(0, 0i64));
  walk(&nodes, 0i64, 100);
  return __copy_prim(at(&nodes, 0i64).value) + __copy_prim(at(&nodes, 1i64).value);
}
"#,
  expect: Expect::Returns(100),
};

#[test]
fn case13_shared_ownership_graph() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
