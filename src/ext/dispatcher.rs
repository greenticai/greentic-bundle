//! Route `render` calls to the correct execution backend based on
//! `describe.json` `execution.kind`.

use crate::ext::builtin_bridge;
use crate::ext::describe::Execution;
use crate::ext::errors::ExtensionError;
use crate::ext::registry::{BuiltinRecipeId, ExtensionRegistry};
use crate::ext::wasm;
use crate::ext::wasm::RenderedArtifact;

pub fn invoke_recipe(
    registry: &ExtensionRegistry,
    extension_id: &str,
    recipe_id: &str,
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    let entry = registry.resolve(extension_id, recipe_id)?;
    match &entry.execution {
        Execution::Builtin { builtin_id } => {
            let id = BuiltinRecipeId::from_str(builtin_id).ok_or_else(|| {
                ExtensionError::InvalidDescriptor(format!("unknown builtinId '{builtin_id}'"))
            })?;
            match id {
                BuiltinRecipeId::Standard => {
                    builtin_bridge::handle_standard(config_json, session_json)
                }
            }
        }
        Execution::Wasm => wasm::invoke_wasm(wasm::WasmInvocation {
            extension_id,
            recipe_id,
            config_json,
            session_json,
        }),
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
    fn wasm_path_returns_mode_b_error() {
        let mut reg = ExtensionRegistry::new();
        register(&mut reg, r#"{ "kind": "wasm" }"#);
        let err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::ModeBNotImplemented));
    }

    #[test]
    fn unknown_builtin_id_errors() {
        let mut reg = ExtensionRegistry::new();
        register(&mut reg, r#"{ "kind": "builtin", "builtinId": "mystery" }"#);
        let err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidDescriptor(_)));
    }

    #[test]
    fn unknown_extension_errors() {
        let reg = ExtensionRegistry::new();
        let err = invoke_recipe(&reg, "x.missing", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::RecipeNotFound { .. }));
    }

    #[test]
    fn builtin_standard_dispatches_to_bridge() {
        let mut reg = ExtensionRegistry::new();
        register(
            &mut reg,
            r#"{ "kind": "builtin", "builtinId": "standard" }"#,
        );
        let config = r#"{
          "metadata": { "name": "demo", "version": "0.1.0" },
          "channels": ["webchat"],
          "format": "gtpack-legacy"
        }"#;
        let session = r#"{
          "flows_json": "[{\"name\":\"main\",\"yaml\":\"n: m\"}]",
          "contents_json": "[]",
          "assets": [],
          "capabilities_used": []
        }"#;
        let out = invoke_recipe(&reg, "x.test", "standard", config, session).unwrap();
        assert!(out.filename.ends_with(".gtpack"));
    }
}
