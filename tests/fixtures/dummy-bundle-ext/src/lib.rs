//! Minimal dummy bundle extension for greentic-bundle integration tests.
//! Returns canned bytes for any render() call.

#[allow(warnings)]
mod bindings;

use bindings::exports::greentic::extension_base::lifecycle;
use bindings::exports::greentic::extension_base::manifest;
use bindings::exports::greentic::extension_bundle::bundling;
use bindings::exports::greentic::extension_bundle::recipes;
use bindings::greentic::extension_base::types;

const DUMMY_BYTES: &[u8] = b"PK\x03\x04dummy-pack-bytes";
const DUMMY_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Component;

impl manifest::Guest for Component {
    fn get_identity() -> types::ExtensionIdentity {
        types::ExtensionIdentity {
            id: "greentic.dummy-bundle-ext".into(),
            version: "0.0.1".into(),
            kind: types::Kind::Bundle,
        }
    }
    fn get_offered() -> Vec<types::CapabilityRef> {
        Vec::new()
    }
    fn get_required() -> Vec<types::CapabilityRef> {
        Vec::new()
    }
}

impl lifecycle::Guest for Component {
    fn init(_config_json: String) -> Result<(), types::ExtensionError> {
        Ok(())
    }
    fn shutdown() {}
}

impl recipes::Guest for Component {
    fn list_recipes() -> Vec<recipes::RecipeSummary> {
        vec![recipes::RecipeSummary {
            id: "dummy".into(),
            display_name: "Dummy".into(),
            description: "Test fixture recipe".into(),
            icon_path: None,
        }]
    }
    fn recipe_config_schema(_recipe_id: String) -> Result<String, types::ExtensionError> {
        Ok("{}".into())
    }
    fn supported_capabilities(_recipe_id: String) -> Result<Vec<String>, types::ExtensionError> {
        Ok(Vec::new())
    }
}

impl bundling::Guest for Component {
    fn validate_config(
        _recipe_id: String,
        _config_json: String,
    ) -> Vec<types::Diagnostic> {
        Vec::new()
    }
    fn render(
        _recipe_id: String,
        _config_json: String,
        _session: bundling::DesignerSession,
    ) -> Result<bundling::BundleArtifact, types::ExtensionError> {
        Ok(bundling::BundleArtifact {
            filename: "dummy-0.0.1.gtpack".into(),
            bytes: DUMMY_BYTES.to_vec(),
            sha256: DUMMY_SHA.into(),
        })
    }
}

bindings::export!(Component with_types_in bindings);
