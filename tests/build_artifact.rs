use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

fn seed_workspace(root: &Path) {
    fs::create_dir_all(root.join("resolved")).expect("resolved dir");
    fs::create_dir_all(root.join("state/setup")).expect("setup dir");
    fs::create_dir_all(root.join("tenants/default")).expect("tenant dir");
    fs::create_dir_all(root.join("packs")).expect("packs dir");
    fs::create_dir_all(root.join("providers/messaging")).expect("provider dir");
    fs::write(
        root.join("bundle.yaml"),
        "\
schema_version: 1
bundle_id: demo-bundle
bundle_name: Demo Bundle
locale: en
mode: create
advanced_setup: true
app_packs:
  - pack-a
extension_providers:
  - provider-a
remote_catalogs:
  - file://catalog.json
hooks:
  - hook-a
subscriptions:
  - subscription-a
capabilities:
  - capability-a
setup_execution_intent: true
export_intent: false
",
    )
    .expect("bundle yaml");
    fs::write(root.join("tenants/default/tenant.gmap"), "_ = forbidden\n").expect("tenant gmap");
    fs::write(
        root.join("resolved/default.yaml"),
        "\
version: 1
tenant: default
project_root: /tmp/demo
bundle:
  bundle_id: demo-bundle
  bundle_name: Demo Bundle
  locale: en
  mode: create
  advanced_setup: true
  setup_execution_intent: true
  export_intent: false
policy:
  source:
    tenant_gmap: tenants/default/tenant.gmap
  default: forbidden
catalogs:
  - file://catalog.json
app_packs:
  - reference: pack-a
    policy: forbidden
extension_providers:
  - provider-a
hooks:
  - hook-a
subscriptions:
  - subscription-a
capabilities:
  - capability-a
",
    )
    .expect("resolved");
    fs::write(root.join("packs/pack-a.gtpack"), "pack-a-bytes").expect("pack file");
    fs::write(
        root.join("providers/messaging/provider-a.gtpack"),
        "provider-a-bytes",
    )
    .expect("provider file");
    fs::write(
        root.join("state/setup/provider-a.json"),
        r#"{"schema_version":1,"provider_id":"provider-a","source_kind":"legacy","form":{"id":"provider-a-setup","title":"Provider A Setup","version":"1.0.0","description":"Provider A provider configuration","questions":[]},"normalized_answers":{},"non_secret_config":{},"secret_values":{}}"#,
    )
    .expect("setup state");
    let lock = greentic_bundle::project::BundleLock {
        schema_version: greentic_bundle::project::LOCK_SCHEMA_VERSION,
        bundle_id: "demo-bundle".to_string(),
        requested_mode: "create".to_string(),
        execution: "execute".to_string(),
        cache_policy: "workspace-local".to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        build_format_version: "bundle-lock-v1".to_string(),
        workspace_root: "bundle.yaml".to_string(),
        lock_file: "bundle.lock.json".to_string(),
        catalogs: vec![greentic_bundle::catalog::resolve::CatalogLockEntry {
            requested_ref: "file://catalog.json".to_string(),
            resolved_ref: "catalog.json".to_string(),
            digest: "sha256:abc".to_string(),
            source: "local_file".to_string(),
            item_count: 1,
            item_ids: vec!["provider-a".to_string()],
            cache_path: None,
        }],
        app_packs: vec![greentic_bundle::project::DependencyLock {
            reference: "pack-a".to_string(),
            digest: None,
        }],
        extension_providers: vec![greentic_bundle::project::DependencyLock {
            reference: "provider-a".to_string(),
            digest: None,
        }],
        setup_state_files: vec!["state/setup/provider-a.json".to_string()],
    };
    greentic_bundle::project::write_bundle_lock(root, &lock).expect("write lock");
}

#[test]
fn build_produces_byte_stable_artifact() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let artifact_one = root.join("one.gtbundle");
    let artifact_two = root.join("two.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact_one), false, false)
        .expect("build one");
    greentic_bundle::build::build_workspace(&root, Some(&artifact_two), false, false)
        .expect("build two");

    assert_eq!(
        fs::read(&artifact_one).expect("artifact one"),
        fs::read(&artifact_two).expect("artifact two")
    );
}

