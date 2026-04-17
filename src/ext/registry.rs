//! Unified registry: built-in recipes + discovered WASM extensions.

use std::collections::BTreeMap;

use crate::ext::describe::{BundleRecipeContribution, Descriptor, Execution};
use crate::ext::errors::ExtensionError;
use crate::ext::loader::DiscoveredExtension;

/// Strongly-typed identifier for built-in recipes. Phase A has one: Standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinRecipeId {
    Standard,
}

impl BuiltinRecipeId {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "standard" => Some(Self::Standard),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecipeEntry {
    pub extension_id: String,
    pub extension_version: String,
    pub recipe: BundleRecipeContribution,
    pub execution: Execution,
    pub descriptor_root: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ExtensionRegistry {
    /// Keyed by `"{extension_id}/{recipe_id}"`.
    entries: BTreeMap<String, RecipeEntry>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_discovered(
        &mut self,
        discovered: Vec<DiscoveredExtension>,
    ) -> Result<(), ExtensionError> {
        for d in discovered {
            self.add_descriptor(d.descriptor, d.root)?;
        }
        Ok(())
    }

    pub fn add_descriptor(
        &mut self,
        descriptor: Descriptor,
        root: std::path::PathBuf,
    ) -> Result<(), ExtensionError> {
        let ext_id = descriptor.metadata.id.clone();
        let ext_ver = descriptor.metadata.version.clone();
        for recipe in descriptor.contributions.recipes {
            let key = format!("{ext_id}/{}", recipe.id);
            if self.entries.contains_key(&key) {
                return Err(ExtensionError::Conflict(key));
            }
            self.entries.insert(
                key,
                RecipeEntry {
                    extension_id: ext_id.clone(),
                    extension_version: ext_ver.clone(),
                    recipe,
                    execution: descriptor.execution.clone(),
                    descriptor_root: root.clone(),
                },
            );
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        extension_id: &str,
        recipe_id: &str,
    ) -> Result<&RecipeEntry, ExtensionError> {
        let key = format!("{extension_id}/{recipe_id}");
        self.entries
            .get(&key)
            .ok_or_else(|| ExtensionError::RecipeNotFound {
                ext: extension_id.into(),
                recipe: recipe_id.into(),
            })
    }

    pub fn list(&self) -> impl Iterator<Item = &RecipeEntry> {
        self.entries.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::describe::Descriptor;
    use std::path::PathBuf;

    fn d(id: &str, recipe_id: &str) -> (Descriptor, PathBuf) {
        let raw = format!(
            r#"{{
              "apiVersion": "greentic.ai/v1",
              "kind": "BundleExtension",
              "metadata": {{ "id": "{id}", "name": "x", "version": "0.1.0" }},
              "runtime": {{ "component": "extension.wasm" }},
              "execution": {{ "kind": "builtin", "builtinId": "standard" }},
              "contributions": {{
                "recipes": [
                  {{ "id": "{recipe_id}", "displayName": "x", "description": "x",
                     "configSchema": "s.json" }}
                ]
              }}
            }}"#
        );
        (Descriptor::from_json(&raw).unwrap(), PathBuf::from("/tmp"))
    }

    #[test]
    fn resolve_unknown_returns_error() {
        let r = ExtensionRegistry::new();
        let err = r.resolve("x", "y").unwrap_err();
        assert!(matches!(err, ExtensionError::RecipeNotFound { .. }));
    }

    #[test]
    fn register_and_resolve() {
        let mut r = ExtensionRegistry::new();
        let (desc, root) = d("greentic.bundle-standard", "standard");
        r.add_descriptor(desc, root).unwrap();
        let entry = r.resolve("greentic.bundle-standard", "standard").unwrap();
        assert_eq!(entry.recipe.id, "standard");
    }

    #[test]
    fn conflict_same_ext_same_recipe() {
        let mut r = ExtensionRegistry::new();
        let (desc1, root1) = d("greentic.bundle-standard", "standard");
        let (desc2, root2) = d("greentic.bundle-standard", "standard");
        r.add_descriptor(desc1, root1).unwrap();
        let err = r.add_descriptor(desc2, root2).unwrap_err();
        assert!(matches!(err, ExtensionError::Conflict(_)));
    }

    #[test]
    fn no_conflict_different_ext_same_recipe_id() {
        let mut r = ExtensionRegistry::new();
        let (desc1, root1) = d("greentic.bundle-a", "standard");
        let (desc2, root2) = d("greentic.bundle-b", "standard");
        r.add_descriptor(desc1, root1).unwrap();
        r.add_descriptor(desc2, root2).unwrap();
        assert_eq!(r.list().count(), 2);
    }

    #[test]
    fn builtin_recipe_id_round_trip() {
        assert_eq!(BuiltinRecipeId::from_str("standard"), Some(BuiltinRecipeId::Standard));
        assert_eq!(BuiltinRecipeId::Standard.as_str(), "standard");
        assert_eq!(BuiltinRecipeId::from_str("unknown"), None);
    }
}
