pub mod backend;
pub mod legacy_formspec;
pub mod persist;
pub mod qa_bridge;

use std::collections::BTreeMap;

use anyhow::Result;
use greentic_deploy_spec::SecretRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SETUP_STATE_DIR: &str = "state/setup";
pub const SETUP_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormSpec {
    pub id: String,
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub questions: Vec<QuestionSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSpec {
    pub id: String,
    pub kind: QuestionKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    pub secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    String,
    Number,
    Boolean,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SetupSpecInput {
    Legacy {
        spec: Value,
    },
    ProviderQa {
        qa_output: Value,
        #[serde(default)]
        i18n: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSetupState {
    pub schema_version: u32,
    pub provider_id: String,
    pub source_kind: String,
    pub form: FormSpec,
    pub normalized_answers: BTreeMap<String, Value>,
    pub non_secret_config: BTreeMap<String, Value>,
    /// `secret://<env>/<bundle>/<key>` references for secret-marked answers.
    /// The plaintext never persists here — it is routed to the env's secrets
    /// backend (the qa_persist path) and only the reference is recorded (B12).
    pub secret_refs: BTreeMap<String, SecretRef>,
}

pub fn form_spec_from_input(
    input: &SetupSpecInput,
    provider_id: &str,
) -> Result<(String, FormSpec)> {
    match input {
        SetupSpecInput::Legacy { spec } => {
            let parsed = match spec {
                Value::String(raw) => legacy_formspec::parse_setup_spec_str(raw)?,
                value => legacy_formspec::parse_setup_spec_value(value.clone())?,
            };
            Ok((
                "legacy".to_string(),
                legacy_formspec::setup_spec_to_form_spec(&parsed, provider_id),
            ))
        }
        SetupSpecInput::ProviderQa { qa_output, i18n } => Ok((
            "provider_qa".to_string(),
            qa_bridge::provider_qa_to_form_spec(qa_output, i18n, provider_id),
        )),
    }
}
