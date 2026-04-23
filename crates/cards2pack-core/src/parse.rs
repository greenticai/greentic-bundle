//! Parse `contents_json` payload into `Vec<CardEntry>`.

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind, HttpConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawEntry {
    id: String,
    json: serde_json::Value,
}

pub fn parse_cards(contents_json: &str) -> Result<Vec<CardEntry>, ConvertError> {
    let raw: Vec<RawEntry> = serde_json::from_str(contents_json)
        .map_err(|e| ConvertError::Parse(e.to_string()))?;

    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let kind = classify(&entry)?;
        out.push(CardEntry { id: entry.id, kind });
    }
    Ok(out)
}

fn classify(entry: &RawEntry) -> Result<CardKind, ConvertError> {
    let ty = entry.json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty == "http" {
        let config_value = entry.json.get("config").cloned().unwrap_or(serde_json::json!({}));
        let cfg: HttpConfig = serde_json::from_value(config_value)
            .map_err(|e| ConvertError::InvalidHttp { id: entry.id.clone(), msg: e.to_string() })?;
        Ok(CardKind::Http(cfg))
    } else {
        Ok(CardKind::AdaptiveCard(entry.json.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_card_and_http() {
        let raw = r#"[
            {"id":"welcome","json":{"type":"AdaptiveCard","version":"1.5"}},
            {"id":"api_x","json":{"type":"http","config":{"url":"http://x","method":"GET","next_entry_id":"after"}}}
        ]"#;
        let cards = parse_cards(raw).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "welcome");
        assert!(matches!(cards[0].kind, CardKind::AdaptiveCard(_)));
        assert!(matches!(cards[1].kind, CardKind::Http(_)));
    }

    #[test]
    fn rejects_invalid_http_config() {
        let raw = r#"[{"id":"bad","json":{"type":"http","config":{"url":42}}}]"#;
        let err = parse_cards(raw).unwrap_err();
        assert_eq!(err.code(), "E_INVALID_HTTP");
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_cards("not json").unwrap_err();
        assert_eq!(err.code(), "E_PARSE");
    }
}
