use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case13_shared_ownership_graph() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
struct Node { value int; peer i64; }
func walk<g'>(nodes &Vec<Node, Global> in g, start i64, hops int) mut(g) {
  cur = __copy_prim(start);
  i = 0;
  while i < __copy_prim(hops) {
    n = (nodes.get(usize(cur))).unwrap();
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
  return __copy_prim((nodes.get(0u)).unwrap().value) + __copy_prim((nodes.get(1u)).unwrap().value);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(100),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
