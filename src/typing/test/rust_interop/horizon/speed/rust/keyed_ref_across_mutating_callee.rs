// GhostCell arm of valen/keyed_ref_across_mutating_callee.rs. No cell is
// needed: nothing in this program takes two entities that may be the same. So
// this arm is the plain arm, and its code is the plain arm's code.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins).
// Bucket: none. This arm does not lose.
//
// See rust/keyed_ref_across_mutating_callee_plain.rs for the reasoning and the
// expected IR. They are the same here.

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
