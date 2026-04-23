//! MockBundleInvoker for unit/integration tests.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::BundleWasmInvoker;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock that returns pre-populated artifacts (or errors) keyed by (extension_id, recipe_id).
#[derive(Default)]
pub struct MockBundleInvoker {
    responses: Mutex<HashMap<(String, String), Result<RenderedArtifact, ExtensionError>>>,
    /// Calls captured for assertion in tests.
    pub call_log: Mutex<Vec<(String, String, String)>>, // (ext_id, recipe_id, config_json)
}

impl MockBundleInvoker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect_render(
        &self,
        extension_id: &str,
        recipe_id: &str,
        result: Result<RenderedArtifact, ExtensionError>,
    ) {
        let mut responses = self.responses.lock().unwrap();
        responses.insert((extension_id.to_owned(), recipe_id.to_owned()), result);
    }
}

impl BundleWasmInvoker for MockBundleInvoker {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        let key = (call.extension_id.to_owned(), call.recipe_id.to_owned());
        {
            let mut log = self.call_log.lock().unwrap();
            log.push((
                call.extension_id.to_owned(),
                call.recipe_id.to_owned(),
                call.config_json.to_owned(),
            ));
        }
        let mut responses = self.responses.lock().unwrap();
        match responses.remove(&key) {
            Some(r) => r,
            None => Err(ExtensionError::Internal(format!(
                "MockBundleInvoker has no response for ({}, {})",
                key.0, key.1
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_artifact() -> RenderedArtifact {
        RenderedArtifact {
            filename: "demo.gtpack".into(),
            bytes: b"PK\x03\x04".to_vec(),
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn returns_canned_response() {
        let mock = MockBundleInvoker::new();
        mock.expect_render("ext.x", "standard", Ok(dummy_artifact()));
        let r = mock
            .invoke(WasmInvocation {
                extension_id: "ext.x",
                recipe_id: "standard",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap();
        assert_eq!(r.filename, "demo.gtpack");
    }

    #[test]
    fn captures_call_log() {
        let mock = MockBundleInvoker::new();
        mock.expect_render("ext.x", "standard", Ok(dummy_artifact()));
        mock.invoke(WasmInvocation {
            extension_id: "ext.x",
            recipe_id: "standard",
            config_json: "{\"foo\":1}",
            session_json: "{}",
        })
        .unwrap();
        let log = mock.call_log.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].2, "{\"foo\":1}");
    }

    #[test]
    fn errors_on_unknown_key() {
        let mock = MockBundleInvoker::new();
        let err = mock
            .invoke(WasmInvocation {
                extension_id: "missing",
                recipe_id: "x",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap_err();
        assert!(matches!(err, ExtensionError::Internal(_)));
    }
}
