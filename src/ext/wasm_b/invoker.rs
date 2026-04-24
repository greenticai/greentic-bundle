//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::bindings::exports::greentic::extension_bundle::bundling::DesignerSession;
use crate::ext::wasm_b::bindings::BundleExtension;
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

    /// Internal accessor used in tests.
    pub(crate) fn runtime(&self) -> &Arc<greentic_ext_runtime::ExtensionRuntime> {
        &self.runtime
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        // 1. Look up loaded extension by id.
        let loaded_map = self.runtime.loaded();
        let ext_id = greentic_ext_runtime::ExtensionId::from(call.extension_id.to_string());
        let loaded = loaded_map.get(&ext_id).ok_or_else(|| {
            ExtensionError::Internal(format!(
                "extension not loaded: {}",
                call.extension_id
            ))
        })?;

        // 2. Build store + instance via ext-runtime's LoadedExtension.
        let (mut store, instance) = loaded
            .build_store_and_instance(self.runtime.engine())
            .map_err(|e| ExtensionError::Internal(format!("instantiate: {e}")))?;

        // 3. Wrap raw wasmtime Instance in typed bindings.
        let bindings = BundleExtension::new(&mut store, &instance)
            .map_err(|e| ExtensionError::Internal(format!("bindings: {e}")))?;

        // 4. Parse session JSON into WIT DesignerSession.
        let session = parse_designer_session(call.session_json)
            .map_err(|e| ExtensionError::InvalidConfig(format!("session_json: {e}")))?;

        // 5. Call render.
        let bundling = bindings.greentic_extension_bundle_bundling();
        let render_result = bundling
            .call_render(
                &mut store,
                call.recipe_id,
                call.config_json,
                &session,
            )
            .map_err(|e| ExtensionError::Internal(format!("call_render trap: {e}")))?;

        // 6. Map WIT result to domain types.
        match render_result {
            Ok(artifact) => Ok(RenderedArtifact {
                filename: artifact.filename,
                bytes: artifact.bytes,
                sha256: artifact.sha256,
            }),
            Err(wit_err) => Err(map_wit_error(wit_err)),
        }
    }
}

/// Parse raw session JSON into the WIT-generated `DesignerSession` type.
/// Uses a private `Raw` helper so the WIT type does not need `serde::Deserialize`.
fn parse_designer_session(json: &str) -> Result<DesignerSession, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        flows_json: String,
        #[serde(default)]
        contents_json: String,
        #[serde(default)]
        assets: Vec<(String, Vec<u8>)>,
        #[serde(default)]
        capabilities_used: Vec<String>,
    }
    let raw: Raw = serde_json::from_str(json)?;
    Ok(DesignerSession {
        flows_json: raw.flows_json,
        contents_json: raw.contents_json,
        assets: raw.assets,
        capabilities_used: raw.capabilities_used,
    })
}

/// Map the WIT `ExtensionError` variant to the domain `ExtensionError`.
fn map_wit_error(
    wit_err: crate::ext::wasm_b::bindings::greentic::extension_base::types::ExtensionError,
) -> ExtensionError {
    use crate::ext::wasm_b::bindings::greentic::extension_base::types::ExtensionError as Wit;
    match wit_err {
        Wit::InvalidInput(msg) => ExtensionError::InvalidConfig(msg),
        Wit::MissingCapability(msg) => ExtensionError::Internal(format!("missing-capability: {msg}")),
        Wit::PermissionDenied(msg) => ExtensionError::Internal(format!("permission-denied: {msg}")),
        Wit::Internal(msg) => ExtensionError::Internal(msg),
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

    #[test]
    fn invoke_unknown_extension_returns_internal_not_loaded() {
        let invoker = WasmtimeBundleInvoker::from_ext_dirs(&[]).unwrap();
        let err = invoker
            .invoke(WasmInvocation {
                extension_id: "ghost",
                recipe_id: "standard",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap_err();
        assert!(
            !matches!(err, ExtensionError::ModeBNotImplemented),
            "got ModeBNotImplemented — regression"
        );
        assert!(
            matches!(&err, ExtensionError::Internal(msg) if msg.contains("not loaded")),
            "expected Internal(not loaded), got {err:?}"
        );
    }
}
