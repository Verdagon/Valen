use crate::typing::rust_interop::corpus::{Case, Expect};
use crate::typing::test::rust_interop::harness::run_case_rustc_driven_and_run;

const CASE: Case = Case {
  fixture: "fixtures",
  name: "speed-08-flattened-vm-accumulator",
  vale: r#"
import rust.mycrate.at;
import rust.mycrate.do_nothing;
import rust.alloc.vec.Vec;
import rust.alloc.alloc.Global;
struct Reg { v int; }
func trace<g'>(regs &Vec<Reg, Global> in g) int {
  return __copy_prim(at(regs, 0i64).v);
}
func step_all<g'>(regs &Vec<Reg, Global> in g, steps int) int mut(g) {
  seen = 0;
  pc = 0;
  while pc < __copy_prim(steps) {
    acc = at(regs, 0i64);
    set acc.v = __copy_prim(acc.v) + 1;
    do_nothing();
    if pc == 49 {
      set seen = trace(regs);
    }
    set pc = pc + 1;
  }
  return seen;
}
exported func main() int {
  regs = Vec.new<Reg>();
  regs.push(Reg(0));
  regs.push(Reg(0));
  regs.push(Reg(0));
  regs.push(Reg(0));
  seen = step_all(&regs, 100);
  return seen + __copy_prim(at(&regs, 0i64).v);
}
"#,
  expect: Expect::Returns(150),
};

#[test]
fn case08_flattened_vm_accumulator() {
  let run = run_case_rustc_driven_and_run(&CASE);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
