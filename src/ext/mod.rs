//! Bundle extension host module (Phase A).
//!
//! Feature-gated by `extensions`. See
//! `docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md`.

pub mod builtin_bridge;
pub mod describe;
pub mod dispatcher;
pub mod errors;
pub mod loader;
pub mod registry;
pub mod wasm;

pub use errors::ExtensionError;
