//! Capability-based pack dependency resolver.
//!
//! Maps `required_capabilities` declared in pack dependencies to catalog
//! entries that advertise matching `provided_capabilities`. Used by the
//! wizard to auto-include dependency packs (e.g. state-memory) when a
//! provider pack (e.g. messaging-slack) requires them.

use std::collections::{BTreeMap, BTreeSet};

use super::registry::CatalogEntry;

/// A dependency requirement extracted from a pack manifest.
#[derive(Debug, Clone)]
pub struct CapabilityRequirement {
    /// The capability string (e.g. `greentic:state/state-store`).
    pub capability: String,
    /// The pack_id that requires this capability.
    pub required_by: String,
}

/// Result of resolving dependencies against the catalog.
#[derive(Debug, Default)]
pub struct DependencyResolution {
    /// Dependencies auto-resolved (only one catalog entry provides them).
    pub auto_resolved: Vec<ResolvedDependency>,
    /// Dependencies with multiple providers — user must choose.
    pub choices: Vec<CapabilityChoice>,
    /// Dependencies that no catalog entry can satisfy.
    pub unresolved: Vec<UnresolvedCapability>,
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub capability: String,
    pub required_by: String,
    pub provider: CatalogEntry,
}

#[derive(Debug, Clone)]
pub struct CapabilityChoice {
    pub capability: String,
    pub required_by: String,
    pub options: Vec<CatalogEntry>,
}

#[derive(Debug, Clone)]
pub struct UnresolvedCapability {
    pub capability: String,
    pub required_by: String,
}

/// Build an index from capability name → catalog entries that provide it.
pub fn build_capability_index(
    catalog_entries: &[CatalogEntry],
) -> BTreeMap<String, Vec<CatalogEntry>> {
    let mut index: BTreeMap<String, Vec<CatalogEntry>> = BTreeMap::new();
    for entry in catalog_entries {
        for cap in &entry.provided_capabilities {
            index
                .entry(cap.clone())
                .or_default()
                .push(entry.clone());
        }
    }
    index
}

/// Resolve a set of capability requirements against the catalog.
///
/// - `requirements`: capabilities needed by packs already in the bundle.
/// - `catalog_entries`: all available catalog entries.
/// - `already_included_ids`: pack IDs already present in the bundle (skip those).
pub fn resolve_capabilities(
    requirements: &[CapabilityRequirement],
    catalog_entries: &[CatalogEntry],
    already_included_ids: &BTreeSet<String>,
) -> DependencyResolution {
    let cap_index = build_capability_index(catalog_entries);
    let mut resolution = DependencyResolution::default();
    let mut resolved_caps = BTreeSet::new();

    for req in requirements {
        if !resolved_caps.insert(req.capability.clone()) {
            // Already resolved this capability from a previous requirement.
            continue;
        }

        let providers: Vec<CatalogEntry> = cap_index
            .get(&req.capability)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| !already_included_ids.contains(&entry.id))
            .collect();

        match providers.len() {
            0 => {
                // Check if any already-included pack provides it.
                let satisfied_by_existing = cap_index
                    .get(&req.capability)
                    .map(|entries| {
                        entries
                            .iter()
                            .any(|e| already_included_ids.contains(&e.id))
                    })
                    .unwrap_or(false);

                if !satisfied_by_existing {
                    resolution.unresolved.push(UnresolvedCapability {
                        capability: req.capability.clone(),
                        required_by: req.required_by.clone(),
                    });
                }
            }
            1 => {
                resolution.auto_resolved.push(ResolvedDependency {
                    capability: req.capability.clone(),
                    required_by: req.required_by.clone(),
                    provider: providers.into_iter().next().unwrap(),
                });
            }
            _ => {
                resolution.choices.push(CapabilityChoice {
                    capability: req.capability.clone(),
                    required_by: req.required_by.clone(),
                    options: providers,
                });
            }
        }
    }

    resolution
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, caps: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            category: None,
            category_label: None,
            category_description: None,
            label: Some(id.to_string()),
            reference: format!("oci://test/{id}:latest"),
            setup: None,
            provided_capabilities: caps.iter().map(|s| s.to_string()).collect(),
            required_capabilities: Vec::new(),
        }
    }

    #[test]
    fn auto_resolves_single_provider() {
        let entries = vec![entry("state-memory", &["greentic:state/state-store"])];
        let reqs = vec![CapabilityRequirement {
            capability: "greentic:state/state-store".to_string(),
            required_by: "messaging-slack".to_string(),
        }];
        let result = resolve_capabilities(&reqs, &entries, &BTreeSet::new());
        assert_eq!(result.auto_resolved.len(), 1);
        assert_eq!(result.auto_resolved[0].provider.id, "state-memory");
        assert!(result.choices.is_empty());
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn presents_choice_when_multiple_providers() {
        let entries = vec![
            entry("state-memory", &["greentic:state/state-store"]),
            entry("state-redis", &["greentic:state/state-store"]),
        ];
        let reqs = vec![CapabilityRequirement {
            capability: "greentic:state/state-store".to_string(),
            required_by: "messaging-slack".to_string(),
        }];
        let result = resolve_capabilities(&reqs, &entries, &BTreeSet::new());
        assert!(result.auto_resolved.is_empty());
        assert_eq!(result.choices.len(), 1);
        assert_eq!(result.choices[0].options.len(), 2);
    }

    #[test]
    fn skips_already_included() {
        let entries = vec![
            entry("state-memory", &["greentic:state/state-store"]),
            entry("state-redis", &["greentic:state/state-store"]),
        ];
        let reqs = vec![CapabilityRequirement {
            capability: "greentic:state/state-store".to_string(),
            required_by: "messaging-slack".to_string(),
        }];
        let included = BTreeSet::from(["state-memory".to_string()]);
        let result = resolve_capabilities(&reqs, &entries, &included);
        // state-memory already included, so only state-redis is a candidate → auto-resolve
        assert_eq!(result.auto_resolved.len(), 1);
        assert_eq!(result.auto_resolved[0].provider.id, "state-redis");
    }

    #[test]
    fn satisfied_by_existing_is_not_unresolved() {
        let entries = vec![entry("state-memory", &["greentic:state/state-store"])];
        let reqs = vec![CapabilityRequirement {
            capability: "greentic:state/state-store".to_string(),
            required_by: "messaging-slack".to_string(),
        }];
        let included = BTreeSet::from(["state-memory".to_string()]);
        let result = resolve_capabilities(&reqs, &entries, &included);
        assert!(result.auto_resolved.is_empty());
        assert!(result.choices.is_empty());
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn unresolved_when_no_provider() {
        let entries = vec![];
        let reqs = vec![CapabilityRequirement {
            capability: "greentic:state/state-store".to_string(),
            required_by: "messaging-slack".to_string(),
        }];
        let result = resolve_capabilities(&reqs, &entries, &BTreeSet::new());
        assert_eq!(result.unresolved.len(), 1);
    }

    #[test]
    fn deduplicates_same_capability() {
        let entries = vec![entry("state-memory", &["greentic:state/state-store"])];
        let reqs = vec![
            CapabilityRequirement {
                capability: "greentic:state/state-store".to_string(),
                required_by: "messaging-slack".to_string(),
            },
            CapabilityRequirement {
                capability: "greentic:state/state-store".to_string(),
                required_by: "messaging-telegram".to_string(),
            },
        ];
        let result = resolve_capabilities(&reqs, &entries, &BTreeSet::new());
        assert_eq!(result.auto_resolved.len(), 1);
    }
}
