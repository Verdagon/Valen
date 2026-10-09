//! Horizon: the full Rust interop, restored beside bifrost.
//!
//! Horizon is selected at build time by the `horizon` cargo feature. The parent
//! `rust_interop/mod.rs` re-exports the items below in place of bifrost's, so core and bifrost's
//! own driver (`drive`, `BifrostRustcCallbacks`) call horizon's versions without knowing which
//! implementation they got. Every item re-exported here must keep the exact signature of its
//! bifrost counterpart.
//!
//! Horizon shares bifrost's oracle vocabulary (`RustOracle`, `TypeR`, `FuncSignatureR`) and its
//! driver; what it replaces is the oracle itself, the importer, and the declaration synthesis.

pub mod declarations;
pub mod generate_final_rust_file;
pub mod importer;
pub mod tyctxt_oracle;
pub mod typeid;

pub use crate::typing::rust_interop::horizon::generate_final_rust_file::generate_final_rust_file_source;
pub use crate::typing::rust_interop::horizon::importer::{
  create_postparsed_function, declare_rust_imports, rust_method_entries,
};
pub use crate::typing::rust_interop::horizon::tyctxt_oracle::TyCtxtOracle as RealRustcOracle;
