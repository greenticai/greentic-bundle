//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.
//!
//! Filled in Tasks 5 + 7.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};

/// Trait abstracting WASM invocation — enables test injection via MockBundleInvoker.
pub trait BundleWasmInvoker: Send + Sync {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError>;
}

/// Production impl placeholder. Implementation arrives in Task 7.
pub struct WasmtimeBundleInvoker;

impl WasmtimeBundleInvoker {
    pub fn new() -> Result<Self, ExtensionError> {
        Ok(Self)
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented) // unstubbed in Task 7
    }
}
