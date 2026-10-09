// GhostCell arm of valen/hp_held_across_sibling_tick.rs.
//
// Arm verdict: rustc could close.
// Tier: none today (the Valen arm reloads after the call too). If S18 is
// resolved: appendix. The Valen arm is red by design, so this test is not
// evidence for any claim today.
// Bucket: A. A view is held across the call.
//
// Why it loses today: `e` is a mutable view of the entity, held for the whole
// loop. It is a local reference, and rustc marks only parameters. LLVM must
// assume `advance_clock` changed `e.hp`, and reloads it after every call. That
// is also what the Valen arm does today.
//
// Why rustc could close it: `e` is a `&mut Entity` held across the call.
// Writing `hp` through another pointer and then using `e` again is undefined
// behavior (Miri rejects it for a view from `borrow_mut` under Stacked Borrows
// and Tree Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a rustc that
// marked local references could keep `hp` in a register across the call, with
// no store before it either. That is more than the Valen arm's intended shape,
// which keeps the store because `advance_clock` is handed the level and may
// read the entity. For Valen to match that, a callee would have to declare that
// it does not read a group it can reach: proposal S21 in
// src/typing/docs/architecture/borrowing-design.md.
//
// How this arm was written: a mirror of the Valen arm. `burn` binds the entity
// once, before the loop, and holds it across `advance_clock`. Fields and
// counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Brand layout: two brands, one for the entities and one for the clock. This is
// the best layout for GhostCell, and nothing in the program pairs an entity
// with the clock. `advance_clock` takes only the clock's token, so the entity
// view can stay alive across it. Three layouts were compiled:
// - Two brands (this arm): each iteration loads `hp`, stores `hp`, and calls.
// - One brand, the clock a cell of the entities' brand: `advance_clock` takes
//   the entities' token, so no view can be held and the arm takes one per step.
//   The same loop today. Its verdict would be "never" (Bucket C): a callee that
//   holds the token may write any cell of the brand.
// - The clock not in a cell: `advance_clock` takes `&mut Level`, so not even a
//   cell pointer into the level can be held across it. Each iteration loads the
//   entities' length, checks the bound, loads the buffer pointer, loads `hp`,
//   stores `hp`, and calls: three loads and a bounds check per step.
//
// Cell layout: one cell per entity, and one for the clock. Neither has anything
// below it to put cells on.
//
// Why `Entity` and the clock are in cells: `heal` writes one entity while it
// reads another that may be the same entity. The clock is written by
// `advance_clock` while `burn` holds a shared reference to the level; without a
// cell that write needs `&mut Level`, which is the third layout above.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, and the call to
//   `advance_clock`.
//
// Expected IR of `burn`, optimized:
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `hp`, stores `hp`, and calls `advance_clock`. The load
//   follows the previous iteration's call.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::{at, do_nothing};

pub struct Entity {
    pub hp: i32,
}

pub struct Level<'e, 'c> {
    pub entities: Vec<GhostCell<'e, Entity>>,
    pub clock: GhostCell<'c, i32>,
}

pub fn heal<'e>(te: &mut GhostToken<'e>, a: &GhostCell<'e, Entity>, b: &GhostCell<'e, Entity>) {
    let amount = b.borrow(te).hp;
    a.borrow_mut(te).hp += amount;
}

#[inline(never)]
pub fn advance_clock<'e, 'c>(tc: &mut GhostToken<'c>, level: &Level<'e, 'c>) {
    *level.clock.borrow_mut(tc) += 1;
    do_nothing();
}

#[inline(never)]
pub fn burn<'e, 'c>(
    te: &mut GhostToken<'e>,
    tc: &mut GhostToken<'c>,
    level: &Level<'e, 'c>,
    i: i64,
    n: i32,
) {
    let e = at(&level.entities, i).borrow_mut(te);
    let mut k: i32 = 0;
    while k < n {
        e.hp -= 1;
        advance_clock(tc, level);
        k += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut te| {
        GhostToken::new(|mut tc| {
            let level = Level {
                entities: vec![GhostCell::new(Entity { hp: 111 }), GhostCell::new(Entity { hp: 9 })],
                clock: GhostCell::new(0),
            };
            heal(&mut te, &level.entities[0], &level.entities[1]);
            heal(&mut te, &level.entities[1], &level.entities[1]);
            burn(&mut te, &mut tc, &level, 0, 50);
            (level.entities[0].borrow(&te).hp + *level.clock.borrow(&tc)) as i64
        })
    })
}