#[test]
fn inspect_output_is_stable() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let report_one = serde_json::to_string_pretty(
        &greentic_bundle::build::inspect_target(Some(&root), None).expect("inspect one"),
    )
    .expect("serialize one");
    let report_two = serde_json::to_string_pretty(
        &greentic_bundle::build::inspect_target(Some(&root), None).expect("inspect two"),
    )
    .expect("serialize two");
    assert_eq!(report_one, report_two);
}

#[test]
fn build_normalized_dir_includes_materialized_pack_and_provider_files() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let build_dir = root.join("state/build/demo-bundle/normalized");
    greentic_bundle::build::build_workspace(&root, None, false, false).expect("build workspace");

    assert_eq!(
        fs::read(build_dir.join("packs/pack-a.gtpack")).expect("built pack"),
        b"pack-a-bytes"
    );
    assert_eq!(
        fs::read(build_dir.join("providers/messaging/provider-a.gtpack")).expect("built provider"),
        b"provider-a-bytes"
    );
}

#[test]
fn build_defaults_artifact_path_to_dist_directory() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let result = greentic_bundle::build::build_workspace(&root, None, false, false).expect("build");
    let expected = root.join("dist/demo-bundle.gtbundle");

    assert_eq!(result.artifact_path, expected.display().to_string());
    assert!(
        expected.exists(),
        "expected artifact at {}",
        expected.display()
    );
}

#[test]
fn inspect_workspace_keeps_workspace_target_not_temp_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let report = greentic_bundle::build::inspect_target(Some(&root), None).expect("inspect");
    assert_eq!(report.target, root.display().to_string());
}

#[test]
fn doctor_validates_workspace_and_artifact() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let artifact = root.join("demo.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact), false, false).expect("build");

    let workspace_report =
        greentic_bundle::build::doctor_target(Some(&root), None).expect("doctor workspace");
    let artifact_report =
        greentic_bundle::build::doctor_target(None, Some(&artifact)).expect("doctor artifact");
    assert!(workspace_report.ok);
    assert!(artifact_report.ok);
}

#[test]
fn inspect_artifact_includes_content_listing() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let artifact = root.join("demo.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact), false, false).expect("build");

    let report = greentic_bundle::build::inspect_target(None, Some(&artifact)).expect("inspect");
    let contents = report.contents.expect("artifact contents");

    assert!(contents.iter().any(|entry| entry == "bundle.yaml"));
    assert!(contents.iter().any(|entry| entry == "bundle-lock.json"));
    assert!(
        contents
            .iter()
            .any(|entry| entry == "resolved/default.yaml")
    );
}

#[test]
fn dry_run_export_computes_plan_without_writing_artifact() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let build_dir = root.join("state/build/demo-bundle/normalized");
    let artifact = root.join("dry-run.gtbundle");

    greentic_bundle::build::build_workspace(&root, None, false, false).expect("seed build dir");
    let result = greentic_bundle::build::export_build_dir(&build_dir, &artifact, true, false)
        .expect("dry-run export");
    assert_eq!(result.artifact_path, artifact.display().to_string());
    assert!(!artifact.exists());
}

#[test]
fn doctor_detects_lock_drift() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    fs::write(
        root.join("bundle.yaml"),
        "\
schema_version: 1
bundle_id: demo-bundle
bundle_name: Demo Bundle
locale: en
mode: create
advanced_setup: true
app_packs:
  - pack-b
extension_providers:
  - provider-a
remote_catalogs:
  - file://catalog.json
hooks:
  - hook-a
subscriptions:
  - subscription-a
capabilities:
  - capability-a
setup_execution_intent: true
export_intent: false
",
    )
    .expect("rewrite bundle");

    let report = greentic_bundle::build::doctor_target(Some(&root), None).expect("doctor");
    assert!(!report.ok);
    let drift = report
        .checks
        .iter()
        .find(|check| check.name == "lock drift")
        .expect("drift check");
    assert!(!drift.ok);
}

