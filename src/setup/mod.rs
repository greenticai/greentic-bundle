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
/// Bumped to `3` for C7: the schema adds a required `env_id` field so each
/// persisted state is bound to the environment that produced it. Older
/// (`schema_version <= 2`) state files have no `env_id` and the wizard now
/// rejects them at re-mint time — they must be re-emitted under the active
/// `--env`. See [`PersistedSetupState::env_id`].
///
/// `2` was the B12 shape (plaintext `secret_values` → reference-only
/// `secret_refs` + `secret_refs` drop from `normalized_answers`). `1` was the
/// pre-B12 plaintext shape. Neither is deserialization-compatible with `3`
/// (the `env_id` field is required, not `#[serde(default)]`).
pub const SETUP_STATE_SCHEMA_VERSION: u32 = 3;

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
    /// Environment id that minted this state (C7). The same scope that builds
    /// the `secret://<env>/<bundle>/<provider>/<question>` URIs in
    /// [`secret_refs`](Self::secret_refs). Re-running the wizard against a
    /// different `--env` for the same provider in the same bundle is rejected
    /// at persist time: an existing state's `env_id` is compared against the
    /// active wizard scope's, and a mismatch fails closed rather than aliasing
    /// two envs' secret refs onto the same provider path.
    pub env_id: String,
    pub provider_id: String,
    pub source_kind: String,
    pub form: FormSpec,
    pub normalized_answers: BTreeMap<String, Value>,
    pub non_secret_config: BTreeMap<String, Value>,
    /// `secret://<env>/<bundle>/<provider_id>/<question_id>` references for
    /// secret-marked answers. Plaintext never persists in this state; only the
    /// reference is recorded (B12). The actual secret bytes are routed to the
    /// env's secrets backend by the *consuming* wizard pipeline (greentic-setup
    /// / greentic-operator `qa_persist` → DevStore), which writes under the
    /// distinct `secrets://<env>/<tenant>/<team>/<provider>/<key>` scheme — a
    /// separate address space. Phase D wires the resolver that bridges these
    /// two URI families (A10's deferred handler-backed `SecretsSink`).
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
