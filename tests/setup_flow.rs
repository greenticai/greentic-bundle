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
    // `secret_values` (gone) or `normalized_answers`. The ref path includes
    // `provider_id` to disambiguate same-named secrets across providers.
    assert!(state_one.get("secret_values").is_none());
    assert_eq!(
        state_one
            .pointer("/secret_refs/api_token")
            .and_then(Value::as_str),
        Some("secret://local/demo-bundle/provider-a/api_token")
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
    // The secret answer is recorded as a
    // `secret://<env>/<bundle>/<provider_id>/<question_id>` ref, never as
    // plaintext — and is dropped from normalized_answers too (B12).
    assert_eq!(
        state.secret_refs.get("api_token").map(|r| r.as_str()),
        Some("secret://local/test-bundle/provider-a/api_token")
    );
    assert!(!state.normalized_answers.contains_key("api_token"));
    assert!(!state.non_secret_config.contains_key("api_token"));
    assert!(root.join("state/setup/provider-a.json").exists());
}

/// Regression for the multi-provider collision flagged by Codex review:
/// two providers in the same bundle commonly share a secret key like
/// `api_token`, so the ref path must include `provider_id` or both refs alias
/// the same backend slot. Asserts the two refs are distinct and provider-scoped.
#[test]
fn secret_refs_disambiguate_same_question_id_across_providers() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().join("bundle");
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([
            ("provider-a".to_string(), spec.clone()),
            ("provider-b".to_string(), spec),
        ]),
        &BTreeMap::from([
            ("provider-a".to_string(), json!({"api_token": "a-secret"})),
            ("provider-b".to_string(), json!({"api_token": "b-secret"})),
        ]),
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

    let by_provider: BTreeMap<&str, &greentic_bundle::setup::PersistedSetupState> = result
        .states
        .iter()
        .map(|s| (s.provider_id.as_str(), s))
        .collect();
    let a = by_provider.get("provider-a").expect("provider-a state");
    let b = by_provider.get("provider-b").expect("provider-b state");
    assert_eq!(
        a.secret_refs.get("api_token").map(|r| r.as_str()),
        Some("secret://local/test-bundle/provider-a/api_token")
    );
    assert_eq!(
        b.secret_refs.get("api_token").map(|r| r.as_str()),
        Some("secret://local/test-bundle/provider-b/api_token")
    );
    assert_ne!(
        a.secret_refs.get("api_token"),
        b.secret_refs.get("api_token"),
        "same question id across providers must mint distinct refs"
    );
}

/// Same answers + spec, different `env_id` on the scope → distinct refs.
/// Closes the env-scoping coverage gap in the replay-determinism test.
#[test]
fn secret_refs_discriminate_on_env_id() {
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("provider-a".to_string(), spec)]),
        &BTreeMap::from([("provider-a".to_string(), json!({"api_token": "sec"}))]),
    )
    .expect("instructions");

    let mk = |env_id: &str| {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("bundle");
        let r = greentic_bundle::setup::persist::persist_setup(
            &root,
            &instructions,
            &greentic_bundle::setup::backend::NoopSetupBackend,
            &greentic_bundle::setup::persist::SetupScope {
                env_id,
                bundle_id: "test-bundle",
            },
        )
        .expect("persist");
        r.states
            .into_iter()
            .next()
            .expect("one state")
            .secret_refs
            .get("api_token")
            .map(|r| r.as_str().to_string())
            .expect("ref")
    };
    let local = mk("local");
    let staging = mk("staging");
    assert_eq!(local, "secret://local/test-bundle/provider-a/api_token");
    assert_eq!(staging, "secret://staging/test-bundle/provider-a/api_token");
    assert_ne!(
        local, staging,
        "different env_id must produce different refs"
    );
}

/// Reject empty or `/`-bearing identifiers at mint time — these would
/// silently corrupt the 4-segment URI structure (SecretRef::try_new only
/// validates scheme + non-empty env segment).
#[test]
fn ref_segment_validation_rejects_slashes_and_empties() {
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "tok", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("p".to_string(), spec.clone())]),
        &BTreeMap::from([("p".to_string(), json!({"tok": "sec"}))]),
    )
    .expect("instructions");
    let mk = |env_id, bundle_id| {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("bundle");
        greentic_bundle::setup::persist::persist_setup(
            &root,
            &instructions,
            &greentic_bundle::setup::backend::NoopSetupBackend,
            &greentic_bundle::setup::persist::SetupScope { env_id, bundle_id },
        )
    };
    assert!(mk("", "test-bundle").is_err(), "empty env_id must error");
    assert!(
        mk("staging/eu", "test-bundle").is_err(),
        "env_id with `/` must error"
    );
    assert!(mk("local", "").is_err(), "empty bundle_id must error");
    assert!(
        mk("local", "test/bundle").is_err(),
        "bundle_id with `/` must error"
    );

    // provider_id with `/` (from a malformed `setup_specs` key)
    let bad_provider = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("my/provider".to_string(), spec.clone())]),
        &BTreeMap::from([("my/provider".to_string(), json!({"tok": "sec"}))]),
    )
    .expect("instructions");
    let tmp = TempDir::new().expect("tempdir");
    assert!(
        greentic_bundle::setup::persist::persist_setup(
            &tmp.path().join("bundle"),
            &bad_provider,
            &greentic_bundle::setup::backend::NoopSetupBackend,
            &greentic_bundle::setup::persist::SetupScope {
                env_id: "local",
                bundle_id: "test-bundle",
            },
        )
        .is_err(),
        "provider_id with `/` must error"
    );
}

