use std::collections::BTreeMap;

use anyhow::{Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerDocument {
    pub wizard_id: String,
    pub schema_id: String,
    pub schema_version: Version,
    pub locale: String,
    /// Environment id the wizard ran under (C7). `None` for documents
    /// constructed via [`AnswerDocument::new`] (no env binding yet) or read
    /// from pre-C7 artifacts; set to `Some(env)` once the wizard's
    /// `execute_request` materializes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_id: Option<String>,
    #[serde(default)]
    pub answers: BTreeMap<String, Value>,
    #[serde(default)]
    pub locks: BTreeMap<String, Value>,
}

impl AnswerDocument {
    pub fn new(locale: &str) -> Self {
        Self {
            wizard_id: crate::wizard::WIZARD_ID.to_string(),
            schema_id: crate::wizard::ANSWER_SCHEMA_ID.to_string(),
            schema_version: Version::new(1, 0, 0),
            locale: crate::i18n::normalize_locale(locale).unwrap_or_else(|| "en".to_string()),
            env_id: None,
            answers: BTreeMap::new(),
            locks: BTreeMap::new(),
        }
    }

    pub fn from_json_str(raw: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(raw)?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        if self.wizard_id.trim().is_empty() {
            bail!("{}", crate::i18n::tr("errors.answer_document.wizard_id"));
        }
        if self.schema_id.trim().is_empty() {
            bail!("{}", crate::i18n::tr("errors.answer_document.schema_id"));
        }
        if self.locale.trim().is_empty() {
            bail!("{}", crate::i18n::tr("errors.answer_document.locale"));
        }
        Ok(())
    }

    pub fn to_pretty_json_string(&self) -> Result<String> {
        self.validate()?;
        let mut rendered = serde_json::to_string_pretty(self)?;
        rendered.push('\n');
        Ok(rendered)
    }
}
