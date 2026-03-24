use std::fs;
use std::process::{Command, Stdio};

use greentic_bundle::catalog::registry::{CatalogEntry, bundled_provider_registry_entries};
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn bundle_bin() -> &'static str {
    env!("CARGO_BIN_EXE_greentic-bundle")
}

fn run_with_stdin(args: &[&str], stdin_payload: &str) -> std::process::Output {
    run_with_stdin_and_env(args, stdin_payload, &[])
}

fn run_with_stdin_and_env(
    args: &[&str],
    stdin_payload: &str,
    envs: &[(&str, &str)],
) -> std::process::Output {
    let mut command = Command::new(bundle_bin());
    command.args(args);
    command.envs(envs.iter().copied());
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn wizard command");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin_payload.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait output")
}

fn run_command(args: &[&str]) -> std::process::Output {
    Command::new(bundle_bin())
        .args(args)
        .output()
        .expect("run command")
}

fn seed_workspace_with_artifact(root: &std::path::Path, artifact: &std::path::Path) {
    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n4\n1\n0\n",
            root.display()
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let build = run_command(&[
        "build",
        "--root",
        &root.display().to_string(),
        "--output",
        &artifact.display().to_string(),
    ]);
    assert!(
        build.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
}

#[test]
fn bare_wizard_uses_back_for_embedded_root_menu() {
    let output = run_with_stdin_and_env(
        &["wizard"],
        "0\n",
        &[("GREENTIC_WIZARD_ROOT_ZERO_ACTION", "back")],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("0. Back"));
    assert!(!stdout.contains("0. Exit"));
    assert!(!stdout.contains("Wizard exited without collecting answers."));
}

#[test]
fn bare_wizard_executes_create_flow_by_default() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n4\n1\n0\n",
            bundle_root.display()
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bundle_root.join("bundle.yaml").exists());
    assert!(bundle_root.join("bundle.lock.json").exists());
    assert!(bundle_root.join("tenants/default/tenant.gmap").exists());
    assert!(bundle_root.join("dist/demo-bundle.gtbundle").exists());
}

#[test]
fn bare_wizard_renders_compact_mode_menu() {
    let output = run_with_stdin(&["wizard", "run", "--dry-run"], "0\n");
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("1. create"));
    assert!(stdout.contains("2. open existing bundle"));
    assert!(stdout.contains("3. validate bundle"));
    assert!(stdout.contains("4. doctor"));
    assert!(stdout.contains("5. inspect"));
    assert!(stdout.contains("6. unbundle"));
    assert!(stdout.contains("Start a new bundle workspace"));
    assert!(stdout.contains("Extract a .gtbundle into a directory"));
}

#[test]
fn bare_wizard_root_form_hides_verbose_qa_status_block() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard", "run", "--dry-run"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n4\n1\n",
            bundle_root.display()
        ),
    );
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("Select number or value: Bundle name"));
    assert_eq!(stdout.matches("Bundle Wizard").count(), 1);
    assert!(!stdout.contains("Status: need_input (0/9)"));
    assert!(!stdout.contains("Visible questions:"));
}

#[test]
fn bare_wizard_update_accepts_gtbundle_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    let edited = temp.path().join("edited-workspace");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "2\n{}\n{}\n\n\n4\n4\n2\n0\n",
            artifact.display(),
            edited.display(),
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains(edited.to_str().expect("utf8 edited")));
    assert!(stdout.contains("\"execution\": \"dry_run\""));
}

#[test]
fn bare_wizard_validate_accepts_gtbundle_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(&["wizard"], &format!("3\n{}\n\n\n0\n", artifact.display()));
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("\"execution\": \"dry_run\""));
    assert!(stdout.contains("\"target_root\""));
}

#[test]
fn bare_wizard_doctor_accepts_gtbundle_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(&["wizard"], &format!("4\n{}\n0\n", artifact.display()));
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("\"target\""));
    assert!(stdout.contains("\"ok\": true"));
}

#[test]
fn bare_wizard_inspect_accepts_gtbundle_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(&["wizard"], &format!("5\n{}\n0\n", artifact.display()));
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("bundle.yaml"));
    assert!(stdout.contains("bundle-lock.json"));
}

