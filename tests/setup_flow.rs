use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

fn bundle_bin() -> &'static str {
    env!("CARGO_BIN_EXE_greentic-bundle")
}

#[test]
fn legacy_setup_spec_converts_deterministically() {
    let raw = r#"
title: Telegram Setup
questions:
  - name: enabled
    kind: boolean
    required: true
  - name: bot_token
    kind: string
    required: true
    secret: true
"#;
    let spec = greentic_bundle::setup::legacy_formspec::parse_setup_spec_str(raw)
        .expect("parse legacy spec");
    let form = greentic_bundle::setup::legacy_formspec::setup_spec_to_form_spec(
        &spec,
        "messaging-telegram",
    );
    assert_eq!(form.id, "messaging-telegram-setup");
    assert_eq!(form.title, "Telegram Setup");
    assert_eq!(
        form.questions[0].kind,
        greentic_bundle::setup::QuestionKind::Boolean
    );
    assert!(form.questions[1].secret);
}

#[test]
fn provider_qa_bridge_output_is_stable() {
    let qa_output = json!({
        "mode": "setup",
        "title": {"key": "telegram.qa.setup.title"},
        "questions": [
            {"id": "enabled", "label": {"key": "telegram.qa.setup.enabled"}, "required": true},
            {"id": "bot_token", "label": {"key": "telegram.qa.setup.bot_token"}, "required": true}
        ]
    });
    let i18n = BTreeMap::from([
        ("telegram.qa.setup.title".to_string(), "Setup".to_string()),
        (
            "telegram.qa.setup.enabled".to_string(),
            "Enable".to_string(),
        ),
        (
            "telegram.qa.setup.bot_token".to_string(),
            "Bot token".to_string(),
        ),
    ]);

    let form = greentic_bundle::setup::qa_bridge::provider_qa_to_form_spec(
        &qa_output,
        &i18n,
        "messaging-telegram",
    );
    assert_eq!(form.id, "messaging-telegram-setup");
    assert_eq!(form.questions.len(), 2);
    assert_eq!(form.questions[0].title, "Enable");
    assert!(form.questions[1].secret);
}

#[test]
fn dry_run_setup_does_not_persist_side_effects() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([(
            "provider-a".to_string(),
            json!({
                "type": "legacy",
                "spec": {
                    "title": "Provider A Setup",
                    "questions": [
                        {"name": "enabled", "kind": "boolean", "required": true}
                    ]
                }
            }),
        )]),
        &BTreeMap::from([("provider-a".to_string(), json!({"enabled": true}))]),
    )
    .expect("instructions");

    let result = greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::NoopSetupBackend,
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect("persist");
    assert_eq!(
        result.writes,
        vec!["state/setup/provider-a.json".to_string()]
    );
    assert!(!root.exists());
}

#[test]
fn replayed_setup_answers_produce_same_normalized_persisted_output() {
    let temp = TempDir::new().expect("tempdir");
    let root_one = temp.path().join("bundle-one");
    let root_two = temp.path().join("bundle-two");
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
    "advanced_setup":true,
    "app_packs":[],
    "extension_providers":["provider-a"],
    "remote_catalogs":[],
    "setup_specs":{{
      "provider-a": {{
        "type":"legacy",
        "spec": {{
          "title":"Provider A Setup",
          "questions":[
            {{"name":"enabled","kind":"boolean","required":true}},
            {{"name":"api_token","kind":"string","required":true,"secret":true}}
          ]
        }}
      }}
    }},
    "setup_answers":{{
      "provider-a": {{"enabled":"true","api_token":"secret123"}}
    }},
    "setup_execution_intent":true,
    "export_intent":false
  }},
  "locks":{{}}
}}"#,
            root_one.display()
        ),
    )
    .expect("answers");

    let output_one = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("apply one");
    assert!(
        output_one.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output_one.stderr)
    );

    let state_one = read_json(&root_one.join("state/setup/provider-a.json"));
    assert!(root_one.join("state/setup/provider-a.json").exists());
    assert_eq!(
        state_one.get("source_kind").and_then(Value::as_str),
        Some("legacy")
    );
    assert!(state_one.get("non_secret_config").is_some());
    // B12: secret answers persist only as `secret://` refs — no plaintext in
    // `secret_values` (gone) or `normalized_answers`.
    assert!(state_one.get("secret_values").is_none());
    assert_eq!(
        state_one
            .pointer("/secret_refs/api_token")
            .and_then(Value::as_str),
        Some("secret://local/demo-bundle/api_token")
    );
    assert!(state_one.pointer("/normalized_answers/api_token").is_none());

    let rewritten = fs::read_to_string(&answers_path).expect("original answers");
    let rewritten = rewritten.replace(
        &root_one.display().to_string(),
        &root_two.display().to_string(),
    );
    fs::write(&answers_path, rewritten).expect("rewrite answers");

    let output_two = Command::new(bundle_bin())
        .args(["wizard", "apply", "--answers"])
        .arg(&answers_path)
        .output()
        .expect("apply two");
    assert!(
        output_two.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output_two.stderr)
    );

    let state_two = read_json(&root_two.join("state/setup/provider-a.json"));
    assert_eq!(state_one, state_two);

    let lock = read_json(&root_one.join("bundle.lock.json"));
    assert_eq!(
        lock.pointer("/setup_state_files/0").and_then(Value::as_str),
        Some("state/setup/provider-a.json")
    );
}

