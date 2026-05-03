use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use greentic_distributor_client::{
    DistClient, DistOptions, ReleaseChannel, ReleaseIndex, ReleaseResolutionContext,
};

const STABLE_OCI_SOURCES: &[&str] = &[
    include_str!("../registries/providers.json"),
    include_str!("../src/catalog/registry.rs"),
    include_str!("../src/cli/info/pack_probe.rs"),
    include_str!("../src/project/mod.rs"),
    include_str!("../src/wizard/mod.rs"),
    include_str!("../crates/cards2pack-core/src/emit.rs"),
];

#[test]
fn stable_oci_refs_used_by_code_resolve_from_local_distribution_cache() {
    let refs = stable_oci_refs_used_by_code();
    assert!(!refs.is_empty(), "expected at least one stable OCI ref");

    let opts = DistOptions {
        offline: true,
        ..DistOptions::default()
    };
    let release_indexes = stable_release_indexes(&opts.cache_dir);
    if release_indexes.is_empty() {
        let release_index_dir = opts
            .cache_dir
            .join("release-index")
            .join("v1")
            .join("stable");
        if stable_oci_cache_test_is_required() {
            panic!(
                "no stable release index is present in the local distribution cache at {}",
                release_index_dir.display()
            );
        }
        eprintln!(
            "skipping stable OCI cache validation: no stable release index is present at {}",
            release_index_dir.display()
        );
        return;
    }

    let client = DistClient::new(opts);
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");

    let mut failures = Vec::new();
    for reference in refs {
        let mut resolved = None;
        let mut resolve_errors = Vec::new();
        for (ctx, path) in &release_indexes {
            match runtime.block_on(client.resolve_oci_ref_with_context(&reference, ctx)) {
                Ok(descriptor) => {
                    resolved = Some(descriptor);
                    break;
                }
                Err(err) => resolve_errors.push(format!(
                    "{reference} via {} ({}): {err}",
                    ctx.release,
                    path.display()
                )),
            }
        }

        let Some(descriptor) = resolved else {
            failures.push(format!(
                "{reference} could not be resolved from the local stable release index:\n{}",
                resolve_errors.join("\n")
            ));
            continue;
        };

        if !descriptor.canonical_ref.contains("@sha256:") {
            failures.push(format!(
                "{reference} resolved to non-digest-pinned ref {}",
                descriptor.canonical_ref
            ));
            continue;
        }

        match client.open_cached(&descriptor.digest) {
            Ok(artifact) => {
                let path = artifact.cache_path.as_ref().unwrap_or(&artifact.local_path);
                match std::fs::metadata(path) {
                    Ok(metadata) if metadata.len() > 0 => {}
                    Ok(_) => failures.push(format!(
                        "{reference} cached artifact is empty at {}",
                        path.display()
                    )),
                    Err(err) => failures.push(format!(
                        "{reference} cached artifact is missing at {}: {err}",
                        path.display()
                    )),
                }
            }
            Err(err) => failures.push(format!(
                "{reference} resolved to {}, but the artifact could not be opened from the local cache: {err}",
                descriptor.canonical_ref
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "stable OCI refs missing from local distribution cache/release index:\n{}",
        failures.join("\n\n")
    );
}

fn stable_release_indexes(cache_dir: &Path) -> Vec<(ReleaseResolutionContext, PathBuf)> {
    let release_index_dir = cache_dir.join("release-index").join("v1").join("stable");
    let Ok(entries) = std::fs::read_dir(&release_index_dir) else {
        return Vec::new();
    };

    let mut indexes = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            let bytes = std::fs::read(&path).ok()?;
            let index = serde_json::from_slice::<ReleaseIndex>(&bytes).ok()?;
            if index.channel != ReleaseChannel::Stable {
                return None;
            }
            Some((
                ReleaseResolutionContext {
                    release: index.release,
                    channel: ReleaseChannel::Stable,
                },
                path,
            ))
        })
        .collect::<Vec<_>>();

    indexes
        .sort_by(|(left, _), (right, _)| compare_release_versions(&right.release, &left.release));
    indexes
}

fn compare_release_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_segments = numeric_version_segments(left);
    let right_segments = numeric_version_segments(right);
    left_segments
        .cmp(&right_segments)
        .then_with(|| left.cmp(right))
}

fn numeric_version_segments(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|segment| segment.parse::<u64>().unwrap_or(0))
        .collect()
}

fn stable_oci_cache_test_is_required() -> bool {
    std::env::var("GREENTIC_REQUIRE_STABLE_OCI_CACHE").is_ok_and(|value| value == "1")
}

fn stable_oci_refs_used_by_code() -> BTreeSet<String> {
    STABLE_OCI_SOURCES
        .iter()
        .flat_map(|source| stable_oci_refs_in(source))
        .filter(|reference| reference.starts_with("oci://ghcr.io/greenticai/"))
        .collect()
}

fn stable_oci_refs_in(source: &str) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    let mut rest = source;
    while let Some(start) = rest.find("oci://") {
        rest = &rest[start..];
        let end = rest
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | ')' | ']' | '}')
            })
            .unwrap_or(rest.len());
        let reference = rest[..end]
            .trim_end_matches([',', '.', ';', ':'])
            .to_string();
        if reference.ends_with(":stable") {
            refs.insert(reference);
        }
        rest = &rest[end..];
    }
    refs
}