#[test]
fn bare_wizard_returns_to_main_menu_after_task() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(&["wizard"], &format!("5\n{}\n0\n", artifact.display()));
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("bundle.yaml"));
    assert!(stdout.matches("Bundle Wizard").count() >= 2);
}

#[test]
fn bare_wizard_unbundle_accepts_gtbundle_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let artifact = temp.path().join("bundle.gtbundle");
    let out = temp.path().join("unbundled");
    seed_workspace_with_artifact(&root, &artifact);

    let output = run_with_stdin(
        &["wizard"],
        &format!("6\n{}\n{}\n0\n", artifact.display(), out.display()),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.join("bundle.yaml").exists());
    assert!(out.join("bundle-lock.json").exists());
}

#[test]
fn create_flow_requires_app_pack_before_continue() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard", "run", "--dry-run"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n4\n1\npack-a\n1\n1\n4\n4\n2\n",
            bundle_root.display()
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("Add at least one app pack before continuing."));
    assert!(stdout.contains("\"execution\": \"dry_run\""));
}

#[test]
fn bare_wizard_create_flow_skips_provider_setup_prompts() {
    let temp = TempDir::new().expect("tempdir");
    let provider_source = temp
        .path()
        .join("provider-source")
        .join("providers")
        .join("deployer")
        .join("provider-a.gtpack");
    fs::create_dir_all(provider_source.parent().expect("provider parent")).expect("provider dir");
    fs::write(&provider_source, "provider-pack-bytes").expect("write provider pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n2\n{}\n4\n1\n0\n",
            bundle_root.display(),
            provider_source.display(),
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(!stdout.contains("Setup form:"));
    assert!(!stdout.contains("Rule path"));
    assert!(stdout.contains("Scope: global"));
    assert!(!bundle_root.join("state/setup/provider-a.json").exists());
}

#[test]
fn common_extension_provider_menu_uses_bundled_well_known_catalog() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let entries = bundled_provider_registry_entries().expect("bundled provider registry");
    assert!(!entries.is_empty(), "bundled catalog should not be empty");

    let grouped_categories = group_catalog_entries_for_test(&entries);
    let selected_category = grouped_categories
        .first()
        .expect("at least one category")
        .0
        .clone();
    let selected_category_index = grouped_categories
        .iter()
        .position(|(category, _)| category == &selected_category)
        .expect("selected category index");
    let mut stdin = format!(
        "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n1\n",
        bundle_root.display()
    );
    if grouped_categories.len() > 1 {
        stdin.push_str(&format!("{}\n", selected_category_index + 1));
    }
    stdin.push_str("1\n4\n2\n");

    let output = run_with_stdin(&["wizard", "run", "--dry-run"], &stdin);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout");
    if grouped_categories.len() > 1 {
        assert!(stdout.contains("Choose extension category:"));
    }
    assert!(stdout.contains("Choose extension provider:"));
}

fn group_catalog_entries_for_test(entries: &[CatalogEntry]) -> Vec<(String, Option<String>)> {
    let mut grouped = Vec::<(String, Option<String>)>::new();
    for entry in entries {
        let category = entry
            .category
            .clone()
            .unwrap_or_else(|| "other".to_string());
        let description = entry.category_description.clone();
        if let Some((_, existing_description)) =
            grouped.iter_mut().find(|(name, _)| name == &category)
        {
            if existing_description.is_none() {
                *existing_description = description.clone();
            }
        } else {
            grouped.push((category, description));
        }
    }
    grouped
}

#[test]
fn bundled_common_extension_provider_is_not_persisted_to_remote_catalogs() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin_and_env(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n1\n2\n5\n4\n1\n0\n",
            bundle_root.display()
        ),
        &[("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")],
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let bundle_yaml = fs::read_to_string(bundle_root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("extension_providers:\n  - oci://ghcr.io/greenticai/packs/"));
    assert!(bundle_yaml.contains(":latest"));
    assert!(bundle_yaml.contains("remote_catalogs: []"));
    assert!(!bundle_yaml.contains("registries/providers.json"));
}

