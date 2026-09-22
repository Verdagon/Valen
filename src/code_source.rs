use crate::builtins::builtins::{builtin_module_code_map, get_code_map as get_builtins_code_map};
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::utils::code_hierarchy::{FileCoordinateMap, PackageCoordinate};
use crate::utils::fx::HashMap;

pub type SourceFn = for<'r, 's> fn(&'r PackageCoordinate<'s>) -> Option<HashMap<String, String>>;

pub enum Source<'a> {
  CodeMap(HashMap<&'a PackageCoordinate<'a>, HashMap<String, String>>),
  Fn(SourceFn),
}

impl<'a> Source<'a> {
  pub fn from_code_map(map: &FileCoordinateMap<'a, String>) -> Self {
    Source::CodeMap(flatten_code_map(map))
  }

  pub fn builtins<'ctx>(parse_arena: &'ctx ParseArena<'a>, keywords: &'ctx Keywords<'a>) -> Self
  where
    'a: 'ctx,
  {
    let map = get_builtins_code_map(parse_arena, keywords);
    Source::CodeMap(flatten_code_map(&map))
  }

  pub fn builtin_module<'ctx>(
    parse_arena: &'ctx ParseArena<'a>,
    keywords: &'ctx Keywords<'a>,
    name: &str,
  ) -> Self
  where
    'a: 'ctx,
  {
    let map = builtin_module_code_map(parse_arena, keywords, name);
    Source::CodeMap(flatten_code_map(&map))
  }
}

pub struct CodeSource<'a> {
  sources: Vec<Source<'a>>,
}

impl<'a> CodeSource<'a> {
  pub fn new(sources: Vec<Source<'a>>) -> Self {
    CodeSource { sources }
  }

  pub fn resolve(
    &self,
    package_coord: &'a PackageCoordinate<'a>,
  ) -> Option<HashMap<String, String>> {
    for source in &self.sources {
      let hit = match source {
        Source::CodeMap(m) => m.get(package_coord).cloned(),
        Source::Fn(f) => f(package_coord),
      };
      if hit.is_some() {
        return hit;
      }
    }
    None
  }
}

fn flatten_code_map<'a>(
  map: &FileCoordinateMap<'a, String>,
) -> HashMap<&'a PackageCoordinate<'a>, HashMap<String, String>> {
  let mut result: HashMap<&'a PackageCoordinate<'a>, HashMap<String, String>> = HashMap::default();
  for (package_coord, file_coords) in &map.package_coord_to_file_coords {
    let mut file_map = HashMap::default();
    for fc in file_coords {
      let contents =
        map.file_coord_to_contents.get(fc).expect("flatten_code_map - file coord missing");
      file_map.insert(fc.filepath.as_str().to_string(), contents.clone());
    }
    result.insert(*package_coord, file_map);
  }
  result
}
