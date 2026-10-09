// Plain Rust arm of valen/aliased_pair_across_other_group_call.rs.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B. No reference that forbids the write is held across the call.
//
// Spelling not used: without `Cell`, `duel` takes the `Vec` of entities and two
// indices, `duel(entities: &mut Vec<Entity>, a, d, log, n)`, and reaches both
// components by index on every swing. Compiled, that loop has six loads and two
// bounds checks per swing, where this arm has two loads. Both spellings are
// admissible; the arm uses the one with the better loop.
//
// Why it loses: this arm holds `sword` and `armor` for the whole loop, as the
// Valen arm does, so it reloads no `components` pointer or length and repeats
// no bounds check. But `value` is a `Cell`, and a `Cell` may be written through
// any shared reference. LLVM must assume `record(log)` changed `sword.value`,
// and reloads it after every call. Each swing loads `sword.value` and
// `armor.value`; the Valen arm loads only `armor.value`.
//
// Size of the win: one load per swing, and all of it comes from the mark on the
// call, which is what this test pins.
//
// Why rustc cannot close it: `sword` is a shared reference to a struct with a
// `Cell` field, so holding it does not forbid a write to `value`. Under both
// current aliasing models (Stacked Borrows and Tree Borrows), unsafe code
// elsewhere may hold a pointer taken with `Cell::as_ptr` and may write `value`
// through it during the call, on every iteration, while the caller holds the
// shared reference and uses the cell around the call (Miri accepts it; results
// are recorded in notes/docs/architecture/rust-interop-design.md, Background).
// rustc must compile such a program correctly, so it cannot tell LLVM that
// `record` leaves the components alone. Valen says so because it assumes every
// imported safe Rust function is a sound API (proposal S30 in the same
// document). The difference is one of language contract, not of how clever the
// compiler is.
//
// Which `Cell` case this is: `a` and `d` are `&Entity`, and an entity holds a
// `Vec` of components. The `Cell`s are behind the `Vec`'s pointer, so `Entity`
// itself has no interior mutability and rustc marks both parameters `noalias`
// and read-only, even when they are the same entity. That is why the pointers
// and lengths are loaded once.
//
// How this arm was written: a mirror of the Valen arm. `duel` takes two
// references to entities that may be the same entity and binds `sword` and
// `armor` once through `at`. The write goes through `Cell::set`, because Rust
// cannot write through a shared reference otherwise. `record` is a mirror.
// Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `duel`, unoptimized:
// - The loop contains a load of `sword.value`, a load of `armor.value`, a store
//   of `sword.value`, and the call to `record`.
//
// Expected IR of `duel`, optimized:
// - The parameters `a` and `d` carry `noalias`.
// - Both `components` pointers and lengths are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `sword.value`, loads `armor.value`, stores
//   `sword.value`, and calls `record`.

use mycrate::{at, do_nothing};
use std::cell::Cell;

pub struct Component {
    pub kind: i32,
    pub value: Cell<i32>,
}

pub struct Entity {
    pub components: Vec<Component>,
}

pub struct Log {
    pub count: i32,
}

#[inline(never)]
pub fn record(log: &mut Log) {
    log.count += 1;
    do_nothing();
}

#[inline(never)]
pub fn duel(a: &Entity, d: &Entity, log: &mut Log, n: i32) {
    let sword = at(&a.components, 0);
    let armor = at(&d.components, 1);
    let mut i: i32 = 0;
    while i < n {
        sword.value.set(sword.value.get() - armor.value.get());
        record(log);
        i += 1;
    }
}

fn component(kind: i32, value: i32) -> Component {
    Component { kind, value: Cell::new(value) }
}

pub fn main_like() -> i64 {
    let entities = vec![
        Entity { components: vec![component(2, 60), component(1, 5)] },
        Entity { components: vec![component(2, 30), component(1, 4)] },
    ];
    let mut log = Log { count: 0 };
    duel(&entities[0], &entities[1], &mut log, 3);
    duel(&entities[1], &entities[1], &mut log, 2);
    (entities[0].components[0].value.get() + entities[1].components[0].value.get() + log.count) as i64
}
