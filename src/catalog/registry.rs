use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::setup::SetupSpecInput;

pub const BUNDLED_PROVIDER_REGISTRY_SOURCE: &str = "registries/providers.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub category_label: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCatalog {
    pub entries: Vec<CatalogEntry>,
    pub summary: CatalogSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ArrayCatalogFormat {
    Categorized(Vec<ProviderRegistryCategory>),
    Flat(Vec<CatalogEntry>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ObjectCatalogFormat {
    Oci(OciProviderRegistryFile),
    Categorized(CategorizedProviderRegistryFile),
    Flat(ProviderRegistryFile),
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProviderRegistryLabel {
    #[serde(default)]
    fallback: String,
}

/// OCI registry format with separate categories and items arrays at root level.
/// Format: `{ "categories": [...], "items": [...] }`
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciProviderRegistryFile {
    #[serde(default)]
    registry_version: Option<String>,
    #[serde(default)]
    categories: Vec<OciProviderRegistryCategory>,
    #[serde(default)]
    items: Vec<OciProviderRegistryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciProviderRegistryCategory {
    id: String,
    #[serde(default)]
    label: ProviderRegistryLabel,
    #[serde(default)]
    description: Option<ProviderRegistryLabel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciProviderRegistryItem {
    id: String,
    #[serde(default)]
    category: Option<String>,
    label: ProviderRegistryLabel,
    #[serde(alias = "ref")]
    reference: String,
    #[serde(default)]
    setup: Option<SetupSpecInput>,
}

pub fn parse_catalog_bytes(bytes: &[u8], source: &str) -> Result<CatalogSummary> {
    let entries = parse_catalog_entries(bytes, source)?;
    Ok(summary_from_entries(&entries))
}

pub fn bundled_provider_registry_entries() -> Result<Vec<CatalogEntry>> {
    load_catalog_entries(
        include_bytes!("../../registries/providers.json"),
        BUNDLED_PROVIDER_REGISTRY_SOURCE,
    )
}

pub fn load_catalog_entries(bytes: &[u8], source: &str) -> Result<Vec<CatalogEntry>> {
    parse_catalog_entries(bytes, source)
}

pub fn parse_catalog(bytes: &[u8], source: &str) -> Result<ParsedCatalog> {
    let entries = parse_catalog_entries(bytes, source)?;
    Ok(ParsedCatalog {
        summary: summary_from_entries(&entries),
        entries,
    })
}

fn parse_catalog_entries(bytes: &[u8], source: &str) -> Result<Vec<CatalogEntry>> {
    let raw = std::str::from_utf8(bytes)
        .with_context(|| format!("catalog {source} must be valid UTF-8 JSON"))?;
    let trimmed = raw.trim_start();

    if trimmed.starts_with('[') {
        return match serde_json::from_str::<ArrayCatalogFormat>(trimmed)
            .with_context(|| format!("parse catalog/provider registry file {source}"))?
        {
            ArrayCatalogFormat::Categorized(categories) => Ok(categories
                .into_iter()
                .flat_map(|category| {
                    let category_name = category.category;
                    let category_description = category.description;
                    category.items.into_iter().map(move |item| {
                        CatalogEntry::from_categorized_item(
                            item,
                            &category_name,
                            None,
                            category_description.as_deref(),
                        )
                    })
                })
                .collect()),
            ArrayCatalogFormat::Flat(entries) => Ok(entries),
        };
    }

    if trimmed.starts_with('{') {
        return match serde_json::from_str::<ObjectCatalogFormat>(trimmed)
            .with_context(|| format!("parse catalog/provider registry file {source}"))?
        {
            ObjectCatalogFormat::Oci(registry) => {
                // Build category lookup for labels and descriptions once per catalog parse.
                let category_map: std::collections::HashMap<String, &OciProviderRegistryCategory> =
                    registry
                        .categories
                        .iter()
                        .map(|c| (c.id.clone(), c))
                        .collect();
                Ok(registry
                    .items
                    .into_iter()
                    .map(|item| {
                        let category_meta = item
                            .category
                            .as_ref()
                            .and_then(|cat_id| category_map.get(cat_id));
                        let category_label = category_meta
                            .filter(|c| !c.label.fallback.is_empty())
                            .map(|c| c.label.fallback.clone());
                        let category_description = category_meta
                            .and_then(|c| c.description.as_ref())
                            .map(|d| d.fallback.clone());
                        CatalogEntry {
                            id: item.id,
                            category: item.category,
                            category_label,
                            category_description,
                            label: (!item.label.fallback.is_empty()).then_some(item.label.fallback),
                            reference: item.reference,
                            setup: item.setup,
                        }
                    })
                    .collect())
            }
            ObjectCatalogFormat::Categorized(registry) => Ok(registry
                .categories
                .into_iter()
                .flat_map(|category| {
                    let category_name = category.category;
                    let category_description = category.description;
                    category.items.into_iter().map(move |item| {
                        CatalogEntry::from_categorized_item(
                            item,
                            &category_name,
                            None,
                            category_description.as_deref(),
                        )
                    })
                })
                .collect()),
            ObjectCatalogFormat::Flat(registry) => {
                Ok(registry.items.into_iter().map(CatalogEntry::from).collect())
            }
        };
    }

    Err(anyhow::anyhow!(
        "parse catalog/provider registry file {source}: expected JSON object or array"
    ))
}

fn summary_from_entries(entries: &[CatalogEntry]) -> CatalogSummary {
    let mut item_ids = entries
        .iter()
        .map(|entry| entry.id.clone())
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
            category_label: None,
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
        category_label: Option<&str>,
        category_description: Option<&str>,
    ) -> Self {
        Self {
            category: Some(category.to_string()),
            // For legacy format, use category name as label if no explicit label provided
            category_label: Some(category_label.unwrap_or(category).to_string()),
            category_description: category_description.map(ToString::to_string),
            ..Self::from(item)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{bundled_provider_registry_entries, load_catalog_entries, parse_catalog_bytes};

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
    fn parses_oci_registry_format_with_root_items() {
        let entries = load_catalog_entries(
            br#"{
  "registry_version": "providers@1",
  "categories": [
    {
      "id": "messaging",
      "label": { "fallback": "Messaging" },
      "description": { "fallback": "Bridges to external messaging services" }
    },
    {
      "id": "events",
      "label": { "fallback": "Events" },
      "description": { "fallback": "Event sources and delivery helpers" }
    }
  ],
  "items": [
    {
      "id": "messaging-teams",
      "category": "messaging",
      "label": { "fallback": "MS-Teams" },
      "ref": "oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest"
    },
    {
      "id": "messaging-telegram",
      "category": "messaging",
      "label": { "fallback": "Telegram" },
      "ref": "oci://ghcr.io/greenticai/packs/messaging/messaging-telegram:latest"
    },
    {
      "id": "events-webhook",
      "category": "events",
      "label": { "fallback": "Webhook" },
      "ref": "oci://ghcr.io/greenticai/packs/events/events-webhook:latest"
    }
  ]
}"#,
            "oci-registry",
        )
        .expect("entries");

        assert_eq!(entries.len(), 3);
        // Check first item
        assert_eq!(entries[0].id, "messaging-teams");
        assert_eq!(entries[0].category.as_deref(), Some("messaging"));
        assert_eq!(entries[0].category_label.as_deref(), Some("Messaging"));
        assert_eq!(entries[0].label.as_deref(), Some("MS-Teams"));
        assert_eq!(
            entries[0].reference,
            "oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest"
        );
        assert_eq!(
            entries[0].category_description.as_deref(),
            Some("Bridges to external messaging services")
        );
        // Check second item
        assert_eq!(entries[1].id, "messaging-telegram");
        assert_eq!(entries[1].category_label.as_deref(), Some("Messaging"));
        assert_eq!(entries[1].label.as_deref(), Some("Telegram"));
        // Check third item (different category)
        assert_eq!(entries[2].id, "events-webhook");
        assert_eq!(entries[2].category.as_deref(), Some("events"));
        assert_eq!(entries[2].category_label.as_deref(), Some("Events"));
        assert_eq!(entries[2].label.as_deref(), Some("Webhook"));
        assert_eq!(
            entries[2].category_description.as_deref(),
            Some("Event sources and delivery helpers")
        );
    }

    #[test]
    fn parses_checked_in_provider_registry_fixture() {
        let entries = bundled_provider_registry_entries().expect("catalog fixture");
        assert!(!entries.is_empty());
        assert_eq!(entries[0].id, "messaging-teams");
        assert_eq!(
            entries[0].reference,
            "oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest"
        );
    }

    #[test]
    fn parses_flat_registry_object_items() {
        let entries = load_catalog_entries(
            br#"{
  "items": [
    {
      "id":"provider-a",
      "label":{"fallback":"Provider A"},
      "ref":"repo://providers/provider-a@1"
    }
  ]
}"#,
            "flat-object",
        )
        .expect("entries");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "provider-a");
        assert_eq!(entries[0].label.as_deref(), Some("Provider A"));
        assert_eq!(entries[0].reference, "repo://providers/provider-a@1");
    }

    #[test]
    fn catalog_summary_deduplicates_item_ids() {
        let summary = parse_catalog_bytes(
            br#"[
  {"id":"provider-b","reference":"repo://providers/provider-b@1"},
  {"id":"provider-a","reference":"repo://providers/provider-a@1"},
  {"id":"provider-a","reference":"repo://providers/provider-a@2"}
]"#,
            "summary",
        )
        .expect("summary");

        assert_eq!(summary.item_count, 2);
        assert_eq!(summary.item_ids, vec!["provider-a", "provider-b"]);
    }
}
