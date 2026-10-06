// Ensures that Valen keeps a register of a flattened VM in a machine register
// across an opaque call, and writes it back and reloads it only around a call
// that is handed the registers.
//
// The program:
//   step_all(regs &Vec<Reg, Global> in g, steps int) int mut(g)
//   acc = regs.get(0u).unwrap()
//   loop steps times: set acc.v = acc.v + 1; do_nothing();
//                     on one step: seen = trace(regs);
//   trace(regs &Vec<Reg, Global> in g) int reads register 0. It is the
//   observer.
//   main calls step_all twice with different counts, so the optimizer cannot
//   fold `steps`.
//
// This is the program of accumulator_with_observer with the registers passed
// as a bare `Vec`, and with no function that takes two registers that may be
// the same.
//
// Mechanism: M3 by argument reach, applied per call. `do_nothing()` receives no
// argument, so it reaches no group, and Valen marks that call `!noalias`
// against the scope of the store to `acc.v`. `trace(regs)` is handed the
// registers, so it gets no such mark. That fact comes from the borrow checker's
// aliasing info, so this test runs with the checker on.
//
// The observer is what forbids keeping the value in a local by hand and writing
// it back at the end: `trace` must see the current value in memory.
//
// What Valen does not claim: that `trace` leaves `acc.v` unchanged. `trace`
// declares no `mut`, but a call's mark has no way to say "reads this group and
// does not write it" (proposal S18 in
// src/typing/docs/architecture/borrowing-design.md). So `acc.v` is reloaded
// after `trace`.
//
// Both Rust arms hold a reference to the register across `do_nothing()`, as
// this arm does, and both reload it today. A rustc that marked local references
// could close that, with no change to the language.
//
// Arms:
// - Plain Rust: rust/case08_flattened_vm_accumulator_plain.rs
// - GhostCell:  rust/case08_flattened_vm_accumulator.rs
//
// Assumptions behind the expected IR:
// - `do_nothing` stays an opaque call that does not unwind.
// - `#inline(never)` keeps `step_all` and `trace` functions, so the call to
//   `trace` stays in the loop and is handed the registers.
// - `get`/`unwrap` is inlined into `step_all`.
//
// Expected IR of `step_all`, unoptimized:
// - The loop contains a load of `v`, a store of `v`, the call to `do_nothing`,
//   and, on the branch taken when `pc == 49`, the call to `trace`.
//
// Expected IR of `step_all`, optimized:
// - The call to `do_nothing` carries `!noalias` naming the scope of the store
//   to `v`. The call to `trace` does not.
// - The buffer pointer and length are loaded before the loop, and the loop
//   contains no bounds check.
// - Each iteration stores `v` once, before the call to `do_nothing`.
// - No load of `v` follows the call to `do_nothing`.
// - The only load of `v` in the loop follows the call to `trace`, in the block
//   that runs when `pc == 49`.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn case08_flattened_vm_accumulator() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.do_nothing;
import std.vec.Vec;
import std.alloc.Global;
import std.option.Option;
struct Reg { v int; }
#inline(never)
func trace<g'>(regs &Vec<Reg, Global> in g) int {
  return __copy_prim((regs.get(0u)).unwrap().v);
}
#inline(never)
func step_all<g'>(regs &Vec<Reg, Global> in g, steps int) int mut(g) {
  acc = (regs.get(0u)).unwrap();
  seen = 0;
  pc = 0;
  while pc < __copy_prim(steps) {
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
  first = step_all(&regs, 60);
  second = step_all(&regs, 50);
  return first + second;
}
"#);
  assert_eq!(
    run.process_exit,
    Some(160),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
