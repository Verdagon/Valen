// GhostCell arm of valen/caller_ref_across_mutating_callee.rs. No cell is
// needed: nothing in this program takes two entities that may be the same. So
// this arm is the plain arm, and its code is the plain arm's code.
//
// Arm verdict: equal today.
// Tier: fence.
// Bucket: none. This arm does not lose.
//
// See rust/caller_ref_across_mutating_callee_plain.rs for the reasoning and the
// expected IR. They are the same here.

use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
}

pub struct Level {
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn strike(e: &mut Entity) {
    e.hp -= 1;
    do_nothing();
}

#[inline(never)]
pub fn siege(level: &mut Level, i: i64, n: i32) -> i32 {
    let e = &mut level.entities[i as usize];
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
    let mut level = Level { entities: vec![Entity { hp: 40 }, Entity { hp: 20 }] };
    let first = siege(&mut level, 0, 3);
    let second = siege(&mut level, 1, 2);
    (first + second) as i64
}
