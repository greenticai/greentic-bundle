//! MockBundleInvoker for unit/integration tests.
//!
//! Filled in Task 6.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::BundleWasmInvoker;

/// Test invoker placeholder; populated in Task 6.
#[derive(Default)]
pub struct MockBundleInvoker;

impl BundleWasmInvoker for MockBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented)
    }
}
