// Phase 0 P0.2 — secret-leak doctor scanner. See plans/next-gen-deployment.md.
//
// Patterns scanned (all default-on):
//   1. Dev-store filename match — anything under `.greentic/dev/`,
//      `.greentic/state/dev/`, or named `.dev.secrets.env`. Reuses
//      `bundle_fs::dev_secret_match` so writer-side denylist and
//      reader-side audit stay aligned.
//   2. `state/setup/*.json` with a non-empty `secret_values` map.
//   3. `state/setup/*.json` where `normalized_answers` retains any
//      `form.questions[*].secret == true` id (parallel leak — Codex
//      caught this during P0.1 review).
//   4. Raw archive bytes contain a dev-store filename substring (catches
//      anything that bypassed per-file enumeration via squashfs/zip
//      metadata, symlink-target strings, etc.).
//
// Pattern 5 (env-shape heuristic over `state/config/**`) is deferred until
// P0.3 / Phase C lands the non-secret config channel — otherwise it would
// false-positive on legitimate non-secret answers.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

const SETUP_STATE_PREFIX: &str = "state/setup/";
const SETUP_STATE_SUFFIX: &str = ".json";
const SECRET_VALUES_KEY: &str = "secret_values";
const NORMALIZED_ANSWERS_KEY: &str = "normalized_answers";
const FORM_KEY: &str = "form";
const QUESTIONS_KEY: &str = "questions";
const QUESTION_ID_KEY: &str = "id";
const QUESTION_SECRET_KEY: &str = "secret";

const DEV_STORE_BYTE_NEEDLES: &[&[u8]] = &[
    b".dev.secrets.env",
    b".greentic/dev/",
    b".greentic/state/dev/",
];

#[derive(Debug, Clone, Serialize)]
pub struct SecretsReport {
    pub ok: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    DevStorePath,
    SecretValuesPopulated,
    NormalizedAnswersLeak,
    ArchiveBytesContainsDevPath,
}

/// Scan a built `.gtbundle` artifact for plaintext-secret leaks. Extracts the
/// archive to a tempdir, walks every file with [`scan_build_dir`], and also
/// inspects the raw archive bytes via [`scan_archive_bytes`] so any path that
/// bypassed extraction (squashfs metadata, symlink targets) still surfaces.
pub fn scan_artifact(artifact: &Path) -> Result<SecretsReport> {
    let tempdir = tempfile::tempdir().context("create tempdir for doctor secret-leak scan")?;
    crate::bundle_fs::extract_bundle(artifact, tempdir.path()).with_context(|| {
        format!(
            "extract artifact {} for secret-leak scan",
            artifact.display()
        )
    })?;
    let mut findings = scan_build_dir(tempdir.path())?.findings;
    let bytes = std::fs::read(artifact)
        .with_context(|| format!("read artifact {} for raw-byte scan", artifact.display()))?;
    findings.extend(scan_archive_bytes(&bytes));
    Ok(report_of(findings))
}