#[test]
fn create_flow_uses_pack_id_for_access_rules_and_resolved_policy() {
    let temp = TempDir::new().expect("tempdir");
    let pack_path = temp.path().join("Cisco Bundle.gtpack");
    fs::write(&pack_path, "fixture").expect("write pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\n{}\n1\n1\n4\n4\n1\n0\n",
            bundle_root.display(),
            pack_path.display(),
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let tenant_gmap =
        fs::read_to_string(bundle_root.join("tenants/default/tenant.gmap")).expect("tenant gmap");
    assert!(tenant_gmap.contains("cisco-bundle = public"));

    let resolved =
        fs::read_to_string(bundle_root.join("resolved/default.yaml")).expect("resolved output");
    assert!(resolved.contains(&format!("reference: {}", pack_path.display())));
    assert!(resolved.contains("policy: public"));
}

#[test]
fn create_flow_reprompts_for_missing_local_app_pack_gtpack() {
    let temp = TempDir::new().expect("tempdir");
    let pack_path = temp.path().join("real-pack.gtpack");
    fs::write(&pack_path, "fixture").expect("write pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard", "run", "--dry-run"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\nfake.gtpack\n{}\n1\n1\n4\n4\n2\n",
            bundle_root.display(),
            pack_path.display(),
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("read local .gtpack"));
    assert!(stdout.contains(&pack_path.display().to_string()));
    assert!(stdout.contains("Resolved app pack:"));
    assert!(stdout.contains("\"execution\": \"dry_run\""));
}

#[test]
fn create_flow_materializes_pack_and_provider_gtpacks_into_bundle_layout() {
    let temp = TempDir::new().expect("tempdir");
    let pack_path = temp.path().join("Cisco Bundle.gtpack");
    fs::write(&pack_path, "app-pack-bytes").expect("write app pack");
    let provider_source = temp
        .path()
        .join("provider-source")
        .join("providers")
        .join("deployer")
        .join("custom-provider.gtpack");
    fs::create_dir_all(provider_source.parent().expect("provider parent")).expect("provider dir");
    fs::write(&provider_source, "provider-pack-bytes").expect("write provider pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &["wizard"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\n{}\n1\n1\n4\n2\n{}\n4\n1\n0\n",
            bundle_root.display(),
            pack_path.display(),
            provider_source.display(),
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        fs::read(bundle_root.join("packs/cisco-bundle.gtpack")).expect("materialized app pack"),
        b"app-pack-bytes"
    );
    assert_eq!(
        fs::read(bundle_root.join("providers/deployer/custom-provider.gtpack"))
            .expect("materialized provider pack"),
        b"provider-pack-bytes"
    );
}

#[test]
fn common_extension_provider_latest_entry_skips_version_prompt_before_build() {
    let temp = TempDir::new().expect("tempdir");
    let pack_path = temp.path().join("Cisco Bundle.gtpack");
    fs::write(&pack_path, "app-pack-bytes").expect("write app pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin_and_env(
        &["wizard", "run", "--dry-run"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\n{}\n1\n1\n4\n1\n1\n1\n4\n2\n",
            bundle_root.display(),
            pack_path.display(),
        ),
        &[("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1")],
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(!stdout.contains("PR version or tag"));
    assert!(stdout.contains("oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest"));
    assert!(stdout.contains("\"execution\": \"dry_run\""));
}

#[test]
fn create_flow_fails_when_remote_provider_cannot_be_materialized() {
    let temp = TempDir::new().expect("tempdir");
    let pack_path = temp.path().join("Cisco Bundle.gtpack");
    fs::write(&pack_path, "app-pack-bytes").expect("write app pack");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin_and_env(
        &["wizard", "--offline"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\n{}\n1\n1\n4\n2\noci://ghcr.io/greenticai/packs/deployer/greentic.fixture.k8s.raw.gtpack:latest\n4\n1\n",
            bundle_root.display(),
            pack_path.display(),
        ),
        &[("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "0")],
    );
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr");
    assert!(stderr.contains("resolve OCI pack ref"));
}

#[test]
fn wizard_update_prefills_existing_bundle_and_applies_changes() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let catalog_path = temp.path().join("catalog.json");
    fs::create_dir_all(&bundle_root).expect("mkdir");
    fs::write(&catalog_path, "[]\n").expect("catalog");
    fs::write(
        bundle_root.join("bundle.yaml"),
        format!(
            "schema_version: 1\n\
bundle_id: existing-bundle\n\
bundle_name: Existing Bundle\n\
locale: en\n\
mode: create\n\
advanced_setup: true\n\
app_packs:\n\
  - pack-a\n\
extension_providers:\n\
  - provider-a\n\
remote_catalogs:\n\
  - file://{}\n\
hooks: []\n\
subscriptions: []\n\
capabilities: []\n\
setup_execution_intent: true\n\
export_intent: false\n",
            catalog_path.display(),
        ),
    )
    .expect("bundle yaml");
    fs::write(
        bundle_root.join("bundle.lock.json"),
        concat!(
            "{\n",
            "  \"schema_version\": 1,\n",
            "  \"bundle_id\": \"existing-bundle\",\n",
            "  \"requested_mode\": \"create\",\n",
            "  \"execution\": \"execute\",\n",
            "  \"cache_policy\": \"workspace-local\",\n",
            "  \"tool_version\": \"0.4.0\",\n",
            "  \"build_format_version\": \"bundle-lock-v1\",\n",
            "  \"workspace_root\": \"bundle.yaml\",\n",
            "  \"lock_file\": \"bundle.lock.json\",\n",
            "  \"catalogs\": [],\n",
            "  \"app_packs\": [{\"reference\":\"pack-a\",\"digest\":null}],\n",
            "  \"extension_providers\": [{\"reference\":\"provider-a\",\"digest\":null}],\n",
            "  \"setup_state_files\": []\n",
            "}\n"
        ),
    )
    .expect("lock");
    fs::create_dir_all(bundle_root.join("tenants/default/teams")).expect("tenant dirs");
    fs::write(
        bundle_root.join("tenants/default/tenant.gmap"),
        "_ = forbidden\n",
    )
    .expect("tenant gmap");

    let output = run_with_stdin(
        &["wizard", "run", "--mode", "update"],
        &format!(
            "{}\nUpdated Bundle\nupdated-bundle\n1\npack-b\n1\n2\ndefault\n4\n4\n1\n",
            bundle_root.display()
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let bundle_yaml = fs::read_to_string(bundle_root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("bundle_name: Updated Bundle"));
    assert!(bundle_yaml.contains("bundle_id: updated-bundle"));
    assert!(bundle_yaml.contains("app_packs:\n  - pack-a\n  - pack-b"));
    assert!(bundle_yaml.contains("scope: tenant"));
}

#[test]
fn wizard_run_emit_answers_writes_envelope() {
    let temp = TempDir::new().expect("tempdir");
    let answers_path = temp.path().join("answers.json");
    let bundle_root = temp.path().join("bundle");

    let output = run_with_stdin(
        &[
            "wizard",
            "run",
            "--schema-version",
            "1.2.3",
            "--emit-answers",
            answers_path.to_str().unwrap(),
            "--dry-run",
        ],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n4\n1\n",
            bundle_root.display()
        ),
    );
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let doc: Value = serde_json::from_slice(&fs::read(&answers_path).expect("read answers"))
        .expect("parse answers");
    assert_eq!(
        doc.get("wizard_id").and_then(Value::as_str),
        Some("greentic-bundle.wizard.run")
    );
    assert_eq!(
        doc.get("schema_id").and_then(Value::as_str),
        Some("greentic-bundle.wizard.answers")
    );
    assert_eq!(
        doc.get("schema_version").and_then(Value::as_str),
        Some("1.2.3")
    );
    assert_eq!(
        doc.pointer("/answers/output_dir").and_then(Value::as_str),
        Some(bundle_root.to_str().unwrap())
    );
    assert!(
        !bundle_root.exists(),
        "dry-run should not write workspace files"
    );
}

#[test]
fn wizard_validate_is_side_effect_free() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
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
    "advanced_setup":false,
    "app_packs":[],
    "extension_providers":[],
    "remote_catalogs":[],
    "setup_execution_intent":false,
    "export_intent":false
  }},
  "locks":{{}}
}}"#,
            bundle_root.display()
        ),
    )
    .expect("write answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "validate", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("run validate");
    assert!(output.status.success());
    assert!(
        !bundle_root.exists(),
        "validate must not create workspace files"
    );
}

