// Plain Rust arm of valen/entity_stores_its_tile.rs.
//
// Spelling not used here: a branded index. `TileIdx<'id>` can only be made by
// checking a number against the tiles of brand `'id`, and then indexes with no
// bounds check, soundly. It is this test's third arm,
// rust/entity_stores_its_tile.rs, and it is equal to the Valen arm's intended
// shape. This arm is the one a Rust programmer writes without that technique.
// The unbranded form (a validated newtype with an unchecked access inside, and
// nothing in the types to keep the tiles from shrinking) is the trust shape,
// and is excluded as unsafe.
//
// Arm verdict: never, for this spelling. Rust as a whole is equal: see the
// branded-index arm.
// Tier: none today (the Valen arm does not compile). If Valen gains stored
// sibling references: fence; Rust already gets this with a branded index.
// Bucket: C for this spelling. A plain `usize` carries no proof that it is in
// range.
//
// Why it loses against the intended shape: an entity cannot hold `&Tile` when
// the level owns both the entities and the tiles. `Level` would then borrow
// one of its own fields, and Rust has no type for a struct that does. So the
// entity stores an index, and each use indexes `tiles`: a bounds check, where
// the Valen arm follows a pointer.
//
// Why no compiler can close it for a plain `usize`: the bounds check is what
// makes the index safe. Nothing in the types says a stored `usize` is within
// `tiles`, so the check cannot be removed. A branded index puts that fact in
// the types.
//
// Other spellings, and why each is excluded:
// - Tiles moved out of `Level` into an arena that outlives it, with entities
//   holding `&'a Tile`. That changes who owns the tiles across the whole
//   program, and the arena can never free a tile.
// - `Rc<Tile>` in each entity. `Rc` is excluded from this corpus.
//
// How this arm was written: a mirror of the Valen arm, except where Rust
// rejects it. `standing_on` is an index where Valen stores a reference; a
// reference there makes `Level` self-referential.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`, and the
// count is `i64`.
//
// Expected IR of `tick`, unoptimized:
// - The loop contains a load of the index, a bounds check against the tiles'
//   length, a load of `hazard`, and a load and a store of `hp`.
//
// Expected IR of `tick`, optimized:
// - The tiles' pointer and length are loaded once, before the loop.
// - The loop contains a load of the index, a bounds check, a load of `hazard`,
//   and a load and a store of `hp`.

pub struct Tile {
    pub hazard: i32,
}

pub struct Entity {
    pub hp: i32,
    pub standing_on: usize,
}

pub struct Level {
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn tick(level: &mut Level, count: i64) {
    let mut i: i64 = 0;
    while i < count {
        let e = &mut level.entities[i as usize];
        e.hp -= level.tiles[e.standing_on].hazard;
        i += 1;
    }
    level.tiles[0].hazard += 1;
    level.tiles[1].hazard += 1;
}

pub fn main_like() -> i64 {
    let mut level = Level { tiles: Vec::new(), entities: Vec::new() };
    level.tiles.push(Tile { hazard: 1 });
    level.tiles.push(Tile { hazard: 2 });
    level.entities.push(Entity { hp: 50, standing_on: 0 });
    level.entities.push(Entity { hp: 50, standing_on: 1 });
    level.entities.push(Entity { hp: 50, standing_on: 0 });
    tick(&mut level, 2);
    tick(&mut level, 3);
    (level.entities[0].hp + level.entities[1].hp + level.entities[2].hp) as i64
}
