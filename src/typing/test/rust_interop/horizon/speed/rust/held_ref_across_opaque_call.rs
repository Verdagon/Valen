// GhostCell arm of valen/held_ref_across_opaque_call.rs.
//
// Verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the call.
//
// Why it loses today: `s` is a local reference, and rustc marks only
// parameters. `s` points at a cell in the `Vec`'s buffer, which is reached
// through a pointer loaded from `ships`. LLVM must assume `do_nothing()` changed
// `s.fuel`, and reloads it after every call.
//
// Why rustc could close it: `s` is a `&mut Ship` held across the call. Writing
// `fuel` through another pointer and then using `s` again is undefined behavior
// (Miri rejects it for a view from `borrow_mut` under Stacked Borrows and Tree
// Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a rustc that
// marked local references could tell LLVM that no reload after the call is
// needed.
//
// What would make this arm never catch up (Bucket C): a callee that takes this
// cell's brand's token. `s` cannot be held across such a call, because `s`
// holds `&mut token`. The arm must then reach `fuel` through the cell each time, and a
// callee holding the token may write any cell of the brand.
//
// How this arm was written: `bump` is a mirror of the Valen arm. Valen binds `s`
// once for the whole loop, and so does this arm: `borrow_mut` is called once,
// before the loop. `do_nothing()` takes no token, so the borrow checker accepts
// that. `refuel` deviates: the token gives one view at a time, so it reads `b`
// and releases it before it writes `a`. Fields and counters use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Brand layout: one brand. `refuel` takes two ships that may be the same ship,
// so both must answer to one token. Every ship is in one collection, so no
// other layout exists.
//
// Why `Ship` is in a cell: `refuel` writes one ship while it reads another that
// may be the same ship. Without cells the borrow checker rejects that pair of
// references.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `bump`, unoptimized:
// - The loop contains a load of `fuel`, a store of `fuel`, and the call to
//   `do_nothing`.
//
// Expected IR of `bump`, optimized:
// - The `Vec`'s buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - The loop contains the call to `do_nothing`, one store of `fuel`, and one
//   load of `fuel`. The load follows the call on every iteration.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::do_nothing;

pub struct Ship {
    pub fuel: i32,
}

pub fn refuel<'b>(t: &mut GhostToken<'b>, a: &GhostCell<'b, Ship>, b: &GhostCell<'b, Ship>) {
    let added = b.borrow(t).fuel;
    a.borrow_mut(t).fuel += added;
}

#[inline(never)]
pub fn bump<'b>(t: &mut GhostToken<'b>, ships: &Vec<GhostCell<'b, Ship>>, n: i32) {
    let s = ships[0].borrow_mut(t);
    let mut i: i32 = 0;
    while i < n {
        s.fuel += 1;
        do_nothing();
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let ships = vec![GhostCell::new(Ship { fuel: 7 }), GhostCell::new(Ship { fuel: 9 })];
        refuel(&mut t, &ships[0], &ships[1]);
        refuel(&mut t, &ships[1], &ships[1]);
        bump(&mut t, &ships, 50);
        (ships[0].borrow(&t).fuel + ships[1].borrow(&t).fuel) as i64
    })
}
