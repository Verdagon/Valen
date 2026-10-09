// GhostCell arm of valen/aliased_pair_two_fields.rs.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins).
// Bucket: none. This arm does not lose.
//
// Spelling not used: two cell pointers, the mirror of the Valen arm,
//   attack(te, a: &GhostCell<Entity>, d: &GhostCell<Entity>, stats: &Stats, n)
// Compiled, that loop loads and stores `a.stamina` and `d.hp` on every
// iteration, which is the Valen arm's expected loop. This arm takes the `Vec`
// of cells and two indices because that spelling has the better loop: none.
//
// Disclosed workaround: `rebalance` could be split into a version for two
// different stats objects and an in-place version for one. Stats would then
// need no cells and no brand. It would change nothing in `attack`, the loop
// this test measures: `stats` already reaches it as a plain read-only
// reference. Splitting a function into an aliasing and a non-aliasing version
// is ruled inadmissible for every arm, so this arm has one `rebalance`.
//
// Why it wins: `attack` takes the `Vec` of entity cells and two indices, so
// both entities are reached from one buffer. LLVM sees one base pointer, an
// index scaled by the size of an entity, and two different field offsets, and
// concludes that `entities[a].stamina` and `entities[d].hp` never overlap,
// whether or not `a` equals `d`. It keeps both in registers and then removes
// the loop: `stamina` goes down by `cost * n` and `hp` by `damage * n`. The
// cells do not get in the way: a view is taken and released on each line, and
// that compiles to nothing.
//
// The Valen arm holds two references in one group. Nothing tells LLVM that they
// point at the same entity or at separate ones, so each store forces the other
// field to be reloaded, and its loop runs `n` times.
//
// How this arm was written: with indices, as the plain arm is. `stats` is
// passed as a view
// the caller borrowed, so it is a read-only `noalias` parameter. `rebalance`
// reads `y` and releases it before it writes `x`, because the token gives one
// view at a time. Fields and counters use the Valen arm's widths: Valen `int`
// is `i32`.
//
// Brand layout: two brands, one for entities and one for stats. Stats are never
// paired with entities, so nothing forces one brand.
//
// Cell layout: one cell per entity and one per stats object.
//
// Why `Entity` and `Stats` are in cells: `attack` writes two entities that may
// be the same entity, and `rebalance` writes one stats object while it reads
// another that may be the same one. Without cells the borrow checker rejects
// each pair of references. (`attack` in this spelling would also work with no
// cells, as the plain arm shows; the cells are for a program that elsewhere
// holds references to two entities at once.)
//
// Expected IR of `attack`, unoptimized:
// - The loop contains two bounds checks, a load of `stats.cost`, a load and a
//   store of `entities[a].stamina`, a load of `stats.damage`, and a load and a
//   store of `entities[d].hp`.
//
// Expected IR of `attack`, optimized (the plain arm's shape):
// - There is no loop.
// - `stats.cost` and `stats.damage` are each loaded once and multiplied by `n`.
// - `entities[a].stamina` and `entities[d].hp` are each loaded once and stored
//   once.

use ghost_cell::{GhostCell, GhostToken};

pub struct Stats {
    pub cost: i32,
    pub damage: i32,
}

pub struct Entity {
    pub hp: i32,
    pub stamina: i32,
}

pub fn rebalance<'s>(ts: &mut GhostToken<'s>, x: &GhostCell<'s, Stats>, y: &GhostCell<'s, Stats>) {
    let added = y.borrow(ts).damage;
    x.borrow_mut(ts).cost += added;
}

#[inline(never)]
pub fn attack<'e>(
    te: &mut GhostToken<'e>,
    entities: &Vec<GhostCell<'e, Entity>>,
    a: usize,
    d: usize,
    stats: &Stats,
    n: i32,
) {
    let mut i: i32 = 0;
    while i < n {
        entities[a].borrow_mut(te).stamina -= stats.cost;
        entities[d].borrow_mut(te).hp -= stats.damage;
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut te| {
        GhostToken::new(|mut ts| {
            let entities = vec![
                GhostCell::new(Entity { hp: 50, stamina: 30 }),
                GhostCell::new(Entity { hp: 60, stamina: 40 }),
            ];
            let calm = GhostCell::new(Stats { cost: 1, damage: 2 });
            let fierce = GhostCell::new(Stats { cost: 2, damage: 3 });
            rebalance(&mut ts, &calm, &fierce);
            rebalance(&mut ts, &fierce, &fierce);
            attack(&mut te, &entities, 0, 1, calm.borrow(&ts), 3);
            attack(&mut te, &entities, 1, 1, fierce.borrow(&ts), 2);
            (entities[0].borrow(&te).stamina
                + entities[1].borrow(&te).stamina
                + entities[1].borrow(&te).hp
                + calm.borrow(&ts).cost
                + fierce.borrow(&ts).cost) as i64
        })
    })
}
