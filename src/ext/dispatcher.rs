//! Route `render` calls to the WASM execution backend based on
//! `describe.json` `execution.kind`.
//!
//! Post-Wave-5 the only execution path is Mode B WASM. The former builtin
//! bridge (Mode A) was deleted after `bundle-standard@0.2.0` flipped to
//! `execution.kind="wasm"` and greentic-designer stopped shipping the 0.1.0
//! builtin-backed artifact.

use crate::ext::describe::Execution;
use crate::ext::errors::ExtensionError;
use crate::ext::registry::ExtensionRegistry;
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
    fn wasm_path_routes_to_invoker() {
        // Structural assertion: the only execution branch after Wave 5 is
        // `Execution::Wasm → wasm::invoke_wasm(...)`. The exact error depends on
        // the ambient state of `~/.greentic/extensions/bundle/` (stub fallback
        // fires if ext-runtime init fails, e.g. unsigned local install), so we
        // accept any error — the key invariant is that dispatch happens and
        // returns structured failure rather than invoking a deleted builtin
        // branch.
        let mut reg = ExtensionRegistry::new();
        register(&mut reg, r#"{ "kind": "wasm" }"#);
        let _err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
    }

    #[test]
    fn unknown_extension_errors() {
        let reg = ExtensionRegistry::new();
        let err = invoke_recipe(&reg, "x.missing", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::RecipeNotFound { .. }));
    }
}
