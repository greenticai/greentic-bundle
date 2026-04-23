use assert_cmd::Command;
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

fn build_fixture_artifact(root: &std::path::Path) -> std::path::PathBuf {
    init_workspace(root);
    let artifact = root.join("demo-bundle.gtbundle");
    cargo_bin()
        .args([
            "build",
            "--root",
            root.to_str().expect("utf8 root"),
            "--output",
            artifact.to_str().expect("utf8 artifact"),
        ])
        .env("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")
        .assert()
        .success();
    artifact
}

#[test]
fn artifact_info_human() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = build_fixture_artifact(&root);

    let out = cargo_bin()
        .args(["info", artifact.to_str().expect("utf8 artifact")])
        .env("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")
        .output()
        .expect("run info");

    assert!(
        out.status.success(),
        "info on artifact failed. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Mode") || stdout.contains("Bundle ID"),
        "expected human output to include Mode or Bundle ID, got: {stdout}"
    );
    assert!(
        stdout.contains("Access"),
        "expected human output to include Access, got: {stdout}"
    );
}

#[test]
fn artifact_info_json() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = build_fixture_artifact(&root);

    let out = cargo_bin()
        .args(["info", artifact.to_str().expect("utf8 artifact"), "--json"])
        .env("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")
        .output()
        .expect("run info --json");

    assert!(
        out.status.success(),
        "info --json on artifact failed. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["info_schema_version"], 1);
    assert!(v["app_packs"].is_array());
    assert!(v["access"].is_object());
}

#[test]
fn workspace_info_human() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    init_workspace(&root);

    let out = cargo_bin()
        .args(["info", root.to_str().expect("utf8 root")])
        .env("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")
        .output()
        .expect("run info on workspace");

    assert!(
        out.status.success(),
        "info on workspace failed. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Mode") || stdout.contains("Bundle ID"),
        "expected human output to include Mode or Bundle ID, got: {stdout}"
    );
}

#[test]
fn missing_file_exits_2() {
    let out = cargo_bin()
        .args(["info", "/nope/does-not-exist.gtbundle"])
        .output()
        .expect("run info on missing path");

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for missing path, got {:?}. stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn wrong_extension_exits_2() {
    // Use Cargo.toml from CARGO_MANIFEST_DIR as a file that exists but has the wrong extension.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let wrong = std::path::Path::new(manifest_dir).join("Cargo.toml");
    assert!(wrong.exists(), "fixture Cargo.toml should exist");

    let out = cargo_bin()
        .args(["info", wrong.to_str().expect("utf8 path")])
        .output()
        .expect("run info on wrong extension");

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 for wrong extension, got {:?}. stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}
