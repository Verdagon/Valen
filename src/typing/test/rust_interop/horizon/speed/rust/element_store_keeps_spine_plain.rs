// Plain Rust arm of valen/element_store_keeps_spine.rs.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins). If S19 is resolved: equal.
// Bucket: none. This arm does not lose.
//
// Spelling not used: without `Cell`, `attack` takes the `Vec` of entities and
// two indices and reaches both entities' components by index on every step.
// Compiled, that loop has six loads and two bounds checks per step, where this
// arm has two loads and one bounds check. Both spellings are admissible; the
// arm uses the one with the better loop.
//
// Why it is ahead: `attack` takes `a: &Entity` and `d: &Entity`, which may be
// the same entity. An entity holds a `Vec` of components, and the `Cell` is
// inside a component, behind the `Vec`'s pointer. So `Entity` itself has no
// interior mutability, and rustc marks both parameters `noalias` and read-only.
// LLVM then knows that the store to `sword.value`, which goes through a pointer
// loaded from `a`, cannot change the pointer or length of `d.components`, which
// are reached through `d` itself. It loads them once. That is the Valen arm's
// intended shape, reached today; the Valen arm reloads both after every store,
// because `a` and `d` share a group and so neither is `noalias`.
//
// How this arm was written: a mirror of the Valen arm. `attack` binds `sword`
// once through `at` and reaches each of the defender's components through
// `at`. The write goes through `Cell::set`, because Rust cannot write through a
// shared reference otherwise. Fields use the Valen arm's widths: Valen `int` is
// `i32`, and `n` and `j` are `i64`.
//
// Expected IR of `attack`, unoptimized:
// - The loop contains a load of the pointer and of the length of
//   `d.components`, a bounds check, a load of `weight`, a load of
//   `sword.value`, and a store of `sword.value`.
//
// Expected IR of `attack`, optimized:
// - The parameters `a` and `d` carry `noalias`.
// - Both `components` pointers and lengths are loaded once, before the loop.
// - Each iteration checks the bound on `j` against the loaded length, loads
//   `sword.value`, loads `weight`, and stores `sword.value`.

use mycrate::at;
use std::cell::Cell;

pub struct Component {
    pub weight: i32,
    pub value: Cell<i32>,
}

pub struct Entity {
    pub components: Vec<Component>,
}

#[inline(never)]
pub fn attack(a: &Entity, d: &Entity, n: i64) {
    let sword = at(&a.components, 0);
    let mut j: i64 = 0;
    while j < n {
        sword.value.set(sword.value.get() - at(&d.components, j).weight);
        j += 1;
    }
}

fn component(weight: i32, value: i32) -> Component {
    Component { weight, value: Cell::new(value) }
}

pub fn main_like() -> i64 {
    let entities = vec![
        Entity { components: vec![component(2, 90), component(3, 5)] },
        Entity { components: vec![component(4, 70), component(1, 6)] },
    ];
    attack(&entities[0], &entities[1], 2);
    attack(&entities[1], &entities[1], 2);
    (entities[0].components[0].value.get() + entities[1].components[0].value.get()) as i64
}
