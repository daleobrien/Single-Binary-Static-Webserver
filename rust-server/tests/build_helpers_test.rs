// Shim to include build_helpers source modules so `cargo test` runs their #[cfg(test)] blocks.
// The build_helpers are normally compiled only as part of build.rs and uses [build-dependencies].
// We mirror the necessary crates in [dev-dependencies] so this integration test can compile.

#[path = "../build_helpers/utils.rs"]
mod utils;

#[path = "../build_helpers/processing.rs"]
mod processing;
