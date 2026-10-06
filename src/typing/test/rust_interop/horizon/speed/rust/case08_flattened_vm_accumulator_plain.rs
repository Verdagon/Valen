// Plain Rust arm of old/case08_flattened_vm_accumulator.rs.
//
// Arm verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the call.
//
// Spellings not used:
// - Indexing `regs[0]` on every step. It compiles to this arm's loop today, and
//   it holds nothing across `do_nothing()`, so its verdict would be "never"
//   (Bucket B). It is not the smallest change the borrow checker's rejection
//   forces, so it is not the arm.
// - Making `v` a `Cell<i32>` and holding a shared reference across `trace` with
//   no re-binding. It compiles to this arm's loop today. A tie, so the arm
//   stays without `Cell`. No pair of registers is aliased in this program.
//
// Why it loses today: `acc` is a local reference, and rustc marks only
// parameters. The register sits in the `Vec`'s buffer, behind a pointer loaded
// from `regs`, so the `noalias` on `regs` does not cover it. LLVM must assume
// `do_nothing()` changed the register, and reloads it after every call. The
// Valen arm marks that call as unable to touch the registers and keeps the
// value in a register across it.
//
// Why rustc could close it: `acc` is a `&mut Reg` held across `do_nothing()`.
// Writing `v` through another pointer and then using `acc` again is undefined
// behavior (Miri rejects it under Stacked Borrows and Tree Borrows; results are
// recorded in notes/docs/architecture/rust-interop-design.md, Background). So a
// rustc that marked local references could tell LLVM that no reload after the
// call is needed.
//
// How this arm was written: `step_all` binds `acc` before the loop, as the
// Valen arm does. The borrow checker rejects holding it across `trace(regs)`,
// which borrows the whole `Vec`, so `step_all` binds it again right after each
// call to `trace`; that is the smallest change the rejection forces. It indexes
// with `&mut regs[0]`
// where Valen calls `at`, because `at` returns a shared reference and Rust
// cannot write through one. `trace` is a mirror. Fields and counters use the
// Valen arm's widths: Valen `int` is `i32`.
//
// Other spellings:
// - If `step_all` and `trace` took the registers as a slice, `&mut [Reg]` and
//   `&[Reg]`, the register would be reached from a `noalias` parameter with no
//   load in between, and rustc keeps it in a register across `do_nothing()`
//   today. That form is excluded: the Valen arm takes the `Vec`, and nothing
//   prompts a programmer who has the `Vec` to slice it first.
// - Keeping the value in a local and writing it back after the loop is excluded
//   as an unprompted hand-optimization, and it is also wrong here: `trace`
//   would read a stale value.
//
// Size of the win today: one load of the register per step, from the mark on
// the call.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `step_all`, unoptimized:
// - The loop contains a load of `v`, a store of `v`, the call to `do_nothing`,
//   and, on the branch taken when `pc == 49`, the call to `trace` and a bounds
//   check.
//
// Expected IR of `step_all`, optimized:
// - The parameter `regs` carries `noalias`.
// - The buffer pointer and length are loaded before the loop, and the loop
//   contains no bounds check.
// - Each iteration loads `v`, stores `v`, and calls `do_nothing`. The load
//   follows the previous iteration's call.

use mycrate::{at, do_nothing};

pub struct Reg {
    pub v: i32,
}

#[inline(never)]
pub fn trace(regs: &Vec<Reg>) -> i32 {
    at(regs, 0).v
}

#[inline(never)]
pub fn step_all(regs: &mut Vec<Reg>, steps: i32) -> i32 {
    let mut acc = &mut regs[0];
    let mut seen: i32 = 0;
    let mut pc: i32 = 0;
    while pc < steps {
        acc.v = acc.v + 1;
        do_nothing();
        if pc == 49 {
            seen = trace(regs);
            acc = &mut regs[0];
        }
        pc += 1;
    }
    seen
}

pub fn main_like() -> i64 {
    let mut regs = vec![Reg { v: 0 }, Reg { v: 0 }, Reg { v: 0 }, Reg { v: 0 }];
    let first = step_all(&mut regs, 60);
    let second = step_all(&mut regs, 50);
    (first + second) as i64
}
