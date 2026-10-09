// Plain Rust arm of valen/pair_held_across_other_collection_call.rs.
//
// Disclosed workaround: `swap_gear` could be split into an aliasing version and
// a non-aliasing version. With `Cell` fields that would change nothing: one
// `swap_gear(x: &Entity, y: &Entity)` already serves every caller. Splitting a
// function that way is ruled inadmissible for every arm.
//
// Spelling not used: a `Vec` and two indices,
// `hot(entities: &mut Vec<Entity>, a: usize, b: usize, reserves: &mut
// Vec<Entity>, n)`, re-indexing in the loop. Observed cost per iteration: load
// `power`, load `hp`, store `hp`, call `drill`, with the element addresses and
// bounds checks before the loop. That is the same loop as this arm. This arm
// uses `Cell` fields instead because it then holds two references, as the
// Valen arm does; re-indexing is for when nothing can be held.
// Where that spelling would be ahead: without the call, it would beat the
// Valen arm. LLVM knows `entities[a].hp` and `entities[b].power` never overlap
// (one buffer, different field offsets), while Valen reloads `power` after the
// `hp` store (see valen/aliased_pair_two_fields.rs). This arm, with two
// `&Entity` pointers, would reload `power` after the `hp` store exactly as the
// Valen arm does.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B. References are held, but they are shared references to `Cell`
// fields, so they promise nothing about the contents.
//
// Why it loses today: `hp` and `power` are `Cell<i32>`, so `a` and `b` can be
// two shared references that may be the same entity, held for the whole loop.
// But `Entity` holds its `Cell`s directly, so `&Entity` is not `Freeze` and
// gets no `noalias`. LLVM must assume `drill(reserves)` changed the entities,
// and reloads `hp` and `power` after every call.
//
// Why rustc can never close it: a `Cell`'s contents may legally be written
// through any shared pointer to it. Miri accepts, under Stacked Borrows and
// Tree Borrows, a program in which a caller holds `&T` to a struct of `Cell`s
// for a whole loop while a no-argument call writes one of them through a
// pointer saved from `Cell::as_ptr`. So a held `&Entity` gives the compiler no
// fact about `hp` or `power` across a call. Closing this would mean changing
// what `UnsafeCell` means.
//
// Why Valen may: `drill` is handed only `reserves`, and safe code cannot reach
// an entity from there. Valen has no unsafe code and assumes every imported
// safe Rust function is a sound API (proposal S30 in rust-interop-design.md).
// This win comes from that assumption, not from group borrowing.
//
// Another spelling, equal today, and excluded: `entities: &mut [Entity]` with
// two indices. The elements are then reached from a `noalias` parameter with no
// load in between, and rustc keeps `hp` and `power` in registers across
// `drill`. It is excluded because the Valen arm's caller holds a `Vec`, and
// slicing before the call is a hand-optimization nobody would be prompted to
// make.
//
// How this arm was written: a mirror of the Valen arm. `hot` and `swap_gear`
// take two references to entities, as Valen does. They are shared references,
// and the fields are `Cell`s, because `&mut Entity` beside `&Entity` is
// rejected when both may name one entity (E0502).
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `hot`, unoptimized:
// - The loop contains a load of `power`, a load of `hp`, a store of `hp`, and
//   the call to `drill`.
//
// Expected IR of `hot`, optimized:
// - The parameters `a` and `b` carry no `noalias`.
// - The loop contains the call to `drill` and one store of `hp`.
// - The loop contains one load of `power` and one load of `hp`. Both follow the
//   call to `drill` on every iteration.

use mycrate::do_nothing;
use std::cell::Cell;

pub struct Entity {
    pub hp: Cell<i32>,
    pub power: Cell<i32>,
}

pub struct Level {
    pub entities: Vec<Entity>,
    pub reserves: Vec<Entity>,
}

pub fn swap_gear(x: &Entity, y: &Entity) {
    let tmp = x.power.get();
    x.power.set(y.power.get());
    y.power.set(tmp);
}

#[inline(never)]
pub fn drill(reserves: &mut Vec<Entity>) {
    reserves[0].hp.set(reserves[0].hp.get() + 1);
    do_nothing();
}

#[inline(never)]
pub fn hot(a: &Entity, b: &Entity, reserves: &mut Vec<Entity>, n: i32) {
    let mut i: i32 = 0;
    while i < n {
        a.hp.set(a.hp.get() - b.power.get());
        drill(reserves);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level { entities: Vec::new(), reserves: Vec::new() };
    level.entities.push(Entity { hp: Cell::new(40), power: Cell::new(1) });
    level.entities.push(Entity { hp: Cell::new(40), power: Cell::new(2) });
    level.reserves.push(Entity { hp: Cell::new(0), power: Cell::new(5) });
    level.reserves.push(Entity { hp: Cell::new(0), power: Cell::new(7) });
    swap_gear(&level.entities[0], &level.reserves[0]);
    swap_gear(&level.entities[0], &level.entities[1]);
    swap_gear(&level.entities[1], &level.entities[1]);
    hot(&level.entities[0], &level.entities[1], &mut level.reserves, 3);
    hot(&level.entities[1], &level.entities[1], &mut level.reserves, 4);
    let e0 = &level.entities[0];
    let e1 = &level.entities[1];
    let r0 = &level.reserves[0];
    (e0.hp.get() + e1.hp.get() + r0.hp.get() + e0.power.get() + r0.power.get()) as i64
}
