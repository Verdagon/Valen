// Content-addressed type identity for the `__ValeOpaque<const T: u64>` wrapper.
//
// A Vale type crossing to rust as an opaque blob (arch §10, §8.5) carries a `u64` typeid as the
// wrapper's const-generic argument, so the `layout_of` override and the inbound callback path can
// recover which Vale type an opaque instantiation stands for. The id is a deterministic FNV-1a hash
// of the type's canonical identity string (its declared name in the stub, or its humanized instantiated
// id in the instantiator). Determinism is load-bearing — a stub emitted at one compile must key the same
// as an id an override computes at another — so the hash consumes only the string, never a pointer or a
// HashMap iteration order. Mirrors Sky's `toylangc/src/typeid.rs` (which uses BLAKE3; FNV-1a is already
// in-tree as stub_gen's `source_digest` and is sufficient at this scale).

/// The FNV-1a 64-bit hash of a type's canonical identity string. Deterministic (fixed offset basis and
/// prime, no seed).
pub fn typeid(identity: &str) -> u64 {
  let mut hash: u64 = 0xcbf29ce484222325;
  for byte in identity.as_bytes() {
    hash ^= *byte as u64;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}

#[cfg(test)]
mod tests {
  use super::typeid;

  // Determinism fence: the hash of a fixed identity must never move, or a stub emitted by one compile
  // stops matching the id an override computes at another (cross-compile reproducibility, arch §7.6).
  // Mirrors Sky's pinned-literal fence (toylangc/src/typeid.rs).
  #[test]
  fn typeid_is_deterministic_and_distinct() {
    assert_eq!(typeid("MyCb"), typeid("MyCb"));
    assert_ne!(typeid("MyCb"), typeid("MyOther"));
    // Pinned value: a change to the FNV-1a algorithm/constants must fail loudly here.
    assert_eq!(typeid("MyCb"), 8706161416459359414);
  }
}
