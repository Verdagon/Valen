
// TODO: I think we can inline this, it's become small enough
pub fn phase0_generate_importer_source(rust_crate_names: &[String]) -> String {
  let mut importer_source = String::new();
  for rust_crate_name in rust_crate_names {
    importer_source.push_str(&format!("extern crate {rust_crate_name};\n"));
  }
  importer_source
}
