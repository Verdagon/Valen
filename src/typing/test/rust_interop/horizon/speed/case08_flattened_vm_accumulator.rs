use crate::typing::test::rust_interop::drive_helpers::drive_and_run_without_borrow_check;

#[test]
fn case08_flattened_vm_accumulator() {
  let run = drive_and_run_without_borrow_check("horizon/main", r#"
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
struct Reg { v int; }
func trace<g'>(regs &Vec<Reg, Global> in g) int {
  return __copy_prim((regs.get(0u)).unwrap().v);
}
func step_all<g'>(regs &Vec<Reg, Global> in g, steps int) int mut(g) {
  seen = 0;
  pc = 0;
  while pc < __copy_prim(steps) {
    acc = (regs.get(0u)).unwrap();
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
  return seen + __copy_prim((regs.get(0u)).unwrap().v);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(150),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
