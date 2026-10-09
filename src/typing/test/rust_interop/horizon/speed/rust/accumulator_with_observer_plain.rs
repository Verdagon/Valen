// Plain Rust arm of valen/accumulator_with_observer.rs.
//
// Verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the call.
//
// Spellings not used:
// - Indexing `level.entities[0]` on every step. It compiles to this arm's loop
//   today, and it holds nothing across `do_nothing()`, so its verdict would be
//   "never" (Bucket B). It is not the smallest change the borrow checker's
//   rejection forces, so it is not the arm.
// - Making `hp` a `Cell<i32>` and holding a shared reference across `trace`
//   with no re-binding. It compiles to this arm's loop today: load `hp`, store
//   `hp`, call. A tie, so the arm stays without `Cell`.
//
// Why it loses today: `e` is a local reference, and rustc marks only
// parameters. The entity sits in the `Vec`'s buffer, behind a pointer loaded
// from `level`, so the `noalias` on `level` does not cover it. LLVM must assume
// `do_nothing()` changed `hp`, and reloads it after every call.
//
// Why rustc could close it: `e` is a `&mut Entity` held across `do_nothing()`.
// Writing `hp` through another pointer and then using `e` again is undefined
// behavior (Miri rejects it under Stacked Borrows and Tree Borrows; results are
// recorded in notes/docs/architecture/rust-interop-design.md, Background). So a
// rustc that marked local references could tell LLVM that no reload after the
// call is needed.
//
// How this arm was written: the borrow checker rejects one thing in the mirror
// of the Valen arm: holding `e` across `trace(level)`, which borrows the whole
// level. The smallest change that fixes it is to bind `e` again right after
// each call to `trace`, so that is what `burn` does. `trace` is a mirror.
// `heal` takes the level and two indices, because the borrow checker rejects
// two references to entities that may be the same entity. Fields and counters
// use the Valen arm's widths: Valen `int` is `i32`.
//
// Other spellings:
// - If `burn` and `trace` took the entities as a slice, `&mut [Entity]` and
//   `&[Entity]`, the entity would be reached from a `noalias` parameter with no
//   load in between. rustc then keeps `hp` in a register across `do_nothing()`
//   today, and does not reload it after `trace` either, because `trace`'s
//   parameter is read-only. That form is excluded: the Valen arm takes the
//   level, the entities live in the level, and nothing prompts a programmer who
//   has the level to pass its buffer instead.
// - Keeping `hp` in a local and writing it back after the loop is excluded as
//   an unprompted hand-optimization, and it is also wrong here: `trace` would
//   read a stale value.
//
// Size of the win today: one load of `hp` per step, from the mark on the call.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, the call to
//   `do_nothing`, and, on the branch taken when `pc == 49`, the call to `trace`
//   and a bounds check.
//
// Expected IR of `burn`, optimized:
// - The parameter `level` carries `noalias`.
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `hp`, stores `hp`, and calls `do_nothing`. The load
//   follows the previous iteration's call.

use mycrate::{at, do_nothing};

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub entities: Vec<Entity>,
}

pub fn heal(level: &mut Level, a: usize, b: usize) {
    level.entities[a].hp += level.entities[b].hp;
}

#[inline(never)]
pub fn trace(level: &Level) -> i32 {
    at(&level.entities, 0).hp
}

#[inline(never)]
pub fn burn(level: &mut Level, steps: i32) -> i32 {
    let mut e = &mut level.entities[0];
    let mut seen: i32 = 0;
    let mut pc: i32 = 0;
    while pc < steps {
        e.hp -= 1;
        do_nothing();
        if pc == 49 {
            seen = trace(level);
            e = &mut level.entities[0];
        }
        pc += 1;
    }
    seen
}

pub fn main_like() -> i64 {
    let mut level = Level { entities: vec![Entity { hp: 111 }, Entity { hp: 9 }] };
    heal(&mut level, 0, 1);
    heal(&mut level, 1, 1);
    let seen = burn(&mut level, 100);
    (seen + level.entities[0].hp) as i64
}
