// Plain Rust arm of valen/two_buffers_one_owner_vectorize.rs.
//
// Verdict: equal today, apart from one overlap check before the loop.
// Tier: fence.
// Bucket: none. This arm does not lose per element.
//
// Disclosed workaround: `diffuse` could be split into a version for two
// different buffers, taking `&mut Vec<Sample>` and `&Vec<Sample>`, and an
// in-place version taking one `&mut Vec<Sample>`. That would spare `diffuse` the
// buffer selectors. It would change nothing in `scale`, the loop this test
// measures: no load more or fewer. Splitting a function into an aliasing and a
// non-aliasing version is ruled inadmissible for every arm, so this arm has one
// `diffuse`.
//
// Why: `level: &mut Level` is a reference parameter, so rustc marks it
// `noalias`. Both `Vec`s' pointers and lengths and `gain` are reached through
// `level` itself, so LLVM loads them once. The two buffers are reached through
// loaded pointers, which carry no `noalias`, so LLVM cannot tell whether they
// overlap. It checks that once before the loop (`vector.memcheck`) and runs the
// vectorized loop when they do not. The Valen arm needs no such check. That one
// check per call is the whole difference.
//
// How this arm was written: `scale` is a mirror of the Valen arm. It reads the
// light sample through `at`, as Valen does, and indexes the heat sample with
// `&mut level.heat[i]` because `at` returns a shared reference and Rust cannot
// write through one. Borrowing two fields of `level` at once is accepted inside
// one function. `diffuse` deviates: the borrow checker rejects
// `diffuse(&mut level.heat, &level.heat)`, so it takes the level and says which
// buffer each operand is, and reads the source sample before it borrows the
// destination. One `diffuse` serves both the same-buffer call and the
// cross-buffer call. Fields use the Valen arm's widths: Valen `int` is `i32`.
//
// A different spelling, `for i in 0..level.heat.len()`, compiles to the same
// shape. It is not the mirror: the Valen arm has no way to read a `Vec`'s
// length, so every arm takes the count `n` as a parameter.
//
// Taking the two buffers as slice parameters, `scale(heat: &mut [Sample],
// light: &[Sample], gain: i32)`, would make both `noalias` and remove the
// overlap check. That form is excluded: the Valen arm takes the level, and
// nothing prompts a programmer who has the level to split it at the call.
//
// Assumption behind the expected IR: LLVM folds the bounds checks into the trip
// count, as it does today.
//
// Expected IR of `scale`, unoptimized:
// - Per element, the loop loads both buffer pointers and both lengths, checks
//   both bounds, loads `gain`, loads the light sample and the heat sample, and
//   stores the heat sample.
//
// Expected IR of `scale`, optimized:
// - The parameter `level` carries `noalias`.
// - Both buffer pointers, both lengths and `gain` are loaded once, before the
//   loop.
// - There is a `vector.memcheck` block that compares the two buffers' address
//   ranges.
// - The vectorized loop body has vector operations on the heat and light
//   samples.
// - A scalar loop handles the remaining elements and keeps both bounds checks.

use mycrate::at;

pub struct Sample {
    pub v: i32,
}

pub struct Params {
    pub gain: i32,
}

pub struct Level {
    pub heat: Vec<Sample>,
    pub light: Vec<Sample>,
    pub params: Params,
}

#[derive(Clone, Copy)]
pub enum Buffer {
    Heat,
    Light,
}

impl Level {
    fn buffer(&self, which: Buffer) -> &Vec<Sample> {
        match which {
            Buffer::Heat => &self.heat,
            Buffer::Light => &self.light,
        }
    }

    fn buffer_mut(&mut self, which: Buffer) -> &mut Vec<Sample> {
        match which {
            Buffer::Heat => &mut self.heat,
            Buffer::Light => &mut self.light,
        }
    }
}

pub fn diffuse(level: &mut Level, dst: Buffer, src: Buffer, n: i64) {
    let mut i: i64 = 0;
    while i < n {
        let added = at(level.buffer(src), i).v / 8;
        let d = &mut level.buffer_mut(dst)[i as usize];
        d.v = d.v + added;
        i += 1;
    }
}

#[inline(never)]
pub fn scale(level: &mut Level, n: i64) {
    let mut i: i64 = 0;
    while i < n {
        let h = &mut level.heat[i as usize];
        h.v = h.v + at(&level.light, i).v * level.params.gain;
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level {
        heat: vec![Sample { v: 16 }, Sample { v: 32 }, Sample { v: 8 }, Sample { v: 24 }],
        light: vec![Sample { v: 8 }, Sample { v: 16 }, Sample { v: 0 }, Sample { v: 8 }],
        params: Params { gain: 2 },
    };
    diffuse(&mut level, Buffer::Heat, Buffer::Heat, 4);
    diffuse(&mut level, Buffer::Heat, Buffer::Light, 4);
    scale(&mut level, 4);
    (level.heat[0].v + level.heat[1].v + level.heat[2].v + level.heat[3].v) as i64
}
