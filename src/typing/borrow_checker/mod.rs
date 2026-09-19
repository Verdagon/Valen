// Two mutually-exclusive borrow-checker impls, selected by the `borrow_checker_symphony` feature.
// Default (feature off) compiles the `experimental` checker, the one the suite is green on; enabling
// the feature compiles the in-progress `symphony` rewrite instead. Both define
// `Compiler::check_function`, so only one is ever compiled.
#[cfg(not(feature = "borrow_checker_symphony"))]
pub mod experimental;
#[cfg(not(feature = "borrow_checker_symphony"))]
pub use experimental::errors::humanize_borrow_error;

#[cfg(feature = "borrow_checker_symphony")]
pub mod symphony;
#[cfg(feature = "borrow_checker_symphony")]
pub use symphony::errors::humanize_borrow_error;

pub mod borrow_error;
pub mod templata_g;
pub mod kind_g;
pub mod group_expr;
pub mod check_usages_types;
pub mod ast_g;
pub mod access_event;
