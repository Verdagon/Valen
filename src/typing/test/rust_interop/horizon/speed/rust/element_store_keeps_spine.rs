// GhostCell arm of valen/element_store_keeps_spine.rs.
//
// Arm verdict: better than Valen today.
// Tier: fence (Rust wins). If S19 is resolved: equal.
// Bucket: none. This arm does not lose.
//
// Placement not used: one cell per entity. The `components` pointer and length
// are then inside the cell. Compiled, each step loads both entities' lengths
// and checks both bounds, loads both pointers, loads `weight`, loads `value`,
// and stores `value`: six loads and two bounds checks, where this arm has two
// loads and one bounds check. The arm uses the placement with the better loop.
//
// Why it is ahead: with one cell per component, an entity is a plain struct
// holding a `Vec` of cells. `attack` takes `a: &Entity` and `d: &Entity`, which
// may be the same entity. A `Vec` keeps its elements behind a pointer, so
// `Entity` itself contains no `UnsafeCell`, and rustc marks both parameters
// `noalias` and read-only. LLVM then knows that the store to `sword.value`,
// which goes through a pointer loaded from `a`, cannot change the pointer or
// length of `d.components`, which are reached through `d` itself. It loads them
// once. That is the Valen arm's intended shape, reached today; the Valen arm
// reloads both after every store.
//
// How this arm was written: a mirror of the Valen arm. `attack` binds `sword`
// once, as a cell pointer, and reaches each of the defender's components
// through `at`. The token gives one view at a time, so each step reads `weight`
// and releases the view before it takes the mutable view of `sword`. Fields use
// the Valen arm's widths: Valen `int` is `i32`, and `n` and `j` are `i64`.
//
// Cell placement: one cell per component. It gives up adding or removing a
// component while a cell pointer is held; this program does neither.
//
// Brand layout: one brand. The sword and the walked components may belong to
// the same entity, so all components answer to one token.
//
// Why `Component` is in a cell: `attack` writes one component while it reads
// others that may belong to the same entity. Without cells the borrow checker
// rejects that pair of references.
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
//   `weight`, loads `sword.value`, and stores `sword.value`.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::at;

pub struct Component {
    pub weight: i32,
    pub value: i32,
}

pub struct Entity<'b> {
    pub components: Vec<GhostCell<'b, Component>>,
}

#[inline(never)]
pub fn attack<'b>(t: &mut GhostToken<'b>, a: &Entity<'b>, d: &Entity<'b>, n: i64) {
    let sword = at(&a.components, 0);
    let mut j: i64 = 0;
    while j < n {
        let weight = at(&d.components, j).borrow(t).weight;
        sword.borrow_mut(t).value -= weight;
        j += 1;
    }
}

fn components<'b>(values: [(i32, i32); 2]) -> Vec<GhostCell<'b, Component>> {
    values.into_iter().map(|(weight, value)| GhostCell::new(Component { weight, value })).collect()
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let entities = vec![
            Entity { components: components([(2, 90), (3, 5)]) },
            Entity { components: components([(4, 70), (1, 6)]) },
        ];
        attack(&mut t, &entities[0], &entities[1], 2);
        attack(&mut t, &entities[1], &entities[1], 2);
        (entities[0].components[0].borrow(&t).value + entities[1].components[0].borrow(&t).value) as i64
    })
}