#[test]
fn doctor_reports_workspace_reader_validation_error_details() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);

    let mut lock = greentic_bundle::project::read_bundle_lock(&root).expect("read lock");
    lock.workspace_root = "wrong.yaml".to_string();
    greentic_bundle::project::write_bundle_lock(&root, &lock).expect("rewrite lock");

    let report = greentic_bundle::build::doctor_target(Some(&root), None).expect("doctor");
    assert!(!report.ok);
    let reader = report
        .checks
        .iter()
        .find(|check| check.name == "reader validation")
        .expect("reader check");
    assert!(!reader.ok);
    assert!(
        reader
            .details
            .as_deref()
            .unwrap_or_default()
            .contains("unexpected workspace_root")
    );
}

#[test]
fn inspect_artifact_reads_embedded_manifest() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let artifact = root.join("demo.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact), false, false).expect("build");

    let report =
        greentic_bundle::build::inspect_target(None, Some(&artifact)).expect("inspect artifact");
    let value = serde_json::to_value(report).expect("to value");
    assert_eq!(
        value.pointer("/manifest/bundle_id").and_then(Value::as_str),
        Some("demo-bundle")
    );
}

#[test]
fn reader_opens_normalized_build_directory() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let result = greentic_bundle::build::build_workspace(&root, None, false, false).expect("build");

    let opened = greentic_bundle_reader::open_build_dir(Path::new(&result.build_dir))
        .expect("open build dir");
    assert_eq!(opened.source_kind.as_str(), "build_dir");
    assert_eq!(opened.manifest.bundle_id, "demo-bundle");
    assert_eq!(
        opened
            .runtime_surface()
            .app_packs
            .iter()
            .map(|entry| entry.reference.clone())
            .collect::<Vec<_>>(),
        vec!["pack-a".to_string()]
    );
}

#[test]
fn reader_opens_artifact_and_exposes_runtime_surface() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let artifact = root.join("demo.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact), false, false).expect("build");

    let opened = greentic_bundle_reader::open_artifact(&artifact).expect("open artifact");
    assert_eq!(opened.source_kind.as_str(), "artifact");
    assert_eq!(
        opened
            .runtime_surface()
            .extension_providers
            .iter()
            .map(|entry| entry.reference.clone())
            .collect::<Vec<_>>(),
        vec!["provider-a".to_string()]
    );
}

#[test]
fn reader_runtime_surface_exposes_catalogs_and_file_views() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    let artifact = root.join("demo.gtbundle");
    greentic_bundle::build::build_workspace(&root, Some(&artifact), false, false).expect("build");

    let surface = greentic_bundle_reader::open_artifact(&artifact)
        .expect("open artifact")
        .runtime_surface();
    assert_eq!(surface.bundle_id, "demo-bundle");
    assert_eq!(surface.execution, "execute");
    assert_eq!(surface.catalogs.len(), 1);
    assert_eq!(surface.catalogs[0].requested_ref, "file://catalog.json");
    assert_eq!(surface.hooks, vec!["hook-a".to_string()]);
    assert_eq!(surface.subscriptions, vec!["subscription-a".to_string()]);
    assert_eq!(surface.capabilities, vec!["capability-a".to_string()]);
    assert_eq!(surface.resolved_targets.len(), 1);
    assert_eq!(surface.resolved_targets[0].tenant, "default");
    assert_eq!(
        surface.resolved_targets[0].app_pack_policies[0].reference,
        "pack-a"
    );
    assert_eq!(
        surface.resolved_targets[0].app_pack_policies[0].policy,
        "forbidden"
    );
    assert_eq!(
        surface.generated_resolved_files[0].path,
        "resolved/default.yaml"
    );
    assert_eq!(
        surface.generated_resolved_files[0].kind,
        greentic_bundle_reader::BundleFileKind::Resolved
    );
    assert_eq!(
        surface.generated_setup_files[0].kind,
        greentic_bundle_reader::BundleFileKind::SetupState
    );
}