#[test]
fn wizard_apply_replays_from_answers() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let answers_path = temp.path().join("answers.json");
    let catalog_path = temp.path().join("catalog.json");
    fs::write(
        &catalog_path,
        r#"[{"id":"provider-a","label":"Provider A","reference":"repo://providers/provider-a@latest"}]"#,
    )
    .expect("write catalog");
    fs::write(
        &answers_path,
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
    "app_packs":["pack-a"],
    "extension_providers":["provider-a"],
    "remote_catalogs":["file://{}"],
    "setup_execution_intent":true,
    "export_intent":true
  }},
  "locks":{{}}
}}"#,
            bundle_root.display(),
            catalog_path.display()
        ),
    )
    .expect("write answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("run apply");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bundle_root.join("bundle.yaml").exists());
    assert!(bundle_root.join("tenants/default/tenant.gmap").exists());
    assert!(bundle_root.join("bundle.lock.json").exists());

    let bundle_yaml = fs::read_to_string(bundle_root.join("bundle.yaml")).expect("bundle yaml");
    assert!(bundle_yaml.contains("bundle_id: demo-bundle"));
    assert!(bundle_yaml.contains("app_packs:\n  - pack-a"));
    let lock: Value =
        serde_json::from_slice(&fs::read(bundle_root.join("bundle.lock.json")).expect("lock"))
            .expect("parse lock");
    assert_eq!(
        lock.pointer("/catalogs/0/source").and_then(Value::as_str),
        Some("local_file")
    );
    assert_eq!(
        lock.pointer("/catalogs/0/item_count")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert!(bundle_root.join("state/cache/catalogs/index.json").exists());
}

#[test]
fn wizard_apply_replays_build_bundle_artifact_when_answers_capture_execute_lock() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
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
    "advanced_setup":false,
    "app_packs":[],
    "extension_providers":[],
    "remote_catalogs":[],
    "setup_execution_intent":false,
    "export_intent":false
  }},
  "locks":{{"execution":"execute"}}
}}"#,
            bundle_root.display()
        ),
    )
    .expect("write answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("run apply");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact = bundle_root.join("dist/demo-bundle.gtbundle");
    assert!(
        artifact.exists(),
        "missing artifact at {}",
        artifact.display()
    );

    let plan: Value = serde_json::from_slice(&output.stdout).expect("parse plan stdout");
    assert_eq!(
        plan.pointer("/ordered_step_list/5/kind")
            .and_then(Value::as_str),
        Some("build_bundle")
    );
    assert!(
        plan.get("expected_file_writes")
            .and_then(Value::as_array)
            .is_some_and(|writes| writes
                .iter()
                .filter_map(Value::as_str)
                .any(|path| path == artifact.to_str().unwrap())),
        "expected_file_writes missing {}",
        artifact.display()
    );
}

