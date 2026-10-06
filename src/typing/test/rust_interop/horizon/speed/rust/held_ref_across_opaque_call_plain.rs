// Plain Rust arm of valen/held_ref_across_opaque_call.rs.
//
// Verdict: rustc could close.
// Tier: appendix.
// Bucket: A. A reference is held across the call.
//
// Spelling not used: making `fuel` a `Cell<i32>` and holding a shared reference
// to the ship. It compiles to this arm's loop: load `fuel`, store `fuel`, call.
// A tie, so the arm stays without `Cell`.
//
// Why it loses today: `s` is a local reference, and rustc marks only
// parameters. `s` points into the `Vec`'s buffer, which is reached through a
// pointer loaded from `ships`, so the `noalias` on `ships` does not cover it.
// LLVM must assume `do_nothing()` changed `s.fuel`, and reloads it after every
// call.
//
// Why rustc could close it: `s` is a `&mut Ship` held across the call. Writing
// `fuel` through another pointer and then using `s` again is undefined behavior
// (Miri rejects it under Stacked Borrows and Tree Borrows; results are recorded
// in notes/docs/architecture/rust-interop-design.md, Background). So a rustc
// that marked local references could tell LLVM that no reload after the call is
// needed.
//
// How this arm was written: `bump` is a mirror of the Valen arm. Valen binds `s`
// once for the whole loop, and so does this arm. It indexes with `&mut ships[0]`
// where Valen calls `at`, because `at` returns a shared reference and Rust
// cannot write through one. `bump` takes `&mut Vec<Ship>` because the Valen arm
// takes the `Vec`. If it took a slice, `&mut [Ship]`, `s` would be derived from
// a `noalias` parameter with no load in between, and rustc keeps `fuel` in a
// register today. That form is excluded: nothing prompts a programmer who has
// the `Vec` to slice it first. `refuel` deviates: the borrow checker rejects two
// references to ships that may be the same ship, so it takes the `Vec` and two
// indices. Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `bump`, unoptimized:
// - The loop contains a load of `fuel`, a store of `fuel`, and the call to
//   `do_nothing`.
//
// Expected IR of `bump`, optimized:
// - The parameter `ships` carries `noalias`.
// - The `Vec`'s buffer pointer and length are loaded before the loop, and the
//   loop contains no bounds check.
// - The loop contains the call to `do_nothing`, one store of `fuel`, and one
//   load of `fuel`. The load follows the call on every iteration.

use mycrate::do_nothing;

pub struct Ship {
    pub fuel: i32,
}

pub fn refuel(ships: &mut Vec<Ship>, a: usize, b: usize) {
    ships[a].fuel += ships[b].fuel;
}

#[inline(never)]
pub fn bump(ships: &mut Vec<Ship>, n: i32) {
    let s = &mut ships[0];
    let mut i: i32 = 0;
    while i < n {
        s.fuel += 1;
        do_nothing();
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut ships = vec![Ship { fuel: 7 }, Ship { fuel: 9 }];
    refuel(&mut ships, 0, 1);
    refuel(&mut ships, 1, 1);
    bump(&mut ships, 50);
    (ships[0].fuel + ships[1].fuel) as i64
}
