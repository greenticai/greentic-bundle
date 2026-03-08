use std::collections::BTreeMap;

use serde_json::Value;

use super::{FormSpec, QuestionKind, QuestionSpec};

pub fn provider_qa_to_form_spec(
    qa_output: &Value,
    i18n: &BTreeMap<String, String>,
    provider_id: &str,
) -> FormSpec {
    let mode = qa_output
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("setup");
    let title_key = qa_output
        .get("title")
        .and_then(|value| value.get("key"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let title = i18n
        .get(title_key)
        .cloned()
        .unwrap_or_else(|| format!("{provider_id} setup"));
    let description = Some(format!(
        "{} provider configuration",
        capitalize(provider_id)
    ));

    let questions = qa_output
        .get("questions")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .filter_map(|question| convert_question(question, i18n))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    FormSpec {
        id: format!("{provider_id}-{mode}"),
        title,
        version: "1.0.0".to_string(),
        description,
        questions,
    }
}

fn convert_question(question: &Value, i18n: &BTreeMap<String, String>) -> Option<QuestionSpec> {
    let id = question.get("id").and_then(Value::as_str)?.to_string();
    let label_key = question
        .get("label")
        .and_then(|value| value.get("key").or(Some(value)))
        .and_then(Value::as_str)
        .unwrap_or(&id)
        .to_string();
    let title = i18n.get(&label_key).cloned().unwrap_or_else(|| id.clone());
    let description_key = description_key(&label_key, &id);
    let description = description_key.and_then(|key| i18n.get(&key).cloned());
    let required = question
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let kind = infer_kind(&id);
    let default_value = question.get("default").cloned();

    Some(QuestionSpec {
        id,
        kind,
        title,
        description,
        required,
        choices: Vec::new(),
        default_value,
        secret: is_secret_key(&label_key),
    })
}

fn description_key(label_key: &str, question_id: &str) -> Option<String> {
    let prefix = label_key.split(".qa.").next()?;
    Some(format!("{prefix}.schema.config.{question_id}.description"))
}

fn infer_kind(id: &str) -> QuestionKind {
    if id == "enabled" {
        QuestionKind::Boolean
    } else {
        QuestionKind::String
    }
}

fn is_secret_key(label_key: &str) -> bool {
    label_key.contains("token") || label_key.contains("secret") || label_key.contains("password")
}

fn capitalize(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
