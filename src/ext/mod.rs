//! Bundle extension host module (Phase A).
//!
//! Feature-gated by `extensions`. See
//! `docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md`.

pub mod describe;
pub mod loader;
pub mod registry;
pub mod dispatcher;
pub mod builtin_bridge;
pub mod wasm;
pub mod errors;

// Re-exports added as sub-modules are implemented (Task 2+).
