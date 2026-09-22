#![feature(box_patterns)]
#![allow(dead_code)]
#![allow(unused_variables, unused_imports)]

extern crate core;

pub mod backend_ffi;
pub mod builtins;
pub mod clang;
pub mod code_source;
pub mod compile_options;
#[cfg(test)]
pub mod end_to_end_tests;
pub mod integration_tests;
pub mod instantiating;
pub mod interner;
pub mod keywords;
pub mod lexing;
pub mod parse_arena;
pub mod parsing;
pub mod pass_manager;
pub mod postparsing;
pub mod scout_arena;
pub mod tests;
pub mod typing;
#[cfg(test)]
pub mod testvm;
pub mod utils;
#[path = "solver/lib.rs"]
pub mod solver;

pub use interner::StrI;
pub use keywords::Keywords;