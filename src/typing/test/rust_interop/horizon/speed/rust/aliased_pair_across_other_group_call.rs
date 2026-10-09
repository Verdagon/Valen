// GhostCell arm of valen/aliased_pair_across_other_group_call.rs.
//
// Verdict: never.
// Tier: second claim ("trust").
// Bucket: B. No view is held across the call.
//
// Why it loses: the arm holds the two cell pointers for the whole loop, so it
// reloads no `components` pointer or length and repeats no bounds check. But a
// view of `armor` and a mutable view of `sword` cannot be held at once, so each
// swing takes them in turn and releases them. Nothing is held across
// `record(log)`, and LLVM must assume the call changed both cells. Each swing
// reloads `sword.value` after the call, where the Valen arm does not.
//
// Why rustc cannot close it: `record` takes no token, so by GhostCell's own
// rules it cannot touch a cell. rustc does not know those rules. A cell's
// contents sit in an `UnsafeCell`. Under both current aliasing models (Stacked
// Borrows and Tree Borrows), unsafe code elsewhere may have taken a pointer to
// a cell's contents with `GhostCell::as_ptr` and may write through it during
// the call, on every iteration, as long as no view is held across it (Miri
// accepts it; results are recorded in
// notes/docs/architecture/rust-interop-design.md, Background). rustc must
// compile such a program correctly. Valen says the call leaves the components
// alone because it assumes every imported safe Rust function is a sound API
// (proposal S30 in the same document). The difference is one of language
// contract, not of how clever the compiler is.
//
// Why this is no more than second claim: `record` does not take the components' token. An
// arm loses for a reason no change to Rust's unsafe-code rules could remove
// only when the callee takes the token of the data held across it.
//
// How this arm was written: `duel` binds `sword` and `armor` once, as the Valen
// arm does, but as cell pointers. The token gives one view at a time, so each
// swing reads `armor`, releases it, and then writes `sword`. `record` is a
// mirror. Fields and counters use the Valen arm's widths: Valen `int` is `i32`.
//
// Cell placement: one cell per component, and `Entity` and `Log` are plain. Of
// the placements compiled, this one has the best loop. `a` and `d` are shared
// references, which may point at the same entity, and the cell pointers are
// reached through them with no token. It gives up adding or removing a
// component while a cell pointer is held; this program does neither.
//
// Placement not used: one cell per entity. The `components` pointer and length
// are then inside the cell. Compiled, that loop loads both lengths, checks both
// bounds, loads both pointers, and loads both values on every swing: six loads
// and two bounds checks, where this arm has two loads. The verdict, bucket and
// tier are the same under both placements.
//
// Brand layout: one brand, for the components. `Log` needs no cell, so it has
// no brand and `record` takes no token.
//
// Why `Component` is in a cell: `duel` writes one component while it reads
// another that may belong to the same entity. Without cells the borrow checker
// rejects that pair of references.
//
// Assumption behind the expected IR: `do_nothing` stays an opaque call that
// does not unwind.
//
// Expected IR of `duel`, unoptimized:
// - The loop contains a load of `armor.value`, a load of `sword.value`, a store
//   of `sword.value`, and the call to `record`.
//
// Expected IR of `duel`, optimized:
// - Both `components` pointers and lengths are loaded before the loop, and the
//   loop contains no bounds check.
// - Each iteration loads `armor.value`, loads `sword.value`, stores
//   `sword.value`, and calls `record`.

use ghost_cell::{GhostCell, GhostToken};
use mycrate::{at, do_nothing};

pub struct Component {
    pub kind: i32,
    pub value: i32,
}

pub struct Entity<'b> {
    pub components: Vec<GhostCell<'b, Component>>,
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
pub fn duel<'b>(t: &mut GhostToken<'b>, a: &Entity<'b>, d: &Entity<'b>, log: &mut Log, n: i32) {
    let sword = at(&a.components, 0);
    let armor = at(&d.components, 1);
    let mut i: i32 = 0;
    while i < n {
        let blunting = armor.borrow(t).value;
        sword.borrow_mut(t).value -= blunting;
        record(log);
        i += 1;
    }
}

fn components<'b>(values: [(i32, i32); 2]) -> Vec<GhostCell<'b, Component>> {
    values.into_iter().map(|(kind, value)| GhostCell::new(Component { kind, value })).collect()
}

pub fn main_like() -> i64 {
    GhostToken::new(|mut t| {
        let entities = vec![
            Entity { components: components([(2, 60), (1, 5)]) },
            Entity { components: components([(2, 30), (1, 4)]) },
        ];
        let mut log = Log { count: 0 };
        duel(&mut t, &entities[0], &entities[1], &mut log, 3);
        duel(&mut t, &entities[1], &entities[1], &mut log, 2);
        (entities[0].components[0].borrow(&t).value + entities[1].components[0].borrow(&t).value + log.count) as i64
    })
}
