// Plain Rust arm of valen/deep_values_across_readonly_call.rs.
//
// Arm verdict: rustc could close.
// Tier: none today (the Valen arm reloads after the call too). If S18 is
// resolved: appendix. The Valen arm is red by design, so this test is not
// evidence for any claim today.
// Bucket: A. A reference is held across the call.
//
// Spelling not used: making `value` a `Cell<i32>`. `survey` never writes the
// component, so the only use of `Cell` would be in `trade`. It compiles to this
// arm's loop: one load of `c.value` and the call per iteration. A tie, so the
// arm stays without `Cell`.
//
// Why it loses today: `c` is a local reference, and rustc marks only
// parameters. `census`'s parameter is marked read-only, but that covers only
// what is reached through that pointer itself, not what is reached through
// pointers loaded from it, and the component is two loaded pointers deep. LLVM
// must assume `census` changed `c.value`, and reloads it after every call. That
// is also what the Valen arm does today.
//
// Why rustc could close it: `c` is a `&TileComponent` held across the call.
// Writing `value` through another pointer while `c` is still in use is
// undefined behavior (Miri rejects it for this exact shape, a shared reference
// to an element two `Vec`s deep held across a call that writes the element,
// under Stacked Borrows and Tree Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a
// rustc that marked local references could tell LLVM that no reload after the
// call is needed.
//
// What holding the reference buys: if `survey` held only `&Level` and reached
// the component by index on every step, the write would be legal Rust (Miri
// accepts it for this exact shape, same document), and the verdict would be
// "never" (Bucket B). The Valen arm binds `c` once, and nothing stops this arm
// doing the same, so it does.
//
// How this arm was written: a mirror of the Valen arm. `survey` binds `c` once
// through `at`, as Valen does, and holds it across `census`; both borrows are
// shared, so the borrow checker accepts that. `trade` takes the `Vec` and two
// indices, because the borrow checker rejects two references to components
// that may be the same component. Fields and counters use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `survey`, unoptimized:
// - The loop contains a load of `c.value` and the call to `census`.
//
// Expected IR of `survey`, optimized:
// - The parameter `level` carries `noalias`.
// - Both buffer pointers and both lengths are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `c.value` and calls `census`.

use mycrate::{at, do_nothing};

pub struct TileComponent {
    pub kind: i32,
    pub value: i32,
}

pub struct Tile {
    pub components: Vec<TileComponent>,
}

pub struct Level {
    pub tiles: Vec<Tile>,
    pub clock: i32,
}

pub fn trade(components: &mut Vec<TileComponent>, a: usize, b: usize) {
    components[a].value += components[b].value;
}

#[inline(never)]
pub fn census(level: &Level) -> i32 {
    do_nothing();
    level.clock
}

#[inline(never)]
pub fn survey(level: &Level, n: i32) -> i32 {
    let c = at(&at(&level.tiles, 0).components, 1);
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        total = total + c.value + census(level);
        i += 1;
    }
    total
}

pub fn main_like() -> i64 {
    let mut level = Level {
        tiles: vec![Tile {
            components: vec![TileComponent { kind: 0, value: 1 }, TileComponent { kind: 1, value: 3 }],
        }],
        clock: 2,
    };
    trade(&mut level.tiles[0].components, 1, 0);
    trade(&mut level.tiles[0].components, 1, 1);
    survey(&level, 10) as i64
}
