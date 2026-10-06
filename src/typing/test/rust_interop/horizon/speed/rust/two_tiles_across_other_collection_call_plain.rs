// Plain Rust arm of valen/two_tiles_across_other_collection_call.rs.
//
// Spelling not used: the map and two keys,
// `spread(tiles: &mut HashMap<i32, Tile>, a: i32, b: i32, ..)`, with
// `tiles.get_mut(&a).unwrap().hazard += tiles.get(&b).unwrap().cover` in the
// loop. Observed cost per iteration: two table lookups (one hashing in the
// loop, one probing with a hash computed before the loop), a load of `cover`, a
// load of `hazard`, a store of `hazard`, and the call. This arm uses `Cell`
// fields instead, because its loop is better: no lookups.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B. References are held, but they are shared references to
// interior-mutable fields, so they promise nothing about the contents.
//
// Why it loses today: `hazard` and `cover` are `Cell<i32>`, so `a` and `b` can
// be two shared references that may be the same tile, held for the whole loop.
// But `Tile` holds its `Cell`s directly, so `&Tile` is not `Freeze` and gets no
// `noalias`. LLVM must assume `muster(entities)` changed the tiles, and reloads
// `hazard` and `cover` after every call.
//
// Why rustc can never close it: a `Cell`'s contents may legally be written
// through any shared pointer to it. Miri accepts, under Stacked Borrows and
// Tree Borrows, a program in which a caller holds `&T` to a struct of `Cell`s
// for a whole loop while a no-argument call writes one of them through a
// pointer saved from `Cell::as_ptr`. So a held `&Tile` gives the compiler no
// fact about `hazard` or `cover` across a call. Closing this would mean
// changing what `UnsafeCell` means.
//
// Why Valen may: `muster` is handed only `entities`, and safe code cannot reach
// a tile from there. Valen has no unsafe code and assumes every imported safe Rust
// function is a sound API (proposal S30 in rust-interop-design.md). This win
// comes from that assumption, not from group borrowing.
//
// Other spellings, and why each is excluded:
// - `get_disjoint_mut([&a, &b])` returns both references, but panics when the
//   keys are equal, and main calls `spread` with equal keys.
// - `if a == b { one body } else { another body }` duplicates the loop.
// No spelling of this program is equal today.
//
// How this arm was written: a mirror of the Valen arm. `spread` takes two
// references to tiles, as Valen does. They are shared references, and the
// fields are `Cell`s, because `&mut Tile` beside `&Tile` is rejected when both
// may name one tile (E0502).
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `cover`, a load of `hazard`, a store of
//   `hazard`, and the call to `muster`.
// - The loop contains no call into `HashMap`.
//
// Expected IR of `spread`, optimized:
// - The parameters `a` and `b` carry no `noalias`.
// - The loop contains the call to `muster` and one store of `hazard`.
// - The loop contains one load of `cover` and one load of `hazard`. Both follow
//   the call to `muster` on every iteration.
// - The loop contains no hashing and no call into `HashMap`.

use mycrate::do_nothing;
use std::cell::Cell;
use std::collections::HashMap;

pub struct Tile {
    pub hazard: Cell<i32>,
    pub cover: Cell<i32>,
}

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub tiles: HashMap<i32, Tile>,
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn muster(entities: &mut Vec<Entity>) {
    entities[0].hp += 1;
    do_nothing();
}

#[inline(never)]
pub fn spread(a: &Tile, b: &Tile, entities: &mut Vec<Entity>, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        a.hazard.set(a.hazard.get() + b.cover.get());
        muster(entities);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level { tiles: HashMap::new(), entities: Vec::new() };
    level.tiles.insert(1, Tile { hazard: Cell::new(0), cover: Cell::new(2) });
    level.tiles.insert(2, Tile { hazard: Cell::new(0), cover: Cell::new(3) });
    level.entities.push(Entity { hp: 0 });
    let k1 = 1;
    let k2 = 2;
    spread(level.tiles.get(&k1).unwrap(), level.tiles.get(&k2).unwrap(), &mut level.entities, 4);
    spread(level.tiles.get(&k2).unwrap(), level.tiles.get(&k2).unwrap(), &mut level.entities, 5);
    let t1 = level.tiles.get(&k1).unwrap();
    let t2 = level.tiles.get(&k2).unwrap();
    let e0 = &level.entities[0];
    (t1.hazard.get() + t2.hazard.get() + e0.hp) as i64
}
