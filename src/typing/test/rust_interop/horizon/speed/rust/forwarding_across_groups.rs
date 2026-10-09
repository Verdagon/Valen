// GhostCell arm of valen/forwarding_across_groups.rs.
//
// Arm verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A view is held across the store.
//
// Spelling not used: taking a fresh view of the entity for the final read. It
// compiles to this arm's code today, and it holds no view across the tile
// store, so its verdict would be "never" (Bucket B, of the weaker kind).
// Nothing forces it: the borrow checker accepts the view held beside the tile
// reference, since they borrow different fields of `level`.
//
// Why it loses today: `e` is a mutable view of the entity and `tile` is a
// reference to a tile, each reached through a pointer loaded from `level`.
// Both are local references, and rustc marks only parameters. LLVM must assume
// the store to `tile.hazard` may have changed `e.hp`, and loads `hp` again for
// the return value. That is one load per call, the same as the plain arm.
//
// Why rustc could close it: `e` is a `&mut Entity` held across the store.
// Under Rust's rules nothing else may write that entity while `e` is in use, so
// a rustc that marked local references could tell LLVM that the store through
// `tile` leaves `e.hp` alone.
//
// How this arm was written: a mirror of the Valen arm. It reaches the entity
// cell through `at` and takes a mutable view. Tiles are not in cells, so the
// tile is indexed with `&mut`, which borrows a different field of `level` from
// the one the view borrows. `heal` reads `b` and releases it before it writes
// `a`, because the token gives one view at a time. Fields use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Cell placement: one cell per entity. The entity has nothing below it but one
// field, so there is no other placement to compare. Tiles are plain: nothing
// writes them through an alias.
//
// Brand layout: one brand. `heal` takes two entities that may be the same
// entity, so both must answer to one token. Every entity is in one collection,
// so no other layout exists.
//
// Why `Entity` is in a cell: `heal` writes one entity while it reads another
// that may be the same entity. Without cells the borrow checker rejects that
// pair of references.
//
// Size of the win today: one load of `hp` per call of `scorch`, from the scopes
// on the store and the load.
//
// Expected IR of `scorch`, unoptimized:
// - Two bounds checks, a load and a store of `hp`, then a load and a store of
//   `hazard`, then a second load of `hp`, which is returned.
//
// Expected IR of `scorch`, optimized (the plain arm's shape):
// - The parameter `level` carries `noalias`.
// - A load and a store of `hp`, then a load and a store of `hazard`, then a
//   second load of `hp`, which is returned.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::at;

pub struct Tile {
    pub hazard: i32,
}

pub struct Entity {
    pub hp: i32,
}

pub struct Level<'b> {
    pub tiles: Vec<Tile>,
    pub entities: Vec<GhostCell<'b, Entity>>,
}

pub fn heal<'b>(t: &mut GhostToken<'b>, a: &GhostCell<'b, Entity>, b: &GhostCell<'b, Entity>) {
    let amount = b.borrow(t).hp;
    a.borrow_mut(t).hp += amount;
}

#[inline(never)]
pub fn scorch<'b>(t: &mut GhostToken<'b>, level: &mut Level<'b>, i: i64, j: i64) -> i32 {
    let e = at(&level.entities, i).borrow_mut(t);
    let tile = &mut level.tiles[j as usize];
    e.hp = e.hp - 3;
    tile.hazard = tile.hazard + 1;
    e.hp
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let mut level = Level {
            tiles: vec![Tile { hazard: 2 }, Tile { hazard: 4 }],
            entities: vec![GhostCell::new(Entity { hp: 10 }), GhostCell::new(Entity { hp: 40 })],
        };
        heal(&mut t, &level.entities[0], &level.entities[1]);
        heal(&mut t, &level.entities[1], &level.entities[1]);
        let first = scorch(&mut t, &mut level, 0, 1);
        let second = scorch(&mut t, &mut level, 1, 1);
        (first + second + level.tiles[1].hazard) as i64
    })
}
