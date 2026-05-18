use std::fs;

use assert_cmd::Command;
use greentic_bundle::catalog::registry::bundled_provider_registry_entries;
use serde_json::Value;
use tempfile::TempDir;

fn cargo_bin() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("greentic-bundle"))
}

fn init_workspace(root: &std::path::Path) {
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
    init_workspace(&root);

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
fn add_all_bundled_provider_registry_entries_execute_successfully() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    let entries = bundled_provider_registry_entries().expect("bundled provider registry");
    assert!(
        !entries.is_empty(),
        "bundled provider registry should not be empty"
    );

    for entry in &entries {
        cargo_bin()
            .env("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")
            .args([
                "add",
                "extension-provider",
                &entry.reference,
                "--root",
                root.to_str().expect("utf8 root"),
                "--execute",
            ])
            .assert()
            .success();
    }

    let bundle_yaml = fs::read_to_string(root.join("bundle.yaml")).expect("bundle yaml");
    let lock = fs::read_to_string(root.join("bundle.lock.json")).expect("lock");
    for entry in &entries {
        assert!(
            bundle_yaml.contains(&entry.reference),
            "bundle.yaml is missing bundled provider {} ({})",
            entry.id,
            entry.reference
        );
        assert!(
            lock.contains(&entry.reference),
            "bundle.lock.json is missing bundled provider {} ({})",
            entry.id,
            entry.reference
        );
    }
}

#[test]
fn remove_extension_provider_execute_updates_bundle_yaml_and_lock() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);
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
    init_workspace(&root);

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

#[test]
fn add_duplicate_app_pack_reports_no_change() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    cargo_bin()
        .args([
            "add",
            "app-pack",
            "pack-a",
            "--root",
            root.to_str().expect("utf8 root"),
            "--execute",
        ])
        .assert()
        .success();

    let output = cargo_bin()
        .args([
            "add",
            "app-pack",
            "pack-a",
            "--root",
            root.to_str().expect("utf8 root"),
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let preview: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(preview.get("changed").and_then(Value::as_bool), Some(false));
    assert_eq!(
        preview.pointer("/next_values/0").and_then(Value::as_str),
        Some("pack-a")
    );
}

#[test]
fn remove_missing_extension_provider_reports_no_change() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    let output = cargo_bin()
        .args([
            "remove",
            "extension-provider",
            "provider-missing",
            "--root",
            root.to_str().expect("utf8 root"),
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let preview: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(preview.get("changed").and_then(Value::as_bool), Some(false));
    assert_eq!(
        preview.pointer("/field").and_then(Value::as_str),
        Some("extension_providers")
    );
}

#[test]
fn init_preview_defaults_name_and_normalizes_bundle_id_from_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("My Demo_Bundle");

    let output = cargo_bin()
        .args(["init", root.to_str().expect("utf8 root")])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let preview: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(
        preview.pointer("/bundle_name").and_then(Value::as_str),
        Some("My Demo_Bundle")
    );
    assert_eq!(
        preview.pointer("/bundle_id").and_then(Value::as_str),
        Some("my-demo-bundle")
    );
    assert!(
        !root.exists(),
        "preview mode must not create workspace files"
    );
}

#[test]
fn inspect_accepts_positional_gtbundle_and_lists_contents() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);
    let artifact = root.join("demo-bundle.gtbundle");

    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    let output = cargo_bin()
        .arg("inspect")
        .arg(artifact.to_str().expect("utf8 artifact"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("utf8 stdout");

    assert!(stdout.contains("bundle.yaml"));
    assert!(stdout.contains("bundle-lock.json"));
    assert!(!stdout.contains("\"kind\""));
}

#[test]
fn inspect_json_flag_keeps_structured_output_for_artifacts() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);
    let artifact = root.join("demo-bundle.gtbundle");

    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    let output = cargo_bin()
        .args([
            "inspect",
            artifact.to_str().expect("utf8 artifact"),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(report.get("kind").and_then(Value::as_str), Some("artifact"));
    assert!(report.get("contents").and_then(Value::as_array).is_some());
}

#[test]
fn doctor_artifact_flag_takes_precedence_over_root() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let bogus_root = temp.path().join("missing-root");
    init_workspace(&root);
    let artifact = root.join("demo-bundle.gtbundle");

    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    let output = cargo_bin()
        .args([
            "doctor",
            "--root",
            bogus_root.to_str().expect("utf8 bogus root"),
            "--artifact",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(
        report.get("target").and_then(Value::as_str),
        Some(artifact.to_str().expect("utf8 artifact"))
    );
    assert_eq!(report.get("ok").and_then(Value::as_bool), Some(true));
}

#[test]
fn doctor_without_artifact_checks_workspace_root() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    let output = cargo_bin()
        .args(["doctor", "--root", root.to_str().expect("utf8 root")])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(
        report.get("target").and_then(Value::as_str),
        Some(root.to_str().expect("utf8 root"))
    );
    assert!(report.get("checks").and_then(Value::as_array).is_some());
}

// Phase 0 P0.2: CI relies on `doctor` exiting non-zero when the secret-leak
// scanner flags findings. We can't produce a leaky artifact through the
// normal build pipeline (P0.1's redactor + writer denylist combine to refuse
// it), so this test bypasses the redactor at the library level — overwrite
// the redacted setup-state with a leaky one between `write_normalized_build_dir`
// and `bundle_fs::write_bundle` — then invokes the CLI doctor and asserts
// the binary exits non-zero with the leak finding in its JSON output.
#[test]
fn doctor_cli_exits_non_zero_when_artifact_carries_plaintext_secret() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    cargo_bin()
        .args(["build", "--root", root.to_str().expect("utf8 root")])
        .assert()
        .success();
    let build_dir = root.join("state/build/demo-bundle/normalized");
    let leaky_state = build_dir.join("state/setup/leaky-provider.json");
    fs::create_dir_all(leaky_state.parent().expect("setup dir")).expect("setup dir");
    fs::write(
        &leaky_state,
        br#"{
            "schema_version":1,
            "provider_id":"leaky-provider",
            "source_kind":"legacy",
            "form":{"id":"leaky-provider-setup","title":"Leaky","version":"1.0.0","questions":[]},
            "normalized_answers":{},
            "non_secret_config":{},
            "secret_values":{"api_token":"sk-LEAK-VIA-CLI-DOCTOR"}
        }"#,
    )
    .expect("write leaky state");

    let artifact = root.join("leaky.gtbundle");
    greentic_bundle::bundle_fs::write_bundle(&build_dir, &artifact)
        .expect("write_bundle ignores JSON contents (only blocks dev-store paths)");

    let result = cargo_bin()
        .args([
            "doctor",
            "--artifact",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .output()
        .expect("invoke doctor");
    assert!(
        !result.status.success(),
        "doctor must exit non-zero on leak; status: {:?}",
        result.status
    );
    let report: Value = serde_json::from_slice(&result.stdout).expect("json on stdout");
    assert_eq!(report.get("ok").and_then(Value::as_bool), Some(false));
    let checks = report
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks array");
    assert!(
        checks.iter().any(|check| check
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.contains("secret_values populated"))),
        "missing secret_values finding in {checks:?}"
    );
}