/// Scan an already-materialized build/extract directory for plaintext-secret
/// leaks. Used by the workspace doctor (pre-archive) and by the artifact
/// doctor (after extracting to a tempdir).
pub fn scan_build_dir(dir: &Path) -> Result<SecretsReport> {
    let mut findings = Vec::new();
    for entry in WalkDir::new(dir).min_depth(1).follow_links(false) {
        let entry =
            entry.with_context(|| format!("walk {} for secret-leak scan", dir.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(dir).unwrap_or(entry.path());
        let rel_str = relative.display().to_string();

        if let Some(reason) = crate::bundle_fs::dev_secret_match(relative) {
            findings.push(Finding {
                kind: FindingKind::DevStorePath,
                path: Some(rel_str.clone()),
                message: format!("dev-store path present in build output ({reason})"),
            });
        }

        if is_setup_state_file(&rel_str) {
            let contents = std::fs::read(entry.path())
                .with_context(|| format!("read setup-state {rel_str} for secret-leak scan"))?;
            findings.extend(scan_setup_state_json(&rel_str, &contents));
        }
    }
    Ok(report_of(findings))
}

/// Scan raw archive bytes for known dev-store filename substrings. This is
/// defense-in-depth against any leak path that the file-walker can't see —
/// e.g., squashfs metadata strings that the reader silently drops, or
/// symlink-target strings stored as bytes inside the archive container.
pub fn scan_archive_bytes(bytes: &[u8]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for needle in DEV_STORE_BYTE_NEEDLES {
        if bytes_contains(bytes, needle) {
            findings.push(Finding {
                kind: FindingKind::ArchiveBytesContainsDevPath,
                path: None,
                message: format!(
                    "raw archive bytes contain dev-store substring \"{}\"",
                    String::from_utf8_lossy(needle)
                ),
            });
        }
    }
    findings
}

fn scan_setup_state_json(path: &str, bytes: &[u8]) -> Vec<Finding> {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let mut findings = Vec::new();

    if let Some(secret_values) = map.get(SECRET_VALUES_KEY).and_then(Value::as_object)
        && !secret_values.is_empty()
    {
        findings.push(Finding {
            kind: FindingKind::SecretValuesPopulated,
            path: Some(path.to_string()),
            message: format!(
                "setup-state {SECRET_VALUES_KEY} contains {} key(s); archive must ship an empty map",
                secret_values.len()
            ),
        });
    }

    let secret_ids = collect_secret_question_ids(map);
    if !secret_ids.is_empty()
        && let Some(answers) = map.get(NORMALIZED_ANSWERS_KEY).and_then(Value::as_object)
    {
        let leaked: Vec<&str> = secret_ids
            .iter()
            .filter(|id| answers.contains_key(*id))
            .map(String::as_str)
            .collect();
        if !leaked.is_empty() {
            findings.push(Finding {
                kind: FindingKind::NormalizedAnswersLeak,
                path: Some(path.to_string()),
                message: format!(
                    "setup-state {NORMALIZED_ANSWERS_KEY} retains secret-marked id(s): {}",
                    leaked.join(", ")
                ),
            });
        }
    }

    findings
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

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn report_of(findings: Vec<Finding>) -> SecretsReport {
    SecretsReport {
        ok: findings.is_empty(),
        findings,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, contents).expect("write fixture file");
    }

    #[test]
    fn clean_build_dir_yields_no_findings() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join("bundle-manifest.json"),
            br#"{"bundle_id":"ok"}"#,
        );
        write(
            &temp.path().join("state/setup/provider-a.json"),
            br#"{"schema_version":1,"secret_values":{},"normalized_answers":{}}"#,
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(report.ok, "expected clean, got {:?}", report.findings);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn dev_store_file_in_build_dir_is_flagged() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join(".greentic/dev/.dev.secrets.env"),
            b"GTC_API_TOKEN=sk-leak",
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(!report.ok);
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f.kind, FindingKind::DevStorePath)
                    && f.path.as_deref() == Some(".greentic/dev/.dev.secrets.env")),
            "missing DevStorePath finding in {:?}",
            report.findings
        );
    }

    #[test]
    fn stray_dev_secrets_env_filename_is_flagged() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join("packs/seed/.dev.secrets.env"),
            b"TOKEN=leak",
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(!report.ok);
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f.kind, FindingKind::DevStorePath)),
            "missing DevStorePath finding for stray .dev.secrets.env"
        );
    }

    #[test]
    fn populated_secret_values_in_setup_state_is_flagged() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join("state/setup/telegram.json"),
            br#"{"secret_values":{"api_token":"sk-LEAK"}}"#,
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(!report.ok);
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f.kind, FindingKind::SecretValuesPopulated)
                    && f.path.as_deref() == Some("state/setup/telegram.json")),
            "missing SecretValuesPopulated finding in {:?}",
            report.findings
        );
    }

    #[test]
    fn normalized_answers_leak_via_form_metadata_is_flagged() {
        let temp = TempDir::new().expect("tempdir");
        let body = br#"{
            "form":{"questions":[
                {"id":"api_token","secret":true},
                {"id":"name","secret":false}
            ]},
            "normalized_answers":{"api_token":"sk-LEAK","name":"bot"},
            "secret_values":{}
        }"#;
        write(&temp.path().join("state/setup/telegram.json"), body);
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(!report.ok);
        let leak = report
            .findings
            .iter()
            .find(|f| matches!(f.kind, FindingKind::NormalizedAnswersLeak))
            .expect("missing NormalizedAnswersLeak finding");
        assert!(leak.message.contains("api_token"));
        assert!(!leak.message.contains("name"));
    }

    #[test]
    fn normalized_answers_without_secret_marked_ids_is_safe() {
        let temp = TempDir::new().expect("tempdir");
        let body = br#"{
            "form":{"questions":[{"id":"name","secret":false}]},
            "normalized_answers":{"name":"bot"},
            "secret_values":{}
        }"#;
        write(&temp.path().join("state/setup/telegram.json"), body);
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(report.ok, "expected clean, got {:?}", report.findings);
    }

    #[test]
    fn malformed_setup_state_json_is_ignored_not_panicked() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join("state/setup/broken.json"),
            b"not-json-at-all",
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(report.ok, "expected clean, got {:?}", report.findings);
    }

    #[test]
    fn non_setup_state_json_with_secret_values_key_is_ignored() {
        let temp = TempDir::new().expect("tempdir");
        write(
            &temp.path().join("resolved/default.yaml"),
            br#"{"secret_values":{"leaked":"value"}}"#,
        );
        let report = scan_build_dir(temp.path()).expect("scan");
        assert!(report.ok, "expected clean, got {:?}", report.findings);
    }

    #[test]
    fn scan_archive_bytes_flags_dev_store_substrings() {
        let bytes = b"...lots of squashfs metadata.../.greentic/dev/secrets...";
        let findings = scan_archive_bytes(bytes);
        assert!(
            findings.iter().any(
                |f| matches!(f.kind, FindingKind::ArchiveBytesContainsDevPath)
                    && f.message.contains(".greentic/dev/")
            ),
            "expected archive-bytes finding, got {findings:?}"
        );
    }

    #[test]
    fn scan_archive_bytes_flags_dev_secrets_env_substring() {
        let bytes = b"some bytes ... .dev.secrets.env ... more bytes";
        let findings = scan_archive_bytes(bytes);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains(".dev.secrets.env")),
            "expected .dev.secrets.env finding, got {findings:?}"
        );
    }

    #[test]
    fn scan_archive_bytes_passes_through_clean_bytes() {
        let bytes = b"a perfectly fine squashfs payload with no dev-store strings";
        let findings = scan_archive_bytes(bytes);
        assert!(findings.is_empty(), "expected clean, got {findings:?}");
    }

    #[test]
    fn bytes_contains_handles_short_haystack() {
        assert!(!bytes_contains(b"abc", b"abcdef"));
        assert!(!bytes_contains(b"", b"abc"));
        assert!(!bytes_contains(b"abc", b""));
        assert!(bytes_contains(b"prefix-abc-suffix", b"abc"));
    }

    #[test]
    fn collects_secret_ids_only_when_secret_true() {
        let map: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{"form":{"questions":[
                {"id":"k1","secret":true},
                {"id":"k2","secret":false},
                {"id":"k3"},
                {"id":"k4","secret":true}
            ]}}"#,
        )
        .unwrap();
        let mut ids = collect_secret_question_ids(&map);
        ids.sort();
        assert_eq!(ids, vec!["k1", "k4"]);
    }

    #[test]
    fn is_setup_state_file_matches_expected_paths() {
        assert!(is_setup_state_file("state/setup/foo.json"));
        assert!(is_setup_state_file("state/setup/nested/foo.json"));
        assert!(!is_setup_state_file("state/setup/foo.txt"));
        assert!(!is_setup_state_file("resolved/foo.json"));
    }
}
