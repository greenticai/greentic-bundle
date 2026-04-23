//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use std::path::PathBuf;
use std::sync::Arc;

/// Trait abstracting WASM invocation — enables test injection via MockBundleInvoker.
pub trait BundleWasmInvoker: Send + Sync {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError>;
}

/// Production impl: wraps greentic-ext-runtime's ExtensionRuntime.
pub struct WasmtimeBundleInvoker {
    runtime: Arc<greentic_ext_runtime::ExtensionRuntime>,
}

impl WasmtimeBundleInvoker {
    /// Construct from an iterator of bundle-extension directories.
    /// Each dir must contain `describe.json` + the WASM component file.
    pub fn from_ext_dirs(ext_dirs: &[PathBuf]) -> Result<Self, ExtensionError> {
        let user_path = ext_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let paths = greentic_ext_runtime::DiscoveryPaths::new(user_path);
        let config = greentic_ext_runtime::RuntimeConfig::from_paths(paths);
        let mut runtime = greentic_ext_runtime::ExtensionRuntime::new(config)
            .map_err(|e| ExtensionError::Internal(format!("ext-runtime init: {e}")))?;

        for d in ext_dirs {
            runtime
                .register_loaded_from_dir(d)
                .map_err(|e| ExtensionError::Internal(format!("register {d:?}: {e}")))?;
        }

        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Internal accessor for invoke() impl in Task 7.
    pub(crate) fn runtime(&self) -> &Arc<greentic_ext_runtime::ExtensionRuntime> {
        &self.runtime
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        // Real impl arrives in Task 7. For now still stubbed.
        Err(ExtensionError::ModeBNotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ext_dirs_handles_empty() {
        let invoker = WasmtimeBundleInvoker::from_ext_dirs(&[]);
        assert!(
            invoker.is_ok(),
            "empty dirs should construct: {:?}",
            invoker.err()
        );
    }
}
