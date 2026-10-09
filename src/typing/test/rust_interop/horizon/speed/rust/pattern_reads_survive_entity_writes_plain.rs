// Plain Rust arm of valen/pattern_reads_survive_entity_writes.rs.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B, of the weaker kind described below.
//
// Why it loses: the entity and the pattern row are both reached through
// pointers loaded from `level`. Rust gives LLVM no type-based alias
// information, so LLVM must assume the store to `e.hp` in `wound` may have
// changed `cost`, and `tire` loads `cost` again. That is one extra load per
// entity. Nothing else differs from the Valen arm: the `noalias` on `level`
// lets both buffer pointers and both lengths be loaded once, and `kind` is
// loaded once.
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
// - The rows and the entities are the buffers of two different `Vec`s. No
//   reference into either buffer is held from `wound` to `tire`. Under either
//   current aliasing model the compiler may not assume two `Vec`s' buffers are
//   disjoint (Miri accepts a program in which they overlap). Such a program
//   violates `Vec::from_raw_parts`' documented contract, so Rust could adopt
//   the assumption without breaking any code the standard library calls sound.
//   This is a weaker "never" than the `Vec::as_mut_ptr` case.
// Valen can state the fact because it has no unsafe code and no type punning.
// This is a win from that, and not from group borrowing.
//
// How this arm was written: a mirror of the Valen arm. `wound` and `tire` each
// index the entity with `&mut level.entities[i]` where Valen calls `at`,
// because `at` returns a shared reference, and each reads the row through `at`.
// Borrowing two fields of `level` at once is accepted inside one function.
// `heal` takes the level and two indices, because the borrow checker rejects
// two references to entities that may be the same entity. Fields use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Other spellings:
// - If `tick`, `wound` and `tire` took the rows and the entities as two slice
//   parameters, `&[Row]` and `&mut [Entity]`, both would be `noalias` and rustc
//   loads `cost` once today. That form is excluded: the Valen arm takes the
//   level, and nothing prompts a programmer who has the level to pass its
//   buffers apart.
// - `tick` could read `cost` once and pass it to both functions, or one
//   function could do both updates. Either removes the second load. Both are
//   excluded as unprompted hand-optimizations: the borrow checker accepts the
//   program as written.
// - Making `hp` and `stamina` `Cell<i32>` and taking `&Level` compiles to this
//   arm's loop, with the same second load of `cost`. A tie, so the arm stays
//   without `Cell`.
//
// Size of the win: one load of `cost` per entity, and all of it comes from the
// scopes on the store and the load, which is what this test pins.
//
// Expected IR of `tick`, unoptimized:
// - Per entity, each of `wound` and `tire` loads both buffer pointers and both
//   lengths, checks both bounds, loads `kind`, and loads `cost`. `wound` loads
//   and stores `hp`; `tire` loads and stores `stamina`.
//
// Expected IR of `tick`, optimized:
// - The parameter `level` carries `noalias`.
// - Both buffer pointers and both lengths are loaded once, before the loop.
// - Per entity there is one load of `kind` and one bounds check against each
//   length.
// - Per entity there are two loads of `cost`: one before the store to `hp`, and
//   one between the store to `hp` and the store to `stamina`.

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

pub struct Level {
    pub pattern: Pattern,
    pub entities: Vec<Entity>,
}

pub fn heal(level: &mut Level, a: usize, b: usize) {
    level.entities[a].hp += level.entities[b].hp;
}

pub fn wound(level: &mut Level, i: i64) {
    let e = &mut level.entities[i as usize];
    e.hp = e.hp - at(&level.pattern.rows, e.kind).cost;
}

pub fn tire(level: &mut Level, i: i64) {
    let e = &mut level.entities[i as usize];
    e.stamina = e.stamina - at(&level.pattern.rows, e.kind).cost;
}

#[inline(never)]
pub fn tick(level: &mut Level, n: i64) {
    let mut i: i64 = 0;
    while i < n {
        wound(level, i);
        tire(level, i);
        i += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level {
        pattern: Pattern { rows: vec![Row { cost: 3 }, Row { cost: 5 }] },
        entities: vec![Entity { hp: 10, stamina: 20, kind: 0 }, Entity { hp: 40, stamina: 30, kind: 1 }],
    };
    heal(&mut level, 0, 1);
    heal(&mut level, 1, 1);
    tick(&mut level, 2);
    let e0 = &level.entities[0];
    let e1 = &level.entities[1];
    (e0.hp + e0.stamina + e1.hp + e1.stamina) as i64
}
