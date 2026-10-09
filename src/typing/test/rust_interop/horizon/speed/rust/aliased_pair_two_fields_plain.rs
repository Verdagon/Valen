// Plain Rust arm of valen/aliased_pair_two_fields.rs.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins).
// Bucket: none. This arm does not lose.
//
// Disclosed workaround: `rebalance` could be split into a version for two
// different stats objects, taking `&mut Stats` and `&Stats`, and an in-place
// version taking one `&mut Stats`. The stats could then stay two locals. It
// would change nothing in `attack`, the loop this test measures. Splitting a
// function into an aliasing and a non-aliasing version is ruled inadmissible
// for every arm, so this arm has one `rebalance`.
//
// Why it wins: the borrow checker rejects two references to entities that may
// be the same entity, so `attack` takes the `Vec` and two indices. That turns
// out to help. Both entities are reached from one buffer, so LLVM sees one base
// pointer, an index scaled by the size of an entity, and two different field
// offsets. It concludes that `entities[a].stamina` and `entities[d].hp` never
// overlap, whether or not `a` equals `d`. It keeps both in registers and then
// removes the loop: `stamina` goes down by `cost * n` and `hp` by `damage * n`.
//
// The Valen arm holds two references in one group. Nothing tells LLVM that they
// point at the same entity or at separate ones, so each store forces the other
// field to be reloaded, and its loop runs `n` times. This arm does two loads,
// two multiplications and two stores in all; the Valen arm does two loads and
// two stores per iteration.
//
// How this arm was written: the indices are the forced reaction to the borrow
// checker's rejection, not a hand-optimization. `stats` is a mirror: a
// reference parameter to a type with no interior mutability, which rustc marks
// `noalias` and read-only. `rebalance` takes a slice of stats and two indices,
// because the borrow checker rejects `rebalance(&mut fierce, &fierce)`. So the
// two stats objects live in one `Vec` in this arm, where the Valen arm has two
// locals. One `rebalance` serves both the two-object call and the same-object
// call. Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains two bounds checks, a load of `stats.cost`, a load and a
//   store of `entities[a].stamina`, a load of `stats.damage`, and a load and a
//   store of `entities[d].hp`.
//
// Expected IR of `attack`, optimized:
// - The parameters `entities` and `stats` carry `noalias`.
// - There is no loop.
// - `stats.cost` and `stats.damage` are each loaded once and multiplied by `n`.
// - `entities[a].stamina` and `entities[d].hp` are each loaded once and stored
//   once.

pub struct Stats {
    pub cost: i32,
    pub damage: i32,
}

pub struct Entity {
    pub hp: i32,
    pub stamina: i32,
}

pub fn rebalance(stats: &mut [Stats], x: usize, y: usize) {
    stats[x].cost += stats[y].damage;
}

#[inline(never)]
pub fn attack(entities: &mut Vec<Entity>, a: usize, d: usize, stats: &Stats, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        entities[a].stamina -= stats.cost;
        entities[d].hp -= stats.damage;
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut entities = vec![Entity { hp: 50, stamina: 30 }, Entity { hp: 60, stamina: 40 }];
    let mut stats = vec![Stats { cost: 1, damage: 2 }, Stats { cost: 2, damage: 3 }];
    rebalance(&mut stats, 0, 1);
    rebalance(&mut stats, 1, 1);
    attack(&mut entities, 0, 1, &stats[0], 3);
    attack(&mut entities, 1, 1, &stats[1], 2);
    (entities[0].stamina + entities[1].stamina + entities[1].hp + stats[0].cost + stats[1].cost) as i64
}
