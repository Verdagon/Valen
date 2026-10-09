// GhostCell arm of valen/pair_held_across_other_collection_call.rs.
//
// Disclosed workaround: if `swap_gear` were split into two functions, one for a
// pair drawn from one collection and one for a pair drawn from two, `entities`
// and `reserves` could have separate brands. `drill` would then take only the
// reserves token. The loop would not change: it would still load `hp` and
// `power` after every call, because cell contents may legally be written behind
// a held cell pointer. But that is the "trust" reason (Bucket B, second claim),
// not this arm's. Splitting a function into an aliasing version and a
// non-aliasing version is ruled inadmissible for every arm.
//
// Cell placement not used: a cell on each field (`hp: GhostCell<i32>`, `power:
// GhostCell<i32>`), with `Entity` itself not in a cell and `hot` taking two
// `&Entity`. Observed cost per iteration: load `power`, load `hp`, store `hp`,
// call `drill`. That is the same loop as one cell per entity, which this arm
// uses: an `&Entity` that holds cells directly is not `Freeze` and gets no
// `noalias`, and `drill` still takes the token. The two tie, so the arm keeps
// one cell per entity.
//
// Verdict: never.
// Tier: second claim (this arm alone is Bucket C; the plain arm is B). This arm
// loses for a stronger reason than the plain arm: one brand is forced, and
// `drill` takes the token. That is a weakness of GhostCell which plain Rust
// with `Cell` does not share, and the test's tier follows the plain arm.
// Bucket: C. The fact is not in the program's types.
//
// Why it loses: `drill` writes a reserve entity, so it takes `&mut token`.
// Every entity of both collections is a cell of that token's brand, so a callee
// holding the token may write any of them, `a` and `b` included. Nothing in the
// types says `drill` writes only reserves. `hot` therefore cannot hold a view
// of `a` or `b` across the call (E0502). It could not hold both views at once
// in any case, because `a` may be `b`. It reaches `hp` and `power` through the
// cells each time, and LLVM reloads both after every call.
//
// Why no compiler can close it: the token is zero-sized and its brand is a
// lifetime, so neither reaches code generation. "This call writes only cells in
// `reserves`" is not written anywhere a compiler could read it. Valen's
// signature says it: `drill` is handed a reference in group `h`, and `a` and
// `b` are in group `g`.
//
// Brand layout: one brand over `entities` and `reserves`.
// Why no better layout exists: `swap_gear(t, x, y)` is called with a pair drawn
// from one collection, which needs `x` and `y` in one brand, and with one
// entity from each collection, which then puts both collections in that brand.
// A two-brand signature would need `&mut` to one token twice for the
// same-collection pair.
//
// Why the entities are in cells: `swap_gear` and `hot` each take two entities
// that may be the same entity, and write one while reading the other.
//
// How this arm was written: a mirror of the Valen arm. `hot` and `swap_gear`
// take two cell pointers where Valen takes two references. Each use goes
// through the token, because a shared view of `b` cannot be held beside an
// exclusive view of `a` when they may be the same cell.
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

use ghost_cell::{GhostCell, GhostToken};
use mycrate::do_nothing;

pub struct Entity {
    pub hp: i32,
    pub power: i32,
}

pub struct Level<'b> {
    pub entities: Vec<GhostCell<'b, Entity>>,
    pub reserves: Vec<GhostCell<'b, Entity>>,
}

pub fn swap_gear<'b>(t: &mut GhostToken<'b>, x: &GhostCell<'b, Entity>, y: &GhostCell<'b, Entity>) {
    let tmp = x.borrow(t).power;
    x.borrow_mut(t).power = y.borrow(t).power;
    y.borrow_mut(t).power = tmp;
}

#[inline(never)]
pub fn drill<'b>(t: &mut GhostToken<'b>, reserves: &Vec<GhostCell<'b, Entity>>) {
    reserves[0].borrow_mut(t).hp += 1;
    do_nothing();
}

#[inline(never)]
pub fn hot<'b>(
    t: &mut GhostToken<'b>,
    a: &GhostCell<'b, Entity>,
    b: &GhostCell<'b, Entity>,
    reserves: &Vec<GhostCell<'b, Entity>>,
    n: i32,
) {
    let mut i: i32 = 0;
    while i < n {
        a.borrow_mut(t).hp -= b.borrow(t).power;
        drill(t, reserves);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let mut level = Level { entities: Vec::new(), reserves: Vec::new() };
        level.entities.push(GhostCell::new(Entity { hp: 40, power: 1 }));
        level.entities.push(GhostCell::new(Entity { hp: 40, power: 2 }));
        level.reserves.push(GhostCell::new(Entity { hp: 0, power: 5 }));
        level.reserves.push(GhostCell::new(Entity { hp: 0, power: 7 }));
        swap_gear(&mut t, &level.entities[0], &level.reserves[0]);
        swap_gear(&mut t, &level.entities[0], &level.entities[1]);
        swap_gear(&mut t, &level.entities[1], &level.entities[1]);
        hot(&mut t, &level.entities[0], &level.entities[1], &level.reserves, 3);
        hot(&mut t, &level.entities[1], &level.entities[1], &level.reserves, 4);
        let e0 = level.entities[0].borrow(&t);
        let e1 = level.entities[1].borrow(&t);
        let r0 = level.reserves[0].borrow(&t);
        (e0.hp + e1.hp + r0.hp + e0.power + r0.power) as i64
    })
}
