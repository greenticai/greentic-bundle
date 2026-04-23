//! Public input + output types. Schema mirrors the existing
//! `greentic-bundle::ext::builtin_bridge::DesignerSession + StandardConfig` so
//! the wrapper transition (Task 21) is mechanical.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct PackInputs<'a> {
    pub config: &'a StandardConfig,
    pub flows: &'a [FlowEntry],
    pub cards: &'a [CardContentEntry],
    pub assets: &'a [(String, Vec<u8>)],
    pub capabilities: &'a [String],
}

#[derive(Debug, Clone)]
pub struct FlowEntry {
    pub name: String,
    pub yaml: String,
}

#[derive(Debug, Clone)]
pub struct CardContentEntry {
    pub id: String,
    pub json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_source")]
    pub source: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            source: default_i18n_source(),
            targets: Vec::new(),
        }
    }
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

#[derive(Debug, Clone)]
pub struct PackOutput {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_round_trips() {
        let raw = r#"{"metadata":{"name":"x","version":"0.1.0"},"channels":["webchat"],"format":"gtpack-legacy"}"#;
        let cfg: StandardConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.metadata.name, "x");
        assert_eq!(cfg.embed_ui, "none");
        assert_eq!(cfg.i18n.source, "en");
    }
}
