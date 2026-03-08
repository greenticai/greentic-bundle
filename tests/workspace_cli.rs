use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn cargo_bin() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-bundle"))
}

#[test]
fn init_execute_creates_workspace_scaffold() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");

    let output = cargo_bin()
        .args([
            "init",
            root.to_str().expect("utf8 root"),
            "--bundle-name",
            "Demo Bundle",
            "--bundle-id",
            "demo-bundle",
            "--execute",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let preview: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(
        preview.pointer("/bundle_id").and_then(Value::as_str),
        Some("demo-bundle")
    );
    assert!(root.join("bundle.yaml").exists());
    assert!(root.join("bundle.lock.json").exists());
    let bundle_yaml = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("hooks: []"));
    assert!(bundle_yaml.contains("subscriptions: []"));
    assert!(bundle_yaml.contains("capabilities: []"));
}

#[test]
fn add_app_pack_execute_updates_bundle_yaml_and_lock() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    cargo_bin()
        .args([
            "init",
            root.to_str().expect("utf8 root"),
            "--bundle-name",
            "Demo Bundle",
            "--bundle-id",
            "demo-bundle",
            "--execute",
        ])
        .assert()
        .success();

    cargo_bin()
        .args([
            "add",
            "app-pack",
            "pack-b",
            "--root",
            root.to_str().expect("utf8 root"),
            "--execute",
        ])
        .assert()
        .success();

    let bundle_yaml = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("app_packs:\n  - pack-b"));
    let lock = fs::read_to_string(root.join("bundle.lock.json")).expect("lock");
    assert!(lock.contains("\"reference\": \"pack-b\""));
}

#[test]
fn remove_extension_provider_execute_updates_bundle_yaml_and_lock() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    cargo_bin()
        .args([
            "init",
            root.to_str().expect("utf8 root"),
            "--bundle-name",
            "Demo Bundle",
            "--bundle-id",
            "demo-bundle",
            "--execute",
        ])
        .assert()
        .success();
    cargo_bin()
        .args([
            "add",
            "extension-provider",
            "provider-a",
            "--root",
            root.to_str().expect("utf8 root"),
            "--execute",
        ])
        .assert()
        .success();

    cargo_bin()
        .args([
            "remove",
            "extension-provider",
            "provider-a",
            "--root",
            root.to_str().expect("utf8 root"),
            "--execute",
        ])
        .assert()
        .success();

    let bundle_yaml = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("extension_providers: []"));
    let lock = fs::read_to_string(root.join("bundle.lock.json")).expect("lock");
    assert!(!lock.contains("\"reference\": \"provider-a\""));
}

#[test]
fn add_dry_run_does_not_write_workspace() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    cargo_bin()
        .args([
            "init",
            root.to_str().expect("utf8 root"),
            "--bundle-name",
            "Demo Bundle",
            "--bundle-id",
            "demo-bundle",
            "--execute",
        ])
        .assert()
        .success();

    let before = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml");
    cargo_bin()
        .args([
            "add",
            "app-pack",
            "pack-z",
            "--root",
            root.to_str().expect("utf8 root"),
            "--dry-run",
        ])
        .assert()
        .success();
    let after = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml after");
    assert_eq!(before, after);
}