/// Duplicate question id in a form spec must fail loudly — the previous
/// behavior would silently drop the second iteration after the first
/// removed the answer from `normalized_answers`.
#[test]
fn duplicate_question_id_in_form_spec_is_rejected() {
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true},
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("p".to_string(), spec)]),
        &BTreeMap::from([("p".to_string(), json!({"api_token": "sec"}))]),
    )
    .expect("instructions");
    let tmp = TempDir::new().expect("tempdir");
    let err = greentic_bundle::setup::persist::persist_setup(
        &tmp.path().join("bundle"),
        &instructions,
        &greentic_bundle::setup::backend::NoopSetupBackend,
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect_err("duplicate question id must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("duplicate question id") && msg.contains("api_token"),
        "error message must name the duplicate id, got: {msg}"
    );
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

/// C7: persisted state records the active scope's `env_id`. Same provider,
/// same answers, two different scopes → two states whose `env_id` matches the
/// scope that built them.
#[test]
fn persisted_setup_state_records_env_id_from_scope() {
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("provider-a".to_string(), spec)]),
        &BTreeMap::from([("provider-a".to_string(), json!({"api_token": "sec"}))]),
    )
    .expect("instructions");

    let mk = |env_id: &str| {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("bundle");
        let r = greentic_bundle::setup::persist::persist_setup(
            &root,
            &instructions,
            &greentic_bundle::setup::backend::NoopSetupBackend,
            &greentic_bundle::setup::persist::SetupScope {
                env_id,
                bundle_id: "test-bundle",
            },
        )
        .expect("persist");
        r.states.into_iter().next().expect("one state").env_id
    };
    assert_eq!(mk("local"), "local");
    assert_eq!(mk("staging"), "staging");
}

/// C7: refuse to overwrite a state that was minted under a different env.
/// Aliases the same provider's `secret://` ref across two envs otherwise.
#[test]
fn persist_rejects_remint_under_different_env_id() {
    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("provider-a".to_string(), spec)]),
        &BTreeMap::from([("provider-a".to_string(), json!({"api_token": "sec"}))]),
    )
    .expect("instructions");

    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("bundle");
    // Mint under `local`.
    greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::FileSetupBackend::new(&root),
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect("first persist");

    // Re-running the wizard under `staging` for the same (bundle, provider) must fail.
    let err = greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::FileSetupBackend::new(&root),
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "staging",
            bundle_id: "test-bundle",
        },
    )
    .expect_err("env_id remint must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("minted under env `local`") && msg.contains("env `staging`"),
        "error must name both envs, got: {msg}"
    );

    // Same env is allowed (overwrite is the happy path).
    greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::FileSetupBackend::new(&root),
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect("same-env remint must succeed");
}

/// C7: pre-C7 on-disk state files (schema_version <= 2, no `env_id`) are
/// rejected at remint time. Operator must delete the file and re-run the
/// wizard so the new state is bound to an env from the start.
#[test]
fn persist_rejects_pre_c7_state_without_env_id() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("bundle");
    fs::create_dir_all(root.join("state/setup")).expect("mkdir");
    fs::write(
        root.join("state/setup/provider-a.json"),
        r#"{"schema_version":2,"provider_id":"provider-a","source_kind":"legacy","form":{"id":"provider-a-setup","title":"x","version":"1.0.0","questions":[]},"normalized_answers":{},"non_secret_config":{},"secret_refs":{}}"#,
    )
    .expect("seed pre-C7 state");

    let spec = json!({
        "type": "legacy",
        "spec": {
            "title": "Provider Setup",
            "questions": [
                {"name": "api_token", "kind": "string", "required": true, "secret": true}
            ]
        }
    });
    let instructions = greentic_bundle::setup::persist::collect_setup_instructions(
        &BTreeMap::from([("provider-a".to_string(), spec)]),
        &BTreeMap::from([("provider-a".to_string(), json!({"api_token": "sec"}))]),
    )
    .expect("instructions");

    let err = greentic_bundle::setup::persist::persist_setup(
        &root,
        &instructions,
        &greentic_bundle::setup::backend::FileSetupBackend::new(&root),
        &greentic_bundle::setup::persist::SetupScope {
            env_id: "local",
            bundle_id: "test-bundle",
        },
    )
    .expect_err("pre-C7 state must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("no env_id") && msg.contains("pre-C7"),
        "error must name the pre-C7 condition, got: {msg}"
    );
}

/// C7: the bumped `SETUP_STATE_SCHEMA_VERSION` is what new states report.
/// Pinning the constant prevents an accidental rollback.
#[test]
fn persisted_state_advertises_c7_schema_version() {
    assert_eq!(greentic_bundle::setup::SETUP_STATE_SCHEMA_VERSION, 3);
}
