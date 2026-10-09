// Plain Rust arm of valen/stamina_vs_components.rs.
//
// Under the current design all three arms compile to the same loop; this test
// shows no win today.
//
// Spelling not used: a `Vec` and two indices,
// `attack(entities: &mut Vec<Entity>, a: usize, d: usize, count: i64)`,
// re-indexing in the loop. Observed cost per iteration: the same six operations
// as this arm (load the components length, bounds check, load the components
// pointer, load `value`, load `stamina`, store `stamina`). This arm gives
// `stamina` a `Cell` type instead because it then holds two references, as the
// Valen arm does; re-indexing is for when nothing can be held. No `Cell`
// spelling exists for the components: they are not `Copy`, and this program
// only reads them.
//
// Verdict: never.
// Tier: none today (all three arms compile to the same loop). If field-level
// scopes land: second claim ("trust"). The verdict is against the Valen arm's
// intended shape, not what Valen emits today.
// Bucket: B. Rust has the fact (the two fields have different types and
// offsets) and may not rely on it, by a choice about unsafe code.
//
// Why it loses against the intended shape: `stamina` is a `Cell<i32>`, so
// `attack` holds two shared references that may be the same entity. But
// `Entity` holds that `Cell` directly, so `&Entity` is not `Freeze` and gets no
// `noalias`, and Rust has no type-based alias information. So each store to
// `stamina` may change the components pointer and length: both reload, the
// bounds check repeats, and `stamina` is reloaded too.
//
// Why rustc can never close it: the loop makes no call, so nothing here depends
// on what a callee may do. Every reload is forced by the arm's own store to
// `stamina`. To remove them, rustc would have to tell LLVM two things:
// - an `i32` field and a pointer or length field of same-typed elements never
//   overlap. C compilers do this by default through type-based alias analysis:
//   clang -O2 keeps `stamina` in a register and loads the components pointer
//   once, and with -fno-strict-aliasing the reloads return (one compiler and
//   version probed; much real C is built with that flag). Rust has no such
//   rule, because unsafe Rust may view the same memory as two types.
// - one `Vec`'s buffer does not overlap another's. Under either current
//   aliasing model the compiler may not assume that (Miri accepts a program in
//   which they overlap). Such a program violates `Vec::from_raw_parts`'
//   documented contract, so Rust could adopt the assumption without breaking
//   any code the standard library calls sound. This is a weaker "never" than
//   the `Vec::as_mut_ptr` case.
//
// Why Valen may: Valen has no unsafe code, so nothing can reinterpret an
// entity's memory as another type, and it assumes `Vec` and `at`, as sound
// APIs, hand back a pointer into a buffer of the `Vec`'s own (proposal S30 in
// rust-interop-design.md). This win would come from those two facts, not from
// group borrowing.
//
// Another spelling, excluded: `entities: &mut [Entity]` with two indices is
// equal to the intended shape today. The elements are then reached from a
// `noalias` parameter with no load in between, and rustc promotes `stamina` and
// vectorizes the loop. It is excluded because the Valen arm's caller holds a
// `Vec`, and slicing before the call is a hand-optimization nobody would be
// prompted to make.
//
// How this arm was written: a mirror of the Valen arm. `attack` takes two
// references to entities, as Valen does. They are shared references, and
// `stamina` is a `Cell`, because `&mut Entity` beside `&Entity` is rejected
// when both may name one entity (E0502).
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`, and the
// index and count are `i64`.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of the components length, a bounds check, a load
//   of the components pointer, a load of `value`, a load of `stamina`, and a
//   store of `stamina`.
//
// Expected IR of `attack`, optimized:
// - The parameters `attacker` and `defender` carry no `noalias`.
// - The loop contains a load of the components length, a bounds check, a load
//   of the components pointer, a load of `value`, a load of `stamina`, and a
//   store of `stamina`.

use std::cell::Cell;

pub struct EntityComponent {
    pub value: i32,
}

pub struct Entity {
    pub stamina: Cell<i32>,
    pub components: Vec<EntityComponent>,
}

pub struct Level {
    pub entities: Vec<Entity>,
}

#[inline(never)]
pub fn attack(attacker: &Entity, defender: &Entity, count: i64) {
    let mut j: i64 = 0;
    while j < count {
        attacker.stamina.set(attacker.stamina.get() - defender.components[j as usize].value);
        j += 1;
    }
}

pub fn main_like() -> i64 {
    let mut level = Level { entities: Vec::new() };
    level.entities.push(Entity {
        stamina: Cell::new(50),
        components: vec![EntityComponent { value: 1 }, EntityComponent { value: 2 }, EntityComponent { value: 3 }],
    });
    level.entities.push(Entity {
        stamina: Cell::new(50),
        components: vec![EntityComponent { value: 4 }, EntityComponent { value: 5 }, EntityComponent { value: 6 }],
    });
    attack(&level.entities[0], &level.entities[1], 2);
    attack(&level.entities[1], &level.entities[1], 3);
    (level.entities[0].stamina.get() + level.entities[1].stamina.get()) as i64
}
