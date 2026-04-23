//! Mode B WASM execution dispatcher for bundle extensions.
//!
//! Wraps `greentic-ext-runtime` for loading + invocation of WASM bundle
//! extensions. Implements the `BundleWasmInvoker` trait with two impls:
//! `WasmtimeBundleInvoker` (production, backed by ext-runtime) and
//! `MockBundleInvoker` (tests).

mod bindings;
mod invoker;
mod mock;

pub use invoker::{BundleWasmInvoker, WasmtimeBundleInvoker};
pub use mock::MockBundleInvoker;
