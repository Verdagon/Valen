// GhostCell arm of valen/pattern_reads_survive_entity_writes.rs.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B, of the weaker kind described below.
//
// Why it loses: the entity cell and the pattern row are both reached through
// pointers loaded from `level`. Rust gives LLVM no type-based alias
// information, so LLVM must assume the store to `e.hp` in `wound` may have
// changed `cost`, and `tire` loads `cost` again. That is one extra load per
// entity, the same as the plain arm. Being in a cell costs this loop nothing
// more: `Level` holds a plain `Vec` of cells, so `&Level` keeps `noalias`, and
// both buffer pointers and both lengths are loaded once.
//
// Why rustc cannot close it: the loop has no call, so this is not about what a
// callee may do. Two facts would each let a compiler skip the second load. Rust
// has both and relies on neither.
// - `cost` is a field of a `Row` and `hp` is a field of an `Entity`. A C
//   compiler concludes from the struct types that a store to one cannot change
//   the other (struct-path type-based alias analysis; both fields are `i32`, so
//   the scalar type alone would not tell them apart). C compilers do this by
//   default: clang 17 at -O2 loads `cost` once per entity for the C equivalent
//   of this loop, and with -fno-strict-aliasing the second load returns. That
//   is one compiler, and much real C is built with that flag. Rust has no
//   type-based alias rule, because unsafe code may view the
//   same memory as two types.
// - The rows and the entity cells are the buffers of two different `Vec`s. No
//   reference into the row buffer is held from `wound` to `tire`. Under either
//   current aliasing model the compiler may not assume two `Vec`s' buffers are
//   disjoint (Miri accepts a program in which they overlap). Such a program
//   violates `Vec::from_raw_parts`' documented contract, so Rust could adopt
//   the assumption without breaking any code the standard library calls sound.
//   This is a weaker "never" than the `Vec::as_mut_ptr` case.
// Valen can state the fact because it has no unsafe code and no type punning.
// This is a win from that, and not from group borrowing.
//
// Why this is no more than second claim: the loss does not come from the token. The plain
// arm, which has no cells, loses the same load for the same reason.
//
// How this arm was written: a mirror of the Valen arm. `wound` and `tire` each
// reach the entity cell through `at`, take a mutable view, and read the row
// through `at`. The pattern is not in a cell, so reading it needs no token and
// the view can stay alive. `heal` reads `b` and releases it before it writes
// `a`, because the token gives one view at a time. Fields use the Valen arm's
// widths: Valen `int` is `i32`.
//
// Other spellings: `tick` could read `cost` once and pass it to both
// functions, or one function could do both updates. Both are excluded as
// unprompted hand-optimizations.
//
// Cell layout: one cell per entity. The entity is the element of its `Vec`, and
// it has nothing below it to put cells on, so there is one placement. `Pattern`
// and its rows are plain: nothing writes them through an alias.
//
// Brand layout: one brand. `heal` takes two entities that may be the same
// entity, so both must answer to one token. Every entity is in one collection,
// so no other layout exists.
//
// Why `Entity` is in a cell: `heal` writes one entity while it reads another
// that may be the same entity. Without cells the borrow checker rejects that
// pair of references.
//
// Size of the win: one load of `cost` per entity, and all of it comes from the
// scopes on the store and the load, which is what this test pins.
//
// Expected IR of `tick`, unoptimized:
// - Per entity, each of `wound` and `tire` loads both buffer pointers and both
//   lengths, checks both bounds, loads `kind`, and loads `cost`. `wound` loads
//   and stores `hp`; `tire` loads and stores `stamina`.
//
// Expected IR of `tick`, optimized (the plain arm's shape):
// - The parameter `level` carries `noalias`.
// - Both buffer pointers and both lengths are loaded once, before the loop.
// - Per entity there is one load of `kind` and one bounds check against each
//   length.
// - Per entity there are two loads of `cost`: one before the store to `hp`, and
//   one between the store to `hp` and the store to `stamina`.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::at;

pub struct Row {
    pub cost: i32,
}

pub struct Pattern {
    pub rows: Vec<Row>,
}

pub struct Entity {
    pub hp: i32,
    pub stamina: i32,
    pub kind: i64,
}

pub struct Level<'b> {
    pub pattern: Pattern,
    pub entities: Vec<GhostCell<'b, Entity>>,
}

pub fn heal<'b>(t: &mut GhostToken<'b>, a: &GhostCell<'b, Entity>, b: &GhostCell<'b, Entity>) {
    let amount = b.borrow(t).hp;
    a.borrow_mut(t).hp += amount;
}

pub fn wound<'b>(t: &mut GhostToken<'b>, level: &Level<'b>, i: i64) {
    let e = at(&level.entities, i).borrow_mut(t);
    e.hp = e.hp - at(&level.pattern.rows, e.kind).cost;
}

pub fn tire<'b>(t: &mut GhostToken<'b>, level: &Level<'b>, i: i64) {
    let e = at(&level.entities, i).borrow_mut(t);
    e.stamina = e.stamina - at(&level.pattern.rows, e.kind).cost;
}

#[inline(never)]
pub fn tick<'b>(t: &mut GhostToken<'b>, level: &Level<'b>, n: i64) {
    let mut i: i64 = 0;
    while i < n {
        wound(t, level, i);
        tire(t, level, i);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let level = Level {
            pattern: Pattern { rows: vec![Row { cost: 3 }, Row { cost: 5 }] },
            entities: vec![
                GhostCell::new(Entity { hp: 10, stamina: 20, kind: 0 }),
                GhostCell::new(Entity { hp: 40, stamina: 30, kind: 1 }),
            ],
        };
        heal(&mut t, &level.entities[0], &level.entities[1]);
        heal(&mut t, &level.entities[1], &level.entities[1]);
        tick(&mut t, &level, 2);
        let e0 = level.entities[0].borrow(&t);
        let e1 = level.entities[1].borrow(&t);
        (e0.hp + e0.stamina + e1.hp + e1.stamina) as i64
    })
}
