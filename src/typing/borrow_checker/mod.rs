pub mod experimental;

// The core error reporter (`crate::typing::compiler_error_reporter`) references
// `borrow_checker::borrow_error::BorrowErrorKind` and lives in the non-AI-editable core, so re-export
// the module to keep that path stable across the move into `experimental`.
pub use experimental::borrow_error;
