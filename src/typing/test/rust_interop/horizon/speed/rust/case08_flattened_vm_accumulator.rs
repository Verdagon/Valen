// GhostCell arm of old/case08_flattened_vm_accumulator.rs.
//
// Arm verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A view is held across the call.
//
// Spelling not used: taking a fresh view with `borrow_mut` on every step. It
// compiles to this arm's loop today, and it holds no view across
// `do_nothing()`, so its verdict would be "never" (Bucket B). It is not the
// smallest change the borrow checker's rejection forces, so it is not the arm.
//
// Why it loses today: `view` is a local reference, and rustc marks only
// parameters. LLVM must assume `do_nothing()` changed `view.v`, and reloads it
// after every call. The Valen arm marks that call as unable to touch the
// registers and keeps the value in a register across it.
//
// Why rustc could close it: `view` is a `&mut Reg` held across `do_nothing()`.
// Writing `v` through another pointer and then using `view` again is undefined
// behavior (Miri rejects it for a view from `borrow_mut` under Stacked Borrows
// and Tree Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a rustc that
// marked local references could tell LLVM that no reload after the call is
// needed.
//
// How this arm was written: `step_all` binds the register once before the
// loop, as the Valen arm does, and takes its view there. The
// borrow checker rejects a mutable view held across `trace`, which takes the
// token, so `step_all` takes the view again right after each call to `trace`;
// that is the smallest change the rejection forces. `trace` takes the token
// shared. Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Cell placement: one cell per register. A register has nothing below it but
// one field, so there is no other placement to compare. `&Vec` of cells keeps
// `noalias`, so the buffer pointer and length are loaded once.
//
// Brand layout: one brand. The program has one collection of cells.
//
// Why `Reg` is in a cell: nothing in this file needs it to be. No function here
// takes two registers that may be the same. The verdict applies to a program
// that has cause to keep registers in cells; in this program as written a
// GhostCell programmer would use no cells, and this arm would be the plain arm.
//
// Size of the win today: one load of the register per step, from the mark on
// the call.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `step_all`, unoptimized:
// - The loop contains a load of `v`, a store of `v`, the call to `do_nothing`,
//   and, on the branch taken when `pc == 49`, the call to `trace`.
//
// Expected IR of `step_all`, optimized:
// - The parameter `regs` carries `noalias`.
// - The buffer pointer and length are loaded before the loop, and the loop
//   contains no bounds check.
// - Each iteration loads `v`, stores `v`, and calls `do_nothing`. The load
//   follows the previous iteration's call.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::{at, do_nothing};

pub struct Reg {
    pub v: i32,
}

#[inline(never)]
pub fn trace<'b>(t: &GhostToken<'b>, regs: &Vec<GhostCell<'b, Reg>>) -> i32 {
    at(regs, 0).borrow(t).v
}

#[inline(never)]
pub fn step_all<'b>(t: &mut GhostToken<'b>, regs: &Vec<GhostCell<'b, Reg>>, steps: i32) -> i32 {
    let acc = at(regs, 0);
    let mut view = acc.borrow_mut(t);
    let mut seen: i32 = 0;
    let mut pc: i32 = 0;
    while pc < steps {
        view.v = view.v + 1;
        do_nothing();
        if pc == 49 {
            seen = trace(t, regs);
            view = acc.borrow_mut(t);
        }
        pc += 1;
    }
    seen
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let regs = vec![
            GhostCell::new(Reg { v: 0 }),
            GhostCell::new(Reg { v: 0 }),
            GhostCell::new(Reg { v: 0 }),
            GhostCell::new(Reg { v: 0 }),
        ];
        let first = step_all(&mut t, &regs, 60);
        let second = step_all(&mut t, &regs, 50);
        (first + second) as i64
    })
}
