use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use greentic_bundle::catalog::client::{CatalogArtifactClient, FetchedCatalog};
use greentic_bundle::catalog::resolve::{CatalogResolveOptions, resolve_catalogs_with_client};
use tempfile::TempDir;

#[derive(Debug)]
struct StaticClient {
    bytes: Vec<u8>,
}

impl CatalogArtifactClient for StaticClient {
    fn fetch_catalog(&self, _root: &Path, reference: &str) -> Result<FetchedCatalog> {
        Ok(FetchedCatalog {
            resolved_ref: reference.to_string(),
            digest: "sha256:timeout".to_string(),
            bytes: self.bytes.clone(),
        })
    }
}

#[test]
fn catalog_resolution_finishes_within_expected_bound() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    std::fs::create_dir_all(&root).expect("mkdir");
    let client = StaticClient {
        bytes: build_catalog(400).into_bytes(),
    };

    let start = Instant::now();
    let resolved = resolve_catalogs_with_client(
        &root,
        &["oci://example/catalogs/demo:1".to_string()],
        &CatalogResolveOptions {
            offline: false,
            write_cache: false,
        },
        &client,
    )
    .expect("resolve");
    let elapsed = start.elapsed();

    assert_eq!(resolved.discovered_items.len(), 400);
    assert!(
        elapsed < Duration::from_secs(2),
        "catalog resolution took too long: {elapsed:?}"
    );
}

fn build_catalog(items: usize) -> String {
    let mut raw = String::from("{\"registry_version\":\"providers@1\",\"categories\":[");
    for idx in 0..8 {
        if idx > 0 {
            raw.push(',');
        }
        raw.push_str(&format!(
            "{{\"id\":\"cat-{idx}\",\"label\":{{\"fallback\":\"Category {idx}\"}},\"description\":{{\"fallback\":\"Category description {idx}\"}}}}"
        ));
    }
    raw.push_str("],\"items\":[");
    for idx in 0..items {
        if idx > 0 {
            raw.push(',');
        }
        raw.push_str(&format!(
            "{{\"id\":\"provider-{idx}\",\"category\":\"cat-{}\",\"label\":{{\"fallback\":\"Provider {idx}\"}},\"ref\":\"oci://ghcr.io/greenticai/packs/provider-{idx}:latest\"}}",
            idx % 8
        ));
    }
    raw.push_str("]}");
    raw
}
