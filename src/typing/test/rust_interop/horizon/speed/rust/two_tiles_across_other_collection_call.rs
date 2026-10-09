// GhostCell arm of valen/two_tiles_across_other_collection_call.rs.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B. Cell pointers are held, but no view of the contents is.
//
// What GhostCell wins back: the two lookups. `spread` holds a
// `&GhostCell<Tile>` for each tile, as the Valen arm holds two references, and
// the loop does no hashing.
//
// Why it still loses: `a` and `b` may be the same cell, so a shared view of `b`
// cannot be held beside an exclusive view of `a` (E0502). The loop reaches
// `hazard` and `cover` through the cells on every iteration. The cell
// parameters wrap `UnsafeCell` and get no `noalias`, so LLVM must assume
// `muster(entities)` changed the tiles, and reloads both fields after every
// call.
//
// Why rustc can never close it: `muster` takes no token, so by the library's
// rules it cannot write a tile. But a compiler cannot rely on that. Unsafe code
// may legally write a cell's contents through a saved pointer while the caller
// holds only the `&GhostCell`. Miri accepts that program under both current
// aliasing models, Stacked Borrows and Tree Borrows (results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background).
//
// Why Valen may: Valen has no unsafe code and assumes every imported safe Rust
// function is a sound API (proposal S30 in rust-interop-design.md). This win
// comes from that assumption, not from group borrowing.
//
// Brand layout: one brand, over the tiles only, with one cell per tile.
// `entities` is a plain `Vec<Entity>` with no cells, passed as `&mut Vec`.
// Why no better layout exists: `muster` already takes no token, which is the
// best a callee can be for this arm. Putting `a` and `b` in different brands
// would let both views be held, but they come from one map and may be the same
// cell, so they share a brand.
//
// Cell placement not used: a cell on each field (`hazard: GhostCell<i32>`,
// `cover: GhostCell<i32>`), `Tile` itself not in a cell, `spread` taking two
// `&Tile`. Observed cost per iteration: load `cover`, load `hazard`, store
// `hazard`, call `muster`: the same loop as one cell per tile. The two tie, so
// the arm keeps one cell per tile.
//
// Why the tiles are in cells: `spread` takes two tiles that may be the same
// tile, and writes one while reading the other.
//
// How this arm was written: a mirror of the Valen arm. `spread` takes two cell
// pointers where Valen takes two references. Each use goes through the token,
// because the two views cannot be held together.
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

use ghost_cell::{GhostCell, GhostToken};
use mycrate::do_nothing;
use std::collections::HashMap;

pub struct Tile {
    pub hazard: i32,
    pub cover: i32,
}

pub struct Entity {
    pub hp: i32,
}

pub struct Level<'b> {
    pub tiles: HashMap<i32, GhostCell<'b, Tile>>,
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn muster(entities: &mut Vec<Entity>) {
    entities[0].hp += 1;
    do_nothing();
}

#[inline(never)]
pub fn spread<'b>(
    t: &mut GhostToken<'b>,
    a: &GhostCell<'b, Tile>,
    b: &GhostCell<'b, Tile>,
    entities: &mut Vec<Entity>,
    n: i32,
) {
    let mut i: i32 = 0;
    while i < n {
        a.borrow_mut(t).hazard += b.borrow(t).cover;
        muster(entities);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let mut level = Level { tiles: HashMap::new(), entities: Vec::new() };
        level.tiles.insert(1, GhostCell::new(Tile { hazard: 0, cover: 2 }));
        level.tiles.insert(2, GhostCell::new(Tile { hazard: 0, cover: 3 }));
        level.entities.push(Entity { hp: 0 });
        let k1 = 1;
        let k2 = 2;
        spread(&mut t, level.tiles.get(&k1).unwrap(), level.tiles.get(&k2).unwrap(), &mut level.entities, 4);
        spread(&mut t, level.tiles.get(&k2).unwrap(), level.tiles.get(&k2).unwrap(), &mut level.entities, 5);
        let t1 = level.tiles.get(&k1).unwrap().borrow(&t);
        let t2 = level.tiles.get(&k2).unwrap().borrow(&t);
        let e0 = &level.entities[0];
        (t1.hazard + t2.hazard + e0.hp) as i64
    })
}
