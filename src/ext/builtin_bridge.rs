//! Builtin bridge: `BuiltinRecipeId::Standard` → thin wrapper calling `bundle_standard_core`.

use serde::Deserialize;

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::RenderedArtifact;

#[derive(Debug, Deserialize)]
pub struct DesignerSession {
    pub flows_json: String,
    pub contents_json: String,
    #[serde(default)]
    pub assets: Vec<(String, Vec<u8>)>,
    #[serde(default)]
    pub capabilities_used: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StandardConfig {
    pub metadata: StandardMetadata,
    pub channels: Vec<String>,
    #[serde(default = "default_embed_ui")]
    pub embed_ui: String,
    #[serde(default)]
    pub i18n: I18nConfig,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct StandardMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_source")]
    pub source: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_embed_ui() -> String {
    "none".into()
}
fn default_format() -> String {
    "gtpack-legacy".into()
}
fn default_i18n_source() -> String {
    "en".into()
}

pub fn handle_standard(
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    use bundle_standard_core::{
        build_pack, CardContentEntry, FlowEntry, PackInputs, StandardConfig as BSConfig,
    };

    // Parse old-shape inputs (preserves backward-compat for callers).
    let session: DesignerSession = serde_json::from_str(session_json)?;
    // bundle-standard-core owns StandardConfig now; parse once, use for validation + build.
    let bs_config: BSConfig = serde_json::from_str(config_json)?;

    let flows: Vec<FlowEntry> = serde_json::from_str::<Vec<serde_json::Value>>(&session.flows_json)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| FlowEntry {
            name: v.get("name").and_then(|x| x.as_str()).map(str::to_owned).unwrap_or_else(|| format!("flow-{i:03}")),
            yaml: v.get("yaml").and_then(|x| x.as_str()).unwrap_or("").to_owned(),
        })
        .collect();

    let cards: Vec<CardContentEntry> = serde_json::from_str::<Vec<serde_json::Value>>(&session.contents_json)?
        .into_iter()
        .filter_map(|v| Some(CardContentEntry {
            id: v.get("id").and_then(|x| x.as_str())?.to_owned(),
            json: v.get("json")?.clone(),
        }))
        .collect();

    let inputs = PackInputs {
        config: &bs_config,
        flows: &flows,
        cards: &cards,
        assets: &session.assets,
        capabilities: &session.capabilities_used,
    };

    let pack = build_pack(&inputs).map_err(|e| match e.code() {
        "E_INVALID_FORMAT" => ExtensionError::InvalidConfig(e.to_string()),
        _ => ExtensionError::Io(std::io::Error::other(e.to_string())),
    })?;

    Ok(RenderedArtifact {
        filename: pack.filename,
        bytes: pack.bytes,
        sha256: pack.sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_CONFIG: &str = r#"{
      "metadata": { "name": "demo", "version": "0.1.0" },
      "channels": ["webchat"],
      "format": "gtpack-legacy"
    }"#;

    const MIN_SESSION: &str = r#"{
      "flows_json": "[{\"name\":\"main\",\"yaml\":\"schemaVersion: 2\\nname: main\"}]",
      "contents_json": "[{\"id\":\"welcome\",\"json\":{\"type\":\"AdaptiveCard\",\"version\":\"1.5\"}}]",
      "assets": [],
      "capabilities_used": ["greentic:adaptive-cards/schema"]
    }"#;

    #[test]
    fn rejects_unsupported_format() {
        let bad_cfg = MIN_CONFIG.replace("gtpack-legacy", "apack");
        let err = handle_standard(&bad_cfg, MIN_SESSION).unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidConfig(_)));
    }

    #[test]
    fn happy_path_produces_artifact() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        assert_eq!(out.filename, "demo-0.1.0.gtpack");
        assert!(!out.bytes.is_empty());
        assert_eq!(out.sha256.len(), 64);
        let again = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        // Deterministic output — same inputs → same sha256.
        assert_eq!(out.sha256, again.sha256);
    }

    #[test]
    fn artifact_is_a_valid_zip_containing_bundle_yaml() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("bundle.yaml")));
        assert!(names.iter().any(|n| n.ends_with("flows/main.ygtc")));
        assert!(
            names
                .iter()
                .any(|n| n.ends_with("assets/cards/welcome.json"))
        );
    }
}
