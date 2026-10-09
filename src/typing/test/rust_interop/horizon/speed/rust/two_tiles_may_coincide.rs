// GhostCell arm of valen/two_tiles_may_coincide.rs.
//
// Verdict: never.
// Tier: none today (the Valen arm reloads after the call too). If S18 is
// resolved: second claim ("trust"). The verdict is against the Valen arm's
// intended shape, not what Valen emits today.
// Bucket: B. Cell pointers are held, but no view of the contents is.
//
// What GhostCell wins back: the two lookups. `spread` holds a
// `&GhostCell<Tile>` for each tile, and the loop does no hashing.
//
// Why it still loses: `a` and `b` may be the same cell, so a shared view of `b`
// cannot be held beside an exclusive view of `a` (E0502). The loop reaches
// `hazard` and `cover` through the cells on every iteration. The cell
// parameters wrap `UnsafeCell` and get no `noalias`, so LLVM must assume
// `tick` changed the tiles, and reloads both fields after every call.
//
// Why rustc can never close it: `tick` takes only the clock's token, so by the
// library's rules it cannot write a tile. But a compiler cannot rely on that.
// Unsafe code may legally write a cell's contents through a saved pointer
// while the caller holds only the `&GhostCell`. Miri accepts that program under
// both current aliasing models, Stacked Borrows and Tree Borrows (results are
// recorded in notes/docs/architecture/rust-interop-design.md, Background).
//
// Why Valen may: Valen has no unsafe code and assumes every imported safe Rust
// function is a sound API (proposal S30 in rust-interop-design.md). This win
// comes from that assumption, not from group borrowing.
//
// Brand layout: two brands. The tiles share one brand; the clock has its own.
// Why this is the best layout: with one brand over both, `tick` would take the
// tiles' token and could write any tile, which is a Bucket C loss. With the
// clock in its own brand, `tick` takes only the clock's token, and the arm
// loses only because cell contents may be written behind a held cell pointer.
// `a` and `b` come from one map and may be the same cell, so they share a
// brand. A std `Cell<i32>` clock with no token at all would also work; it
// changes neither the loop nor the bucket. A cell on each tile field, in place
// of one per tile, compiles to the same loop (observed on
// rust/two_tiles_across_other_collection_call.rs's shape).
//
// Why the tiles are in cells: `spread` takes two tiles that may be the same
// tile, and writes one while reading the other.
// Why the clock is in a cell: `spread` holds cell pointers into `level.tiles`,
// so `level` is shared, and `tick` must write the clock through `&Level`.
//
// How this arm was written: a mirror of the Valen arm. `spread` takes the level
// and two cell pointers, where Valen takes the level and two references. Each
// use goes through the token, because the two views cannot be held together.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `spread`, unoptimized:
// - The loop contains a load of `cover`, a load of `hazard`, a store of
//   `hazard`, and the call to `tick`.
//
// Expected IR of `spread`, optimized:
// - The parameters `a` and `b` carry no `noalias`.
// - The loop contains the call to `tick` and one store of `hazard`.
// - The loop contains one load of `cover` and one load of `hazard`. Both follow
//   the call to `tick` on every iteration.
// - The loop contains no hashing and no call into `HashMap`.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::do_nothing;
use std::collections::HashMap;

pub struct Tile {
    pub hazard: i32,
    pub cover: i32,
}

pub struct Level<'t, 'c> {
    pub tiles: HashMap<i32, GhostCell<'t, Tile>>,
    pub clock: GhostCell<'c, i32>,
}

#[inline(never)]
pub fn tick<'t, 'c>(tc: &mut GhostToken<'c>, level: &Level<'t, 'c>) {
    *level.clock.borrow_mut(tc) += 1;
    do_nothing();
}

#[inline(never)]
pub fn spread<'t, 'c>(
    tt: &mut GhostToken<'t>,
    tc: &mut GhostToken<'c>,
    level: &Level<'t, 'c>,
    a: &GhostCell<'t, Tile>,
    b: &GhostCell<'t, Tile>,
    n: i32,
) {
    let mut i: i32 = 0;
    while i < n {
        a.borrow_mut(tt).hazard += b.borrow(tt).cover;
        tick(tc, level);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut tt| {
        GhostToken::new(|mut tc| {
            let mut level = Level { tiles: HashMap::new(), clock: GhostCell::new(0) };
            level.tiles.insert(1, GhostCell::new(Tile { hazard: 0, cover: 2 }));
            level.tiles.insert(2, GhostCell::new(Tile { hazard: 0, cover: 3 }));
            let k1 = 1;
            let k2 = 2;
            spread(&mut tt, &mut tc, &level, level.tiles.get(&k1).unwrap(), level.tiles.get(&k2).unwrap(), 4);
            spread(&mut tt, &mut tc, &level, level.tiles.get(&k2).unwrap(), level.tiles.get(&k2).unwrap(), 5);
            let t1 = level.tiles.get(&k1).unwrap().borrow(&tt);
            let t2 = level.tiles.get(&k2).unwrap().borrow(&tt);
            (t1.hazard + t2.hazard + *level.clock.borrow(&tc)) as i64
        })
    })
}
