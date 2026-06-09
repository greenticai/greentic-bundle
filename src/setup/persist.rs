use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use greentic_deploy_spec::SecretRef;
use serde_json::Value;

use super::backend::SetupBackend;
use super::legacy_formspec::normalize_answer_value;
use super::{
    FormSpec, PersistedSetupState, QuestionSpec, SETUP_STATE_DIR, SETUP_STATE_SCHEMA_VERSION,
    SetupSpecInput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupInstruction {
    pub provider_id: String,
    pub spec_input: SetupSpecInput,
    pub answers: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPersistenceResult {
    pub states: Vec<PersistedSetupState>,
    pub writes: Vec<String>,
}

/// Env + bundle scope used to mint
/// `secret://<env>/<bundle>/<provider_id>/<question_id>` references for
/// secret-marked answers (B12, plan §246 app-bundle form). `provider_id` is
/// part of the path so two providers in one bundle can carry the same secret
/// key (e.g. `api_token`) without colliding on the same ref.
#[derive(Debug, Clone, Copy)]
pub struct SetupScope<'a> {
    pub env_id: &'a str,
    pub bundle_id: &'a str,
}

pub fn collect_setup_instructions(
    specs: &BTreeMap<String, Value>,
    answers: &BTreeMap<String, Value>,
) -> Result<Vec<SetupInstruction>> {
    let mut provider_ids = specs.keys().cloned().collect::<Vec<_>>();
    provider_ids.sort();
    provider_ids.dedup();

    let mut instructions = Vec::new();
    for provider_id in provider_ids {
        let spec_input = parse_spec_input(
            specs
                .get(&provider_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing setup spec for {provider_id}"))?,
        )?;
        let provider_answers = answers
            .get(&provider_id)
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        instructions.push(SetupInstruction {
            provider_id,
            spec_input,
            answers: provider_answers,
        });
    }
    Ok(instructions)
}

pub fn persist_setup(
    root: &Path,
    instructions: &[SetupInstruction],
    backend: &dyn SetupBackend,
    scope: &SetupScope<'_>,
) -> Result<SetupPersistenceResult> {
    let mut states = Vec::new();
    let mut writes = Vec::new();
    for instruction in instructions {
        let state = build_state(instruction, scope)?;
        reject_env_id_remint(root, &state)?;
        if let Some(path) = backend.persist(&state)? {
            writes.push(relative_display(root, &path));
        } else {
            writes.push(format!(
                "{SETUP_STATE_DIR}/{}.json",
                instruction.provider_id
            ));
        }
        states.push(state);
    }
    writes.sort();
    writes.dedup();
    Ok(SetupPersistenceResult { states, writes })
}

/// Reject re-minting an existing state under a different env (C7).
///
/// Re-running the wizard for the same `(bundle, provider)` under a new
/// `--env` would aliase two envs' `secret://` refs onto the same provider
/// path — the on-disk URI now reads as one env, while the in-memory scope
/// (and live DevStore writes) point at another. Detect at the persist
/// boundary by reading whatever's on disk and comparing `env_id`. Older
/// `schema_version <= 2` states have no `env_id` and are likewise rejected:
/// they were minted before C7's binding existed, so the conservative answer
/// is to require a fresh emission.
fn reject_env_id_remint(root: &Path, state: &PersistedSetupState) -> Result<()> {
    let path = root
        .join(SETUP_STATE_DIR)
        .join(format!("{}.json", state.provider_id));
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(());
    };
    let existing: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => bail!(
            "persisted setup state at `{}` is corrupt JSON ({e}), refusing to overwrite (delete the file and re-run the wizard)",
            path.display()
        ),
    };
    let existing_env = existing.get("env_id").and_then(|v| v.as_str());
    match existing_env {
        Some(env) if env == state.env_id => Ok(()),
        Some(env) => bail!(
            "persisted setup state at `{}` was minted under env `{env}`, refusing to overwrite under env `{}` (use a different bundle_id to keep both envs)",
            path.display(),
            state.env_id
        ),
        None => bail!(
            "persisted setup state at `{}` has no env_id (pre-C7 schema), refusing to overwrite under env `{}` (delete the file and re-run the wizard to mint fresh state)",
            path.display(),
            state.env_id
        ),
    }
}

