use super::util::assert_borrow_check_passes;

// The caller side of read-world-mut-part: `harvest_nature_energy` takes the whole level read-only
// plus a mutable tile inside it, so its only churn is `level.tiles[]`. The caller's borrow into
// `level.entities[]` therefore survives the call and is written through afterward. Rust cannot pass
// `&Level` alongside a `&mut Tile` and an `&Entity` that both point into it, so its caller has to
// break `Level` into separate fields.
#[test]
fn test_entity_borrow_survives_sibling_tile_mutation_through_callee() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Tile { nature_energy int; }
struct Entity { loc int; }
struct Level { tiles []Tile; entities []Entity; mana_modifier int; }
func observe<T>(x &T) { }
func harvest_nature_energy(level &Level, tile &Tile in level.tiles[] mut, entity &Entity in level.entities[]) int {
  set tile.nature_energy = 1;
  return level.mana_modifier;
}
func continue_chase(level &Level mut, entity &Entity in level.entities[]) {
  entity_ref = &level.entities[0];
  tile = &level.tiles[0];
  speed = level.harvest_nature_energy(tile, entity_ref);
  set entity_ref.loc = speed;
  observe(entity);
}
exported func main() int {
  level = Level(Array<Tile>(2), Array<Entity>(2), 3);
  continue_chase(&level, &level.entities[1]);
  return 0;
}
"#);
}