#[test]
fn setup_persistence_applies_defaults_and_records_secret_refs() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([(
            "provider-a".to_string(),
            json!({
                "type": "legacy",
                "spec": {
                    "title": "Provider A Setup",
                    "questions": [
                        {"name": "enabled", "kind": "boolean", "required": true},
                        {"name": "region", "kind": "string", "required": false, "default": "eu-west-1"},
                        {"name": "api_token", "kind": "string", "required": true, "secret": true}
                    ]
                }
            }),
        )]),
        &BTreeMap::from([(
            "provider-a".to_string(),
            json!({"enabled": "true", "api_token": "secret123"}),
        )]),
    )
    .expect("instructions");

    let result = greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::FileSetupBackend::new(&root),
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect("persist");

    assert_eq!(
        result.writes,
        vec!["state/setup/provider-a.json".to_string()]
    );
    let state = &result.states[0];
    assert_eq!(state.normalized_answers.get("enabled"), Some(&json!(true)));
    assert_eq!(
        state.normalized_answers.get("region"),
        Some(&json!("eu-west-1"))
    );
    assert_eq!(
        state.non_secret_config.get("region"),
        Some(&json!("eu-west-1"))
    );
    // The secret answer is recorded as a `secret://<env>/<bundle>/<key>` ref,
    // never as plaintext — and is dropped from normalized_answers too (B12).
    assert_eq!(
        state.secret_refs.get("api_token").map(|r| r.as_str()),
        Some("secret://local/test-bundle/api_token")
    );
    assert!(!state.normalized_answers.contains_key("api_token"));
    assert!(!state.non_secret_config.contains_key("api_token"));
    assert!(root.join("state/setup/provider-a.json").exists());
}

#[test]
fn provider_qa_bridge_falls_back_to_ids_and_description_keys() {
    let qa_output = json!({
        "questions": [
            {
                "id": "api_token",
                "label": {"key": "provider.qa.setup.api_token"},
                "required": true,
                "default": "seed"
            }
        ]
    });
    let i18n = BTreeMap::from([(
        "provider.schema.config.api_token.description".to_string(),
        "API token for outbound calls".to_string(),
    )]);

    let form = greentic_bundle::setup::qa_bridge::provider_qa_to_form_spec(
        &qa_output,
        &i18n,
        "provider-a",
    );

    assert_eq!(form.id, "provider-a-setup");
    assert_eq!(form.title, "provider-a setup");
    assert_eq!(form.questions.len(), 1);
    assert_eq!(form.questions[0].title, "api_token");
    assert_eq!(
        form.questions[0].description.as_deref(),
        Some("API token for outbound calls")
    );
    assert_eq!(
        form.questions[0].default_value.as_ref(),
        Some(&json!("seed"))
    );
    assert!(form.questions[0].secret);
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}
