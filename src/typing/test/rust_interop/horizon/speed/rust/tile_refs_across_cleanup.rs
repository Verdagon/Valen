// GhostCell arm of valen/tile_refs_across_cleanup.rs: identical to the plain
// arm. Nothing in this program writes through an alias, so no cell and no token
// are needed.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// Nothing in this program needs a cell. No function takes two references that
// may be the same object, and no reference is held to something another
// function writes. So this arm has no `GhostCell` and no token, and its code is
// the plain arm's (rust/tile_refs_across_cleanup_plain.rs).
//
// Why it is equal: `t: &mut Tile` is a reference parameter, so rustc marks it
// `noalias`. LLVM then knows `cleanup(entities)` cannot change `t.hazard`, and
// keeps the value in a register across the call.
//
// How this arm was written: a mirror of the Valen arm. The borrow checker
// rejects nothing here, so nothing deviates. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `wave`, unoptimized:
// - The loop contains a load of `t.hazard` (twice), a store of `t.hazard`, a
//   load and a store of the entity's `hp`, and the call to `cleanup`.
//
// Expected IR of `wave`, optimized (the same shape as the Valen arm):
// - The parameter `t` carries `noalias`.
// - `t.hazard` is loaded once, before the loop.
// - The loop contains the call to `cleanup` and one store of `t.hazard`.
// - The loop contains no load of `t.hazard`.
// - The entities' pointer, length and first element are loaded again after
//   every call to `cleanup`, which changes them.

use mycrate::do_nothing;

pub struct Tile {
    pub hazard: i32,
}

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn cleanup(entities: &mut Vec<Entity>) {
    entities.clear();
    entities.push(Entity { hp: 10 });
    do_nothing();
}

#[inline(never)]
pub fn wave(t: &mut Tile, entities: &mut Vec<Entity>, n: i32) -> i32 {
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        entities[0].hp -= t.hazard;
        total += 10 - entities[0].hp;
        cleanup(entities);
        t.hazard += 1;
        i += 1;
    }
    total
}

pub fn main_like() -> i64 {
    let mut level = Level { tiles: Vec::new(), entities: Vec::new() };
    level.tiles.push(Tile { hazard: 1 });
    level.tiles.push(Tile { hazard: 2 });
    level.entities.push(Entity { hp: 10 });
    let s1 = wave(&mut level.tiles[0], &mut level.entities, 3);
    let s2 = wave(&mut level.tiles[1], &mut level.entities, 4);
    (s1 + s2 + level.tiles[0].hazard + level.tiles[1].hazard) as i64
}
