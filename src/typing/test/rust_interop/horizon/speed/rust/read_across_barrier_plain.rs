// Plain Rust arm of valen/read_across_barrier.rs.
//
// Verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// Why: `e: &Entity` is a reference parameter to a type with no interior
// mutability, so rustc marks it `noalias` and `readonly`. LLVM then knows
// `do_nothing()` cannot change `e.hp`, and reads it once. That is the same fact
// the Valen arm gives LLVM.
//
// How this arm was written: a mirror of the Valen arm. The borrow checker
// rejects nothing here, so nothing deviates. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `total`, unoptimized:
// - The loop contains a load of `hp` and the call to `do_nothing`.
//
// Expected IR of `total`, optimized (the same shape as the Valen arm):
// - The parameter `e` carries `noalias`.
// - The loop contains the call to `do_nothing`.
// - The loop contains no load of `hp` and no store. `hp` is loaded at most
//   once, outside the loop.

use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
}

#[inline(never)]
pub fn total(e: &Entity, n: i32) -> i32 {
    let mut sum: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        sum += e.hp;
        do_nothing();
        i += 1;
    }
    sum
}

pub fn main_like() -> i64 {
    let e = Entity { hp: 2 };
    total(&e, 50) as i64
}
