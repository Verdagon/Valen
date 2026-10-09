// Plain Rust arm of valen/caller_ref_across_mutating_callee.rs.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// In the source: `siege` binds `e` once, before the loop, and lends it to
// `strike` on each iteration. The loan ends when `strike` returns, and `e` is
// usable again. The Valen arm cannot do that: its checker ends the borrow at
// the call ("Used a borrow after invalidated"), and it reaches the entity again
// twice per iteration.
//
// In the compiled loop: a tie. Written the Valen arm's way, with
// `level.entities[i]` indexed twice per iteration, this function compiles to
// the same loop: the buffer pointer and length are loaded once, the bound is
// checked once, and each iteration calls `strike` and loads `hp`. The loops tie
// only because the container is a `Vec` reached from a `noalias` parameter that
// is not captured, so LLVM hoists the pointer, the length and the bounds check
// in either spelling. If `level` is not `noalias` and uncaptured in the Valen
// arm, Valen pays three loads and a check per iteration and this arm is ahead.
//
// How this arm was written: the plainest form of the program. It is not a
// mirror: the Valen arm's two `at` calls per iteration are forced by Valen's
// checker, and nothing in Rust forces them. It indexes with `&mut` where Valen
// calls `at`, because `at` returns a shared reference and Rust cannot write
// through one. Fields and counters use the Valen arm's widths: Valen `int` is
// `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `siege`, unoptimized:
// - One bounds check before the loop. Each iteration calls `strike` and loads
//   `hp`.
//
// Expected IR of `siege`, optimized:
// - The parameter `level` carries `noalias`.
// - The buffer pointer and length are loaded before the loop, and the bound on
//   `i` is checked once.
// - Each iteration calls `strike` and then loads `hp`.

use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn strike(e: &mut Entity) {
    e.hp -= 1;
    do_nothing();
}

#[inline(never)]
pub fn siege(level: &mut Level, i: i64, n: i32) -> i32 {
    let e = &mut level.entities[i as usize];
    let mut total: i32 = 0;
    let mut k: i32 = 0;
    while k < n {
        strike(e);
        total += e.hp;
        k += 1;
    }
    total
}

pub fn main_like() -> i64 {
    let mut level = Level { entities: vec![Entity { hp: 40 }, Entity { hp: 20 }] };
    let first = siege(&mut level, 0, 3);
    let second = siege(&mut level, 1, 2);
    (first + second) as i64
}