#[test]
fn wizard_apply_discovers_setup_specs_from_catalog_entries() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let answers_path = temp.path().join("answers.json");
    let catalog_path = temp.path().join("catalog.json");
    fs::write(
        &catalog_path,
        r#"[
  {
    "id":"provider-a",
    "label":"Provider A",
    "reference":"repo://providers/provider-a@latest",
    "setup":{
      "type":"legacy",
      "spec":{
        "title":"Provider A Setup",
        "questions":[
          {"name":"enabled","kind":"boolean","required":true},
          {"name":"api_token","kind":"string","required":true,"secret":true}
        ]
      }
    }
  }
]"#,
    )
    .expect("write catalog");
    fs::write(
        &answers_path,
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
    "app_packs":[],
    "extension_providers":["provider-a"],
    "remote_catalogs":["file://{}"],
    "setup_answers":{{
      "provider-a": {{"enabled": true, "api_token":"secret123"}}
    }},
    "setup_execution_intent":true,
    "export_intent":false
  }},
  "locks":{{}}
}}"#,
            bundle_root.display(),
            catalog_path.display()
        ),
    )
    .expect("write answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("run apply");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let state: Value = serde_json::from_slice(
        &fs::read(bundle_root.join("state/setup/provider-a.json")).expect("setup state"),
    )
    .expect("parse setup state");
    assert_eq!(
        state.get("source_kind").and_then(Value::as_str),
        Some("legacy")
    );

    let emitted_plan: Value = serde_json::from_slice(&output.stdout).expect("parse plan");
    assert_eq!(
        emitted_plan
            .pointer("/normalized_input_summary/setup_spec_providers/0")
            .and_then(Value::as_str),
        Some("provider-a")
    );
}

