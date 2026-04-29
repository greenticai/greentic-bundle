//! Route `render` calls based on `describe.json` `execution.kind`.
//!
//! WASM execution (Mode B) is currently disabled in the OSS build because the
//! `greentic-ext-runtime` / `greentic-ext-contract` crates are not yet
//! published to crates.io (tracked in
//! https://github.com/greenticai/greentic/issues/175). Until those land, every
//! dispatch returns `ExtensionError::ModeBNotImplemented`. The host-side
//! pieces of the extension surface (discovery, validation, CLI plumbing) keep
//! working and stay shipped.

use crate::ext::describe::Execution;
use crate::ext::errors::ExtensionError;
use crate::ext::registry::ExtensionRegistry;

/// The artifact returned by `bundling.render`.
#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub fn invoke_recipe(
    registry: &ExtensionRegistry,
    extension_id: &str,
    recipe_id: &str,
    _config_json: &str,
    _session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    let entry = registry.resolve(extension_id, recipe_id)?;
    match &entry.execution {
        Execution::Wasm => Err(ExtensionError::ModeBNotImplemented),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::describe::Descriptor;
    use crate::ext::loader::DiscoveredExtension;
    use std::path::PathBuf;

    fn register(reg: &mut ExtensionRegistry, kind_json: &str) {
        let raw = format!(
            r#"{{
              "apiVersion": "greentic.ai/v1",
              "kind": "BundleExtension",
              "metadata": {{ "id": "x.test", "name": "t", "version": "0.0.1" }},
              "runtime": {{ "component": "extension.wasm" }},
              "execution": {kind_json},
              "contributions": {{
                "recipes": [
                  {{ "id": "standard", "displayName": "x", "description": "x",
                     "configSchema": "s.json" }}
                ]
              }}
            }}"#
        );
        let d = Descriptor::from_json(&raw).unwrap();
        let discovered = DiscoveredExtension {
            root: PathBuf::from("/tmp"),
            descriptor: d,
        };
        reg.register_discovered(vec![discovered]).unwrap();
    }

    #[test]
    fn wasm_dispatch_returns_not_implemented() {
        let mut reg = ExtensionRegistry::new();
        register(&mut reg, r#"{ "kind": "wasm" }"#);
        let err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
        assert!(
            matches!(err, ExtensionError::ModeBNotImplemented),
            "expected ModeBNotImplemented, got {err:?}"
        );
    }

    #[test]
    fn unknown_extension_errors() {
        let reg = ExtensionRegistry::new();
        let err = invoke_recipe(&reg, "x.missing", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::RecipeNotFound { .. }));
    }
}
