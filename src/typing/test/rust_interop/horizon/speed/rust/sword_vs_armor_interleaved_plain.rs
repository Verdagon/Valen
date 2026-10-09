// Plain Rust arm of valen/sword_vs_armor_interleaved.rs.
//
// Arm verdict: equal today.
// Tier: fence (control).
// Bucket: none. This arm does not lose.
//
// Spelling not used: without `Cell`, `duel` takes the `Vec` of entities and two
// indices and reaches both components by index on every swing. Compiled, that
// loop has eight loads and three bounds checks per swing, where this arm has
// two loads. The cell-free spelling can hold nothing, since the two components
// may belong to one entity, so the arm uses the `Cell` spelling, which holds
// both references.
//
// Why it is equal: `sword` and `armor` may be the same component as far as the
// compiler can tell, so each store forces the other value to be reloaded. That
// is true in this arm and in the Valen arm alike. Each swing loads
// `sword.value` and stores it, then loads `armor.value` and stores it.
//
// Which `Cell` case this is: `attacker` and `defender` are `&Entity`, and an
// entity holds a `Vec` of components. The `Cell`s are behind the `Vec`'s
// pointer, so `Entity` itself has no interior mutability and rustc marks both
// parameters `noalias` and read-only, even when they are the same entity. That
// is why both `components` pointers and lengths are loaded once.
//
// How this arm was written: a mirror of the Valen arm. `duel` takes two
// references to entities that may be the same entity and binds `sword` and
// `armor` once through `at`. Writes go through `Cell::set`, because Rust cannot
// write through a shared reference otherwise. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Expected IR of `duel`, unoptimized:
// - The loop contains a load of `armor.value`, a load and a store of
//   `sword.value`, and a load and a store of `armor.value`.
//
// Expected IR of `duel`, optimized (the same loop as the Valen arm):
// - The parameters `attacker` and `defender` carry `noalias`.
// - Both `components` pointers and lengths are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `sword.value` and stores it, then loads `armor.value`
//   and stores it.

use mycrate::at;
use std::cell::Cell;

pub struct EntityComponent {
    pub kind: i32,
    pub value: Cell<i32>,
}

pub struct Entity {
    pub components: Vec<EntityComponent>,
}

#[inline(never)]
pub fn duel(attacker: &Entity, defender: &Entity, swings: i32) {
    let sword = at(&attacker.components, 0);
    let armor = at(&defender.components, 1);
    let mut n: i32 = 0;
    while n < swings {
        sword.value.set(sword.value.get() - armor.value.get());
        armor.value.set(armor.value.get() - 1);
        n += 1;
    }
}

fn component(kind: i32, value: i32) -> EntityComponent {
    EntityComponent { kind, value: Cell::new(value) }
}

pub fn main_like() -> i64 {
    let entities = vec![
        Entity { components: vec![component(2, 20), component(1, 5)] },
        Entity { components: vec![component(2, 9), component(1, 4)] },
    ];
    duel(&entities[0], &entities[1], 3);
    duel(&entities[1], &entities[1], 2);
    let e0 = &entities[0];
    let e1 = &entities[1];
    (e0.components[0].value.get() + e1.components[0].value.get() + e1.components[1].value.get() + 10) as i64
}
