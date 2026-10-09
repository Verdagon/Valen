//! Horizon's instantiating side: the rustc query providers and hooks for the full Rust interop,
//! one file per rustc query or hook, like bifrost's.
//!
//! Selected at build time by the `horizon` cargo feature. The parent `rust_interop/mod.rs`
//! re-exports the items below in place of bifrost's, so bifrost's driver and callbacks install
//! horizon's providers and state without naming horizon. Every item re-exported here must keep the
//! exact signature of its bifrost counterpart.
//!
//! Beyond bifrost, horizon handles Rust→Vale trait-impl callbacks, Vale types crossing to Rust as
//! `__ValeOpaque<typeid>` (sized by the `layout_of` override), generic Rust callees, `deref`, interface
//! receivers, and the `Indirect`/`Cast`/`Pair` pass modes.

pub mod bifrost_state;
mod collect_and_partition_mono_items;
mod deduced_param_attrs;
pub mod driver_state;
mod extern_abi;
pub mod fill_extra_modules;
mod layout_of;
pub mod override_queries;
mod per_instance_mir;
mod resolve_request;
pub(crate) mod rustc_ty;

pub use crate::instantiating::rust_interop::horizon::bifrost_state::BifrostState;
pub use crate::instantiating::rust_interop::horizon::driver_state::set_bifrost_state_ptr;
pub use crate::instantiating::rust_interop::horizon::fill_extra_modules::consumer_fill_modules;
pub use crate::instantiating::rust_interop::horizon::override_queries::vale_override_queries;
