use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn bundle_bin() -> &'static str {
    env!("CARGO_BIN_EXE_greentic-bundle")
}

#[test]
fn access_dry_run_prints_preview_without_writing_workspace() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");

    let output = Command::new(bundle_bin())
        .args([
            "access",
            "allow",
            "pack-a/main",
            "--root",
            root.to_str().expect("root"),
            "--tenant",
            "tenant-a",
        ])
        .output()
        .expect("run access dry-run");

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.exists(), "dry-run must not create workspace files");

    let preview: Value = serde_json::from_slice(&output.stdout).expect("parse preview");
    assert_eq!(
        preview.get("gmap_path").and_then(Value::as_str),
        Some("tenants/tenant-a/tenant.gmap")
    );
    assert_eq!(
        preview.pointer("/writes/1").and_then(Value::as_str),
        Some("resolved/tenant-a.yaml")
    );
    assert_eq!(
        preview.pointer("/writes/2").and_then(Value::as_str),
        Some("state/resolved/tenant-a.yaml")
    );
}

#[test]
fn access_execute_updates_tenant_gmap_and_resolved_outputs() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");

    let output = Command::new(bundle_bin())
        .args([
            "access",
            "allow",
            "pack-a/main",
            "--root",
            root.to_str().expect("root"),
            "--tenant",
            "tenant-a",
            "--execute",
        ])
        .output()
        .expect("run access execute");

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let tenant_gmap = root.join("tenants/tenant-a/tenant.gmap");
    assert_eq!(
        fs::read_to_string(&tenant_gmap).expect("tenant gmap"),
        "_ = forbidden\npack-a/main = public\n"
    );

    assert_resolved_manifest(
        &root.join("resolved/tenant-a.yaml"),
        "tenant-a",
        None,
        "tenants/tenant-a/tenant.gmap",
        None,
    );
    assert_resolved_manifest(
        &root.join("state/resolved/tenant-a.yaml"),
        "tenant-a",
        None,
        "tenants/tenant-a/tenant.gmap",
        None,
    );
}

#[test]
fn access_execute_updates_team_gmap_and_team_resolved_output() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");

    let output = Command::new(bundle_bin())
        .args([
            "access",
            "forbid",
            "pack-a/main/node-x",
            "--root",
            root.to_str().expect("root"),
            "--tenant",
            "tenant-a",
            "--team",
            "ops",
            "--execute",
        ])
        .output()
        .expect("run team access execute");

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let tenant_gmap = root.join("tenants/tenant-a/tenant.gmap");
    assert_eq!(
        fs::read_to_string(&tenant_gmap).expect("tenant gmap"),
        "_ = forbidden\n"
    );

    let team_gmap = root.join("tenants/tenant-a/teams/ops/team.gmap");
    assert_eq!(
        fs::read_to_string(&team_gmap).expect("team gmap"),
        "_ = forbidden\npack-a/main/node-x = forbidden\n"
    );

    assert_resolved_manifest(
        &root.join("resolved/tenant-a.ops.yaml"),
        "tenant-a",
        Some("ops"),
        "tenants/tenant-a/tenant.gmap",
        Some("tenants/tenant-a/teams/ops/team.gmap"),
    );
}

#[test]
fn upsert_policy_preserves_comments_and_blank_lines() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("tenant.gmap");
    fs::write(&path, "# access rules\n\n_ = forbidden\n").expect("seed gmap");

    greentic_bundle::access::upsert_policy(
        &path,
        "pack-a/main",
        greentic_bundle::access::Policy::Public,
    )
    .expect("upsert");

    assert_eq!(
        fs::read_to_string(&path).expect("read gmap"),
        "# access rules\n\n_ = forbidden\n\npack-a/main = public\n"
    );
}

#[test]
fn upsert_policy_rewrites_existing_rule_without_duplication() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("tenant.gmap");
    fs::write(&path, "_ = forbidden\npack-a/main = forbidden\n").expect("seed gmap");

    greentic_bundle::access::upsert_policy(
        &path,
        "pack-a/main",
        greentic_bundle::access::Policy::Public,
    )
    .expect("upsert");

    assert_eq!(
        fs::read_to_string(&path).expect("read gmap"),
        "_ = forbidden\npack-a/main = public\n"
    );
}

fn assert_resolved_manifest(
    path: &Path,
    tenant: &str,
    team: Option<&str>,
    tenant_gmap: &str,
    team_gmap: Option<&str>,
) {
    let contents = fs::read_to_string(path).expect("resolved manifest");
    assert!(contents.contains("version: 1"));
    assert!(contents.contains(&format!("tenant: {tenant}")));
    if let Some(team) = team {
        assert!(contents.contains(&format!("team: {team}")));
    }
    assert!(contents.contains(&format!("tenant_gmap: {tenant_gmap}")));
    match team_gmap {
        Some(team_gmap) => assert!(contents.contains(&format!("team_gmap: {team_gmap}"))),
        None => assert!(!contents.contains("team_gmap:")),
    }
    assert!(contents.contains("default: forbidden"));
}