#[test]
fn build_accepts_quoted_scalars_and_inline_yaml_lists() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    fs::write(
        root.join("bundle.yaml"),
        "\
schema_version: 1
bundle_id: \"demo-bundle\"
bundle_name: \"Demo Bundle\"
locale: \"en\"
mode: \"create\"
advanced_setup: true
app_packs: [pack-a, pack-b]
extension_providers: [provider-a, provider-b]
remote_catalogs: [file://catalog-a.json, file://catalog-b.json]
hooks: [hook-a, hook-b]
subscriptions: [subscription-a, subscription-b]
capabilities: [capability-a, capability-b]
setup_execution_intent: true
export_intent: false
",
    )
    .expect("rewrite bundle yaml");

    let report = greentic_bundle::build::build_workspace(&root, None, false, false).expect("build");
    let opened = greentic_bundle_reader::open_build_dir(Path::new(&report.build_dir))
        .expect("open build dir");
    let surface = opened.runtime_surface();

    assert_eq!(surface.bundle_id, "demo-bundle");
    assert_eq!(surface.bundle_name, "Demo Bundle");
    assert_eq!(surface.locale, "en");
    assert_eq!(surface.requested_mode, "create");
    assert_eq!(
        opened
            .manifest
            .app_packs
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["pack-a", "pack-b"]
    );
    assert_eq!(
        opened
            .manifest
            .extension_providers
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["provider-a", "provider-b"]
    );
    assert_eq!(
        opened
            .manifest
            .catalogs
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["file://catalog-a.json", "file://catalog-b.json"]
    );
    assert_eq!(
        surface.hooks,
        vec!["hook-a".to_string(), "hook-b".to_string()]
    );
    assert_eq!(
        surface.subscriptions,
        vec!["subscription-a".to_string(), "subscription-b".to_string()]
    );
    assert_eq!(
        surface.capabilities,
        vec!["capability-a".to_string(), "capability-b".to_string()]
    );
}

#[test]
fn build_parses_nested_resolved_target_metadata() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    seed_workspace(&root);
    fs::write(
        root.join("resolved/default.yaml"),
        "\
version: 1
tenant: default
team: ops
project_root: /tmp/demo
bundle:
  bundle_id: demo-bundle
  bundle_name: Demo Bundle
  locale: en
  mode: create
policy:
  source:
    tenant_gmap: tenants/default/tenant.gmap
    team_gmap: tenants/default/teams/ops/team.gmap
app_packs:
  - reference: pack-a
    policy: public
  - reference: pack-b
    policy: forbidden
",
    )
    .expect("rewrite resolved");

    let report = greentic_bundle::build::build_workspace(&root, None, false, false).expect("build");
    let target = greentic_bundle_reader::open_build_dir(Path::new(&report.build_dir))
        .expect("open build dir")
        .runtime_surface()
        .resolved_targets
        .into_iter()
        .next()
        .expect("resolved target");

    assert_eq!(target.tenant, "default");
    assert_eq!(target.team.as_deref(), Some("ops"));
    assert_eq!(target.default_policy, "forbidden");
    assert_eq!(target.tenant_gmap, "tenants/default/tenant.gmap");
    assert_eq!(
        target.team_gmap.as_deref(),
        Some("tenants/default/teams/ops/team.gmap")
    );
    assert_eq!(
        target
            .app_pack_policies
            .iter()
            .map(|policy| (policy.reference.as_str(), policy.policy.as_str()))
            .collect::<Vec<_>>(),
        vec![("pack-a", "public"), ("pack-b", "forbidden")]
    );
}

#[test]
fn reader_rejects_mismatched_setup_file_contract() {
    let temp = TempDir::new().expect("tempdir");
    let build_dir = temp.path().join("normalized");
    fs::create_dir_all(&build_dir).expect("build dir");
    fs::write(
        build_dir.join("bundle-manifest.json"),
        r#"{
  "format_version":"gtbundle-v1",
  "bundle_id":"demo-bundle",
  "bundle_name":"Demo Bundle",
  "requested_mode":"create",
  "locale":"en",
  "artifact_extension":".gtbundle",
  "generated_resolved_files":["resolved/default.yaml"],
  "generated_setup_files":["state/setup/provider-a.json"],
  "app_packs":["pack-a"],
  "extension_providers":["provider-a"],
  "catalogs":["file://catalog.json"],
  "hooks":[],
  "subscriptions":[],
  "capabilities":[]
}"#,
    )
    .expect("write manifest");
    fs::write(
        build_dir.join("bundle-lock.json"),
        r#"{
  "schema_version":1,
  "bundle_id":"demo-bundle",
  "requested_mode":"create",
  "execution":"execute",
  "cache_policy":"workspace-local",
  "tool_version":"0.4.0",
  "build_format_version":"bundle-lock-v1",
  "workspace_root":"bundle.yaml",
  "lock_file":"bundle.lock.json",
  "catalogs":[],
  "app_packs":[{"reference":"pack-a"}],
  "extension_providers":[{"reference":"provider-a"}],
  "setup_state_files":[]
}"#,
    )
    .expect("write lock");

    let error = greentic_bundle_reader::open_build_dir(&build_dir).expect_err("mismatch error");
    assert!(error.details.contains("setup state files"));
}

