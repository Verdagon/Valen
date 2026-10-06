// GhostCell arm of valen/sword_vs_armor_interleaved.rs.
//
// Arm verdict: equal today.
// Tier: fence (control).
// Bucket: none. This arm does not lose.
//
// Placement not used: one cell per entity. The `components` pointer and length
// are then inside the cell. Compiled, that loop has eight loads and three
// bounds checks per swing, where this arm has two loads. The arm uses the
// placement with the better loop.
//
// Why it is equal: `sword` and `armor` may be the same component as far as the
// compiler can tell, so each store forces the other value to be reloaded. That
// is true in this arm and in the Valen arm alike. Each swing loads
// `sword.value` and stores it, then loads `armor.value` and stores it.
//
// How this arm was written: `duel` binds `sword` and `armor` once, as the Valen
// arm does, but as cell pointers. The token gives one view at a time, so each
// swing reads `armor`, releases it, writes `sword`, releases it, and writes
// `armor`. The views compile to nothing. Fields and counters use the Valen
// arm's widths: Valen `int` is `i32`.
//
// Cell placement: one cell per component, and `Entity` is plain. `attacker`
// and `defender` are shared references, which may point at the same entity,
// and rustc marks both `noalias` and read-only, since an entity holds its cells
// behind a `Vec`'s pointer. It gives up adding or removing a component while a
// cell pointer is held; this program does neither.
//
// Brand layout: one brand. The sword and the armor may belong to the same
// entity, so all components answer to one token.
//
// Why `EntityComponent` is in a cell: `duel` writes one component while it
// reads and writes another that may belong to the same entity. Without cells
// the borrow checker rejects that pair of references.
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

use ghost_cell::{GhostCell, GhostToken};
use mycrate::at;

pub struct EntityComponent {
    pub kind: i32,
    pub value: i32,
}

pub struct Entity<'b> {
    pub components: Vec<GhostCell<'b, EntityComponent>>,
}

#[inline(never)]
pub fn duel<'b>(t: &mut GhostToken<'b>, attacker: &Entity<'b>, defender: &Entity<'b>, swings: i32) {
    let sword = at(&attacker.components, 0);
    let armor = at(&defender.components, 1);
    let mut n: i32 = 0;
    while n < swings {
        let blunting = armor.borrow(t).value;
        sword.borrow_mut(t).value -= blunting;
        armor.borrow_mut(t).value -= 1;
        n += 1;
    }
}

fn components<'b>(values: [(i32, i32); 2]) -> Vec<GhostCell<'b, EntityComponent>> {
    values.into_iter().map(|(kind, value)| GhostCell::new(EntityComponent { kind, value })).collect()
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let entities = vec![
            Entity { components: components([(2, 20), (1, 5)]) },
            Entity { components: components([(2, 9), (1, 4)]) },
        ];
        duel(&mut t, &entities[0], &entities[1], 3);
        duel(&mut t, &entities[1], &entities[1], 2);
        let e0 = &entities[0];
        let e1 = &entities[1];
        (e0.components[0].borrow(&t).value
            + e1.components[0].borrow(&t).value
            + e1.components[1].borrow(&t).value
            + 10) as i64
    })
}
