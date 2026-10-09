// Plain Rust arm of valen/hp_held_across_sibling_tick.rs.
//
// Arm verdict: never.
// Tier: none today (this arm's loop is the Valen arm's loop today). If S18 is
// resolved: appendix. A test's tier follows its best-placed Rust arm, and the
// GhostCell arm here holds a view across the call (Bucket A). The Valen arm is
// red by design, so this test is not evidence for any claim today.
// Bucket: C. The callee is handed `&Level`, and safe code may write any `Cell`
// reachable from it.
//
// Spelling not used: without `Cell`, `advance_clock` takes `&mut Level`, the
// borrow checker rejects a reference to the entity held across it, and `burn`
// indexes on every step. Compiled, that loop loads the entities' length, checks
// the bound, loads the buffer pointer, loads `hp`, stores `hp`, and calls:
// three loads and a bounds check per step, where this arm has one load. Both
// spellings are admissible; the arm uses the one with the better loop.
//
// Why it loses to the intended Valen shape: this arm holds `e` across
// `advance_clock`, as the Valen arm does. `hp` and `clock` are `Cell`s, and
// `advance_clock` is handed the level, so it may write `e.hp` through the level
// as far as LLVM can tell. Each step loads `hp`, stores `hp`, and calls. That is
// also what the Valen arm does today.
//
// Why no compiler can close it: that `advance_clock` leaves the entities alone
// is in no Rust type here. It takes `&Level`, and safe code holding `&Level` may
// set any entity's `hp`. No proposal adds the fact: view types, an unaccepted
// proposal by Niko Matsakis, describe a callee taking `&mut self`, not one that
// takes a shared reference and writes through a `Cell`. (The spelling not used
// has a `&mut self` callee, and for it view types would apply.)
//
// Which `Cell` case this is: `Level` holds `clock`, a `Cell`, directly, so
// `&Level` has interior mutability and rustc does not mark it `noalias`. That
// costs this loop nothing: the entity is reached once, before the loop.
//
// How this arm was written: a mirror of the Valen arm. `burn` binds `e` once
// through `at` and holds it across `advance_clock`. Writes go through
// `Cell::set`, because Rust cannot write through a shared reference otherwise.
// `heal` takes two references to entities that may be the same entity, which
// `Cell` allows. Fields and counters use the Valen arm's widths: Valen `int` is
// `i32`.
//
// Another spelling: `advance_clock` could be a free function on the clock
// alone. It is excluded because it changes the callee: the Valen arm's
// `advance_clock` takes the level.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, and the call to
//   `advance_clock`.
//
// Expected IR of `burn`, optimized:
// - The parameter `level` carries no `noalias`.
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `hp`, stores `hp`, and calls `advance_clock`. The load
//   follows the previous iteration's call.

use mycrate::{at, do_nothing};
use std::cell::Cell;

pub struct Entity {
    pub hp: Cell<i32>,
}

pub struct Level {
    pub entities: Vec<Entity>,
    pub clock: Cell<i32>,
}

pub fn heal(a: &Entity, b: &Entity) {
    a.hp.set(a.hp.get() + b.hp.get());
}

#[inline(never)]
pub fn advance_clock(level: &Level) {
    level.clock.set(level.clock.get() + 1);
    do_nothing();
}

#[inline(never)]
pub fn burn(level: &Level, i: i64, n: i32) {
    let e = at(&level.entities, i);
    let mut k: i32 = 0;
    while k < n {
        e.hp.set(e.hp.get() - 1);
        advance_clock(level);
        k += 1;
    }
}

pub fn main_like() -> i64 {
    let level = Level {
        entities: vec![Entity { hp: Cell::new(111) }, Entity { hp: Cell::new(9) }],
        clock: Cell::new(0),
    };
    heal(&level.entities[0], &level.entities[1]);
    heal(&level.entities[1], &level.entities[1]);
    burn(&level, 0, 50);
    (level.entities[0].hp.get() + level.clock.get()) as i64
}
