// GhostCell arm of valen/accumulator_with_observer.rs.
//
// Verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A view is held across the call.
//
// Spelling not used: taking a fresh view with `borrow_mut` on every step. It
// compiles to this arm's loop today, and it holds no view across
// `do_nothing()`, so its verdict would be "never" (Bucket B). It is not the
// smallest change the borrow checker's rejection forces, so it is not the arm.
//
// Why it loses today: `view` is a local reference, and rustc marks only
// parameters. LLVM must assume `do_nothing()` changed `view.hp`, and reloads it
// after every call.
//
// Why rustc could close it: `view` is a `&mut Entity` held across
// `do_nothing()`. Writing `hp` through another pointer and then using `view`
// again is undefined behavior (Miri rejects it for a view from `borrow_mut`
// under Stacked Borrows and Tree Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a rustc that
// marked local references could tell LLVM that no reload after the call is
// needed.
//
// What would make this arm never catch up (Bucket C): a callee in the hot path
// that takes this cell's brand's token. `do_nothing()` takes none.
//
// How this arm was written: the borrow checker rejects one thing in the mirror
// of the Valen arm: a mutable view held across `trace`, which takes the token.
// The smallest change that fixes it is to take the view again right after each
// call to `trace`, so that is what `burn` does. `trace` is a mirror and takes
// the token shared. `heal` reads `b` and releases it before it writes `a`,
// because the token gives one view at a time. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Cell placement: one cell per entity. The entity has nothing below it but one
// field, so there is no other placement to compare. `Level` holds a plain `Vec`
// of cells, so `&Level` keeps `noalias` and the buffer pointer and length are
// loaded once.
//
// Brand layout: one brand. `heal` takes two entities that may be the same
// entity, so both must answer to one token. Every entity is in one collection,
// so no other layout exists.
//
// Why `Entity` is in a cell: `heal` writes one entity while it reads another
// that may be the same entity. Without cells the borrow checker rejects that
// pair of references.
//
// Size of the win today: one load of `hp` per step, from the mark on the call.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `burn`, unoptimized:
// - The loop contains a load of `hp`, a store of `hp`, the call to
//   `do_nothing`, and, on the branch taken when `pc == 49`, the call to `trace`.
//
// Expected IR of `burn`, optimized:
// - The parameter `level` carries `noalias`.
// - The entities' buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `hp`, stores `hp`, and calls `do_nothing`. The load
//   follows the previous iteration's call.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::{at, do_nothing};

pub struct Entity {
    pub hp: i32,
}

pub struct Level<'b> {
    pub entities: Vec<GhostCell<'b, Entity>>,
}

pub fn heal<'b>(t: &mut GhostToken<'b>, a: &GhostCell<'b, Entity>, b: &GhostCell<'b, Entity>) {
    let amount = b.borrow(t).hp;
    a.borrow_mut(t).hp += amount;
}

#[inline(never)]
pub fn trace<'b>(t: &GhostToken<'b>, level: &Level<'b>) -> i32 {
    at(&level.entities, 0).borrow(t).hp
}

#[inline(never)]
pub fn burn<'b>(t: &mut GhostToken<'b>, level: &Level<'b>, steps: i32) -> i32 {
    let e = at(&level.entities, 0);
    let mut view = e.borrow_mut(t);
    let mut seen: i32 = 0;
    let mut pc: i32 = 0;
    while pc < steps {
        view.hp -= 1;
        do_nothing();
        if pc == 49 {
            seen = trace(t, level);
            view = e.borrow_mut(t);
        }
        pc += 1;
    }
    seen
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let level = Level { entities: vec![GhostCell::new(Entity { hp: 111 }), GhostCell::new(Entity { hp: 9 })] };
        heal(&mut t, &level.entities[0], &level.entities[1]);
        heal(&mut t, &level.entities[1], &level.entities[1]);
        let seen = burn(&mut t, &level, 100);
        (seen + level.entities[0].borrow(&t).hp) as i64
    })
}
