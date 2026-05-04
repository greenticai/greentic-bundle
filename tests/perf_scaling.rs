use std::path::Path;
use std::sync::Arc;
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
            digest: "sha256:scale".to_string(),
            bytes: self.bytes.clone(),
        })
    }
}

#[test]
fn catalog_resolution_parallel_runs_scale_without_pathological_slowdown() {
    let catalog = Arc::new(build_catalog(250).into_bytes());
    let t1 = run_workload(1, 6, Arc::clone(&catalog));
    let t4 = run_workload(4, 6, Arc::clone(&catalog));
    let t8 = run_workload(8, 6, catalog);

    assert!(
        per_run(t4, 4) <= per_run(t1, 1).mul_f64(1.8),
        "4-thread per-run cost regressed too much: t1={t1:?}, t4={t4:?}"
    );
    assert!(
        per_run(t8, 8) <= per_run(t4, 4).mul_f64(1.8),
        "8-thread per-run cost regressed too much: t4={t4:?}, t8={t8:?}"
    );
}

fn run_workload(threads: usize, iterations_per_thread: usize, catalog: Arc<Vec<u8>>) -> Duration {
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let catalog = Arc::clone(&catalog);
            std::thread::spawn(move || {
                for _ in 0..iterations_per_thread {
                    let temp = TempDir::new().expect("tempdir");
                    let root = temp.path().join("bundle");
                    std::fs::create_dir_all(&root).expect("mkdir");
                    let client = StaticClient {
                        bytes: (*catalog).clone(),
                    };
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
                    assert_eq!(resolved.entries.len(), 1);
                    assert_eq!(resolved.discovered_items.len(), 250);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("join");
    }
    start.elapsed()
}

fn per_run(elapsed: Duration, threads: usize) -> Duration {
    elapsed.div_f64(threads as f64)
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
            "{{\"id\":\"provider-{idx}\",\"category\":\"cat-{}\",\"label\":{{\"fallback\":\"Provider {idx}\"}},\"ref\":\"oci://ghcr.io/greenticai/packs/provider-{idx}:stable\"}}",
            idx % 8
        ));
    }
    raw.push_str("]}");
    raw
}
