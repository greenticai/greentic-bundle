use std::borrow::Cow;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

const SETUP_STATE_PREFIX: &str = "state/setup/";
const SETUP_STATE_SUFFIX: &str = ".json";
const SECRET_VALUES_KEY: &str = "secret_values";
const NORMALIZED_ANSWERS_KEY: &str = "normalized_answers";
const FORM_KEY: &str = "form";
const QUESTIONS_KEY: &str = "questions";
const QUESTION_ID_KEY: &str = "id";
const QUESTION_SECRET_KEY: &str = "secret";

#[derive(Debug, Clone)]
pub struct ExportPlan {
    pub artifact_path: String,
    pub build_dir: String,
    pub manifest_path: String,
}

pub fn export_plan(state: &crate::build::plan::BuildState, artifact: &Path) -> ExportPlan {
    ExportPlan {
        artifact_path: artifact.display().to_string(),
        build_dir: state.build_dir.display().to_string(),
        manifest_path: state
            .build_dir
            .join("bundle-manifest.json")
            .display()
            .to_string(),
    }
}

pub fn write_build_outputs(
    state: &crate::build::plan::BuildState,
    artifact: &Path,
    warmup: bool,
) -> Result<crate::build::BuildResult> {
    write_normalized_build_dir(state, &state.build_dir)?;
    if warmup {
        crate::build::warmup::warmup_build_dir(&state.build_dir)?;
    }
    crate::bundle_fs::write_bundle(&state.build_dir, artifact)?;
    Ok(crate::build::BuildResult {
        artifact_path: artifact.display().to_string(),
        build_dir: state.build_dir.display().to_string(),
        manifest_path: state
            .build_dir
            .join("bundle-manifest.json")
            .display()
            .to_string(),
    })
}

pub fn write_normalized_build_dir(
    state: &crate::build::plan::BuildState,
    build_dir: &Path,
) -> Result<()> {
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)?;
    }
    fs::create_dir_all(build_dir)?;
    fs::write(
        build_dir.join("bundle-manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&state.manifest)?),
    )?;
    fs::write(
        build_dir.join("bundle-lock.json"),
        format!("{}\n", serde_json::to_string_pretty(&state.lock)?),
    )?;
    fs::write(build_dir.join("bundle.yaml"), &state.bundle_yaml)?;
    for (name, contents) in &state.resolved_files {
        let path = build_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
    }
    for (name, contents) in &state.setup_files {
        let path = build_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let redacted = redact_secret_values(name, contents)?;
        fs::write(path, redacted.as_bytes())?;
    }
    for (name, contents) in &state.asset_files {
        let path = build_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
    }
    Ok(())
}

