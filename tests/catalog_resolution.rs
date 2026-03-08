use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;
use tempfile::TempDir;

fn bundle_bin() -> &'static str {
    env!("CARGO_BIN_EXE_greentic-bundle")
}

#[test]
fn local_catalog_resolution_writes_deterministic_lock_and_cache() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let catalog = temp.path().join("providers.json");
    fs::write(
        &catalog,
        r#"[{"id":"alpha","label":"Alpha","reference":"repo://packs/alpha@1"},{"id":"beta","label":"Beta","reference":"repo://packs/beta@1"}]"#,
    )
    .expect("catalog");

    let answers = temp.path().join("answers.json");
    fs::write(
        &answers,
        format!(
            r#"{{
  "wizard_id":"greentic-bundle.wizard.run",
  "schema_id":"greentic-bundle.wizard.answers",
  "schema_version":"1.0.0",
  "locale":"en",
  "answers":{{
    "mode":"create",
    "bundle_name":"Demo Bundle",
    "bundle_id":"demo-bundle",
    "output_dir":"{}",
    "advanced_setup":true,
    "app_packs":["pack-b","pack-a"],
    "extension_providers":["provider-b","provider-a"],
    "remote_catalogs":["file://{}"],
    "setup_execution_intent":false,
    "export_intent":false
  }},
  "locks":{{}}
}}"#,
            root.display(),
            catalog.display()
        ),
    )
    .expect("answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers)
        .output()
        .expect("wizard apply");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let lock = read_json(&root.join("bundle.lock.json"));
    assert_eq!(
        lock.get("bundle_id").and_then(Value::as_str),
        Some("demo-bundle")
    );
    assert_eq!(
        lock.pointer("/catalogs/0/item_ids/0")
            .and_then(Value::as_str),
        Some("alpha")
    );
    assert_eq!(
        lock.pointer("/catalogs/0/item_ids/1")
            .and_then(Value::as_str),
        Some("beta")
    );
    assert_eq!(
        lock.pointer("/app_packs/0/reference")
            .and_then(Value::as_str),
        Some("pack-a")
    );
    assert_eq!(
        lock.pointer("/app_packs/1/reference")
            .and_then(Value::as_str),
        Some("pack-b")
    );
    assert_eq!(
        lock.pointer("/extension_providers/0/reference")
            .and_then(Value::as_str),
        Some("provider-a")
    );
    let cache_path = lock
        .pointer("/catalogs/0/cache_path")
        .and_then(Value::as_str)
        .expect("cache path");
    assert!(root.join(cache_path).exists());
    assert!(root.join("state/cache/catalogs/index.json").exists());
}

#[test]
fn offline_replay_uses_workspace_cached_catalog() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let reference = "oci://ghcr.io/greentic/catalogs/demo:1";
    let digest = "sha256:abc123";
    let digest_cache = root
        .join("state/cache/catalogs/by-digest")
        .join(format!("{digest}.json"));
    fs::create_dir_all(digest_cache.parent().expect("parent")).expect("mkdir");
    fs::write(
        &digest_cache,
        r#"[{"id":"cached","label":"Cached","reference":"repo://packs/cached@1"}]"#,
    )
    .expect("digest cache");
    fs::create_dir_all(root.join("state/cache/catalogs")).expect("mkdir cache");
    fs::write(
        root.join("state/cache/catalogs/index.json"),
        format!(r#"{{"refs":{{"{reference}":"{digest}"}}}}"#),
    )
    .expect("index");

    let resolution = greentic_bundle::catalog::resolve::resolve_catalogs(
        &root,
        &[reference.to_string()],
        &greentic_bundle::catalog::resolve::CatalogResolveOptions {
            offline: true,
            write_cache: false,
        },
    )
    .expect("resolve from cache");

    assert_eq!(resolution.entries.len(), 1);
    assert_eq!(resolution.entries[0].source, "workspace_cache");
    assert_eq!(resolution.entries[0].item_ids, vec!["cached".to_string()]);
    assert!(resolution.cache_writes.is_empty());
}

#[test]
fn offline_without_cache_returns_recovery_hint() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let error = greentic_bundle::catalog::resolve::resolve_catalogs(
        &root,
        &["oci://ghcr.io/greentic/catalogs/missing:1".to_string()],
        &greentic_bundle::catalog::resolve::CatalogResolveOptions {
            offline: true,
            write_cache: false,
        },
    )
    .expect_err("expected offline error");

    let message = error.to_string();
    assert!(message.contains("seed the workspace-local cache first"));
    assert!(message.contains("offline mode is enabled"));
}

#[test]
fn ghcr_shortcut_remote_ref_uses_catalog_client_seam() {
    #[derive(Debug)]
    struct FakeClient;

    impl greentic_bundle::catalog::client::CatalogArtifactClient for FakeClient {
        fn fetch_catalog(
            &self,
            _root: &Path,
            reference: &str,
        ) -> Result<greentic_bundle::catalog::client::FetchedCatalog> {
            let mapped = greentic_bundle::catalog::client::map_remote_catalog_reference(reference)?;
            Ok(greentic_bundle::catalog::client::FetchedCatalog {
                resolved_ref: mapped.oci_reference,
                digest: "sha256:feedbeef".to_string(),
                bytes: br#"[{"id":"well-known","label":"Well Known","reference":"repo://packs/well-known@1"}]"#
                    .to_vec(),
            })
        }
    }

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let resolution = greentic_bundle::catalog::resolve::resolve_catalogs_with_client(
        &root,
        &["ghcr://packs/well-known.json".to_string()],
        &greentic_bundle::catalog::resolve::CatalogResolveOptions {
            offline: false,
            write_cache: true,
        },
        &FakeClient,
    )
    .expect("resolve");

    assert_eq!(resolution.entries.len(), 1);
    assert_eq!(
        resolution.entries[0].resolved_ref,
        "ghcr.io/greenticai/packs/well-known.json:latest"
    );
    assert_eq!(resolution.entries[0].source, "remote");
    assert_eq!(
        resolution.entries[0].item_ids,
        vec!["well-known".to_string()]
    );
}

#[test]
fn inspect_prints_sorted_lock_output() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(
        root.join("bundle.yaml"),
        "schema_version: 1\nbundle_id: demo-bundle\nbundle_name: Demo Bundle\nlocale: en\nmode: create\napp_packs: []\nextension_providers: []\nremote_catalogs: []\n",
    )
    .expect("bundle yaml");
    fs::write(
        root.join("bundle.lock.json"),
        r#"{
  "schema_version": 1,
  "bundle_id": "demo-bundle",
  "requested_mode": "create",
  "execution": "execute",
  "cache_policy": "workspace-local",
  "tool_version": "0.1.0",
  "build_format_version": "bundle-lock-v1",
  "workspace_root": "bundle.yaml",
  "lock_file": "bundle.lock.json",
  "catalogs": [],
  "app_packs": [],
  "extension_providers": [],
  "setup_state_files": []
}"#,
    )
    .expect("lock");

    let output = Command::new(bundle_bin())
        .args(["inspect", "--root"])
        .arg(&root)
        .output()
        .expect("inspect");
    assert!(output.status.success());

    let printed: Value = serde_json::from_slice(&output.stdout).expect("printed inspect json");
    assert_eq!(
        printed.pointer("/lock/bundle_id").and_then(Value::as_str),
        Some("demo-bundle")
    );
    assert_eq!(
        printed
            .pointer("/manifest/bundle_id")
            .and_then(Value::as_str),
        Some("demo-bundle")
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}
