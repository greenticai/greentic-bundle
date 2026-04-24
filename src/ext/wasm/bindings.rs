//! wasmtime::component::bindgen! generated host bindings for the
//! `greentic:extension-bundle/bundle-extension` WIT world.
//!
//! The macro generates:
//! - `BundleExtension` struct: instantiation entry point with typed export accessors
//! - `add_to_linker` helpers for each imported interface
//!
//! Imports we wire (delegated to greentic-ext-runtime's HostState impl):
//! - greentic:extension-base/types
//! - greentic:extension-host/logging
//! - greentic:extension-host/i18n
//! - greentic:extension-host/broker

#![allow(clippy::too_many_arguments)]

wasmtime::component::bindgen!({
    path: "wit",
    world: "greentic:extension-bundle/bundle-extension",
});
