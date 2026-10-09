// GhostCell arm of valen/two_buffers_one_owner_vectorize.rs.
//
// Verdict: equal today, apart from one overlap check before the loop.
// Tier: fence.
// Bucket: none. This arm does not lose per element.
//
// Disclosed workaround: `diffuse` could be split into a version for two
// different buffers and an in-place version for one. The two buffers could then
// have separate brands, and `scale` could hold a view of each at once. It
// would change nothing in `scale`: with one cell per sample this arm already
// vectorizes behind one overlap check, so the split would remove no load, by
// the same reasoning. (The two-brand `scale` was not compiled.)
// Splitting a function into an aliasing and a non-aliasing version is ruled
// inadmissible for every arm, so this arm has one `diffuse` and one brand.
//
// Why: with one cell per sample, `Level` holds two plain `Vec`s of cells. A
// `Vec` keeps its elements behind a pointer, so `&Level` itself contains no
// `UnsafeCell` and rustc marks the parameter `noalias`. LLVM then loads both
// buffer pointers, both lengths and `gain` once. It cannot tell whether the two
// buffers overlap, so it checks that once before the loop (`vector.memcheck`)
// and runs the vectorized loop when they do not. The Valen arm needs no such
// check. That one check per call is the whole difference.
//
// How this arm was written: `scale` is a mirror of the Valen arm. It reaches
// each sample through `at`, as Valen does. The token gives one view at a time,
// so it reads the light sample and releases it before it takes the mutable view
// of the heat sample. `diffuse` does the same. Fields use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Brand layout: one brand. `diffuse` is called with (heat, heat) and with
// (heat, light), so samples of both buffers must answer to one token.
//
// Cell layout: one cell per sample. This is the best layout for GhostCell. A
// sample is the element of its `Vec`, so this is a cell per element, not a cell
// on each field below the element. The
// other layout puts each `Vec` in a cell (`heat: GhostCell<Vec<Sample>>`). Then
// `&Level` contains an `UnsafeCell` and gets no `noalias`, a store to a heat
// sample may change either `Vec`'s pointer and length, and the loop stays scalar
// and reloads both pointers, both lengths and `gain` for every element. A
// struct that holds a `GhostCell` directly loses `noalias`; a struct that holds
// a `Vec` of cells keeps it. The per-sample layout is at least as natural and is
// faster, so it is the one used here.
//
// What the per-sample layout gives up: a `Vec` cannot grow or shrink while a
// pointer to one of its cells is held. This program never changes a buffer's
// length.
//
// Why `Sample` is in a cell: `diffuse` writes samples of one buffer while it
// reads samples of a buffer that may be the same one. Without cells the borrow
// checker rejects that pair of references.
//
// Assumption behind the expected IR: LLVM folds the bounds checks that `at`
// makes into the trip count, as it does today.
//
// Expected IR of `scale`, unoptimized:
// - Per element, the loop loads both buffer pointers and both lengths, checks
//   both bounds, loads `gain`, loads the light sample and the heat sample, and
//   stores the heat sample.
//
// Expected IR of `scale`, optimized (the plain arm's shape):
// - The parameter `level` carries `noalias`.
// - Both buffer pointers, both lengths and `gain` are loaded once, before the
//   loop.
// - There is a `vector.memcheck` block that compares the two buffers' address
//   ranges.
// - The vectorized loop body has vector operations on the heat and light
//   samples.
// - A scalar loop handles the remaining elements and keeps both bounds checks.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::at;

pub struct Sample {
    pub v: i32,
}

pub struct Params {
    pub gain: i32,
}

pub struct Level<'b> {
    pub heat: Vec<GhostCell<'b, Sample>>,
    pub light: Vec<GhostCell<'b, Sample>>,
    pub params: Params,
}

pub fn diffuse<'b>(
    t: &mut GhostToken<'b>,
    dst: &Vec<GhostCell<'b, Sample>>,
    src: &Vec<GhostCell<'b, Sample>>,
    n: i64,
) {
    let mut i: i64 = 0;
    while i < n {
        let x = at(src, i).borrow(t).v;
        let d = at(dst, i).borrow_mut(t);
        d.v = d.v + x / 8;
        i += 1;
    }
}

#[inline(never)]
pub fn scale<'b>(t: &mut GhostToken<'b>, level: &Level<'b>, n: i64) {
    let mut i: i64 = 0;
    while i < n {
        let x = at(&level.light, i).borrow(t).v;
        let h = at(&level.heat, i).borrow_mut(t);
        h.v = h.v + x * level.params.gain;
        i += 1;
    }
}

fn samples<'b>(values: [i32; 4]) -> Vec<GhostCell<'b, Sample>> {
    values.into_iter().map(|v| GhostCell::new(Sample { v })).collect()
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let level = Level {
            heat: samples([16, 32, 8, 24]),
            light: samples([8, 16, 0, 8]),
            params: Params { gain: 2 },
        };
        diffuse(&mut t, &level.heat, &level.heat, 4);
        diffuse(&mut t, &level.heat, &level.light, 4);
        scale(&mut t, &level, 4);
        let heat = &level.heat;
        (heat[0].borrow(&t).v + heat[1].borrow(&t).v + heat[2].borrow(&t).v + heat[3].borrow(&t).v) as i64
    })
}
