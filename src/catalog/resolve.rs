use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cache;
use super::client::{CatalogArtifactClient, DistributorCatalogClient};
use super::registry::{CatalogEntry, load_catalog_entries, parse_catalog_bytes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResolveOptions {
    pub offline: bool,
    pub write_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogLockEntry {
    pub requested_ref: String,
    pub resolved_ref: String,
    pub digest: String,
    pub source: String,
    pub item_count: usize,
    pub item_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogResolution {
    pub entries: Vec<CatalogLockEntry>,
    pub cache_writes: Vec<String>,
    pub discovered_items: Vec<CatalogEntry>,
}

impl CatalogResolution {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            cache_writes: Vec::new(),
            discovered_items: Vec::new(),
        }
    }
}

pub fn resolve_catalogs(
    root: &Path,
    references: &[String],
    options: &CatalogResolveOptions,
) -> Result<CatalogResolution> {
    resolve_catalogs_with_client(root, references, options, &DistributorCatalogClient)
}

pub fn resolve_catalogs_with_client(
    root: &Path,
    references: &[String],
    options: &CatalogResolveOptions,
    client: &dyn CatalogArtifactClient,
) -> Result<CatalogResolution> {
    let mut entries = Vec::new();
    let mut cache_writes = Vec::new();
    let mut discovered_items = Vec::new();

    let mut refs = references.to_vec();
    refs.sort();
    refs.dedup();

    for reference in refs {
        let resolved = resolve_one(root, &reference, options, client)?;
        cache_writes.extend(resolved.1);
        discovered_items.extend(resolved.2);
        entries.push(resolved.0);
    }

    cache_writes.sort();
    cache_writes.dedup();
    discovered_items.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then(left.reference.cmp(&right.reference))
    });
    discovered_items
        .dedup_by(|left, right| left.id == right.id && left.reference == right.reference);

    Ok(CatalogResolution {
        entries,
        cache_writes,
        discovered_items,
    })
}

fn resolve_one(
    root: &Path,
    reference: &str,
    options: &CatalogResolveOptions,
    client: &dyn CatalogArtifactClient,
) -> Result<(CatalogLockEntry, Vec<String>, Vec<CatalogEntry>)> {
    if let Some(local_path) = parse_local_reference(root, reference) {
        let resolved_path = if local_path.is_absolute() {
            local_path
        } else {
            root.join(local_path)
        };
        let bytes = std::fs::read(&resolved_path)
            .with_context(|| format!("read catalog {}", resolved_path.display()))?;
        let digest = digest_hex(&bytes);
        let catalog_entries = load_catalog_entries(&bytes, &resolved_path.display().to_string())?;
        let summary = parse_catalog_bytes(&bytes, &resolved_path.display().to_string())?;
        let cache_paths = if options.write_cache {
            cache::cache_catalog_bytes(root, reference, &digest, &bytes)?
        } else {
            Vec::new()
        };
        return Ok((
            CatalogLockEntry {
                requested_ref: reference.to_string(),
                resolved_ref: resolved_path.display().to_string(),
                digest,
                source: "local_file".to_string(),
                item_count: summary.item_count,
                item_ids: summary.item_ids,
                cache_path: cache::resolve_cached_path(root, reference)?
                    .map(|path| relative_display(root, &path)),
            },
            cache_paths
                .into_iter()
                .map(|path| relative_display(root, &path))
                .collect(),
            catalog_entries,
        ));
    }

    if let Some(cached_path) = cache::resolve_cached_path(root, reference)? {
        let bytes = std::fs::read(&cached_path)
            .with_context(|| format!("read cached catalog {}", cached_path.display()))?;
        let digest = digest_hex(&bytes);
        let catalog_entries = load_catalog_entries(&bytes, &cached_path.display().to_string())?;
        let summary = parse_catalog_bytes(&bytes, &cached_path.display().to_string())?;
        return Ok((
            CatalogLockEntry {
                requested_ref: reference.to_string(),
                resolved_ref: reference.to_string(),
                digest,
                source: "workspace_cache".to_string(),
                item_count: summary.item_count,
                item_ids: summary.item_ids,
                cache_path: Some(relative_display(root, &cached_path)),
            },
            Vec::new(),
            catalog_entries,
        ));
    }

    if options.offline {
        bail!(
            "catalog {reference} is not cached in {} and offline mode is enabled; seed the workspace-local cache first or rerun without --offline",
            root.join(super::CACHE_ROOT_DIR).display()
        );
    }

    let fetched = client.fetch_catalog(root, reference)?;
    let catalog_entries = load_catalog_entries(&fetched.bytes, reference)?;
    let summary = parse_catalog_bytes(&fetched.bytes, reference)?;
    let cache_paths = if options.write_cache {
        cache::cache_catalog_bytes(root, reference, &fetched.digest, &fetched.bytes)?
    } else {
        Vec::new()
    };
    Ok((
        CatalogLockEntry {
            requested_ref: reference.to_string(),
            resolved_ref: fetched.resolved_ref,
            digest: fetched.digest,
            source: "remote".to_string(),
            item_count: summary.item_count,
            item_ids: summary.item_ids,
            cache_path: cache::resolve_cached_path(root, reference)?
                .map(|path| relative_display(root, &path)),
        },
        cache_paths
            .into_iter()
            .map(|path| relative_display(root, &path))
            .collect(),
        catalog_entries,
    ))
}

fn parse_local_reference(root: &Path, reference: &str) -> Option<PathBuf> {
    if let Some(path) = reference.strip_prefix("file://") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(PathBuf::from(trimmed));
    }
    if reference.contains("://") {
        return None;
    }
    let candidate = PathBuf::from(reference);
    if candidate.is_absolute() || candidate.exists() || root.join(&candidate).exists() {
        return Some(candidate);
    }
    None
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::from("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
