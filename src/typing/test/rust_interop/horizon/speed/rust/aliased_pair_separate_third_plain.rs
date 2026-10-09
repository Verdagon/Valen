// Plain Rust arm of valen/aliased_pair_separate_third.rs.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// Disclosed workaround: `rebalance` could be split into a version for two
// different stats objects, taking `&mut Stats` and `&Stats`, and an in-place
// version taking one `&mut Stats`. The stats could then stay two locals. It
// would change nothing in `attack`, the loop this test measures: no load more
// or fewer. Splitting a function into an aliasing and a non-aliasing version is
// ruled inadmissible for every arm, so this arm has one `rebalance`.
//
// Why: `stats: &Stats` is a reference parameter to a type with no interior
// mutability, so rustc marks it `noalias` and read-only. LLVM then knows the
// stores to the entities cannot change `stats.cost` or `stats.damage`, and
// loads each once. That is the same fact the Valen arm gives LLVM. The two
// entities may be the same, so each `hp` is reloaded after the other's store,
// as in the Valen arm.
//
// How this arm was written: the borrow checker rejects two references to
// entities that may be the same entity, so `attack` takes the `Vec` and two
// indices. `stats` is a mirror. `rebalance` takes a slice of stats and two
// indices, for the same reason: the borrow checker rejects
// `rebalance(&mut fierce, &fierce)`. So the two stats objects live in one `Vec`
// in this arm, where the Valen arm has two locals. One `rebalance` serves both
// the two-object call and the same-object call. Fields and counters use the
// Valen arm's widths: Valen `int` is `i32`.
//
// What indices buy this arm: if the two writes went to different fields, LLVM
// would see that two different fields of one buffer's elements never overlap,
// and would replace the loop with two multiplications. The Valen arm's two
// pointers do not carry that fact. Both writes here go to `hp`, so that does
// not arise.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains two bounds checks, a load of `stats.cost`, a load and a
//   store of `entities[a].hp`, a load of `stats.damage`, and a load and a store
//   of `entities[d].hp`.
//
// Expected IR of `attack`, optimized (the same loop as the Valen arm):
// - The parameters `entities` and `stats` carry `noalias`.
// - The buffer pointer and length are loaded once, and both bounds checks sit
//   before the loop.
// - `stats.cost` and `stats.damage` are loaded once, before the loop.
// - Each iteration loads and stores `entities[a].hp`, then loads and stores
//   `entities[d].hp`.

pub struct Stats {
    pub cost: i32,
    pub damage: i32,
}

pub struct Entity {
    pub hp: i32,
}

pub fn rebalance(stats: &mut [Stats], x: usize, y: usize) {
    stats[x].cost += stats[y].damage;
}

#[inline(never)]
pub fn attack(entities: &mut Vec<Entity>, a: usize, d: usize, stats: &Stats, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        entities[a].hp -= stats.cost;
        entities[d].hp -= stats.damage;
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut entities = vec![Entity { hp: 50 }, Entity { hp: 60 }];
    let mut stats = vec![Stats { cost: 1, damage: 2 }, Stats { cost: 2, damage: 3 }];
    rebalance(&mut stats, 0, 1);
    rebalance(&mut stats, 1, 1);
    attack(&mut entities, 0, 1, &stats[0], 3);
    attack(&mut entities, 1, 1, &stats[1], 2);
    (entities[0].hp + entities[1].hp + stats[0].cost + stats[1].cost) as i64
}
