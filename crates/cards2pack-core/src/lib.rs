//! Pure-Rust cards-to-flow YGTc converter.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tokio`, NO subprocess,
//! NO native filesystem mutation. Inputs are deserialized JSON; outputs are strings
//! and structured warnings.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

mod types;
mod errors;
