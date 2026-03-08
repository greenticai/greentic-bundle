use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{FormSpec, QuestionKind, QuestionSpec};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub questions: Vec<SetupQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupQuestion {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub title: Option<String>,
}

fn default_kind() -> String {
    "string".to_string()
}

pub fn parse_setup_spec_value(value: Value) -> Result<SetupSpec> {
    serde_json::from_value(value).context("parse legacy setup spec")
}

pub fn parse_setup_spec_str(raw: &str) -> Result<SetupSpec> {
    serde_json::from_str(raw)
        .or_else(|_| serde_yaml_bw::from_str(raw))
        .context("parse legacy setup spec")
}

pub fn setup_spec_to_form_spec(spec: &SetupSpec, provider_id: &str) -> FormSpec {
    let display_name = provider_id
        .strip_prefix("messaging-")
        .or_else(|| provider_id.strip_prefix("events-"))
        .unwrap_or(provider_id);
    let display_name = capitalize(display_name);
    let title = spec
        .title
        .clone()
        .unwrap_or_else(|| format!("{display_name} setup"));

    FormSpec {
        id: format!("{provider_id}-setup"),
        title,
        version: "1.0.0".to_string(),
        description: Some(format!("{display_name} provider configuration")),
        questions: spec.questions.iter().map(convert_question).collect(),
    }
}

pub fn normalize_answer_value(question: &QuestionSpec, input: &Value) -> Result<Value> {
    match question.kind {
        QuestionKind::Boolean => normalize_boolean(input),
        QuestionKind::Number => normalize_number(input),
        QuestionKind::Enum => normalize_enum(input, &question.choices),
        QuestionKind::String => normalize_string_like(input),
    }
}

fn convert_question(q: &SetupQuestion) -> QuestionSpec {
    let inferred = infer_question_kind(&q.name, &q.kind);
    let kind = if q.kind == "string" {
        inferred.0
    } else {
        explicit_kind(&q.kind)
    };
    let secret = q.secret || inferred.1;
    let title = q.title.clone().unwrap_or_else(|| q.name.clone());

    QuestionSpec {
        id: q.name.clone(),
        kind,
        title,
        description: q.help.clone(),
        required: q.required,
        choices: q.choices.clone(),
        default_value: q.default.clone(),
        secret,
    }
}

fn explicit_kind(kind: &str) -> QuestionKind {
    match kind {
        "boolean" => QuestionKind::Boolean,
        "number" => QuestionKind::Number,
        "choice" | "enum" => QuestionKind::Enum,
        _ => QuestionKind::String,
    }
}

fn infer_question_kind(id: &str, fallback: &str) -> (QuestionKind, bool) {
    match id {
        "enabled" => (QuestionKind::Boolean, false),
        id if id.ends_with("_token") || id.contains("secret") || id.contains("password") => {
            (explicit_kind(fallback), true)
        }
        _ => (explicit_kind(fallback), false),
    }
}

fn normalize_boolean(input: &Value) -> Result<Value> {
    match input {
        Value::Bool(flag) => Ok(Value::Bool(*flag)),
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
            "false" | "f" | "no" | "n" | "0" => Ok(Value::Bool(false)),
            _ => bail!("invalid boolean value {text}"),
        },
        other => bail!("invalid boolean value {other}"),
    }
}

fn normalize_number(input: &Value) -> Result<Value> {
    match input {
        Value::Number(number) => Ok(Value::Number(number.clone())),
        Value::String(text) => Ok(Value::Number(
            text.parse::<serde_json::Number>()
                .with_context(|| format!("invalid number value {text}"))?,
        )),
        other => bail!("invalid number value {other}"),
    }
}

fn normalize_enum(input: &Value, choices: &[String]) -> Result<Value> {
    let text = normalize_string_like(input)?;
    let value = text.as_str().expect("string");
    if choices.is_empty() || choices.iter().any(|choice| choice == value) {
        Ok(text)
    } else {
        bail!("invalid choice {value}")
    }
}

fn normalize_string_like(input: &Value) -> Result<Value> {
    match input {
        Value::String(text) => Ok(Value::String(text.clone())),
        Value::Bool(flag) => Ok(Value::String(flag.to_string())),
        Value::Number(number) => Ok(Value::String(number.to_string())),
        other => bail!("invalid string value {other}"),
    }
}

fn capitalize(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