// Phase 0 P0.2: positive-side coverage that the doctor's secret-leak scan
// surfaces in the CLI JSON output for clean bundles.
#[test]
fn doctor_cli_reports_secret_leak_scan_check_on_clean_bundle() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);
    let artifact = root.join("clean.gtbundle");
    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    let output = cargo_bin()
        .args([
            "doctor",
            "--artifact",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(report.get("ok").and_then(Value::as_bool), Some(true));
    let checks = report
        .get("checks")
        .and_then(Value::as_array)
        .expect("checks array");
    assert!(
        checks.iter().any(|check| {
            check.get("name").and_then(Value::as_str) == Some("secret-leak scan")
                && check.get("ok").and_then(Value::as_bool) == Some(true)
        }),
        "missing passing secret-leak scan check in {checks:?}"
    );
}

#[test]
fn build_dry_run_reports_artifact_path_without_writing_it() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = root.join("dry-run.gtbundle");
    init_workspace(&root);

    let output = cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(
        report.get("artifact_path").and_then(Value::as_str),
        Some(artifact.to_str().expect("utf8 artifact"))
    );
    assert!(!artifact.exists(), "dry-run build must not write artifact");
}

#[test]
fn export_dry_run_reports_output_without_writing_artifact() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = root.join("exported.gtbundle");
    init_workspace(&root);

    cargo_bin()
        .args(["build", "--root", root.to_str().expect("utf8 root")])
        .assert()
        .success();

    let build_dir = root.join("state/build/demo-bundle/normalized");
    let output = cargo_bin()
        .args([
            "export",
            "--build-dir",
            build_dir.to_str().expect("utf8 build dir"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(
        report.get("artifact_path").and_then(Value::as_str),
        Some(artifact.to_str().expect("utf8 artifact"))
    );
    assert!(!artifact.exists(), "dry-run export must not write artifact");
}

#[test]
fn inspect_directory_target_reports_workspace_kind() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    let output = cargo_bin()
        .arg("inspect")
        .arg(root.to_str().expect("utf8 root"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(
        report.get("kind").and_then(Value::as_str),
        Some("workspace")
    );
    assert_eq!(
        report.get("target").and_then(Value::as_str),
        Some(root.to_str().expect("utf8 root"))
    );
}

#[test]
fn unbundle_extracts_into_specified_output_dir() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let out = temp.path().join("expanded");
    init_workspace(&root);
    let artifact = root.join("demo-bundle.gtbundle");

    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    cargo_bin()
        .args([
            "unbundle",
            artifact.to_str().expect("utf8 artifact"),
            "--out",
            out.to_str().expect("utf8 out"),
        ])
        .assert()
        .success();

    assert!(out.join("bundle.yaml").exists());
    assert!(out.join("bundle-lock.json").exists());
}

#[test]
fn unbundle_defaults_to_current_directory() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&cwd).expect("cwd");
    init_workspace(&root);
    let artifact = root.join("demo-bundle.gtbundle");

    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .assert()
        .success();

    cargo_bin()
        .current_dir(&cwd)
        .args(["unbundle", artifact.to_str().expect("utf8 artifact")])
        .assert()
        .success();

    assert!(cwd.join("bundle.yaml").exists());
    assert!(cwd.join("bundle-lock.json").exists());
}
