pub mod bifrost;
pub use crate::typing::rust_interop::bifrost::callbacks::*;
pub use crate::typing::rust_interop::bifrost::dir_structure::*;
pub use crate::typing::rust_interop::bifrost::driver::*;
pub use crate::typing::rust_interop::bifrost::env::*;
pub use crate::typing::rust_interop::bifrost::error::*;
pub use crate::typing::rust_interop::bifrost::generate_final_rust_file::*;
pub use crate::typing::rust_interop::bifrost::generate_importer_file::*;
pub use crate::typing::rust_interop::bifrost::importer::*;
pub use crate::typing::rust_interop::bifrost::oracle::*;
pub use crate::typing::rust_interop::bifrost::orchestrator::*;
pub use crate::typing::rust_interop::bifrost::real_rustc_oracle::*;
pub use crate::typing::rust_interop::bifrost::rust_method_entries::*;
pub use crate::typing::rust_interop::bifrost::rustc_args::*;

pub fn typeid(identity: &str) -> u64 {
  let mut hash: u64 = 0xcbf29ce484222325;
  for byte in identity.as_bytes() {
    hash ^= *byte as u64;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}

pub fn source_digest(src: &str) -> u64 {
  let mut hash: u64 = 0xcbf29ce484222325;
  for byte in src.as_bytes() {
    hash ^= *byte as u64;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}
