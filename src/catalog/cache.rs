use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::CACHE_ROOT_DIR;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogCacheIndex {
    #[serde(default)]
    pub refs: BTreeMap<String, String>,
}

pub fn ref_cache_path(root: &Path, reference: &str) -> PathBuf {
    root.join(CACHE_ROOT_DIR)
        .join("by-ref")
        .join(format!("{}.json", slug(reference)))
}

pub fn digest_cache_path(root: &Path, digest: &str) -> PathBuf {
    root.join(CACHE_ROOT_DIR)
        .join("by-digest")
        .join(format!("{digest}.json"))
}

pub fn index_path(root: &Path) -> PathBuf {
    root.join(CACHE_ROOT_DIR).join("index.json")
}

pub fn load_index(root: &Path) -> Result<CatalogCacheIndex> {
    let path = index_path(root);
    if !path.exists() {
        return Ok(CatalogCacheIndex::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read catalog cache index {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("parse catalog cache index {}", path.display()))
}

pub fn write_index(root: &Path, index: &CatalogCacheIndex) -> Result<()> {
    let path = index_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(index)?))
        .with_context(|| format!("write catalog cache index {}", path.display()))?;
    Ok(())
}

pub fn cache_catalog_bytes(
    root: &Path,
    reference: &str,
    digest: &str,
    bytes: &[u8],
) -> Result<Vec<PathBuf>> {
    let digest_path = digest_cache_path(root, digest);
    let ref_path = ref_cache_path(root, reference);
    for path in [&digest_path, &ref_path] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
    }

    let mut index = load_index(root)?;
    index.refs.insert(reference.to_string(), digest.to_string());
    write_index(root, &index)?;

    Ok(vec![digest_path, ref_path, index_path(root)])
}

pub fn resolve_cached_path(root: &Path, reference: &str) -> Result<Option<PathBuf>> {
    let by_ref = ref_cache_path(root, reference);
    if by_ref.exists() {
        return Ok(Some(by_ref));
    }
    let index = load_index(root)?;
    let Some(digest) = index.refs.get(reference) else {
        return Ok(None);
    };
    let by_digest = digest_cache_path(root, digest);
    if by_digest.exists() {
        return Ok(Some(by_digest));
    }
    Ok(None)
}

pub fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "catalog".to_string()
    } else {
        out
    }
}
