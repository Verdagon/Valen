// GhostCell arm of valen/read_across_barrier.rs.
//
// Verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the call.
//
// Why it loses today: `view` is a local reference, and rustc marks only
// parameters. The parameter it came from, `e`, wraps an `UnsafeCell` and gets
// no `noalias` either. LLVM must assume `do_nothing()` changed `view.hp`, and
// reloads it after every call.
//
// Why rustc could close it: `view` is a `&Entity` held across the call.
// Writing `hp` through another pointer and then using `view` again is undefined
// behavior (Miri rejects it under Stacked Borrows and Tree Borrows; results are
// recorded in notes/docs/architecture/rust-interop-design.md, Background). So a
// rustc that marked local references could tell LLVM that no reload after the
// call is needed.
//
// What would make this arm never catch up (Bucket C): a callee that takes this
// cell's brand's token mutably. `view` cannot be held across such a call,
// because the call needs `&mut token` while `view` holds `&token`. The arm must
// then reach `hp` through the cell each time, and a callee holding the token
// may write any cell of the brand.
//
// How this arm was written: a mirror of the Valen arm. Valen binds `e` once for
// the whole loop, and so does this arm: `borrow` is called once, before the
// loop. `do_nothing()` takes no token, so the borrow checker accepts that.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Brand layout: one brand. The program has one cell, so no other layout exists.
//
// Why `Entity` is in a cell: nothing in this file needs it to be. This fence
// isolates what a cell costs. The verdict applies to a program that has cause
// to keep `Entity` in a cell, such as one with a function that takes two
// entities that may be the same.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `total`, unoptimized:
// - The loop contains a load of `hp` and the call to `do_nothing`.
//
// Expected IR of `total`, optimized:
// - The parameter `e` carries no `noalias`.
// - The loop contains the call to `do_nothing` and one load of `hp`. The load
//   runs on every iteration.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
}

#[inline(never)]
pub fn total<'b>(t: &GhostToken<'b>, e: &GhostCell<'b, Entity>, n: i32) -> i32 {
    let view = e.borrow(t);
    let mut sum: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        sum += view.hp;
        do_nothing();
        i += 1;
    }
    sum
}

pub fn main_like() -> i64 {
    GhostToken::new(|t| {
        let e = GhostCell::new(Entity { hp: 2 });
        total(&t, &e, 50) as i64
    })
}
