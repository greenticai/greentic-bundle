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

use crate::ext::errors::ExtensionError;

/// Invocation parameters passed across the host-WASM boundary.
pub struct WasmInvocation<'a> {
    pub extension_id: &'a str,
    pub recipe_id: &'a str,
    pub config_json: &'a str,
    pub session_json: &'a str,
}

/// The artifact returned by `bundling.render`.
#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

/// Process-wide singleton invoker. Initialized lazily on first call.
///
/// In production, `default_invoker()` builds a `WasmtimeBundleInvoker` from
/// `$GREENTIC_BUNDLE_EXT_DIR` (or `~/.greentic/extensions/bundle/` default).
/// Tests can inject a `MockBundleInvoker` via `set_invoker()` BEFORE the first
/// `invoke_wasm()` call.
static INVOKER: std::sync::OnceLock<Box<dyn BundleWasmInvoker>> = std::sync::OnceLock::new();

/// Install a custom invoker (test injection point).
/// Returns Err if invoker was already set.
pub fn set_invoker(invoker: Box<dyn BundleWasmInvoker>) -> Result<(), &'static str> {
    INVOKER
        .set(invoker)
        .map_err(|_| "invoker already set; tests must run sequentially")
}

/// Default invoker constructor: read `GREENTIC_BUNDLE_EXT_DIR` env (or default
/// `~/.greentic/extensions/bundle/`), enumerate ext dirs, build WasmtimeBundleInvoker.
fn default_invoker() -> Box<dyn BundleWasmInvoker> {
    let ext_root = std::env::var("GREENTIC_BUNDLE_EXT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = dirs::home_dir().unwrap_or_default();
            p.push(".greentic/extensions/bundle");
            p
        });

    let ext_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&ext_root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("describe.json").exists())
                .collect()
        })
        .unwrap_or_default();

    match WasmtimeBundleInvoker::from_ext_dirs(&ext_dirs) {
        Ok(inv) => Box::new(inv),
        Err(e) => {
            tracing::error!(error = %e, "failed to init WasmtimeBundleInvoker; using stub");
            Box::new(StubInvoker)
        }
    }
}

/// Fallback invoker that returns ModeBNotImplemented. Used only if
/// WasmtimeBundleInvoker init fails.
struct StubInvoker;
impl BundleWasmInvoker for StubInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented)
    }
}

/// Public dispatcher entry point: routes Mode B invocations to the configured invoker.
pub fn invoke_wasm(call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
    let invoker = INVOKER.get_or_init(default_invoker);
    invoker.invoke(call)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_construct() {
        let _ = WasmInvocation {
            extension_id: "x",
            recipe_id: "y",
            config_json: "{}",
            session_json: "{}",
        };
        let _ = RenderedArtifact {
            filename: "x.gtpack".into(),
            bytes: vec![],
            sha256: "0".repeat(64),
        };
    }
}