#[test]
fn reader_rejects_build_dir_with_missing_listed_file() {
    let temp = TempDir::new().expect("tempdir");
    let build_dir = temp.path().join("normalized");
    fs::create_dir_all(build_dir.join("resolved")).expect("resolved dir");
    fs::write(
        build_dir.join("bundle-manifest.json"),
        r#"{
  "format_version":"gtbundle-v1",
  "bundle_id":"demo-bundle",
  "bundle_name":"Demo Bundle",
  "requested_mode":"create",
  "locale":"en",
  "artifact_extension":".gtbundle",
  "generated_resolved_files":["resolved/default.yaml"],
  "generated_setup_files":[],
  "app_packs":[],
  "extension_providers":[],
  "catalogs":[],
  "hooks":[],
  "subscriptions":[],
  "capabilities":[]
}"#,
    )
    .expect("write manifest");
    fs::write(
        build_dir.join("bundle-lock.json"),
        r#"{
  "schema_version":1,
  "bundle_id":"demo-bundle",
  "requested_mode":"create",
  "execution":"execute",
  "cache_policy":"workspace-local",
  "tool_version":"0.4.0",
  "build_format_version":"bundle-lock-v1",
  "workspace_root":"bundle.yaml",
  "lock_file":"bundle.lock.json",
  "catalogs":[],
  "app_packs":[],
  "extension_providers":[],
  "setup_state_files":[]
}"#,
    )
    .expect("write lock");
    fs::write(build_dir.join("bundle.yaml"), "bundle_id: demo-bundle\n").expect("bundle yaml");

    let error = greentic_bundle_reader::open_build_dir(&build_dir).expect_err("missing file");
    assert!(error.details.contains("missing required bundle file"));
    assert!(error.details.contains("resolved/default.yaml"));
}

#[test]
fn reader_rejects_artifact_with_missing_listed_file() {
    let temp = TempDir::new().expect("tempdir");
    let build_dir = temp.path().join("normalized");
    fs::create_dir_all(&build_dir).expect("build dir");
    fs::write(
        build_dir.join("bundle-manifest.json"),
        r#"{
  "format_version":"gtbundle-v1",
  "bundle_id":"demo-bundle",
  "bundle_name":"Demo Bundle",
  "requested_mode":"create",
  "locale":"en",
  "artifact_extension":".gtbundle",
  "generated_resolved_files":["resolved/default.yaml"],
  "generated_setup_files":[],
  "app_packs":[],
  "extension_providers":[],
  "catalogs":[],
  "hooks":[],
  "subscriptions":[],
  "capabilities":[]
}"#,
    )
    .expect("write manifest");
    fs::write(
        build_dir.join("bundle-lock.json"),
        r#"{
  "schema_version":1,
  "bundle_id":"demo-bundle",
  "requested_mode":"create",
  "execution":"execute",
  "cache_policy":"workspace-local",
  "tool_version":"0.4.0",
  "build_format_version":"bundle-lock-v1",
  "workspace_root":"bundle.yaml",
  "lock_file":"bundle.lock.json",
  "catalogs":[],
  "app_packs":[],
  "extension_providers":[],
  "setup_state_files":[]
}"#,
    )
    .expect("write lock");
    fs::write(build_dir.join("bundle.yaml"), "bundle_id: demo-bundle\n").expect("bundle yaml");
    let artifact = temp.path().join("broken.gtbundle");

    greentic_bundle::build::export_build_dir(&build_dir, &artifact, false, false).expect("export");
    let error =
        greentic_bundle_reader::open_artifact(&artifact).expect_err("missing artifact file");
    assert!(error.details.contains("unsquashfs failed"));
    assert!(error.details.contains("resolved/default.yaml"));
}
