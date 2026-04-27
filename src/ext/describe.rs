//! Parse and validate `describe.json` for bundle extensions.

use serde::Deserialize;

use crate::ext::errors::ExtensionError;

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Execution {
    Wasm,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BundleRecipeContribution {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    pub icon_path: Option<String>,
    pub config_schema: String,
    #[serde(default)]
    pub supported_capabilities: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Contributions {
    #[serde(default)]
    pub recipes: Vec<BundleRecipeContribution>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub component: String,
    #[serde(default)]
    pub memory_limit_mb: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub runtime: Runtime,
    pub execution: Execution,
    #[serde(default)]
    pub contributions: Contributions,
}

impl Descriptor {
    pub fn from_json(raw: &str) -> Result<Self, ExtensionError> {
        let v: Self = serde_json::from_str(raw)?;
        if v.kind != "BundleExtension" {
            return Err(ExtensionError::InvalidDescriptor(format!(
                "kind must be 'BundleExtension', got '{}'",
                v.kind
            )));
        }
        if v.contributions.recipes.is_empty() {
            return Err(ExtensionError::InvalidDescriptor(
                "at least one recipe contribution required".into(),
            ));
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
      "apiVersion": "greentic.ai/v1",
      "kind": "BundleExtension",
      "metadata": {
        "id": "greentic.bundle-standard",
        "name": "Standard Bundle Recipe",
        "version": "0.2.0"
      },
      "runtime": { "component": "extension.wasm" },
      "execution": { "kind": "wasm" },
      "contributions": {
        "recipes": [
          {
            "id": "standard",
            "displayName": "Standard",
            "description": "Package designer session",
            "configSchema": "schemas/standard.config.schema.json"
          }
        ]
      }
    }"#;

    #[test]
    fn parse_valid_wasm() {
        let d = Descriptor::from_json(VALID).unwrap();
        assert_eq!(d.metadata.id, "greentic.bundle-standard");
        assert!(matches!(d.execution, Execution::Wasm));
        assert_eq!(d.contributions.recipes.len(), 1);
    }

    #[test]
    fn reject_wrong_kind() {
        let raw = VALID.replace(r#""BundleExtension""#, r#""DesignExtension""#);
        let err = Descriptor::from_json(&raw).unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidDescriptor(_)));
    }

    #[test]
    fn reject_empty_recipes() {
        let raw = VALID.replace(
            r#""recipes": [
          {
            "id": "standard",
            "displayName": "Standard",
            "description": "Package designer session",
            "configSchema": "schemas/standard.config.schema.json"
          }
        ]"#,
            r#""recipes": []"#,
        );
        let err = Descriptor::from_json(&raw).unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidDescriptor(_)));
    }

    #[test]
    fn reject_malformed_json() {
        let err = Descriptor::from_json("{not json").unwrap_err();
        assert!(matches!(err, ExtensionError::Json(_)));
    }

    #[test]
    fn reject_unknown_execution_kind() {
        let raw = VALID.replace(r#""kind": "wasm""#, r#""kind": "sandboxed""#);
        let err = Descriptor::from_json(&raw).unwrap_err();
        assert!(matches!(err, ExtensionError::Json(_)));
    }

    #[test]
    fn reject_legacy_builtin_kind() {
        // Wave 5: builtin dispatch removed entirely. Any describe.json still
        // declaring execution.kind="builtin" must fail to parse cleanly.
        let raw = VALID.replace(
            r#""execution": { "kind": "wasm" }"#,
            r#""execution": { "kind": "builtin", "builtinId": "standard" }"#,
        );
        let err = Descriptor::from_json(&raw).unwrap_err();
        assert!(matches!(err, ExtensionError::Json(_)));
    }
}
