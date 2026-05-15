use std::borrow::Cow;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

const SETUP_STATE_PREFIX: &str = "state/setup/";
const SETUP_STATE_SUFFIX: &str = ".json";
const SECRET_VALUES_KEY: &str = "secret_values";

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

// Phase 0 secret-leak hotfix: setup-state JSON files carry a `secret_values`
// map. The runtime still reads plaintext from the on-disk source-of-truth, but
// the archived copy that ships in the .gtbundle must never carry plaintext.
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
    if !map.contains_key(SECRET_VALUES_KEY) {
        return Ok(Cow::Borrowed(contents));
    }
    map.insert(SECRET_VALUES_KEY.to_string(), json!({}));
    let redacted = serde_json::to_string_pretty(&value)
        .with_context(|| format!("re-serialize redacted setup-state JSON: {name}"))?;
    Ok(Cow::Owned(format!("{redacted}\n")))
}

fn is_setup_state_file(name: &str) -> bool {
    name.starts_with(SETUP_STATE_PREFIX) && name.ends_with(SETUP_STATE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_plaintext_secret_values_in_setup_state() {
        let input = r#"{"schema_version":1,"provider_id":"p","source_kind":"legacy","form":{"id":"f","title":"t","version":"1","questions":[]},"normalized_answers":{},"non_secret_config":{},"secret_values":{"api_token":"sk-PLAINTEXT-LEAK"}}"#;
        let out = redact_secret_values("state/setup/p.json", input).expect("redact");
        let parsed: Value = serde_json::from_str(out.as_ref()).expect("parse output");
        assert_eq!(parsed["secret_values"], json!({}));
        assert!(!out.contains("sk-PLAINTEXT-LEAK"));
        assert_eq!(parsed["non_secret_config"], json!({}));
    }

    #[test]
    fn preserves_empty_secret_values_field() {
        let input = r#"{"secret_values":{}}"#;
        let out = redact_secret_values("state/setup/p.json", input).expect("redact");
        let parsed: Value = serde_json::from_str(out.as_ref()).expect("parse output");
        assert_eq!(parsed["secret_values"], json!({}));
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