// Phase 0 secret-leak hotfix: setup-state JSON files carry plaintext secrets
// in TWO places — `secret_values` (the split-out map) and `normalized_answers`
// (the full pre-split map, which retains every answer including secret ones —
// see greentic-bundle/src/setup/persist.rs:84-105). The runtime still reads
// plaintext from the on-disk source-of-truth, but the archived copy that ships
// in the .gtbundle must never carry plaintext.
//
// Strategy: parse with serde_json::Value (tolerant to schema drift), discover
// the secret question IDs from the embedded `form.questions[*].secret` flag,
// then drop those IDs from `normalized_answers` AND clear `secret_values`.
// See plans/next-gen-deployment.md P0.1.
fn redact_secret_values<'a>(name: &str, contents: &'a str) -> Result<Cow<'a, str>> {
    if !is_setup_state_file(name) {
        return Ok(Cow::Borrowed(contents));
    }
    let mut value: Value = serde_json::from_str(contents)
        .with_context(|| format!("parse setup-state JSON for secret_values redaction: {name}"))?;
    let Some(map) = value.as_object_mut() else {
        return Ok(Cow::Borrowed(contents));
    };
    let secret_ids = collect_secret_question_ids(map);
    let mut changed = false;
    // Drop the legacy `secret_values` key entirely (B12 producers no longer
    // emit it; leaving a stale `{}` would round-trip through a B12-aware
    // deserializer as an unknown field and mask a real missing `secret_refs`).
    if map.remove(SECRET_VALUES_KEY).is_some() {
        changed = true;
    }
    if !secret_ids.is_empty()
        && let Some(answers) = map
            .get_mut(NORMALIZED_ANSWERS_KEY)
            .and_then(Value::as_object_mut)
    {
        for id in &secret_ids {
            if answers.remove(id).is_some() {
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(Cow::Borrowed(contents));
    }
    let redacted = serde_json::to_string_pretty(&value)
        .with_context(|| format!("re-serialize redacted setup-state JSON: {name}"))?;
    Ok(Cow::Owned(format!("{redacted}\n")))
}

fn collect_secret_question_ids(map: &serde_json::Map<String, Value>) -> Vec<String> {
    let Some(questions) = map
        .get(FORM_KEY)
        .and_then(|form| form.get(QUESTIONS_KEY))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    questions
        .iter()
        .filter(|q| {
            q.get(QUESTION_SECRET_KEY)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|q| {
            q.get(QUESTION_ID_KEY)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn is_setup_state_file(name: &str) -> bool {
    name.starts_with(SETUP_STATE_PREFIX) && name.ends_with(SETUP_STATE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_plaintext_secret_values_in_setup_state() {
        let input = r#"{"schema_version":1,"provider_id":"p","source_kind":"legacy","form":{"id":"f","title":"t","version":"1","questions":[]},"normalized_answers":{},"non_secret_config":{},"secret_values":{"api_token":"sk-PLAINTEXT-LEAK"}}"#;
        let out = redact_secret_values("state/setup/p.json", input).expect("redact");
        let parsed: Value = serde_json::from_str(out.as_ref()).expect("parse output");
        // B12: the legacy `secret_values` key is removed entirely (not cleared)
        // so the redacted file deserializes cleanly into the new schema.
        assert!(parsed.get("secret_values").is_none());
        assert!(!out.contains("sk-PLAINTEXT-LEAK"));
        assert_eq!(parsed["non_secret_config"], json!({}));
    }

    // Codex adversarial review caught this: persist.rs:84-105 writes the full
    // pre-split map to `normalized_answers`, then copies secret-marked values
    // into `secret_values`. Both fields ship in the archive. Redacting only
    // `secret_values` leaves the plaintext alive in `normalized_answers`.
    #[test]
    fn redacts_plaintext_secrets_from_normalized_answers_via_form_metadata() {
        let input = r#"{
            "schema_version":1,
            "provider_id":"telegram",
            "source_kind":"legacy",
            "form":{
                "id":"telegram","title":"Telegram","version":"1",
                "questions":[
                    {"id":"api_token","kind":"string","title":"Token","required":true,"secret":true},
                    {"id":"name","kind":"string","title":"Name","required":true,"secret":false}
                ]
            },
            "normalized_answers":{"api_token":"sk-PLAINTEXT-LEAK","name":"my-bot"},
            "non_secret_config":{"name":"my-bot"},
            "secret_values":{"api_token":"sk-PLAINTEXT-LEAK"}
        }"#;
        let out = redact_secret_values("state/setup/telegram.json", input).expect("redact");
        assert!(
            !out.contains("sk-PLAINTEXT-LEAK"),
            "redacted JSON must not contain the secret token, got:\n{out}"
        );
        let parsed: Value = serde_json::from_str(out.as_ref()).expect("parse output");
        assert!(parsed.get("secret_values").is_none());
        assert_eq!(parsed["normalized_answers"], json!({"name": "my-bot"}));
        assert_eq!(parsed["non_secret_config"], json!({"name": "my-bot"}));
    }

    #[test]
    fn collects_secret_ids_from_embedded_form_metadata() {
        let map: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{
                "form": {
                    "questions": [
                        {"id":"k1","secret":true},
                        {"id":"k2","secret":false},
                        {"id":"k3","secret":true}
                    ]
                }
            }"#,
        )
        .unwrap();
        let mut ids = collect_secret_question_ids(&map);
        ids.sort();
        assert_eq!(ids, vec!["k1".to_string(), "k3".to_string()]);
    }

    #[test]
    fn collects_no_secret_ids_when_form_missing() {
        let map: serde_json::Map<String, Value> =
            serde_json::from_str(r#"{"normalized_answers":{}}"#).unwrap();
        assert!(collect_secret_question_ids(&map).is_empty());
    }

    #[test]
    fn removes_empty_secret_values_field() {
        let input = r#"{"secret_values":{}}"#;
        let out = redact_secret_values("state/setup/p.json", input).expect("redact");
        let parsed: Value = serde_json::from_str(out.as_ref()).expect("parse output");
        // B12: a stray `secret_values` key (even empty) is dropped so it can't
        // mask a missing `secret_refs` for a B12-aware reader.
        assert!(parsed.get("secret_values").is_none());
    }

    #[test]
    fn passes_through_non_setup_state_files() {
        let input = r#"{"secret_values":{"leaked":"value"}}"#;
        let out = redact_secret_values("resolved/default.yaml", input).expect("redact");
        assert!(matches!(out, Cow::Borrowed(_)));
        assert!(out.contains("leaked"));
    }

    #[test]
    fn passes_through_setup_state_without_secret_values_field() {
        let input = r#"{"schema_version":1}"#;
        let out = redact_secret_values("state/setup/p.json", input).expect("redact");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn bails_on_invalid_setup_state_json() {
        let input = "not-json-at-all";
        let err = redact_secret_values("state/setup/p.json", input).expect_err("must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("state/setup/p.json"));
    }

    #[test]
    fn rejects_setup_state_files_outside_setup_dir() {
        assert!(!is_setup_state_file("resolved/foo.json"));
        assert!(!is_setup_state_file("state/setup/foo.txt"));
        assert!(is_setup_state_file("state/setup/foo.json"));
        assert!(is_setup_state_file("state/setup/nested/foo.json"));
    }
}
