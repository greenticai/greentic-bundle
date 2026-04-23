//! Public input + output types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct CardEntry {
    pub id: String,
    pub kind: CardKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CardKind {
    AdaptiveCard(serde_json::Value),
    Http(HttpConfig),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HttpConfig {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: serde_json::Value,
    #[serde(default)]
    pub body_mapping: serde_json::Value,
    #[serde(default)]
    pub next_entry_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub flow_name: String,
    pub strict: bool,
}

#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub flow_yaml: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticKind {
    UnreachableCard,
    DanglingRoute,
    DuplicateRouteKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_config_roundtrips_json() {
        let raw = r#"{"url":"https://x","method":"POST","next_entry_id":"next"}"#;
        let cfg: HttpConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.url, "https://x");
        assert_eq!(cfg.method, "POST");
        assert_eq!(cfg.next_entry_id.as_deref(), Some("next"));
    }
}
