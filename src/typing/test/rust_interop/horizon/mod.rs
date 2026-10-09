//! Rust interop tests that only horizon passes. They share the parent folder's harness and fixtures
//! with the tests that pass under both bifrost and horizon, and only build with the `horizon` feature.
#![cfg(feature = "horizon")]

mod cases;
mod drive_tests;
mod speed;
