use std::collections::BTreeMap;
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

/// Env + bundle scope used to mint `secret://<env>/<bundle>/<key>` references
/// for secret-marked answers (B12, plan §246 app-bundle form).
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

fn build_state(
    instruction: &SetupInstruction,
    scope: &SetupScope<'_>,
) -> Result<PersistedSetupState> {
    let (source_kind, form) =
        super::form_spec_from_input(&instruction.spec_input, &instruction.provider_id)?;

    let mut normalized_answers = normalize_answers(&form, &instruction.answers)?;
    let mut non_secret_config = BTreeMap::new();
    let mut secret_refs = BTreeMap::new();
    for question in &form.questions {
        let Some(value) = normalized_answers.get(&question.id).cloned() else {
            continue;
        };
        if question.secret {
            // Secret answers never persist as plaintext: the value is routed to
            // the env's secrets backend (the qa_persist path) and only a
            // `secret://` reference is recorded here. Drop it from
            // normalized_answers too so the on-disk state carries no secret
            // material.
            normalized_answers.remove(&question.id);
            let uri = format!(
                "secret://{}/{}/{}",
                scope.env_id, scope.bundle_id, question.id
            );
            let secret_ref = SecretRef::try_new(uri)
                .with_context(|| format!("mint secret ref for answer {}", question.id))?;
            secret_refs.insert(question.id.clone(), secret_ref);
        } else {
            non_secret_config.insert(question.id.clone(), value);
        }
    }

    Ok(PersistedSetupState {
        schema_version: SETUP_STATE_SCHEMA_VERSION,
        provider_id: instruction.provider_id.clone(),
        source_kind,
        form,
        normalized_answers,
        non_secret_config,
        secret_refs,
    })
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
