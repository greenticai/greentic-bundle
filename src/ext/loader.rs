//! Filesystem discovery of installed extensions.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ext::describe::Descriptor;
use crate::ext::errors::ExtensionError;

#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    pub root: PathBuf,
    pub descriptor: Descriptor,
}

impl DiscoveredExtension {
    /// Absolute path to `extension.wasm` (may or may not be loaded in Phase A).
    pub fn wasm_path(&self) -> PathBuf {
        self.root.join(&self.descriptor.runtime.component)
    }

    /// Absolute path to a config schema file referenced by a recipe.
    pub fn schema_path(&self, recipe_id: &str) -> Option<PathBuf> {
        self.descriptor
            .contributions
            .recipes
            .iter()
            .find(|r| r.id == recipe_id)
            .map(|r| self.root.join(&r.config_schema))
    }
}

pub fn load_from_dir(dir: &Path) -> Result<Vec<DiscoveredExtension>, ExtensionError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let root = entry.path();
        let describe_path = root.join("describe.json");
        if !describe_path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&describe_path)?;
        let descriptor = Descriptor::from_json(&raw)?;
        out.push(DiscoveredExtension { root, descriptor });
    }
    out.sort_by(|a, b| a.descriptor.metadata.id.cmp(&b.descriptor.metadata.id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_fixture(root: &Path, id: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let exec = r#"{ "kind": "wasm" }"#;
        let describe = format!(
            r#"{{
              "apiVersion": "greentic.ai/v1",
              "kind": "BundleExtension",
              "metadata": {{ "id": "{id}", "name": "x", "version": "0.1.0" }},
              "runtime": {{ "component": "extension.wasm" }},
              "execution": {exec},
              "contributions": {{
                "recipes": [
                  {{ "id": "standard", "displayName": "x", "description": "x",
                     "configSchema": "schemas/standard.config.schema.json" }}
                ]
              }}
            }}"#,
        );
        fs::write(dir.join("describe.json"), describe).unwrap();
        fs::write(dir.join("extension.wasm"), b"\0asm\x01\0\0\0").unwrap();
    }

    #[test]
    fn empty_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let out = load_from_dir(tmp.path()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn missing_dir_returns_empty() {
        let out = load_from_dir(Path::new("/definitely/does/not/exist")).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn loads_multiple_sorted_by_id() {
        let tmp = TempDir::new().unwrap();
        write_fixture(tmp.path(), "greentic.bundle-beta");
        write_fixture(tmp.path(), "greentic.bundle-alpha");
        let out = load_from_dir(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].descriptor.metadata.id, "greentic.bundle-alpha");
        assert_eq!(out[1].descriptor.metadata.id, "greentic.bundle-beta");
    }

    #[test]
    fn skips_child_dirs_without_describe() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("junk")).unwrap();
        write_fixture(tmp.path(), "greentic.bundle-ok");
        let out = load_from_dir(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn propagates_invalid_descriptor() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("describe.json"), "{not json").unwrap();
        let err = load_from_dir(tmp.path()).unwrap_err();
        assert!(matches!(err, ExtensionError::Json(_)));
    }
}
