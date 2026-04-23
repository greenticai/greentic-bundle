//! Pure-Rust workspace assembly + ZIP for the bundle-standard recipe.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tempfile`, NO `walkdir`,
//! NO `std::fs::write/create_dir_all`. Everything in-memory: `Vec<u8>` bytes,
//! `Vec<(String, Vec<u8>)>` entries.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]
