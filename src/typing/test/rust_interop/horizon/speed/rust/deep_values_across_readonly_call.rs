// GhostCell arm of valen/deep_values_across_readonly_call.rs.
//
// Arm verdict: rustc could close.
// Tier: none today (the Valen arm reloads after the call too). If S18 is
// resolved: appendix. The Valen arm is red by design, so this test is not
// evidence for any claim today.
// Bucket: A. A view is held across the call.
//
// Why it loses today: `c` is a shared view of the component, held for the whole
// loop. It is a local reference, and rustc marks only parameters. LLVM must
// assume `census` changed `c.value`, and reloads it after every call. That is
// also what the Valen arm does today.
//
// Why rustc could close it: `c` is a `&TileComponent` held across the call.
// Writing `value` through another pointer while `c` is still in use is
// undefined behavior (Miri rejects it for a view from `borrow` under Stacked
// Borrows and Tree Borrows; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). So a rustc that
// marked local references could tell LLVM that no reload after the call is
// needed.
//
// How this arm was written: a mirror of the Valen arm. `survey` writes nothing,
// so it takes the token shared, binds the view once, and holds it across
// `census`. `census` reads only the clock, which is not in a cell, so it takes
// no token. `trade` reads `b` and releases it before it writes `a`, because the
// token gives one view at a time. Fields and counters use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Cell placement: one cell per component. It gives up adding or removing a
// component while a cell pointer is held; this program does neither.
//
// Placement not used: one cell per tile. `survey` then holds a view of the tile
// and reaches the component through it. Compiled, it is this arm's loop: one
// load of `c.value` and the call per iteration. A tie, so the arm stays as it
// is. The verdict, bucket and tier are the same under both placements.
//
// Brand layout: one brand. `trade` takes two components that may be the same
// component, so both must answer to one token. Nothing else is in a cell.
//
// Why `TileComponent` is in a cell: `trade` writes one component while it reads
// another that may be the same component. Without cells the borrow checker
// rejects that pair of references.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `survey`, unoptimized:
// - The loop contains a load of `c.value` and the call to `census`.
//
// Expected IR of `survey`, optimized (the plain arm's shape):
// - The parameter `level` carries `noalias`.
// - Both buffer pointers and both lengths are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `c.value` and calls `census`.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::{at, do_nothing};

pub struct TileComponent {
    pub kind: i32,
    pub value: i32,
}

pub struct Tile<'b> {
    pub components: Vec<GhostCell<'b, TileComponent>>,
}

pub struct Level<'b> {
    pub tiles: Vec<Tile<'b>>,
    pub clock: i32,
}

pub fn trade<'b>(t: &mut GhostToken<'b>, a: &GhostCell<'b, TileComponent>, b: &GhostCell<'b, TileComponent>) {
    let added = b.borrow(t).value;
    a.borrow_mut(t).value += added;
}

#[inline(never)]
pub fn census(level: &Level) -> i32 {
    do_nothing();
    level.clock
}

#[inline(never)]
pub fn survey<'b>(t: &GhostToken<'b>, level: &Level<'b>, n: i32) -> i32 {
    let c = at(&at(&level.tiles, 0).components, 1).borrow(t);
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        total = total + c.value + census(level);
        i += 1;
    }
    total
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let level = Level {
            tiles: vec![Tile {
                components: vec![
                    GhostCell::new(TileComponent { kind: 0, value: 1 }),
                    GhostCell::new(TileComponent { kind: 1, value: 3 }),
                ],
            }],
            clock: 2,
        };
        let components = &level.tiles[0].components;
        trade(&mut t, &components[1], &components[0]);
        trade(&mut t, &components[1], &components[1]);
        survey(&t, &level, 10) as i64
    })
}
