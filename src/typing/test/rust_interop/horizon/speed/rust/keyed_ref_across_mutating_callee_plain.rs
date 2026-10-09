// Plain Rust arm of valen/keyed_ref_across_mutating_callee.rs.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins).
// Bucket: none. This arm does not lose.
//
// Why it is ahead: `siege` looks the entity up once, before the loop, and lends
// the reference to `strike` on each iteration. The loan ends when `strike`
// returns, and `e` is usable again. The Valen arm cannot do that: its checker
// ends the borrow at the call ("Used a borrow after invalidated"), and it looks
// the entity up again twice per iteration.
//
// What that is worth in compiled code: at least one table lookup per
// iteration. Written the Valen arm's way, with a `get_mut` for `strike` and a
// `get` for the read on every iteration, this function hashes the key once
// before the loop, since the `noalias` on `level` lets LLVM hoist that, but
// keeps both probes in the loop: each loads a group of control bytes and a
// stored key and compares them. Written with one `get_mut` per iteration, after
// `strike`, the lookup is not inlined, and each iteration makes one call into
// the map, hashing included. This arm's loop has the call to `strike` and one
// load of `hp`.
//
// How this arm was written: the plainest form of the program. It is not a
// mirror: the Valen arm's lookups in the loop are forced by Valen's checker,
// and nothing in Rust forces them. It uses `get_mut` where Valen calls `get`,
// because Rust cannot write through the shared reference `get` returns. Fields
// and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `siege`, unoptimized:
// - One lookup before the loop. Each iteration calls `strike` and loads `hp`.
//
// Expected IR of `siege`, optimized:
// - The parameter `level` carries `noalias`.
// - The key is hashed and the table probed once, before the loop.
// - Each iteration calls `strike` and then loads `hp`. The loop contains no
//   hashing and no probe of the table.

use mycrate::do_nothing;
use std::collections::HashMap;

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub entities: HashMap<i32, Entity>,
}

#[inline(never)]
pub fn strike(e: &mut Entity) {
    e.hp -= 1;
    do_nothing();
}

#[inline(never)]
pub fn siege(level: &mut Level, key: i32, n: i32) -> i32 {
    let e = level.entities.get_mut(&key).unwrap();
    let mut total: i32 = 0;
    let mut k: i32 = 0;
    while k < n {
        strike(e);
        total += e.hp;
        k += 1;
    }
    total
}

pub fn main_like() -> i64 {
    let mut level = Level { entities: HashMap::new() };
    level.entities.insert(1, Entity { hp: 40 });
    level.entities.insert(2, Entity { hp: 20 });
    let first = siege(&mut level, 1, 3);
    let second = siege(&mut level, 2, 2);
    (first + second) as i64
}