#[test]
fn wizard_validate_with_migrate_reemits_document() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let input_path = temp.path().join("legacy.json");
    let output_path = temp.path().join("migrated.json");
    fs::write(
        &input_path,
        format!(
            r#"{{
  "answers":{{
    "mode":"create",
    "bundle_name":"Legacy Bundle",
    "bundle_id":"legacy-bundle",
    "output_dir":"{}",
    "advanced_setup":false,
    "app_packs":[],
    "extension_providers":[],
    "remote_catalogs":[],
    "setup_execution_intent":false,
    "export_intent":false
  }},
  "locks":{{"legacy":"x"}}
}}"#,
            bundle_root.display()
        ),
    )
    .expect("write legacy answers");

    let output = Command::new(bundle_bin())
        .args([
            "wizard",
            "validate",
            "--answers",
            input_path.to_str().unwrap(),
            "--migrate",
            "--schema-version",
            "2.0.0",
            "--emit-answers",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run validate migrate");
    assert!(output.status.success());

    let doc: Value = serde_json::from_slice(&fs::read(&output_path).expect("read migrated doc"))
        .expect("parse migrated doc");
    assert_eq!(
        doc.get("schema_version").and_then(Value::as_str),
        Some("2.0.0")
    );
    assert_eq!(
        doc.pointer("/locks/legacy").and_then(Value::as_str),
        Some("x")
    );
}

#[test]
fn dry_run_prints_deterministic_plan_without_writes() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let answers_path = temp.path().join("answers.json");
    fs::write(
        &answers_path,
        format!(
            r#"{{
  "wizard_id":"greentic-bundle.wizard.run",
  "schema_id":"greentic-bundle.wizard.answers",
  "schema_version":"1.0.0",
  "locale":"en",
  "answers":{{
    "mode":"doctor",
    "bundle_name":"Doctor Bundle",
    "bundle_id":"doctor-bundle",
    "output_dir":"{}",
    "advanced_setup":false,
    "app_packs":[],
    "extension_providers":[],
    "remote_catalogs":[],
    "setup_execution_intent":false,
    "export_intent":false
  }},
  "locks":{{}}
}}"#,
            bundle_root.display()
        ),
    )
    .expect("write answers");

    let output = Command::new(bundle_bin())
        .args(["wizard", "run", "--answers"])
        .arg(&answers_path)
        .arg("--dry-run")
        .output()
        .expect("run dry-run");
    assert!(output.status.success());
    assert!(!bundle_root.exists());

    let plan: Value = serde_json::from_slice(&output.stdout).expect("parse plan stdout");
    assert_eq!(
        plan.pointer("/requested_action").and_then(Value::as_str),
        Some("doctor")
    );
    assert_eq!(
        plan.pointer("/ordered_step_list/5/kind")
            .and_then(Value::as_str),
        Some("build_bundle")
    );
}

#[test]
fn locale_aware_wizard_rendering_uses_embedded_prompts() {
    let temp = TempDir::new().expect("tempdir");
    let bundle_root = temp.path().join("bundle");
    let output = run_with_stdin(
        &["--locale", "en-US", "wizard", "run", "--dry-run"],
        &format!(
            "1\nDemo Bundle\ndemo-bundle\n{}\n1\npack-a\n1\n1\n4\n5\n4\n2\n",
            bundle_root.display()
        ),
    );
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(predicate::str::contains("Bundle Wizard").eval(&stdout));
    assert!(predicate::str::contains("Bundle name").eval(&stdout));
    assert!(predicate::str::contains("Output directory").eval(&stdout));
}

#[test]
fn wizard_apply_help_mentions_mode_flag() {
    let output = Command::new(bundle_bin())
        .args(["wizard", "apply", "--help"])
        .output()
        .expect("help output");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("--mode <MODE>"));
}
