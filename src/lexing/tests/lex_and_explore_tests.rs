use crate::code_source::{CodeSource, Source};
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::parsing::tests::parser_test_compilation;
use crate::utils::code_hierarchy::FileCoordinateMap;
use bumpalo::Bump;

#[test]
fn import_of_rust_crate_is_not_explored() {
  let parse_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let keywords = Keywords::new_for_parse(&parse_arena);
  let rust_crates =
    parse_arena.alloc_slice_from_vec(vec![parse_arena.intern_str("mycrate")]);

  let test_package_coord =
    parse_arena.intern_package_coordinate(parse_arena.intern_str("test"), &[]);
  let mut code_map = FileCoordinateMap::new();
  code_map.put(
    parse_arena.intern_file_coordinate(test_package_coord, "test.vale"),
    "import mycrate.Slot;\nfunc main() int { return 42; }".to_string(),
  );
  let code_source = CodeSource::new(vec![Source::from_code_map(&code_map)]);

  let mut compilation = parser_test_compilation::test(
    &parse_arena,
    &keywords,
    &rust_crates,
    &code_source,
    test_package_coord,
  );
  let parseds = compilation.get_parseds().expect("parsing failed");
  let parsed_packages: Vec<_> =
    parseds.file_coord_to_contents.keys().map(|file_coord| file_coord.package_coord).collect();
  assert_eq!(parsed_packages, vec![test_package_coord]);
}

#[test]
#[should_panic(expected = "both a Rust crate and a Vale module")]
fn name_that_is_both_rust_crate_and_vale_module_is_rejected() {
  let parse_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let keywords = Keywords::new_for_parse(&parse_arena);
  let rust_crates =
    parse_arena.alloc_slice_from_vec(vec![parse_arena.intern_str("mycrate")]);

  let test_package_coord =
    parse_arena.intern_package_coordinate(parse_arena.intern_str("test"), &[]);
  let mycrate_package_coord =
    parse_arena.intern_package_coordinate(parse_arena.intern_str("mycrate"), &[]);
  let mut code_map = FileCoordinateMap::new();
  code_map.put(
    parse_arena.intern_file_coordinate(test_package_coord, "test.vale"),
    "import mycrate.Slot;\nfunc main() int { return 42; }".to_string(),
  );
  code_map.put(
    parse_arena.intern_file_coordinate(mycrate_package_coord, "slot.vale"),
    "struct Slot { }".to_string(),
  );
  let code_source = CodeSource::new(vec![Source::from_code_map(&code_map)]);

  let mut compilation = parser_test_compilation::test(
    &parse_arena,
    &keywords,
    &rust_crates,
    &code_source,
    test_package_coord,
  );
  let _ = compilation.get_parseds();
}
