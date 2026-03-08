use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::setup::SetupSpecInput;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(alias = "ref")]
    pub reference: String,
    #[serde(default)]
    pub setup: Option<SetupSpecInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSummary {
    pub item_count: usize,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderRegistryFile {
    #[serde(default)]
    items: Vec<ProviderRegistryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderRegistryItem {
    id: String,
    label: ProviderRegistryLabel,
    #[serde(alias = "ref")]
    reference: String,
    #[serde(default)]
    setup: Option<SetupSpecInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderRegistryLabel {
    #[serde(default)]
    fallback: String,
}

pub fn parse_catalog_bytes(bytes: &[u8], source: &str) -> Result<CatalogSummary> {
    let entries = load_catalog_entries(bytes, source)?;
    Ok(summary_from_entries(entries))
}

pub fn load_catalog_entries(bytes: &[u8], source: &str) -> Result<Vec<CatalogEntry>> {
    let raw = std::str::from_utf8(bytes)
        .with_context(|| format!("catalog {source} must be valid UTF-8 JSON"))?;

    if let Ok(entries) = serde_json::from_str::<Vec<CatalogEntry>>(raw) {
        return Ok(entries);
    }

    let registry: ProviderRegistryFile = serde_json::from_str(raw)
        .with_context(|| format!("parse catalog/provider registry file {source}"))?;
    let entries = registry
        .items
        .into_iter()
        .map(|item| CatalogEntry {
            id: item.id,
            label: (!item.label.fallback.is_empty()).then_some(item.label.fallback),
            reference: item.reference,
            setup: item.setup,
        })
        .collect();
    Ok(entries)
}

fn summary_from_entries(entries: Vec<CatalogEntry>) -> CatalogSummary {
    let mut item_ids = entries
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    item_ids.sort();
    item_ids.dedup();
    let item_count = item_ids.len();
    CatalogSummary {
        item_count,
        item_ids,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::load_catalog_entries;

    #[test]
    fn parses_inline_setup_metadata_from_array_catalog() {
        let entries = load_catalog_entries(
            br#"[
  {
    "id":"provider-a",
    "reference":"repo://providers/provider-a@1",
    "setup":{
      "type":"legacy",
      "spec":{
        "title":"Provider A Setup",
        "questions":[{"name":"enabled","kind":"boolean","required":true}]
      }
    }
  }
]"#,
            "inline",
        )
        .expect("entries");

        assert_eq!(entries.len(), 1);
        let setup = entries[0].setup.as_ref().expect("setup metadata");
        assert_eq!(
            serde_json::to_value(setup).expect("setup json"),
            json!({
              "type":"legacy",
              "spec":{
                "title":"Provider A Setup",
                "questions":[{"name":"enabled","kind":"boolean","required":true}]
              }
            })
        );
    }

    #[test]
    fn parses_checked_in_well_known_catalog_fixture() {
        let entries = load_catalog_entries(
            include_bytes!("../../packs/well-known-packs.json"),
            "packs/well-known-packs.json",
        )
        .expect("catalog fixture");
        assert_eq!(entries.len(), 7);
        assert_eq!(entries[0].id, "greentic.deployer.serverless");
        assert_eq!(
            entries[0].reference,
            "oci://ghcr.io/greenticai/packs/deployer/greentic.fixture.serverless.gtpack:latest"
        );
    }
}