fn build_state(
    instruction: &SetupInstruction,
    scope: &SetupScope<'_>,
) -> Result<PersistedSetupState> {
    let (source_kind, form) =
        super::form_spec_from_input(&instruction.spec_input, &instruction.provider_id)?;

    // Validate the three identifiers that flow into the `secret://` URI path.
    // SecretRef::try_new only enforces scheme + non-empty env segment, so a
    // `/` or empty value here would silently corrupt the 4-segment layout and
    // alias unrelated slots. bundle_id is normalized upstream by
    // normalize_bundle_id but we still validate it for defense-in-depth.
    validate_ref_segment("env_id", scope.env_id)?;
    validate_ref_segment("bundle_id", scope.bundle_id)?;
    validate_ref_segment("provider_id", &instruction.provider_id)?;

    let mut normalized_answers = normalize_answers(&form, &instruction.answers)?;
    let mut non_secret_config = BTreeMap::new();
    let mut secret_refs = BTreeMap::new();
    let mut seen_ids = BTreeSet::new();
    for question in &form.questions {
        if !seen_ids.insert(question.id.clone()) {
            // Duplicate id in the form spec would let the first iteration
            // remove the answer from normalized_answers (for secrets) and the
            // second iteration's `.get(...)` would return None — silently
            // dropping the second question. Fail loudly instead.
            bail!(
                "duplicate question id `{}` in setup spec for provider `{}`",
                question.id,
                instruction.provider_id
            );
        }
        let Some(value) = normalized_answers.get(&question.id).cloned() else {
            continue;
        };
        if question.secret {
            // Plaintext never persists in PersistedSetupState. This crate
            // records only a `secret://` reference; the actual secret bytes
            // are routed to the env's secrets backend by the *consuming*
            // wizard pipeline (greentic-setup / greentic-operator qa_persist
            // → DevStore). NOTE: the two URI schemes are distinct address
            // spaces — DevStore writes under `secrets://<env>/<tenant>/<team>/
            // <provider>/<key>`, this crate mints `secret://<env>/<bundle>/
            // <provider>/<question>`. Phase D wires the resolver that bridges
            // them (A10's deferred real `SecretsSink`); today the on-disk
            // state is intent + provenance, not storage.
            //
            // Drop the answer from normalized_answers too so the state file
            // carries zero secret material.
            validate_ref_segment("question.id", &question.id)?;
            normalized_answers.remove(&question.id);
            let uri = format!(
                "secret://{}/{}/{}/{}",
                scope.env_id, scope.bundle_id, instruction.provider_id, question.id
            );
            let secret_ref = SecretRef::try_new(uri)
                .with_context(|| format!("mint secret ref for answer {}", question.id))?;
            secret_refs.insert(question.id.clone(), secret_ref);
        } else {
            non_secret_config.insert(question.id.clone(), value);
        }
    }

    if !secret_refs.is_empty() {
        tracing::debug!(
            provider = %instruction.provider_id,
            count = secret_refs.len(),
            "recorded secret refs in setup state — backend population is the consuming wizard pipeline's responsibility (qa_persist → secrets backend)",
        );
    }

    Ok(PersistedSetupState {
        schema_version: SETUP_STATE_SCHEMA_VERSION,
        env_id: scope.env_id.to_string(),
        provider_id: instruction.provider_id.clone(),
        source_kind,
        form,
        normalized_answers,
        non_secret_config,
        secret_refs,
    })
}

/// Reject empty or `/`-containing identifiers that would corrupt the
/// `secret://<env>/<bundle>/<provider>/<question>` path structure
/// (SecretRef::try_new only validates the scheme + non-empty env segment).
fn validate_ref_segment(label: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} must not be empty when minting a secret ref");
    }
    if value.contains('/') {
        bail!("{label} `{value}` contains '/' which would corrupt the secret-ref URI path");
    }
    Ok(())
}

fn normalize_answers(form: &FormSpec, answers: &Value) -> Result<BTreeMap<String, Value>> {
    let map = answers
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("setup answers must be a JSON object"))?;
    let mut normalized = BTreeMap::new();
    for question in &form.questions {
        match map.get(&question.id) {
            Some(value) => {
                normalized.insert(
                    question.id.clone(),
                    normalize_question_answer(question, value)?,
                );
            }
            None if question.required => bail!("missing required setup answer for {}", question.id),
            None => {
                if let Some(default) = &question.default_value {
                    normalized.insert(question.id.clone(), default.clone());
                }
            }
        }
    }
    Ok(normalized)
}

fn normalize_question_answer(question: &QuestionSpec, value: &Value) -> Result<Value> {
    normalize_answer_value(question, value)
}

fn parse_spec_input(value: Value) -> Result<SetupSpecInput> {
    serde_json::from_value(value).map_err(Into::into)
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
