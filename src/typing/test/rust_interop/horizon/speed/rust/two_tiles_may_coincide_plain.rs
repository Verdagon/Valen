// Plain Rust arm of valen/two_tiles_may_coincide.rs.
//
// Spelling not used: the level and two keys,
// `spread(level: &mut Level, a: i32, b: i32, ..)` with `tick(level: &mut
// Level)`, looking both tiles up in the loop. Its cost per iteration is two
// table lookups, a load of `cover`, a load of `hazard`, a store of `hazard`,
// and the call (the lookups were observed on the same loop in
// rust/two_tiles_across_other_collection_call_plain.rs before it changed
// spelling). This arm uses `Cell` fields instead, because its loop is better:
// no lookups.
//
// Verdict: never.
// Tier: none today (the Valen arm reloads after the call too). If S18 is
// resolved: second claim. The tier follows the GhostCell arm, which is the
// better-informed Rust program and is Bucket B. The verdict is against the
// Valen arm's intended shape, not what Valen emits today.
// Bucket: C. The fact is not in the program's types: safe code holding `&Level`
// may write any tile.
//
// Why it loses against the intended shape: `hazard`, `cover` and `clock` are
// `Cell<i32>`, so `spread` holds two shared references that may be the same
// tile, and `tick` takes the level shared. But `Tile` holds its `Cell`s
// directly, so `&Tile` is not `Freeze` and gets no `noalias`. LLVM must assume
// `tick(level)` changed the tiles, and reloads `hazard` and `cover` after every
// call.
//
// Why no compiler can close it: `tick` is handed `&Level`, and safe code
// holding `&Level` may call `set` on any tile. Nothing in Rust's types says
// `tick` leaves the tiles alone. This is the plain-Rust counterpart of a
// GhostCell callee that takes the token. A held `&Tile` does not help: a
// `Cell`'s contents may legally be written through any shared pointer to it
// (Miri accepts that under Stacked Borrows and Tree Borrows, with the caller
// holding `&T` for the whole loop).
//
// Why Valen could: its signature says it. `tick` declares `mut(l.clock)`, which
// does not include the tiles. Turning that into a fact LLVM can use is what S18
// blocks.
//
// Other spellings, and why each is excluded:
// - `get_disjoint_mut([&a, &b])` panics when the keys are equal, and main calls
//   `spread` with equal keys.
// - `if a == b { one body } else { another body }` duplicates the loop.
// No spelling of this program is equal to the Valen arm's intended shape.
//
// How this arm was written: a mirror of the Valen arm. `spread` takes the level
// and two references to tiles, as Valen does. The references are shared and the
// fields are `Cell`s, because `&mut Tile` beside `&Tile` is rejected when both
// may name one tile (E0502). `clock` is a `Cell` because `tick` must write it
// through `&Level` while `spread` holds references into `level.tiles`.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `cover`, a load of `hazard`, a store of
//   `hazard`, and the call to `tick`.
// - The loop contains no call into `HashMap`.
//
// Expected IR of `spread`, optimized:
// - The parameters `a` and `b` carry no `noalias`.
// - The loop contains the call to `tick` and one store of `hazard`.
// - The loop contains one load of `cover` and one load of `hazard`. Both follow
//   the call to `tick` on every iteration.
// - The loop contains no hashing and no call into `HashMap`.

use mycrate::do_nothing;
use std::cell::Cell;
use std::collections::HashMap;

pub struct Tile {
    pub hazard: Cell<i32>,
    pub cover: Cell<i32>,
}

pub struct Level {
    pub tiles: HashMap<i32, Tile>,
    pub clock: Cell<i32>,
}

#[inline(never)]
pub fn tick(level: &Level) {
    level.clock.set(level.clock.get() + 1);
    do_nothing();
}

#[inline(never)]
pub fn spread(level: &Level, a: &Tile, b: &Tile, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        a.hazard.set(a.hazard.get() + b.cover.get());
        tick(level);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level { tiles: HashMap::new(), clock: Cell::new(0) };
    level.tiles.insert(1, Tile { hazard: Cell::new(0), cover: Cell::new(2) });
    level.tiles.insert(2, Tile { hazard: Cell::new(0), cover: Cell::new(3) });
    let k1 = 1;
    let k2 = 2;
    spread(&level, level.tiles.get(&k1).unwrap(), level.tiles.get(&k2).unwrap(), 4);
    spread(&level, level.tiles.get(&k2).unwrap(), level.tiles.get(&k2).unwrap(), 5);
    let t1 = level.tiles.get(&k1).unwrap();
    let t2 = level.tiles.get(&k2).unwrap();
    (t1.hazard.get() + t2.hazard.get() + level.clock.get()) as i64
}
