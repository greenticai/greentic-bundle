use std::fs;
use std::path::Path;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use greentic_bundle::access::{GmapPath, GmapRule, Policy, eval_policy};
use greentic_bundle::build::plan::build_state;
use greentic_bundle::catalog::client::{CatalogArtifactClient, FetchedCatalog};
use greentic_bundle::catalog::resolve::{CatalogResolveOptions, resolve_catalogs_with_client};
use serde_json::json;
use tempfile::TempDir;

#[derive(Debug)]
struct StaticClient {
    bytes: Vec<u8>,
}

impl CatalogArtifactClient for StaticClient {
    fn fetch_catalog(&self, _root: &Path, reference: &str) -> anyhow::Result<FetchedCatalog> {
        Ok(FetchedCatalog {
            resolved_ref: reference.to_string(),
            digest: "sha256:bench".to_string(),
            bytes: self.bytes.clone(),
        })
    }
}

fn bench_catalog_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("catalog_resolution");
    for size in [50usize, 200usize, 500usize] {
        let catalog = build_catalog(size);
        let client = StaticClient {
            bytes: catalog.clone().into_bytes(),
        };

        group.bench_with_input(BenchmarkId::new("remote_no_cache", size), &size, |b, _| {
            b.iter_batched(
                || TempDir::new().expect("tempdir"),
                |temp| {
                    let root = temp.path().join("bundle");
                    fs::create_dir_all(&root).expect("mkdir");
                    let result = resolve_catalogs_with_client(
                        &root,
                        &["oci://example/catalogs/demo:1".to_string()],
                        &CatalogResolveOptions {
                            offline: false,
                            write_cache: false,
                        },
                        &client,
                    )
                    .expect("resolve");
                    assert_eq!(result.discovered_items.len(), size);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_i18n_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("i18n");
    group.bench_function("tr_for_locale_fallback", |b| {
        b.iter(|| {
            let value = greentic_bundle::i18n::tr_for("en-US", "cli.root.about");
            assert!(!value.is_empty());
        });
    });
    group.bench_function("trf_for_substitution", |b| {
        b.iter(|| {
            let value = greentic_bundle::i18n::trf_for(
                "en-US",
                "errors.i18n.missing_locale",
                &[("locale", "en-US")],
            );
            assert!(value.contains("en-US"));
        });
    });
    group.finish();
}

fn bench_build_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_state");
    for files in [10usize, 50usize, 100usize] {
        group.bench_with_input(
            BenchmarkId::new("resolved_targets", files),
            &files,
            |b, &files| {
                b.iter_batched(
                    || create_build_state_workspace(files),
                    |temp| {
                        let root = temp.path().join("bundle");
                        let state = build_state(&root).expect("build state");
                        assert_eq!(state.manifest.resolved_targets.len(), files);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_access_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("access_eval");
    for rules in [100usize, 1000usize, 5000usize] {
        let input = build_access_rules(rules);
        let target = GmapPath {
            pack: Some("pack-7".to_string()),
            flow: Some("main".to_string()),
            node: Some("node-3".to_string()),
        };
        group.bench_with_input(BenchmarkId::new("eval_policy", rules), &rules, |b, _| {
            b.iter(|| {
                let decision = eval_policy(&input, &target).expect("decision");
                assert_eq!(decision.rank, 5);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_catalog_resolution,
    bench_i18n_lookup,
    bench_build_state,
    bench_access_eval
);
criterion_main!(benches);

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

fn create_build_state_workspace(files: usize) -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    fs::create_dir_all(root.join("resolved")).expect("resolved dir");
    fs::write(
        root.join("bundle.yaml"),
        "schema_version: 1\nbundle_id: demo-bundle\nbundle_name: Demo Bundle\nlocale: en\nmode: create\napp_packs: []\nextension_providers: []\nremote_catalogs: []\n",
    )
    .expect("bundle yaml");
    fs::write(
        root.join("bundle.lock.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "bundle_id": "demo-bundle",
            "requested_mode": "create",
            "execution": "execute",
            "cache_policy": "workspace-local",
            "tool_version": "0.4.28",
            "build_format_version": "bundle-lock-v1",
            "workspace_root": "bundle.yaml",
            "lock_file": "bundle.lock.json",
            "catalogs": [],
            "app_packs": [],
            "extension_providers": [],
            "setup_state_files": []
        }))
        .expect("lock json"),
    )
    .expect("lock");
    for idx in 0..files {
        fs::write(
            root.join("resolved").join(format!("target-{idx}.yaml")),
            build_resolved_target(idx),
        )
        .expect("resolved target");
    }
    temp
}

fn build_resolved_target(idx: usize) -> String {
    format!(
        "version: 1\ntenant: tenant-{idx}\nteam: team-{idx}\npolicy:\n  default: allowed\n  source:\n    tenant_gmap: tenants/tenant-{idx}/tenant.gmap\n    team_gmap: tenants/tenant-{idx}/teams/team-{idx}.gmap\napp_packs:\n  - reference: repo://packs/a-{idx}@1\n    policy: allowed\n  - reference: repo://packs/b-{idx}@1\n    policy: forbidden\n"
    )
}

fn build_access_rules(count: usize) -> Vec<GmapRule> {
    let mut rules = Vec::with_capacity(count + 2);
    rules.push(GmapRule {
        path: GmapPath {
            pack: None,
            flow: None,
            node: None,
        },
        policy: Policy::Forbidden,
        line: 1,
    });
    for idx in 0..count {
        rules.push(GmapRule {
            path: GmapPath {
                pack: Some(format!("pack-{}", idx % 20)),
                flow: Some(if idx % 3 == 0 {
                    "_".to_string()
                } else {
                    "main".to_string()
                }),
                node: (idx % 5 == 0).then(|| format!("node-{}", idx % 11)),
            },
            policy: if idx % 2 == 0 {
                Policy::Public
            } else {
                Policy::Forbidden
            },
            line: idx + 2,
        });
    }
    rules.push(GmapRule {
        path: GmapPath {
            pack: Some("pack-7".to_string()),
            flow: Some("main".to_string()),
            node: Some("node-3".to_string()),
        },
        policy: Policy::Public,
        line: count + 2,
    });
    rules
}
