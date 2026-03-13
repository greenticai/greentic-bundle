use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::setup::SetupSpecInput;

pub const BUNDLED_WELL_KNOWN_SOURCE: &str = "packs/well-known.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub category_description: Option<String>,
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
struct CategorizedProviderRegistryFile {
    #[serde(default)]
    categories: Vec<ProviderRegistryCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderRegistryCategory {
    category: String,
    #[serde(default)]
    description: Option<String>,
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

pub fn bundled_well_known_catalog_entries() -> Result<Vec<CatalogEntry>> {
    load_catalog_entries(
        include_bytes!("../../packs/well-known.json"),
        BUNDLED_WELL_KNOWN_SOURCE,
    )
}

pub fn load_catalog_entries(bytes: &[u8], source: &str) -> Result<Vec<CatalogEntry>> {
    let raw = std::str::from_utf8(bytes)
        .with_context(|| format!("catalog {source} must be valid UTF-8 JSON"))?;

    let value: serde_json::Value = serde_json::from_str(raw)
        .with_context(|| format!("parse catalog/provider registry file {source}"))?;

    if let Some(values) = value.as_array() {
        let looks_categorized = values.iter().all(|entry| {
            entry
                .as_object()
                .map(|object| object.contains_key("category") && object.contains_key("items"))
                .unwrap_or(false)
        });
        if looks_categorized {
            let categories: Vec<ProviderRegistryCategory> = serde_json::from_value(value)
                .with_context(|| format!("parse categorized catalog array {source}"))?;
            return Ok(categories
                .into_iter()
                .flat_map(|category| {
                    let category_name = category.category;
                    let category_description = category.description;
                    category.items.into_iter().map(move |item| {
                        CatalogEntry::from_categorized_item(
                            item,
                            &category_name,
                            category_description.as_deref(),
                        )
                    })
                })
                .collect());
        }

        return serde_json::from_value::<Vec<CatalogEntry>>(serde_json::Value::Array(
            values.to_vec(),
        ))
        .with_context(|| format!("parse catalog array {source}"));
    }

    if value.get("categories").is_some() {
        let registry: CategorizedProviderRegistryFile = serde_json::from_value(value)
            .with_context(|| {
                format!("parse categorized catalog/provider registry file {source}")
            })?;
        return Ok(registry
            .categories
            .into_iter()
            .flat_map(|category| {
                let category_name = category.category;
                let category_description = category.description;
                category.items.into_iter().map(move |item| {
                    CatalogEntry::from_categorized_item(
                        item,
                        &category_name,
                        category_description.as_deref(),
                    )
                })
            })
            .collect());
    }

    let registry: ProviderRegistryFile = serde_json::from_value(value)
        .with_context(|| format!("parse flat catalog/provider registry file {source}"))?;
    Ok(registry.items.into_iter().map(CatalogEntry::from).collect())
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

impl From<ProviderRegistryItem> for CatalogEntry {
    fn from(item: ProviderRegistryItem) -> Self {
        Self {
            id: item.id,
            category: None,
            category_description: None,
            label: (!item.label.fallback.is_empty()).then_some(item.label.fallback),
            reference: item.reference,
            setup: item.setup,
        }
    }
}

impl CatalogEntry {
    fn from_categorized_item(
        item: ProviderRegistryItem,
        category: &str,
        category_description: Option<&str>,
    ) -> Self {
        Self {
            category: Some(category.to_string()),
            category_description: category_description.map(ToString::to_string),
            ..Self::from(item)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{bundled_well_known_catalog_entries, load_catalog_entries};

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
    fn parses_categorized_registry_file() {
        let entries = load_catalog_entries(
            br#"[
    {
      "category": "deployer",
      "description": "deployment helpers for rollout targets",
      "items": [
        {
          "id":"provider-a",
          "label":{"fallback":"Provider A"},
          "reference":"repo://providers/provider-a@1"
        }
      ]
    },
    {
      "category": "oauth",
      "description": "OAuth provider helpers and identity integrations",
      "items": [
        {
          "id":"provider-b",
          "label":{"fallback":"Provider B"},
          "reference":"repo://providers/provider-b@1"
        }
      ]
    }
]"#,
            "inline",
        )
        .expect("entries");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "provider-a");
        assert_eq!(entries[1].id, "provider-b");
        assert_eq!(entries[0].category.as_deref(), Some("deployer"));
        assert_eq!(entries[1].category.as_deref(), Some("oauth"));
        assert_eq!(
            entries[0].category_description.as_deref(),
            Some("deployment helpers for rollout targets")
        );
        assert_eq!(
            entries[1].category_description.as_deref(),
            Some("OAuth provider helpers and identity integrations")
        );
    }

    #[test]
    fn parses_checked_in_well_known_catalog_fixture() {
        let entries = bundled_well_known_catalog_entries().expect("catalog fixture");
        assert_eq!(entries.len(), 47);
        assert_eq!(entries[0].id, "greentic.deployer.serverless");
        assert_eq!(
            entries[0].reference,
            "oci://ghcr.io/greenticai/packs/deployer/greentic.fixture.serverless.gtpack:latest"
        );
    }
}
