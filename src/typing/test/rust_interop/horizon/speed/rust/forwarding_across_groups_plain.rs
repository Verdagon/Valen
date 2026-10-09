// Plain Rust arm of valen/forwarding_across_groups.rs.
//
// Arm verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the store.
//
// Spellings not used:
// - Indexing `level.entities[i]` again for the final read. It compiles to this
//   arm's code today, and it holds nothing across the tile store, so its
//   verdict would be "never" (Bucket B, of the weaker kind: Rust has no
//   type-based alias rule, and may not assume two `Vec`s' buffers are
//   disjoint). Nothing forces it: the borrow checker accepts both references
//   held together, since they borrow different fields of `level`.
// - Making `hp` and `hazard` `Cell<i32>` and taking `&Level`. It compiles to
//   this arm's code today, with the same second load of `hp`. A tie, so the arm
//   stays without `Cell`.
//
// Why it loses today: `e` and `t` are local references into two buffers, each
// reached through a pointer loaded from `level`. rustc marks only parameters,
// so LLVM must assume the store to `t.hazard` may have changed `e.hp`, and
// loads `hp` again for the return value. That is one load per call.
//
// Why rustc could close it: `e` is a `&mut Entity` held across the store.
// Under Rust's rules nothing else may write that entity while `e` is in use, so
// a rustc that marked local references could tell LLVM that the store through
// `t` leaves `e.hp` alone.
//
// How this arm was written: a mirror of the Valen arm. It indexes with `&mut`
// where Valen calls `at`, because `at` returns a shared reference and Rust
// cannot write through one. `heal` takes the level and two indices, because the
// borrow checker rejects two references to entities that may be the same
// entity. Fields use the Valen arm's widths: Valen `int` is `i32`.
//
// Another spelling: if `scorch` took the tiles and the entities as two slice
// parameters, `&mut [Tile]` and `&mut [Entity]`, both would be `noalias` and
// the load would go today. That form is excluded: the Valen arm takes the
// level, and nothing prompts a programmer who has the level to pass its
// buffers apart.
//
// Size of the win today: one load of `hp` per call of `scorch`, from the scopes
// on the store and the load.
//
// Expected IR of `scorch`, unoptimized:
// - Two bounds checks, a load and a store of `hp`, then a load and a store of
//   `hazard`, then a second load of `hp`, which is returned.
//
// Expected IR of `scorch`, optimized:
// - The parameter `level` carries `noalias`.
// - A load and a store of `hp`, then a load and a store of `hazard`, then a
//   second load of `hp`, which is returned.

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

pub fn heal(level: &mut Level, a: usize, b: usize) {
    level.entities[a].hp += level.entities[b].hp;
}

#[inline(never)]
pub fn scorch(level: &mut Level, i: i64, j: i64) -> i32 {
    let e = &mut level.entities[i as usize];
    let t = &mut level.tiles[j as usize];
    e.hp = e.hp - 3;
    t.hazard = t.hazard + 1;
    e.hp
}

pub fn main_like() -> i64 {
    let mut level = Level {
        tiles: vec![Tile { hazard: 2 }, Tile { hazard: 4 }],
        entities: vec![Entity { hp: 10 }, Entity { hp: 40 }],
    };
    heal(&mut level, 0, 1);
    heal(&mut level, 1, 1);
    let first = scorch(&mut level, 0, 1);
    let second = scorch(&mut level, 1, 1);
    (first + second + level.tiles[1].hazard) as i64
}
