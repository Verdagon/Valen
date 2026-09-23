use crate::code_source::Source;
use crate::interner::StrI;
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::utils::code_hierarchy::{FileCoordinateMap, PackageCoordinate};
use crate::utils::fx::HashMap;

pub const ENTRIES: &[(&str, &str, &str)] = &[
  ("arith", "arith.vale", include_str!("resources/arith.vale")),
  ("logic", "logic.vale", include_str!("resources/logic.vale")),
  ("migrate",                        "migrate.vale",                        include_str!("resources/migrate.vale")),
  ("str", "str.vale", include_str!("resources/str.vale")),
  ("drop", "drop.vale", include_str!("resources/drop.vale")),
  ("clone", "clone.vale", include_str!("resources/clone.vale")),
  ("implicit_clone", "implicit_clone.vale", include_str!("resources/implicit_clone.vale")),
  ("arrays", "arrays.vale", include_str!("resources/arrays.vale")),
  ("mainargs", "mainargs.vale", include_str!("resources/mainargs.vale")),
  ("print", "print.vale", include_str!("resources/print.vale")),
  ("tup0", "tup0.vale", include_str!("resources/tup0.vale")),
  ("tup1", "tup1.vale", include_str!("resources/tup1.vale")),
  ("tup2", "tup2.vale", include_str!("resources/tup2.vale")),
  ("tupN", "tupN.vale", include_str!("resources/tupN.vale")),
  ("streq", "streq.vale", include_str!("resources/streq.vale")),
  ("panic", "panic.vale", include_str!("resources/panic.vale")),
  ("panicutils", "panicutils.vale", include_str!("resources/panicutils.vale")),
  // VCOORD: re-enable interfaces
  // ("as", "as.vale", include_str!("resources/as.vale")),
  // ("opt", "opt.vale", include_str!("resources/opt.vale")),
  // ("result", "result.vale", include_str!("resources/result.vale")),
  ("box", "box.vale", include_str!("resources/box.vale")),
  ("sameinstance", "sameinstance.vale", include_str!("resources/sameinstance.vale")),
  // VCOORD: re-enable weaks
  //("weak",                           "weak.vale",                           include_str!("resources/weak.vale")),
];

pub fn builtin_module_code_map<'a>(
  parse_arena: &ParseArena<'a>,
  keywords: &Keywords<'a>,
  name: &str,
) -> FileCoordinateMap<'a, String> {
  let (_, filename, contents) = ENTRIES
    .iter()
    .find(|(n, _, _)| *n == name)
    .unwrap_or_else(|| panic!("Unknown builtin module: {}", name));
  let module_stri = parse_arena.intern_str(name);
  let package_coord =
    parse_arena.intern_package_coordinate(keywords.v, &[keywords.builtins, module_stri]);
  let file_coord = parse_arena.intern_file_coordinate(package_coord, filename);
  let mut result = FileCoordinateMap::new();
  result.put(file_coord, contents.to_string());
  result
}

pub fn empty_v_builtins_stub<'a>(coord: &PackageCoordinate<'a>) -> Option<HashMap<String, String>> {
  if coord.is_builtin() { Some(HashMap::default()) } else { None }
}

pub fn builtin_source_bundle<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
  names: &[&str],
) -> Source<'a>
where
  'a: 'ctx,
{
  let mut result = FileCoordinateMap::new();
  for name in names {
    let (_, filename, contents) = ENTRIES
      .iter()
      .find(|(n, _, _)| n == name)
      .unwrap_or_else(|| panic!("Unknown builtin module: {}", name));
    let module_stri = parse_arena.intern_str(name);
    let package_coord =
      parse_arena.intern_package_coordinate(keywords.v, &[keywords.builtins, module_stri]);
    let file_coord = parse_arena.intern_file_coordinate(package_coord, filename);
    result.put(file_coord, contents.to_string());
  }
  Source::from_code_map(&result)
}

pub fn builtin_source_for_panicutils<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(parse_arena, keywords, &["panicutils", "panic", "print", "str"])
}

pub fn builtin_source_for_arith<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(parse_arena, keywords, &["arith", "implicit_clone"])
}

pub fn builtin_source_for_arrays<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(parse_arena, keywords, &["arrays", "arith", "drop", "implicit_clone"])
}

pub fn builtin_source_for_opt<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(
    parse_arena,
    keywords,
    &["opt", "drop", "implicit_clone", "panicutils", "panic", "print", "str"],
  )
}

pub fn builtin_source_for_weak<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(
    parse_arena,
    keywords,
    &["weak", "opt", "drop", "implicit_clone", "panicutils", "panic", "print", "str"],
  )
}

pub fn builtin_source_for_as<'a, 'ctx>(
  parse_arena: &'ctx ParseArena<'a>,
  keywords: &'ctx Keywords<'a>,
) -> Source<'a>
where
  'a: 'ctx,
{
  builtin_source_bundle(
    parse_arena,
    keywords,
    &[
      "as",
      "result",
      "logic",
      "drop",
      "implicit_clone",
      "arith",
      "panicutils",
      "panic",
      "print",
      "str",
    ],
  )
}

pub fn get_embedded_modulized_code_map<'a>(
  parse_arena: &ParseArena<'a>,
  keywords: &Keywords<'a>,
) -> FileCoordinateMap<'a, String> {
  let mut result = FileCoordinateMap::new();
  for (module_name, filename, contents) in ENTRIES {
    let module_name_stri = parse_arena.intern_str(module_name);
    let package_coord =
      parse_arena.intern_package_coordinate(keywords.v, &[keywords.builtins, module_name_stri]);
    let file_coord = parse_arena.intern_file_coordinate(package_coord, filename);
    result.put(file_coord, contents.to_string());
  }
  result
}

// Add an empty v.builtins.whatever so that the aforementioned imports still work.
// But load the actual files all inside the root package.
pub fn get_code_map<'a>(
  parse_arena: &ParseArena<'a>,
  keywords: &Keywords<'a>,
) -> FileCoordinateMap<'a, String> {
  let builtin_namespace_coord = parse_arena.intern_package_coordinate(keywords.empty_string, &[]);
  let mut result = FileCoordinateMap::new();

  for (module_name, filename, contents) in ENTRIES {
    let module_name_stri = parse_arena.intern_str(module_name);
    // Put empty string for v.builtins.moduleName
    let modulized_package_coord =
      parse_arena.intern_package_coordinate(keywords.v, &[keywords.builtins, module_name_stri]);
    let modulized_file_coord =
      parse_arena.intern_file_coordinate(modulized_package_coord, filename);
    result.put(modulized_file_coord, String::new());
    // Put actual code for root package
    let root_file_coord = parse_arena.intern_file_coordinate(builtin_namespace_coord, filename);
    result.put(root_file_coord, contents.to_string());
  }

  result
}
