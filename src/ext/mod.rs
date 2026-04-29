//! Bundle extension host module.
//!
//! Feature-gated by `extensions`. Provides discovery, descriptor parsing,
//! registry, and dispatch for bundle extensions. WASM execution backend is
//! currently disabled — see `dispatcher.rs` for context.

pub mod describe;
pub mod dispatcher;
pub mod errors;
pub mod loader;
pub mod registry;

pub use errors::ExtensionError;
