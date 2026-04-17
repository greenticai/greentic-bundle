//! Mode B (full-WASM) execution seam. Stubbed in Phase A; wired to
//! `greentic-ext-runtime` in Phase B.

use crate::ext::errors::ExtensionError;

pub struct WasmInvocation<'a> {
    pub extension_id: &'a str,
    pub recipe_id: &'a str,
    pub config_json: &'a str,
    pub session_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub fn invoke_wasm(_call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
    Err(ExtensionError::ModeBNotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_mode_b_not_implemented() {
        let err = invoke_wasm(WasmInvocation {
            extension_id: "x",
            recipe_id: "y",
            config_json: "{}",
            session_json: "{}",
        })
        .unwrap_err();
        assert!(matches!(err, ExtensionError::ModeBNotImplemented));
    }
}
