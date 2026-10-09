// Names that horizon's typing side and instantiating side must agree on for a Vale type crossing to
// Rust. The typeid hash itself is the parent's `rust_interop::typeid`, shared with bifrost.

/// The Rust identifier for the anonymous substruct auto-generated for an imported trait, so a lambda
/// handed to that trait (`SomeTrait((..) => {..})`) reaches Rust codegen as a named projected type.
///
/// **The one name-agreement seam.** Both sides that must agree call this exactly: the final-file
/// generator, which emits `pub struct <name><F>(..)` + `impl<F> SomeTrait for <name><F>`, and the
/// instantiator's `citizen_def_id_and_args`, which resolves the anon substruct to this same name via
/// `resolve_local_type`. If the two ever disagree the callback is silently dropped (`collect_callback`
/// finds no impl, no diagnostic), so there is exactly one definition of the name and both read it.
/// Vale's internal `<interface>.anonymous` name is not a valid Rust identifier; this maps it
/// deterministically to `<interface>__anon` (the interface's short name is already unique per
/// compilation, and the anon substruct is one-per-interface).
pub fn anon_substruct_rust_name(interface_human_name: &str) -> String {
  format!("{interface_human_name}__anon")
}

#[cfg(test)]
mod tests {
  use crate::typing::rust_interop::typeid;

  // Determinism fence: the hash of a fixed identity must never move, or a final file emitted by one
  // compile stops matching the id an override computes at another (cross-compile reproducibility).
  #[test]
  fn typeid_is_deterministic_and_distinct() {
    assert_eq!(typeid("MyCb"), typeid("MyCb"));
    assert_ne!(typeid("MyCb"), typeid("MyOther"));
    // Pinned value: a change to the FNV-1a algorithm/constants must fail loudly here.
    assert_eq!(typeid("MyCb"), 8706161416459359414);
  }
}
