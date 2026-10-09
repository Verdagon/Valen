// GhostCell arm of valen/move_result_references_tile.rs: identical to the plain
// arm. Nothing in this program writes through an alias, so no cell and no token
// are needed.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// Why it is equal: `e: &mut Entity` is a reference parameter, so rustc marks it
// `noalias`. LLVM then knows that the `blocked_by` pointer loaded out of the
// result cannot alias the entity, and the store to `e.stamina` does not force a
// reload of `cover`.
//
// How this arm was written: a mirror of the Valen arm. The borrow checker
// rejects nothing here, so nothing deviates. Fields use the Valen arm's widths:
// Valen `int` is `i32`, and `next` is `i64`.
//
// Expected IR of `resolve`, unoptimized:
// - Two loads of the `blocked_by` pointer, two loads of `cover`, a load and a
//   store of `stamina`, a load and a store of `hp`.
//
// Expected IR of `resolve`, optimized (the same shape as the Valen arm):
// - The parameter `e` carries `noalias`.
// - One load of `cover`, used by both statements. No load of `cover` follows
//   the store to `stamina`.
// - One load and one store each of `stamina` and `hp`.

pub struct TileComponent {
    pub cover: i32,
}

pub struct Tile {
    pub components: Vec<TileComponent>,
}

pub struct Entity {
    pub hp: i32,
    pub stamina: i32,
    pub next: i64,
}

pub struct Level {
    pub tiles: Vec<Tile>,
    pub entities: Vec<Entity>,
}

pub struct MoveResult<'a> {
    pub blocked_by: &'a TileComponent,
    pub cost: i32,
}

#[inline(never)]
pub fn try_move<'a>(tiles: &'a Vec<Tile>, e: &Entity) -> MoveResult<'a> {
    let target = &tiles[e.next as usize];
    MoveResult { blocked_by: &target.components[1], cost: 2 }
}

#[inline(never)]
pub fn resolve(e: &mut Entity, r: &MoveResult) {
    e.stamina = e.stamina - r.cost - r.blocked_by.cover;
    e.hp = e.hp - r.blocked_by.cover;
}

pub fn main_like() -> i64 {
    let mut level = Level { tiles: Vec::new(), entities: Vec::new() };
    level.tiles.push(Tile { components: vec![TileComponent { cover: 0 }] });
    level.tiles.push(Tile { components: vec![TileComponent { cover: 0 }, TileComponent { cover: 3 }] });
    level.entities.push(Entity { hp: 30, stamina: 20, next: 1 });
    let e = &mut level.entities[0];
    let mut n = 0;
    while n < 2 {
        let r = try_move(&level.tiles, e);
        resolve(e, &r);
        n += 1;
    }
    (e.hp + e.stamina) as i64
}
