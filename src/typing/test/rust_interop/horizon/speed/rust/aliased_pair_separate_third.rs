// GhostCell arm of valen/aliased_pair_separate_third.rs.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose, as written.
//
// Disclosed workaround: `rebalance` could be split into a version for two
// different stats objects and an in-place version for one. Stats would then
// need no cells and no brand. It would change nothing in `attack`, the loop
// this test measures: `stats` already reaches it as a plain read-only
// reference. Splitting a function into an aliasing and a non-aliasing version
// is ruled inadmissible for every arm, so this arm has one `rebalance`.
//
// Why: `attack` takes `stats: &Stats`, a view the caller borrowed from the
// stats cell. That is a reference parameter to a type with no interior
// mutability, so rustc marks it `noalias` and read-only, and LLVM loads
// `stats.cost` and `stats.damage` once. The two entities may be the same, so
// each `hp` is reloaded after the other's store, as in the Valen arm.
//
// How this arm was written: `attack` holds the two entity cell pointers and
// takes a mutable view of each in turn, because the token gives one view at a
// time. `rebalance` reads `y` and releases it before it writes `x`. Fields and
// counters use the Valen arm's widths: Valen `int` is `i32`.
//
// The other way to pass stats, and what it costs: `attack` could take the
// stats cell and its token, and borrow the view itself:
//   attack(te, ts: &GhostToken<'s>, a, d, stats: &GhostCell<'s, Stats>, n)
//   let stats = stats.borrow(ts);
// The view is then a local reference, which rustc does not mark. Compiled, that
// loop loads `stats.cost` and `stats.damage` on every iteration: four loads per
// iteration where this arm has two. It holds the view across the stores, so its
// verdict would be "rustc could close" (Bucket A, appendix). This arm passes
// the view because that is the better spelling for GhostCell, and GhostCell
// uses whatever serves it best.
//
// Brand layout: two brands, one for entities and one for stats. Stats are never
// paired with entities, so nothing forces one brand. With one brand the caller
// could not lend a view of the stats while `attack` holds the token mutably,
// and the arm would have to borrow stats inside the loop.
//
// Cell layout: one cell per entity and one per stats object. Neither has
// anything below it to put cells on.
//
// Why `Entity` and `Stats` are in cells: `attack` writes two entities that may
// be the same entity, and `rebalance` writes one stats object while it reads
// another that may be the same one. Without cells the borrow checker rejects
// each pair of references.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of `stats.cost`, a load and a store of `a.hp`, a
//   load of `stats.damage`, and a load and a store of `d.hp`.
//
// Expected IR of `attack`, optimized (the same loop as the Valen arm):
// - The parameter `stats` carries `noalias`.
// - `stats.cost` and `stats.damage` are loaded once, before the loop.
// - Each iteration loads and stores `a.hp`, then loads and stores `d.hp`.

use ghost_cell::{GhostCell, GhostToken};

pub struct Stats {
    pub cost: i32,
    pub damage: i32,
}

pub struct Entity {
    pub hp: i32,
}

pub fn rebalance<'s>(ts: &mut GhostToken<'s>, x: &GhostCell<'s, Stats>, y: &GhostCell<'s, Stats>) {
    let added = y.borrow(ts).damage;
    x.borrow_mut(ts).cost += added;
}

#[inline(never)]
pub fn attack<'e>(
    te: &mut GhostToken<'e>,
    a: &GhostCell<'e, Entity>,
    d: &GhostCell<'e, Entity>,
    stats: &Stats,
    n: i32,
) {
    let mut i: i32 = 0;
    while i < n {
        a.borrow_mut(te).hp -= stats.cost;
        d.borrow_mut(te).hp -= stats.damage;
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut te| {
        GhostToken::new(|mut ts| {
            let entities = vec![GhostCell::new(Entity { hp: 50 }), GhostCell::new(Entity { hp: 60 })];
            let calm = GhostCell::new(Stats { cost: 1, damage: 2 });
            let fierce = GhostCell::new(Stats { cost: 2, damage: 3 });
            rebalance(&mut ts, &calm, &fierce);
            rebalance(&mut ts, &fierce, &fierce);
            attack(&mut te, &entities[0], &entities[1], calm.borrow(&ts), 3);
            attack(&mut te, &entities[1], &entities[1], fierce.borrow(&ts), 2);
            (entities[0].borrow(&te).hp
                + entities[1].borrow(&te).hp
                + calm.borrow(&ts).cost
                + fierce.borrow(&ts).cost) as i64
        })
    })
}
