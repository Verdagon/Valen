// GhostCell arm of valen/stamina_vs_components.rs.
//
// Under the current design all three arms compile to the same loop; this test
// shows no win today.
//
// Verdict: never.
// Tier: none today (all three arms compile to the same loop). If field-level
// scopes land: second claim ("trust"). The verdict is against the Valen arm's
// intended shape, not what Valen emits today.
// Bucket: B. Rust has the fact (the two fields have different types and
// offsets) and may not rely on it, by a choice about unsafe code.
//
// Why it loses against the intended shape: `attacker` and `defender` may be the
// same cell, so a shared view of the defender cannot be held beside an
// exclusive view of the attacker (E0502). The loop reaches `stamina` and
// `components` through the cells on every iteration. LLVM is not told that a
// store to `stamina` leaves the components pointer and length alone. So both
// reload, the bounds check repeats, and `stamina` is reloaded too.
//
// Why rustc can never close it: the loop makes no call, so nothing here depends
// on what a callee may do. The reloads come from the store itself. The fact
// that would remove them is in the types: two `&GhostCell<Entity>` are the same
// cell or disjoint, so `stamina` (an `i32`) never overlaps the components
// pointer and length. C compilers do this by default through type-based alias
// analysis: clang -O2 keeps `stamina` in a register and loads the components
// pointer once, and with -fno-strict-aliasing the reloads return (one compiler
// and version probed; much real C is built with that flag). Rust has no such
// rule, because unsafe Rust may view the same memory as two types. Plain Rust
// has the same gap; the cells are not the cause.
//
// Why Valen may: Valen has no unsafe code, so nothing can reinterpret an
// entity's memory as another type. This win would come from that, not from
// group borrowing.
//
// Brand layout: one brand, one cell per entity.
// Why no better layout exists: `attacker` and `defender` come from one
// collection and may be the same cell, so they share a brand. The loop makes no
// call, so there is no token to keep away from a callee.
//
// Where the cells sit: two placements, and both compile to the same loop.
// - A cell per entity (`Vec<GhostCell<Entity>>`), which this arm uses. Observed
//   per iteration: load the components length, bounds check, load the
//   components pointer, load `value`, load `stamina`, store `stamina`.
// - Cells below the entity (`stamina: GhostCell<i32>`, `Entity` itself not in a
//   cell, `attack` taking two `&Entity`). Observed per iteration: the same six
//   operations. An `&Entity` that holds a cell directly gets no `noalias`, so
//   the store to `stamina` still forces the components reloads.
// The two tie, so the arm keeps one cell per entity. No placement leaves
// `Entity` free of cells: `stamina` is written through a shared pointer, so it
// needs a cell of its own or one around it.
//
// Why the entities are in cells: `attack` takes two entities that may be the
// same entity, and writes one while reading the other.
//
// How this arm was written: a mirror of the Valen arm. `attack` takes two cell
// pointers where Valen takes two references. Each use goes through the token,
// because the two views cannot be held together.
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

use ghost_cell::{GhostCell, GhostToken};

pub struct EntityComponent {
    pub value: i32,
}

pub struct Entity {
    pub stamina: i32,
    pub components: Vec<EntityComponent>,
}

pub struct Level<'b> {
    pub entities: Vec<GhostCell<'b, Entity>>,
}

#[inline(never)]
pub fn attack<'b>(t: &mut GhostToken<'b>, attacker: &GhostCell<'b, Entity>, defender: &GhostCell<'b, Entity>, count: i64) {
    let mut j: i64 = 0;
    while j < count {
        attacker.borrow_mut(t).stamina -= defender.borrow(t).components[j as usize].value;
        j += 1;
    }
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let mut level = Level { entities: Vec::new() };
        level.entities.push(GhostCell::new(Entity {
            stamina: 50,
            components: vec![EntityComponent { value: 1 }, EntityComponent { value: 2 }, EntityComponent { value: 3 }],
        }));
        level.entities.push(GhostCell::new(Entity {
            stamina: 50,
            components: vec![EntityComponent { value: 4 }, EntityComponent { value: 5 }, EntityComponent { value: 6 }],
        }));
        attack(&mut t, &level.entities[0], &level.entities[1], 2);
        attack(&mut t, &level.entities[1], &level.entities[1], 3);
        (level.entities[0].borrow(&t).stamina + level.entities[1].borrow(&t).stamina) as i64
    })
}
