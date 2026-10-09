// Plain Rust arm of valen/rmw_across_barrier.rs.
//
// Verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// Why: `e: &mut Entity` is a reference parameter, so rustc marks it `noalias`.
// LLVM then knows `do_nothing()` cannot change `e.hp`, and keeps the value in a
// register across the call. That is the same fact the Valen arm gives LLVM.
//
// How this arm was written: a mirror of the Valen arm. The borrow checker
// rejects nothing here, so nothing deviates. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `bump`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, and the call to
//   `do_nothing`.
//
// Expected IR of `bump`, optimized (the same shape as the Valen arm):
// - The parameter `e` carries `noalias`.
// - The loop contains the call to `do_nothing` and one store of `hp`.
// - The loop contains no load of `hp`. The one load of `hp` sits before the
//   loop.

use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
}

#[inline(never)]
pub fn bump(e: &mut Entity, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        e.hp -= 1;
        do_nothing();
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut e = Entity { hp: 150 };
    bump(&mut e, 50);
    e.hp as i64
}
