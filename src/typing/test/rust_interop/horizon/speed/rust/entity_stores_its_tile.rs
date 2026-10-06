// Branded-index arm of valen/entity_stores_its_tile.rs. It takes the GhostCell
// slot: it uses the same technique GhostCell does, a brand lifetime, applied to
// what this test is about. No cell is needed, because nothing in the program
// writes through an alias.
//
// The technique: `Tiles<'id>` and `TileIdx<'id>` share a brand. A `TileIdx` can
// only be made by checking a number against the tiles of its brand, and `Tiles`
// offers no way to shrink. So indexing with a `TileIdx` needs no bounds check.
// It is safe Rust over one `unsafe` access inside a small library, written out
// in this file, as GhostCell is safe Rust over `UnsafeCell`.
//
// Read the container (`Brand`, `Tiles`, `TileIdx`, `with_tiles`) as a separate
// module. Rust privacy is per module, so `tick` and `main_like` in this file
// could reach the private `Vec` inside `Tiles` and break the invariant. They do
// not, and in a real library they could not.
//
// Arm verdict: equal today, to the Valen arm's intended shape.
// Tier: fence. Rust already gets this with a branded index.
// Bucket: none. This arm does not lose.
//
// What it costs against a stored pointer: the same loads as a stored reference,
// plus one address computation. The Valen arm would load a pointer and then
// load `hazard`. This arm loads the index, adds it to the tiles' buffer pointer
// (loaded once, before the loop), and loads `hazard`. No bounds check, no
// branch. The range fact reaches LLVM as an assumption: a comparison of the
// index against the length that feeds only `llvm.assume`.
//
// What it costs the program: a brand lifetime on `Level` and `Entity`, and
// tiles that can never shrink while the brand lives. This program's tiles never
// do.
//
// How this arm was written: a mirror of the Valen arm, except where Rust
// rejects it. `standing_on` is a branded index where Valen stores a reference;
// a reference there makes `Level` borrow one of its own fields. The borrow
// checker accepts an entity that stores a `TileIdx<'id>` beside the `Tiles<'id>`
// it indexes, in one `Level<'id>`.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`, and the
// count is `i64`.
//
// Expected IR of `tick`, unoptimized:
// - The loop contains a load of the index, a load of `hazard`, and a load and a
//   store of `hp`. It contains no bounds check on the tile.
//
// Expected IR of `tick`, optimized:
// - The tiles' pointer is loaded once, before the loop.
// - The loop contains a load of the index, a load of `hazard`, and a load and a
//   store of `hp`.
// - The loop contains no bounds check on the tile and no branch that depends on
//   the index.

use std::marker::PhantomData;

type Brand<'id> = PhantomData<fn(&'id ()) -> &'id ()>;

pub struct Tile {
    pub hazard: i32,
}

pub struct Tiles<'id> {
    v: Vec<Tile>,
    _brand: Brand<'id>,
}

#[derive(Clone, Copy)]
pub struct TileIdx<'id> {
    i: usize,
    _brand: Brand<'id>,
}

pub fn with_tiles<R>(v: Vec<Tile>, f: impl for<'id> FnOnce(Tiles<'id>) -> R) -> R {
    f(Tiles { v, _brand: PhantomData })
}

impl<'id> Tiles<'id> {
    pub fn check(&self, i: usize) -> Option<TileIdx<'id>> {
        if i < self.v.len() {
            Some(TileIdx { i, _brand: PhantomData })
        } else {
            None
        }
    }

    pub fn get(&self, ix: TileIdx<'id>) -> &Tile {
        unsafe { self.v.get_unchecked(ix.i) }
    }

    pub fn get_mut(&mut self, ix: TileIdx<'id>) -> &mut Tile {
        unsafe { self.v.get_unchecked_mut(ix.i) }
    }
}

pub struct Entity<'id> {
    pub hp: i32,
    pub standing_on: TileIdx<'id>,
}

pub struct Level<'id> {
    pub tiles: Tiles<'id>,
    pub entities: Vec<Entity<'id>>,
}

#[inline(never)]
pub fn tick<'id>(level: &mut Level<'id>, count: i64) {
    let mut i: i64 = 0;
    while i < count {
        let e = &mut level.entities[i as usize];
        e.hp -= level.tiles.get(e.standing_on).hazard;
        i += 1;
    }
    let t0 = level.tiles.check(0).unwrap();
    level.tiles.get_mut(t0).hazard += 1;
    let t1 = level.tiles.check(1).unwrap();
    level.tiles.get_mut(t1).hazard += 1;
}

pub fn main_like() -> i64 {
    with_tiles(vec![Tile { hazard: 1 }, Tile { hazard: 2 }], |tiles| {
        let t0 = tiles.check(0).unwrap();
        let t1 = tiles.check(1).unwrap();
        let mut level = Level { tiles, entities: Vec::new() };
        level.entities.push(Entity { hp: 50, standing_on: t0 });
        level.entities.push(Entity { hp: 50, standing_on: t1 });
        level.entities.push(Entity { hp: 50, standing_on: t0 });
        tick(&mut level, 2);
        tick(&mut level, 3);
        (level.entities[0].hp + level.entities[1].hp + level.entities[2].hp) as i64
    })
}
