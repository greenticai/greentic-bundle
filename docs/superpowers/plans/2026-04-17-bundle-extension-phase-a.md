# Bundle Extension Phase A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Phase A of the bundle-extension migration — a feature-gated `src/ext/` module in `greentic-bundle` plus a new sibling repo `greentic-bundle-extensions` with the `bundle-standard` reference extension, enabling designer UI to produce `.gtpack` bytes via a uniform extension dispatch without altering any existing CLI subcommand behavior.

**Architecture:** Option C hybrid. `greentic-bundle/src/ext/` (feature-gated `extensions`, default-off) discovers + dispatches WASM extensions declared with `execution.kind="builtin"`, routing `render` calls to a built-in `Standard` recipe handler that reuses the existing build pipeline. A new sibling repo `greentic-bundle-extensions` ships `bundle-standard` as the first reference extension. Mode B (full-WASM) declared but dispatches return `ExtensionError::ModeBNotImplemented`. Mirrors the deploy-ext migration pattern (`spec/wasm-deploy-extensions`).

**Tech Stack:** Rust 1.94.0, edition 2024, clap v4, serde + serde_json, sha2, tempfile, anyhow + thiserror, jsonschema v0.18, wit-bindgen, cargo-component, `greentic-ext-runtime` + `greentic-ext-contract` (git-dep from `greenticai/greentic-designer-extensions` pinned rev).

**Spec:** `docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md` (commit `4dc5a11`).

---

## PR #1 — `greentic-bundle` (branch `feat/ext-phase-a` from `origin/main`)

### Task 1: Bootstrap feature scaffold

**Goal:** Add the `extensions` feature gate and empty `src/ext/` module; confirm both builds (with and without the feature) compile clean.

**Files:**
- Modify: `Cargo.toml`
- Create: `src/ext/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create the feature branch**

```bash
git checkout origin/main
git checkout -b feat/ext-phase-a
git branch --unset-upstream
```

- [ ] **Step 2: Add optional deps and `[features]` section to `Cargo.toml`**

Locate the `[dependencies]` section and append (replace `REV_PLACEHOLDER` with the current HEAD of `greenticai/greentic-designer-extensions` `main` at task-run time; record it in the commit body):

```toml
# feature: extensions (bundle-ext Phase A)
greentic-ext-contract = { git = "ssh://git@github.com/greenticai/greentic-designer-extensions", rev = "REV_PLACEHOLDER", optional = true }
greentic-ext-runtime  = { git = "ssh://git@github.com/greenticai/greentic-designer-extensions", rev = "REV_PLACEHOLDER", optional = true }
jsonschema = { version = "0.18", optional = true, default-features = false }
sha2 = { workspace = true, optional = true }
tempfile = { workspace = true, optional = true }
thiserror = { version = "2", optional = true }
```

Add at the bottom of the file:

```toml
[features]
default = []
extensions = [
  "dep:greentic-ext-contract",
  "dep:greentic-ext-runtime",
  "dep:jsonschema",
  "dep:sha2",
  "dep:tempfile",
  "dep:thiserror",
]
```

- [ ] **Step 3: Create the empty `src/ext/mod.rs`**

```rust
//! Bundle extension host module (Phase A).
//!
//! Feature-gated by `extensions`. See `docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md`.

pub mod describe;
pub mod loader;
pub mod registry;
pub mod dispatcher;
pub mod builtin_bridge;
pub mod wasm;
pub mod errors;

pub use errors::ExtensionError;
```

- [ ] **Step 4: Add feature-gated `pub mod ext;` to `src/lib.rs`**

Insert after `pub mod cli;`:

```rust
#[cfg(feature = "extensions")]
pub mod ext;
```

- [ ] **Step 5: Create empty sibling files so `mod.rs` resolves**

Create seven empty files (each a one-line doc comment so clippy doesn't complain):

```bash
for f in describe loader registry dispatcher builtin_bridge wasm errors; do
  echo "//! Placeholder — implemented in a later task." > src/ext/$f.rs
done
```

- [ ] **Step 6: Verify both builds compile**

```bash
cargo build --no-default-features 2>&1 | tail -5
cargo build --features extensions 2>&1 | tail -5
```

Expected: both report `Finished` with zero errors. The `--features extensions` build may warn about unused deps — that's fine, they're used in later tasks.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/lib.rs src/ext/
git commit -m "feat(ext): bootstrap feature-gated ext module scaffold

Add extensions feature flag with optional git-dep pins to
greentic-ext-{contract,runtime} plus jsonschema, sha2, tempfile,
thiserror. Scaffold empty src/ext/ module tree; verify both
default-features and --features extensions builds compile."
```

---

### Task 2: `ExtensionError` type (TDD)

**Goal:** Define the error enum that all `ext` submodules use.

**Files:**
- Modify: `src/ext/errors.rs`
- Create: inline `#[cfg(test)]` tests

- [ ] **Step 1: Write failing tests**

Replace `src/ext/errors.rs` content:

```rust
//! Error type for the `ext` module.

use std::io;

#[derive(thiserror::Error, Debug)]
pub enum ExtensionError {
    #[error("extension not found: {0}")]
    NotFound(String),

    #[error("recipe not found: {ext}/{recipe}")]
    RecipeNotFound { ext: String, recipe: String },

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("invalid descriptor: {0}")]
    InvalidDescriptor(String),

    #[error("conflict: recipe id `{0}` offered by multiple extensions")]
    Conflict(String),

    #[error("Mode B (full WASM) not implemented in Phase A")]
    ModeBNotImplemented,

    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_not_found() {
        let e = ExtensionError::NotFound("greentic.bundle-standard".into());
        assert_eq!(e.to_string(), "extension not found: greentic.bundle-standard");
    }

    #[test]
    fn display_recipe_not_found() {
        let e = ExtensionError::RecipeNotFound {
            ext: "greentic.bundle-standard".into(),
            recipe: "unknown".into(),
        };
        assert_eq!(
            e.to_string(),
            "recipe not found: greentic.bundle-standard/unknown",
        );
    }

    #[test]
    fn mode_b_variant_distinct() {
        let e = ExtensionError::ModeBNotImplemented;
        assert_eq!(e.to_string(), "Mode B (full WASM) not implemented in Phase A");
    }

    #[test]
    fn from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let e: ExtensionError = io_err.into();
        assert!(matches!(e, ExtensionError::Io(_)));
    }

    #[test]
    fn from_json_error() {
        let bad: Result<serde_json::Value, _> = serde_json::from_str("{bad json");
        let e: ExtensionError = bad.unwrap_err().into();
        assert!(matches!(e, ExtensionError::Json(_)));
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL (compile error — deps not in scope yet)**

```bash
cargo test --features extensions --lib ext::errors 2>&1 | tail -20
```

Expected: compile error or "test result: ok" depending on serde_json presence. If pass immediately, that's fine — the enum is trivial.

- [ ] **Step 3: Run tests — expect PASS**

```bash
cargo test --features extensions --lib ext::errors
```

Expected: `test result: ok. 5 passed`.

- [ ] **Step 4: Commit**

```bash
git add src/ext/errors.rs
git commit -m "feat(ext): add ExtensionError enum with thiserror"
```

---

### Task 3: `describe.rs` — descriptor parsing (TDD)

**Goal:** Parse `describe.json` into typed structs, including the Phase-A `execution` tagged union.

**Files:**
- Modify: `src/ext/describe.rs`

- [ ] **Step 1: Write failing tests**

Replace `src/ext/describe.rs`:

```rust
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
    Builtin {
        #[serde(rename = "builtinId")]
        builtin_id: String,
    },
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

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Contributions {
    #[serde(default)]
    pub recipes: Vec<BundleRecipeContribution>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
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
        "version": "0.1.0"
      },
      "runtime": { "component": "extension.wasm" },
      "execution": { "kind": "builtin", "builtinId": "standard" },
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
    fn parse_valid_builtin() {
        let d = Descriptor::from_json(VALID).unwrap();
        assert_eq!(d.metadata.id, "greentic.bundle-standard");
        match &d.execution {
            Execution::Builtin { builtin_id } => assert_eq!(builtin_id, "standard"),
            _ => panic!("expected builtin"),
        }
        assert_eq!(d.contributions.recipes.len(), 1);
    }

    #[test]
    fn parse_wasm_execution() {
        let raw = VALID.replace(
            r#"{ "kind": "builtin", "builtinId": "standard" }"#,
            r#"{ "kind": "wasm" }"#,
        );
        let d = Descriptor::from_json(&raw).unwrap();
        assert!(matches!(d.execution, Execution::Wasm));
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
        let raw = VALID.replace(r#""kind": "builtin""#, r#""kind": "sandboxed""#);
        let err = Descriptor::from_json(&raw).unwrap_err();
        assert!(matches!(err, ExtensionError::Json(_)));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --features extensions --lib ext::describe
```

Expected: `test result: ok. 6 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/describe.rs
git commit -m "feat(ext): parse describe.json with Execution tagged union"
```

---

### Task 4: `loader.rs` — filesystem discovery (TDD)

**Goal:** Scan an install directory and load each child directory as a `DiscoveredExtension`.

**Files:**
- Modify: `src/ext/loader.rs`

- [ ] **Step 1: Write the loader with tests**

Replace `src/ext/loader.rs`:

```rust
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

    fn write_fixture(root: &Path, id: &str, builtin: bool) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let exec = if builtin {
            r#"{ "kind": "builtin", "builtinId": "standard" }"#
        } else {
            r#"{ "kind": "wasm" }"#
        };
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
        write_fixture(tmp.path(), "greentic.bundle-beta", true);
        write_fixture(tmp.path(), "greentic.bundle-alpha", false);
        let out = load_from_dir(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].descriptor.metadata.id, "greentic.bundle-alpha");
        assert_eq!(out[1].descriptor.metadata.id, "greentic.bundle-beta");
    }

    #[test]
    fn skips_child_dirs_without_describe() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("junk")).unwrap();
        write_fixture(tmp.path(), "greentic.bundle-ok", true);
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test --features extensions --lib ext::loader
```

Expected: `test result: ok. 5 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/loader.rs
git commit -m "feat(ext): filesystem discovery of installed extensions"
```

---

### Task 5: `registry.rs` — unify + conflict detection (TDD)

**Goal:** Register discovered extensions + built-in recipes into one registry with conflict detection and lookup by recipe id.

**Files:**
- Modify: `src/ext/registry.rs`

- [ ] **Step 1: Implement with tests**

Replace `src/ext/registry.rs`:

```rust
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

#[derive(Debug, Clone)]
pub struct ExtensionRegistry {
    /// Keyed by `"{extension_id}/{recipe_id}"`.
    entries: BTreeMap<String, RecipeEntry>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
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

    fn add_descriptor(
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

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test --features extensions --lib ext::registry
```

Expected: `test result: ok. 5 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/registry.rs
git commit -m "feat(ext): unified extension registry with conflict detection"
```

---

### Task 6: `wasm.rs` — Mode B stub

**Goal:** Declare the Mode B seam; all calls return `ModeBNotImplemented` in Phase A.

**Files:**
- Modify: `src/ext/wasm.rs`

- [ ] **Step 1: Implement**

Replace `src/ext/wasm.rs`:

```rust
//! Mode B (full-WASM) execution seam. Stubbed in Phase A; wired to
//! greentic-ext-runtime in Phase B.

use crate::ext::errors::ExtensionError;

pub struct WasmInvocation<'a> {
    pub extension_id: &'a str,
    pub recipe_id: &'a str,
    pub config_json: &'a str,
    pub session_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub fn invoke_wasm(_call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
    Err(ExtensionError::ModeBNotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_mode_b_not_implemented() {
        let err = invoke_wasm(WasmInvocation {
            extension_id: "x",
            recipe_id: "y",
            config_json: "{}",
            session_json: "{}",
        })
        .unwrap_err();
        assert!(matches!(err, ExtensionError::ModeBNotImplemented));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --features extensions --lib ext::wasm
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/wasm.rs
git commit -m "feat(ext): Mode B execution stub returning ModeBNotImplemented"
```

---

### Task 7: `builtin_bridge.rs` — Standard recipe handler (TDD, largest task)

**Goal:** Given a `DesignerSession` + validated config, produce a `.gtpack` ZIP via the existing build pipeline.

**Files:**
- Modify: `src/ext/builtin_bridge.rs`

Context for the implementer: the existing `crate::build` module already knows how to produce a `.gtpack` ZIP from a normalized workspace. This bridge:
1. Deserializes `DesignerSession` and `StandardConfig`
2. Computes a deterministic `session-id` from the inputs
3. Writes an ephemeral `BundleWorkspaceDefinition` into a tmpdir
4. Invokes the existing `crate::build::assemble` pipeline
5. Reads the resulting ZIP into memory and computes sha256

If any `crate::build` API details differ at implementation time, adapt call sites. The test below pins the contract of this bridge only.

- [ ] **Step 1: Write the bridge with tests**

Replace `src/ext/builtin_bridge.rs`:

```rust
//! Builtin bridge: `BuiltinRecipeId::Standard` → existing build pipeline.

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::RenderedArtifact;

#[derive(Debug, Deserialize)]
pub struct DesignerSession {
    pub flows_json: String,
    pub contents_json: String,
    #[serde(default)]
    pub assets: Vec<(String, Vec<u8>)>,
    #[serde(default)]
    pub capabilities_used: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StandardConfig {
    pub metadata: StandardMetadata,
    pub channels: Vec<String>,
    #[serde(default = "default_embed_ui")]
    pub embed_ui: String,
    #[serde(default)]
    pub i18n: I18nConfig,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct StandardMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_source")]
    pub source: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_embed_ui() -> String { "none".into() }
fn default_format() -> String { "gtpack-legacy".into() }
fn default_i18n_source() -> String { "en".into() }

pub fn handle_standard(
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    let config: StandardConfig = serde_json::from_str(config_json)?;
    let session: DesignerSession = serde_json::from_str(session_json)?;

    if config.format != "gtpack-legacy" {
        return Err(ExtensionError::InvalidConfig(format!(
            "format '{}' not supported in Phase A (only 'gtpack-legacy')",
            config.format,
        )));
    }

    let session_id = compute_session_id(&session, config_json);
    let tmp_root = tempfile::Builder::new()
        .prefix(&format!("ext-render-{session_id}-"))
        .tempdir()?;

    write_ephemeral_workspace(tmp_root.path(), &session, &config)?;

    // Delegate to existing build pipeline.
    // NOTE: adapt the call to match the current `crate::build::assemble` signature
    // at implementation time. Expected shape:
    //   let bytes = crate::build::assemble_to_bytes(tmp_root.path())?;
    let bytes = crate::ext::builtin_bridge::invoke_existing_build(tmp_root.path())?;

    let sha256 = hex_sha256(&bytes);
    let filename = format!("{}-{}.gtpack", config.metadata.name, config.metadata.version);
    Ok(RenderedArtifact { filename, bytes, sha256 })
}

/// Deterministic 16-hex-char session id.
fn compute_session_id(session: &DesignerSession, config_json: &str) -> String {
    let mut h = Sha256::new();
    h.update(session.flows_json.as_bytes());
    h.update(b"\x00");
    h.update(session.contents_json.as_bytes());
    h.update(b"\x00");
    let mut assets = session.assets.clone();
    assets.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in &assets {
        h.update(k.as_bytes());
        h.update(b"\x00");
        h.update(v);
        h.update(b"\x00");
    }
    h.update(config_json.as_bytes());
    let out = h.finalize();
    hex_encode(&out[..8])
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_encode(&h.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn write_ephemeral_workspace(
    root: &Path,
    session: &DesignerSession,
    config: &StandardConfig,
) -> Result<(), ExtensionError> {
    use std::fs;
    fs::create_dir_all(root.join("flows"))?;
    fs::create_dir_all(root.join("assets").join("cards"))?;
    fs::create_dir_all(root.join("tenants").join("default").join("teams"))?;

    // flows — session.flows_json is a JSON array of { "name": ..., "yaml": ... } entries.
    let flows: Vec<serde_json::Value> = serde_json::from_str(&session.flows_json)?;
    for (i, f) in flows.iter().enumerate() {
        let name = f
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("flow-{i:03}"));
        let yaml = f
            .get("yaml")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ExtensionError::InvalidConfig(format!("flow '{name}' missing 'yaml'"))
            })?;
        fs::write(root.join("flows").join(format!("{name}.ygtc")), yaml)?;
    }

    // contents — similar shape: array of { "id": ..., "json": ... }.
    let contents: Vec<serde_json::Value> = serde_json::from_str(&session.contents_json)?;
    for c in contents.iter() {
        let id = c
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExtensionError::InvalidConfig("content missing 'id'".into()))?;
        let json = c.get("json").ok_or_else(|| {
            ExtensionError::InvalidConfig(format!("content '{id}' missing 'json'"))
        })?;
        fs::write(
            root.join("assets").join("cards").join(format!("{id}.json")),
            serde_json::to_vec_pretty(json)?,
        )?;
    }

    // raw assets.
    for (rel, bytes) in &session.assets {
        let dst = root.join("assets").join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dst, bytes)?;
    }

    // synthesize bundle.yaml.
    let bundle_yaml = format!(
        "apiVersion: greentic.ai/v1\nkind: BundleWorkspace\nmetadata:\n  name: {}\n  version: {}\nchannels:\n{}",
        config.metadata.name,
        config.metadata.version,
        config
            .channels
            .iter()
            .map(|c| format!("  - {c}\n"))
            .collect::<String>(),
    );
    fs::write(root.join("bundle.yaml"), bundle_yaml)?;

    // synthesize tenant.gmap.
    let tenant_gmap = format!(
        "# generated by ext bridge\ntenant: default\ncapabilities:\n{}",
        session
            .capabilities_used
            .iter()
            .map(|c| format!("  - {c}\n"))
            .collect::<String>(),
    );
    fs::write(
        root.join("tenants").join("default").join("tenant.gmap"),
        tenant_gmap,
    )?;

    Ok(())
}

/// Thin adapter over the current build pipeline. Implementer: adjust to match the
/// actual `crate::build::assemble` API at the time of execution.
pub fn invoke_existing_build(workspace_root: &Path) -> Result<Vec<u8>, ExtensionError> {
    use std::fs;
    use std::io::Write;

    // Walk workspace_root and produce a ZIP in memory. This mirrors the
    // `.gtpack` ZIP step the existing build pipeline uses for pack export.
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for entry in walkdir::WalkDir::new(workspace_root) {
            let entry = entry.map_err(|e| {
                ExtensionError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;
            let path = entry.path();
            let rel = path.strip_prefix(workspace_root).unwrap_or(path);
            if rel.as_os_str().is_empty() {
                continue;
            }
            if entry.file_type().is_dir() {
                zip.add_directory(rel.to_string_lossy(), options)
                    .map_err(zip_io)?;
                continue;
            }
            if entry.file_type().is_file() {
                zip.start_file(rel.to_string_lossy(), options)
                    .map_err(zip_io)?;
                let bytes = fs::read(path)?;
                zip.write_all(&bytes)?;
            }
        }
        zip.finish().map_err(zip_io)?;
    }
    Ok(buf)
}

fn zip_io(e: zip::result::ZipError) -> ExtensionError {
    ExtensionError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_CONFIG: &str = r#"{
      "metadata": { "name": "demo", "version": "0.1.0" },
      "channels": ["webchat"],
      "format": "gtpack-legacy"
    }"#;

    const MIN_SESSION: &str = r#"{
      "flows_json": "[{\"name\":\"main\",\"yaml\":\"schemaVersion: 2\\nname: main\"}]",
      "contents_json": "[{\"id\":\"welcome\",\"json\":{\"type\":\"AdaptiveCard\",\"version\":\"1.5\"}}]",
      "assets": [],
      "capabilities_used": ["greentic:adaptive-cards/schema"]
    }"#;

    #[test]
    fn session_id_deterministic() {
        let a = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        let b = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn session_id_differs_on_different_inputs() {
        let a = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        let other_cfg = MIN_CONFIG.replace("demo", "other");
        let b = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            &other_cfg,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_unsupported_format() {
        let bad_cfg = MIN_CONFIG.replace("gtpack-legacy", "apack");
        let err = handle_standard(&bad_cfg, MIN_SESSION).unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidConfig(_)));
    }

    #[test]
    fn happy_path_produces_artifact() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        assert_eq!(out.filename, "demo-0.1.0.gtpack");
        assert!(!out.bytes.is_empty());
        assert_eq!(out.sha256.len(), 64);
        // Reproducible: same inputs → same sha256.
        let again = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        assert_eq!(out.sha256, again.sha256);
    }

    #[test]
    fn artifact_is_a_valid_zip_containing_bundle_yaml() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("bundle.yaml")));
        assert!(names.iter().any(|n| n.ends_with("flows/main.ygtc")));
        assert!(names.iter().any(|n| n.ends_with("assets/cards/welcome.json")));
    }
}
```

- [ ] **Step 2: Add `zip` and `walkdir` to optional extensions deps in `Cargo.toml`**

Append to the `[dependencies]` section:

```toml
zip = { version = "5", optional = true, default-features = false, features = ["deflate"] }
walkdir = { version = "2", optional = true }
```

Add to `[features] extensions`:

```toml
"dep:zip",
"dep:walkdir",
```

- [ ] **Step 3: Run tests**

```bash
cargo test --features extensions --lib ext::builtin_bridge
```

Expected: `test result: ok. 5 passed`. If the existing build pipeline signature differs, adapt `invoke_existing_build` in place until tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/ext/builtin_bridge.rs
git commit -m "feat(ext): builtin bridge for Standard recipe

Synthesize an ephemeral workspace from DesignerSession + StandardConfig,
invoke the existing build pipeline, and return a bundle-artifact with a
deterministic sha256 and a 16-hex session-id for traceability. Phase A
only supports format='gtpack-legacy'."
```

---

### Task 8: `dispatcher.rs` — route by execution kind (TDD)

**Goal:** Unify `invoke_recipe`: given an ext+recipe+config+session, route to `builtin_bridge::handle_standard` or `wasm::invoke_wasm` based on `execution.kind`.

**Files:**
- Modify: `src/ext/dispatcher.rs`

- [ ] **Step 1: Implement with tests**

Replace `src/ext/dispatcher.rs`:

```rust
//! Route `render` calls to the correct execution backend based on
//! `describe.json` `execution.kind`.

use crate::ext::builtin_bridge;
use crate::ext::describe::Execution;
use crate::ext::errors::ExtensionError;
use crate::ext::registry::{BuiltinRecipeId, ExtensionRegistry};
use crate::ext::wasm;
use crate::ext::wasm::RenderedArtifact;

pub fn invoke_recipe(
    registry: &ExtensionRegistry,
    extension_id: &str,
    recipe_id: &str,
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    let entry = registry.resolve(extension_id, recipe_id)?;
    match &entry.execution {
        Execution::Builtin { builtin_id } => {
            let id = BuiltinRecipeId::from_str(builtin_id).ok_or_else(|| {
                ExtensionError::InvalidDescriptor(format!(
                    "unknown builtinId '{builtin_id}'"
                ))
            })?;
            match id {
                BuiltinRecipeId::Standard => {
                    builtin_bridge::handle_standard(config_json, session_json)
                }
            }
        }
        Execution::Wasm => wasm::invoke_wasm(wasm::WasmInvocation {
            extension_id,
            recipe_id,
            config_json,
            session_json,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::describe::Descriptor;
    use std::path::PathBuf;

    fn register(reg: &mut ExtensionRegistry, kind_json: &str) {
        let raw = format!(
            r#"{{
              "apiVersion": "greentic.ai/v1",
              "kind": "BundleExtension",
              "metadata": {{ "id": "x.test", "name": "t", "version": "0.0.1" }},
              "runtime": {{ "component": "extension.wasm" }},
              "execution": {kind_json},
              "contributions": {{
                "recipes": [
                  {{ "id": "standard", "displayName": "x", "description": "x",
                     "configSchema": "s.json" }}
                ]
              }}
            }}"#
        );
        let d = Descriptor::from_json(&raw).unwrap();
        // add_descriptor is pub(crate) / pub in registry tests — expose via register_discovered.
        let discovered = crate::ext::loader::DiscoveredExtension {
            root: PathBuf::from("/tmp"),
            descriptor: d,
        };
        reg.register_discovered(vec![discovered]).unwrap();
    }

    #[test]
    fn wasm_path_returns_mode_b_error() {
        let mut reg = ExtensionRegistry::new();
        register(&mut reg, r#"{ "kind": "wasm" }"#);
        let err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::ModeBNotImplemented));
    }

    #[test]
    fn unknown_builtin_id_errors() {
        let mut reg = ExtensionRegistry::new();
        register(
            &mut reg,
            r#"{ "kind": "builtin", "builtinId": "mystery" }"#,
        );
        let err = invoke_recipe(&reg, "x.test", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidDescriptor(_)));
    }

    #[test]
    fn unknown_extension_errors() {
        let reg = ExtensionRegistry::new();
        let err = invoke_recipe(&reg, "x.missing", "standard", "{}", "{}").unwrap_err();
        assert!(matches!(err, ExtensionError::RecipeNotFound { .. }));
    }

    #[test]
    fn builtin_standard_dispatches_to_bridge() {
        let mut reg = ExtensionRegistry::new();
        register(
            &mut reg,
            r#"{ "kind": "builtin", "builtinId": "standard" }"#,
        );
        // Minimal valid config + session.
        let config = r#"{
          "metadata": { "name": "demo", "version": "0.1.0" },
          "channels": ["webchat"],
          "format": "gtpack-legacy"
        }"#;
        let session = r#"{
          "flows_json": "[{\"name\":\"main\",\"yaml\":\"n: m\"}]",
          "contents_json": "[]",
          "assets": [],
          "capabilities_used": []
        }"#;
        let out = invoke_recipe(&reg, "x.test", "standard", config, session).unwrap();
        assert!(out.filename.ends_with(".gtpack"));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --features extensions --lib ext::dispatcher
```

Expected: `test result: ok. 4 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/dispatcher.rs
git commit -m "feat(ext): dispatcher routes by execution.kind"
```

---

### Task 9: `src/cli/ext.rs` — CLI argument definitions

**Goal:** Define the `Ext` subcommand clap surface with `list`, `info`, `validate`, `render`, `install-dir` operations.

**Files:**
- Create: `src/cli/ext.rs`

- [ ] **Step 1: Create the module**

Write `src/cli/ext.rs`:

```rust
//! `greentic-bundle ext …` subcommand (feature-gated).

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ExtArgs {
    /// Override the install directory (defaults to `state/ext/`).
    #[arg(long = "extension-dir", value_name = "DIR", global = true)]
    pub extension_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: ExtCommand,
}

#[derive(Debug, Subcommand)]
pub enum ExtCommand {
    /// List all discovered extensions and their recipes.
    #[command(about = "cli.ext.list.about")]
    List,

    /// Print metadata for one extension.
    #[command(about = "cli.ext.info.about")]
    Info {
        /// Extension id (e.g. `greentic.bundle-standard`).
        extension_id: String,
    },

    /// Validate a config JSON against a recipe's schema.
    #[command(about = "cli.ext.validate.about")]
    Validate {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },

    /// Render a bundle artifact via the ext dispatcher (Mode A only in Phase A).
    #[command(about = "cli.ext.render.about")]
    Render {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        /// Path to a designer session JSON file.
        #[arg(long, value_name = "FILE")]
        session: PathBuf,
        /// Output file (default: stdout).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },

    /// Print the resolved install directory.
    #[command(about = "cli.ext.install_dir.about")]
    InstallDir,
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build --features extensions 2>&1 | tail -5
```

Expected: `Finished`.

- [ ] **Step 3: Commit**

```bash
git add src/cli/ext.rs
git commit -m "feat(cli): add Ext subcommand argument definitions"
```

---

### Task 10: Wire `Ext` into `cli/mod.rs` + implement run dispatcher

**Goal:** Add the `Ext(ExtArgs)` variant to the `Commands` enum (feature-gated) and wire the dispatch in `run()`.

**Files:**
- Modify: `src/cli/mod.rs`

- [ ] **Step 1: Register the module**

Add to the `pub mod …` list at the top of `src/cli/mod.rs`:

```rust
#[cfg(feature = "extensions")]
pub mod ext;
```

- [ ] **Step 2: Add the `Ext` variant to the `Commands` enum**

Insert in the `enum Commands` right before `Init(init::InitArgs),`:

```rust
    #[cfg(feature = "extensions")]
    #[command(about = "cli.ext.about")]
    Ext(ext::ExtArgs),
```

- [ ] **Step 3: Add the dispatch arm in `run()`**

Locate the `match cli.command` in the `run()` function. Add after the `Init` arm:

```rust
        #[cfg(feature = "extensions")]
        Commands::Ext(args) => run_ext(args),
```

Also add, just below the `run()` function, the helper:

```rust
#[cfg(feature = "extensions")]
fn run_ext(args: ext::ExtArgs) -> Result<()> {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::ext::dispatcher::invoke_recipe;
    use crate::ext::loader::load_from_dir;
    use crate::ext::registry::ExtensionRegistry;

    let install_dir = args
        .extension_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("state").join("ext"));

    let mut registry = ExtensionRegistry::new();
    let discovered = load_from_dir(&install_dir)?;
    registry.register_discovered(discovered)?;

    match args.command {
        ext::ExtCommand::List => {
            for e in registry.list() {
                println!(
                    "{ext} {ver} recipe={recipe} kind={kind:?}",
                    ext = e.extension_id,
                    ver = e.extension_version,
                    recipe = e.recipe.id,
                    kind = e.execution,
                );
            }
        }
        ext::ExtCommand::Info { extension_id } => {
            let mut any = false;
            for e in registry.list().filter(|e| e.extension_id == extension_id) {
                any = true;
                println!(
                    "{ext} {ver}\n  recipe: {rid} — {display}\n  schema: {schema}\n  capabilities: {caps}",
                    ext = e.extension_id,
                    ver = e.extension_version,
                    rid = e.recipe.id,
                    display = e.recipe.display_name,
                    schema = e.recipe.config_schema,
                    caps = e.recipe.supported_capabilities.join(", "),
                );
            }
            if !any {
                return Err(anyhow::anyhow!(
                    crate::i18n::tf("cli.ext.info.not_found", &[("id", &extension_id)])
                ));
            }
        }
        ext::ExtCommand::Validate {
            extension_id,
            recipe_id,
            config,
        } => {
            let entry = registry.resolve(&extension_id, &recipe_id)?;
            let schema_path = entry.descriptor_root.join(&entry.recipe.config_schema);
            let schema_raw = fs::read_to_string(&schema_path)?;
            let schema_json: serde_json::Value = serde_json::from_str(&schema_raw)?;
            let config_raw = fs::read_to_string(&config)?;
            let config_json: serde_json::Value = serde_json::from_str(&config_raw)?;
            let validator = jsonschema::validator_for(&schema_json)
                .map_err(|e| anyhow::anyhow!("schema load error: {e}"))?;
            let errors: Vec<String> = validator
                .iter_errors(&config_json)
                .map(|e| format!("{}: {}", e.instance_path, e))
                .collect();
            if errors.is_empty() {
                println!("{}", crate::i18n::t("cli.ext.validate.ok"));
            } else {
                for e in &errors {
                    eprintln!("{e}");
                }
                return Err(anyhow::anyhow!(
                    crate::i18n::t("cli.ext.validate.failed")
                ));
            }
        }
        ext::ExtCommand::Render {
            extension_id,
            recipe_id,
            config,
            session,
            out,
        } => {
            let config_json = fs::read_to_string(&config)?;
            let session_json = fs::read_to_string(&session)?;
            let art = invoke_recipe(
                &registry,
                &extension_id,
                &recipe_id,
                &config_json,
                &session_json,
            )?;
            match out {
                Some(path) => {
                    fs::write(&path, &art.bytes)?;
                    println!(
                        "{}",
                        crate::i18n::tf(
                            "cli.ext.render.wrote",
                            &[("file", &path.display().to_string()), ("sha256", &art.sha256)],
                        )
                    );
                }
                None => {
                    std::io::stdout().write_all(&art.bytes)?;
                }
            }
        }
        ext::ExtCommand::InstallDir => {
            println!("{}", install_dir.display());
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Verify compile**

```bash
cargo build --features extensions 2>&1 | tail -5
cargo build --no-default-features 2>&1 | tail -5
```

Expected: both `Finished`.

- [ ] **Step 5: Verify feature gate works**

```bash
cargo run --no-default-features -- ext list 2>&1 | tail -5
```

Expected: clap error about unknown subcommand `ext` (because the variant is compiled out). Not a panic.

- [ ] **Step 6: Commit**

```bash
git add src/cli/mod.rs
git commit -m "feat(cli): wire Ext subcommand with feature-gated dispatch"
```

---

### Task 11: i18n keys for `ext.*` namespace

**Goal:** Add all new user-facing strings to the i18n catalog in `en.json` plus stubs in 5 priority locales (id, ja, zh, es, de). All other locales pick up English fallback until `greentic-i18n-translator` is re-run.

**Files:**
- Modify: `i18n/en.json`
- Modify: `i18n/id.json`, `i18n/ja.json`, `i18n/zh.json`, `i18n/es.json`, `i18n/de.json`

- [ ] **Step 1: Add keys to `i18n/en.json`**

Insert before the closing `}` (keep alphabetical ordering with existing keys):

```json
  "cli.ext.about": "Bundle extensions (feature-gated)",
  "cli.ext.list.about": "List installed bundle extensions and their recipes",
  "cli.ext.info.about": "Print metadata for one installed bundle extension",
  "cli.ext.info.not_found": "Extension '{id}' not found in install directory",
  "cli.ext.validate.about": "Validate a config JSON against a recipe's config schema",
  "cli.ext.validate.ok": "Config is valid",
  "cli.ext.validate.failed": "Config validation failed",
  "cli.ext.render.about": "Render a bundle artifact via the ext dispatcher",
  "cli.ext.render.wrote": "Wrote {file} (sha256={sha256})",
  "cli.ext.install_dir.about": "Print the resolved extension install directory",
  "cli.ext.feature_disabled": "The 'extensions' feature is not enabled in this build. Rebuild with `cargo build --features extensions`.",
```

- [ ] **Step 2: Add the same keys to 5 priority locales**

Run this helper script in repo root to copy-and-stub. Each file gets the same English values as placeholders so `greentic-i18n-translator` can pick them up later.

```bash
python3 - <<'PY'
import json, pathlib
src = json.loads(pathlib.Path("i18n/en.json").read_text())
keys = [k for k in src if k.startswith("cli.ext.")]
for locale in ["id","ja","zh","es","de"]:
    p = pathlib.Path(f"i18n/{locale}.json")
    data = json.loads(p.read_text())
    changed = False
    for k in keys:
        if k not in data:
            data[k] = src[k]
            changed = True
    if changed:
        p.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
        print(f"updated {p}")
PY
```

- [ ] **Step 3: Run the i18n check**

```bash
python3 ci/i18n_check.py validate 2>&1 | tail -20
```

Expected: no errors for the new keys. If the tool reports a missing key in any locale file, re-run step 2 for that locale.

- [ ] **Step 4: Commit**

```bash
git add i18n/
git commit -m "i18n: add cli.ext.* keys for extensions subcommand

Adds keys to en.json (source) and stubs the same strings in id, ja,
zh, es, de so greentic-i18n-translator can translate them on the next
translation pass. Other locales pick up English fallback."
```

---

### Task 12: Fixture extension artifacts

**Goal:** Create a minimal fixture extension under `testdata/ext/` so integration tests exercise the end-to-end load→dispatch path without requiring the sibling repo to be built.

**Files:**
- Create: `testdata/ext/fixture-bundle/describe.json`
- Create: `testdata/ext/fixture-bundle/schemas/standard.config.schema.json`
- Create: `testdata/ext/fixture-bundle/extension.wasm` (a minimal WASM component bytes fixture — 8 bytes is enough since Mode A never loads it)
- Create: `testdata/ext/README.md`

- [ ] **Step 1: Create the fixture directory and artifacts**

```bash
mkdir -p testdata/ext/fixture-bundle/schemas
```

Write `testdata/ext/fixture-bundle/describe.json`:

```json
{
  "apiVersion": "greentic.ai/v1",
  "kind": "BundleExtension",
  "metadata": {
    "id": "greentic.bundle-fixture",
    "name": "Fixture Bundle Extension",
    "version": "0.0.1"
  },
  "runtime": { "component": "extension.wasm", "memoryLimitMB": 64 },
  "execution": { "kind": "builtin", "builtinId": "standard" },
  "contributions": {
    "recipes": [
      {
        "id": "standard",
        "displayName": "Standard",
        "description": "Fixture — delegates to builtin",
        "configSchema": "schemas/standard.config.schema.json",
        "supportedCapabilities": ["greentic:flows/*"]
      }
    ]
  }
}
```

Write `testdata/ext/fixture-bundle/schemas/standard.config.schema.json` (same as the schema in §4 of the spec, trimmed for fixture):

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["metadata", "channels"],
  "additionalProperties": false,
  "properties": {
    "metadata": {
      "type": "object",
      "required": ["name", "version"],
      "properties": {
        "name":    { "type": "string" },
        "version": { "type": "string" }
      }
    },
    "channels": {
      "type": "array",
      "items": { "enum": ["slack","teams","webchat","telegram","whatsapp","webex","email"] },
      "uniqueItems": true,
      "minItems": 1
    },
    "embed_ui": { "enum": ["none","webchat"], "default": "none" },
    "i18n": {
      "type": "object",
      "properties": {
        "source":  { "type": "string", "default": "en" },
        "targets": { "type": "array", "items": { "type": "string" }, "default": [] }
      }
    },
    "format": { "enum": ["gtpack-legacy"], "default": "gtpack-legacy" }
  }
}
```

Create a tiny stub WASM (Mode A never loads it, so 8-byte magic header is enough):

```bash
printf '\0asm\x01\0\0\0' > testdata/ext/fixture-bundle/extension.wasm
```

Write `testdata/ext/README.md`:

```markdown
# Extension test fixtures

`fixture-bundle/` is the minimal bundle extension used by integration tests in
`tests/ext_*.rs`. It declares `execution.kind="builtin"`, so the runtime never
instantiates the WASM binary — the 8-byte stub here is a placeholder magic
header only.

Regenerate with:

    printf '\0asm\x01\0\0\0' > fixture-bundle/extension.wasm

Do not delete or rename — tests reference these paths by string.
```

- [ ] **Step 2: Commit**

```bash
git add testdata/ext/
git commit -m "test(ext): fixture extension for integration tests

Minimal fixture declaring execution.kind=builtin so the runtime never
instantiates the 8-byte placeholder WASM. Used by the ext_* integration
tests added in a subsequent task."
```

---

### Task 13: Integration tests (`tests/ext_*.rs`)

**Goal:** Four end-to-end tests that invoke the `greentic-bundle` binary via `assert_cmd` with `--features extensions`.

**Files:**
- Create: `tests/ext_list_smoke.rs`
- Create: `tests/ext_info_smoke.rs`
- Create: `tests/ext_validate_smoke.rs`
- Create: `tests/ext_render_builtin.rs`
- Create: `tests/data/designer-session.json`
- Create: `tests/data/config-minimal.json`

- [ ] **Step 1: Create the shared test data**

Write `tests/data/config-minimal.json`:

```json
{
  "metadata": { "name": "smoke-demo", "version": "0.1.0" },
  "channels": ["webchat"],
  "format": "gtpack-legacy"
}
```

Write `tests/data/designer-session.json`:

```json
{
  "flows_json": "[{\"name\":\"main\",\"yaml\":\"schemaVersion: 2\\nname: main\"}]",
  "contents_json": "[{\"id\":\"welcome\",\"json\":{\"type\":\"AdaptiveCard\",\"version\":\"1.5\"}}]",
  "assets": [],
  "capabilities_used": ["greentic:adaptive-cards/schema"]
}
```

- [ ] **Step 2: Write `tests/ext_list_smoke.rs`**

```rust
#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_list_finds_fixture() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("greentic.bundle-fixture"))
        .stdout(predicate::str::contains("recipe=standard"))
        .stdout(predicate::str::contains("Builtin"));
}

#[test]
fn ext_list_empty_dir_prints_nothing() {
    let tmp = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            tmp.path().to_str().unwrap(),
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
```

- [ ] **Step 3: Write `tests/ext_info_smoke.rs`**

```rust
#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_info_prints_metadata() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "info",
            "greentic.bundle-fixture",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("greentic.bundle-fixture 0.0.1"))
        .stdout(predicate::str::contains("recipe: standard"))
        .stdout(predicate::str::contains("greentic:flows/*"));
}

#[test]
fn ext_info_missing_returns_error() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "info",
            "greentic.bundle-missing",
        ])
        .assert()
        .failure();
}
```

- [ ] **Step 4: Write `tests/ext_validate_smoke.rs`**

```rust
#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_validate_ok() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "validate",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config is valid"));
}

#[test]
fn ext_validate_rejects_invalid_config() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        r#"{ "metadata": { "name": "x", "version": "0.1.0" }, "channels": [] }"#,
    )
    .unwrap();
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "validate",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}
```

- [ ] **Step 5: Write `tests/ext_render_builtin.rs`**

```rust
#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_render_produces_valid_gtpack() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("smoke-demo-0.1.0.gtpack");

    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
            "--session",
            "tests/data/designer-session.json",
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sha256="));

    let bytes = std::fs::read(&out).unwrap();
    assert!(!bytes.is_empty());
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n.ends_with("bundle.yaml")));
    assert!(names.iter().any(|n| n.ends_with("flows/main.ygtc")));
    assert!(names.iter().any(|n| n.ends_with("assets/cards/welcome.json")));
}

#[test]
fn ext_render_reproducible() {
    let tmp = tempfile::TempDir::new().unwrap();
    let a = tmp.path().join("a.gtpack");
    let b = tmp.path().join("b.gtpack");

    for out in [&a, &b] {
        Command::cargo_bin("greentic-bundle")
            .unwrap()
            .args([
                "ext",
                "--extension-dir",
                "testdata/ext",
                "render",
                "greentic.bundle-fixture",
                "standard",
                "--config",
                "tests/data/config-minimal.json",
                "--session",
                "tests/data/designer-session.json",
                "--out",
                out.to_str().unwrap(),
            ])
            .assert()
            .success();
    }

    // sha256 must match.
    use sha2::{Digest, Sha256};
    let hash = |p: &std::path::Path| -> String {
        let mut h = Sha256::new();
        h.update(std::fs::read(p).unwrap());
        format!("{:x}", h.finalize())
    };
    assert_eq!(hash(&a), hash(&b));
}
```

- [ ] **Step 6: Add test-only deps if needed**

Append to `[dev-dependencies]` in `Cargo.toml`:

```toml
zip = { version = "5", default-features = false, features = ["deflate"] }
```

(If already present from Task 7's optional dep, skip. Dev-deps don't need `optional`.)

- [ ] **Step 7: Run the integration tests**

```bash
cargo test --features extensions --test ext_list_smoke --test ext_info_smoke --test ext_validate_smoke --test ext_render_builtin
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add tests/
git commit -m "test(ext): end-to-end integration tests for ext subcommand

Covers list/info/validate/render against the fixture extension.
Render test verifies pack structure and deterministic sha256."
```

---

### Task 14: CI verification

**Goal:** Make sure `ci/local_check.sh` exercises both builds (default and `--features extensions`) and binary size regression guard.

**Files:**
- Modify: `ci/local_check.sh`

- [ ] **Step 1: Inspect current script**

```bash
cat ci/local_check.sh | head -60
```

- [ ] **Step 2: Add extensions feature to the test and clippy phases**

Find the `cargo test` line and add directly after it:

```bash
cargo test --features extensions --all-targets
```

Find the `cargo clippy` line and add directly after:

```bash
cargo clippy --features extensions --all-targets --all-features -- -D warnings
```

Add a binary-size regression guard near the end of the script (before the final success echo):

```bash
# Phase A compile-regression guard: extensions-off binary must be within 500 KB
# of main-branch baseline. Fails loudly if we accidentally wire extensions
# into the default build.
if [ -f /tmp/greentic-bundle-baseline.size ]; then
  cargo build --release --no-default-features
  NEW=$(stat -c '%s' target/release/greentic-bundle)
  BASE=$(cat /tmp/greentic-bundle-baseline.size)
  DELTA=$(( (NEW - BASE) / 1024 ))
  echo "binary size delta: ${DELTA} KB (baseline ${BASE}, new ${NEW})"
  if [ "$DELTA" -gt 500 ]; then
    echo "ERROR: binary grew by ${DELTA} KB (>500 KB budget)" >&2
    exit 1
  fi
fi
```

Note to implementer: the baseline file is provisioned by a one-time call on `main`. In CI workflows this will be replaced by a cache step. For local runs, the guard is skipped if the file is absent.

- [ ] **Step 3: Run the full check**

```bash
bash ci/local_check.sh 2>&1 | tail -30
```

Expected: exits 0.

- [ ] **Step 4: Commit**

```bash
git add ci/local_check.sh
git commit -m "ci: exercise --features extensions in test and clippy phases

Adds a second pass of cargo test and cargo clippy with the extensions
feature enabled, plus a soft binary-size regression guard that fails
the build if the no-default-features binary grows by more than 500 KB
vs the baseline recorded on main."
```

---

### Task 15: PR #1 handoff

**Goal:** Verify all acceptance criteria and prepare the PR body.

- [ ] **Step 1: Run the full local check one final time**

```bash
bash ci/local_check.sh
```

Expected: green.

- [ ] **Step 2: Confirm feature-off behavior is unchanged**

```bash
cargo run --no-default-features -- --help 2>&1 | grep -c ext
```

Expected output: `0` (zero occurrences — the `ext` subcommand does not appear when the feature is disabled).

- [ ] **Step 3: Confirm feature-on behavior adds the subcommand**

```bash
cargo run --features extensions -- --help 2>&1 | grep -c "ext "
```

Expected output: `1` or more.

- [ ] **Step 4: Push branch and open PR**

```bash
git push -u origin feat/ext-phase-a
gh pr create \
  --title "feat(ext): Phase A — feature-gated bundle extension host" \
  --body "$(cat <<'EOF'
## Summary
- Adds a feature-gated `src/ext/` module with descriptor parsing, filesystem discovery, unified registry, builtin bridge, and dispatcher
- Adds `greentic-bundle ext {list,info,validate,render,install-dir}` subcommand (feature-gated)
- Adds `BuiltinRecipeId::Standard` + handler that synthesizes an ephemeral workspace from a `DesignerSession` and routes through the existing build pipeline
- Adds fixture extension under `testdata/ext/` and four integration tests (`tests/ext_*.rs`)
- Adds i18n keys for the `cli.ext.*` namespace

## Non-goals
- No rewrite of existing `build/`, `wizard/`, `project/`, `catalog/`, `access/` code
- Mode B (full WASM) returns `ExtensionError::ModeBNotImplemented` — implemented in Phase B
- No bundle-core pure-Rust extraction — deferred to Mode B
- No change to `greentic-cards2pack`

## Acceptance
- `cargo test` (default features): existing behavior unchanged
- `cargo test --features extensions`: new tests green
- Binary size delta with feature off: < 500 KB vs main
- `cargo run --no-default-features -- ext …` returns a clap-level unknown-subcommand error (feature gate honored)

## Test plan
- [x] Unit tests cover describe, loader, registry, dispatcher, builtin_bridge, wasm, errors
- [x] Integration tests cover ext list/info/validate/render against fixture
- [x] i18n validation passes for new `cli.ext.*` keys
- [x] `bash ci/local_check.sh` green

Spec: `docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md`.
EOF
)"
```

- [ ] **Step 5: Record the merged commit SHA in the memory doc**

After PR #1 merges, update `~/.claude/projects/-home-bimbim-works-greentic/memory/bundle-extension-migration.md` with the commit SHA and the `greentic-ext-runtime` git-dep rev actually used.

---

## PR #2 — `greentic-bundle-extensions` (new repo)

### Task 16: Scaffold the new repo

**Goal:** Create the sibling repo with workspace, toolchain, CI skeleton, and license.

**Files:**
- Create: new directory `/home/bimbim/works/greentic/greentic-bundle-extensions/` with all scaffolding

- [ ] **Step 1: Create the directory structure**

```bash
cd /home/bimbim/works/greentic
mkdir -p greentic-bundle-extensions/{ci,wit,reference-extensions}
cd greentic-bundle-extensions
git init -b main
```

- [ ] **Step 2: Create `rust-toolchain.toml`**

```toml
# Canonical toolchain for Greentic host repos.
# Source: greenticai/.github/toolchain/host/rust-toolchain.toml
[toolchain]
channel = "1.94.0"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 3: Create `rustfmt.toml`**

```toml
edition = "2024"
max_width = 100
```

- [ ] **Step 4: Create `.gitignore`**

```
/target
/**/bindings.rs
*.wasm
!testdata/**/*.wasm
!reference-extensions/**/extension.wasm
/**/*.gtxpack.tmp
```

- [ ] **Step 5: Create root `Cargo.toml`**

```toml
[workspace]
members = [
  "reference-extensions/bundle-standard",
]
resolver = "2"

[workspace.package]
edition = "2024"
license = "MIT"
rust-version = "1.94"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
wit-bindgen = "0.36"
wit-bindgen-rt = "0.36"
```

- [ ] **Step 6: Create `README.md`**

```markdown
# greentic-bundle-extensions

Reference bundle extensions for Greentic Designer. Each extension packages a
designer session into an Application Pack (`.gtpack` today, `.apack` when
Module 8 freezes).

## Reference extensions

- **bundle-standard** — `.gtpack` ZIP output with configurable channels, i18n,
  and optional embedded WebChat UI.

## Building

```bash
cd reference-extensions/bundle-standard
./build.sh
```

Produces `greentic.bundle-standard-<version>.gtxpack` next to `build.sh`.

## Installing for use with `greentic-bundle`

```bash
greentic-bundle ext install ./greentic.bundle-standard-0.1.0.gtxpack
```

(Install command extracts the archive into `state/ext/greentic.bundle-standard/`.)
```

- [ ] **Step 7: Create `CLAUDE.md`**

```markdown
# CLAUDE.md

This file guides Claude Code when working in this repository.

## What this is

Reference bundle extensions for Greentic Designer. Each package implements
the WIT world `greentic:extension-bundle@0.1.0` (vendored in `wit/`) and
declares `execution.kind="builtin"` in `describe.json` so the host
dispatches `render` to a built-in handler in `greentic-bundle`.

## Conventions

- **Rust 1.94.0**, edition 2024
- **WASM target:** `wasm32-wasip2`
- Max 500 lines per source file
- English only in source, tests, comments, commits
- No Claude co-authorship on commits
- Feature branches + PRs — never push to `main` directly

## Build one extension

```bash
cd reference-extensions/<name>
./build.sh
```

## Add a new reference extension

1. Copy `reference-extensions/bundle-standard/` to `reference-extensions/<new-name>/`
2. Update `Cargo.toml`, `describe.json`, `schemas/`, `src/lib.rs`
3. Add the member to the root `Cargo.toml` workspace list
4. Run `./build.sh` from the new directory

## Spec

Related spec: `greentic-bundle/docs/superpowers/specs/2026-04-17-bundle-extension-migration-design.md`.
```

- [ ] **Step 8: Create `LICENSE` (MIT)**

Copy from `greentic-bundle/LICENSE` to keep terms consistent.

- [ ] **Step 9: Create `ci/local_check.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Build the first reference extension.
(cd reference-extensions/bundle-standard && ./build.sh)
```

Mark executable: `chmod +x ci/local_check.sh`.

- [ ] **Step 10: Commit the scaffold**

```bash
git add .
git commit -m "chore: scaffold greentic-bundle-extensions workspace

Initial repo layout: workspace Cargo.toml, toolchain, rustfmt, gitignore,
README, CLAUDE.md, MIT LICENSE, local_check.sh. No extensions yet."
```

---

### Task 17: Vendor WIT files

**Goal:** Copy the frozen WIT interfaces from `greentic-designer-extensions` with a header recording the pinned commit.

**Files:**
- Create: `wit/extension-base.wit`
- Create: `wit/extension-host.wit`
- Create: `wit/extension-bundle.wit`

- [ ] **Step 1: Record the pin commit**

```bash
(cd ../greentic-designer-extensions && git rev-parse HEAD)
```

Record the SHA (call it `REV`).

- [ ] **Step 2: Copy each WIT file and prepend a header**

```bash
for f in extension-base.wit extension-host.wit extension-bundle.wit; do
  {
    echo "// Vendored from greenticai/greentic-designer-extensions"
    echo "// Pinned commit: <REPLACE_WITH_REV>"
    echo "// Do not edit locally — run ./tools/sync-wit.sh to refresh."
    echo
    cat ../greentic-designer-extensions/wit/$f
  } > wit/$f
done
```

(Replace `<REPLACE_WITH_REV>` with the actual SHA from Step 1 before committing.)

- [ ] **Step 3: Commit**

```bash
git add wit/
git commit -m "chore(wit): vendor extension-base, extension-host, extension-bundle

Vendored from greenticai/greentic-designer-extensions at commit <REV>.
The three interfaces form the frozen 0.1.0 contract that bundle
extensions implement. Refresh via a tools/sync-wit.sh helper (added
later) whenever the contract version bumps."
```

---

### Task 18: `bundle-standard` package skeleton

**Goal:** Stand up the reference extension crate with `cargo-component` metadata.

**Files:**
- Create: `reference-extensions/bundle-standard/Cargo.toml`
- Create: `reference-extensions/bundle-standard/src/lib.rs` (initial stub)

- [ ] **Step 1: Create `reference-extensions/bundle-standard/Cargo.toml`**

```toml
[package]
name = "greentic-ext-bundle-standard"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
publish = false
description = "Standard bundle extension — packages a designer session into a .gtpack ZIP."

[lib]
crate-type = ["cdylib", "rlib"]
path = "src/lib.rs"

[dependencies]
wit-bindgen = { workspace = true }
wit-bindgen-rt = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[package.metadata.component]
package = "greentic:bundle-standard"

[package.metadata.component.target]
path = "wit"
world = "bundle-extension"

[package.metadata.component.target.dependencies]
"greentic:extension-base"   = { path = "../../wit/extension-base.wit" }
"greentic:extension-host"   = { path = "../../wit/extension-host.wit" }
"greentic:extension-bundle" = { path = "../../wit/extension-bundle.wit" }
```

- [ ] **Step 2: Create `reference-extensions/bundle-standard/wit/` as a symlink or per-crate copy**

`cargo-component` expects WIT under the crate's own `wit/` directory. Symlink to the workspace `wit/`:

```bash
cd reference-extensions/bundle-standard
ln -s ../../wit wit
cd ../..
```

If symlinks aren't desirable (Windows support), copy instead:

```bash
cp -r ../../wit reference-extensions/bundle-standard/wit
```

- [ ] **Step 3: Create initial `src/lib.rs` stub**

```rust
//! greentic.bundle-standard — standard bundle recipe.
//!
//! This extension declares `execution.kind="builtin"` in describe.json, so
//! the host NEVER calls the WASM `render` function. This file only exists
//! to supply manifest/lifecycle/recipes metadata and `validate-config`.

#![allow(clippy::used_underscore_items)]

#[allow(warnings)]
mod bindings;

use bindings::exports::greentic::extension_base::{lifecycle, manifest};
use bindings::exports::greentic::extension_bundle::{bundling, recipes};
use bindings::greentic::extension_base::types;

const SCHEMA: &str = include_str!("../schemas/standard.config.schema.json");

struct Component;

impl manifest::Guest for Component {
    fn get_identity() -> types::ExtensionIdentity {
        types::ExtensionIdentity {
            id: "greentic.bundle-standard".into(),
            version: "0.1.0".into(),
            kind: types::Kind::Bundle,
        }
    }
    fn get_offered() -> Vec<types::CapabilityRef> {
        vec![types::CapabilityRef {
            id: "greentic:bundle/standard".into(),
            version: "0.1.0".into(),
        }]
    }
    fn get_required() -> Vec<types::CapabilityRef> {
        Vec::new()
    }
}

impl lifecycle::Guest for Component {
    fn init(_config_json: String) -> Result<(), types::ExtensionError> {
        Ok(())
    }
    fn shutdown() {}
}

impl recipes::Guest for Component {
    fn list_recipes() -> Vec<recipes::RecipeSummary> {
        vec![recipes::RecipeSummary {
            id: "standard".into(),
            display_name: "Standard Greentic Pack".into(),
            description: "Package designer session into a .gtpack archive".into(),
            icon_path: None,
        }]
    }
    fn recipe_config_schema(recipe_id: String) -> Result<String, types::ExtensionError> {
        match recipe_id.as_str() {
            "standard" => Ok(SCHEMA.into()),
            other => Err(types::ExtensionError::InvalidInput(format!(
                "unknown recipe: {other}"
            ))),
        }
    }
    fn supported_capabilities(recipe_id: String) -> Result<Vec<String>, types::ExtensionError> {
        match recipe_id.as_str() {
            "standard" => Ok(vec![
                "greentic:adaptive-cards/*".into(),
                "greentic:flows/*".into(),
                "greentic:channels/*".into(),
            ]),
            other => Err(types::ExtensionError::InvalidInput(format!(
                "unknown recipe: {other}"
            ))),
        }
    }
}

impl bundling::Guest for Component {
    fn validate_config(_recipe_id: String, _config_json: String) -> Vec<types::Diagnostic> {
        // Phase A: the host does JSON-schema validation via jsonschema crate
        // before invoking bundling::render. The WASM side returns an empty
        // diagnostic list. A second-pass validator can be added later.
        Vec::new()
    }

    fn render(
        _recipe_id: String,
        _config_json: String,
        _session: bundling::DesignerSession,
    ) -> Result<bundling::BundleArtifact, types::ExtensionError> {
        // Host never calls this in Mode A (execution.kind="builtin" routes
        // render to the native builtin bridge). If invoked, signal clearly.
        Err(types::ExtensionError::Internal(
            "bundle-standard runs as execution.kind=builtin; this render export is unreachable".into(),
        ))
    }
}

bindings::export!(Component with_types_in bindings);
```

- [ ] **Step 4: Register the package in the workspace**

Already done in Task 16 Step 5. Verify:

```bash
grep bundle-standard /home/bimbim/works/greentic/greentic-bundle-extensions/Cargo.toml
```

- [ ] **Step 5: Verify the WIT path resolves (build will fail on missing schema file — that's fine, added next task)**

```bash
cd /home/bimbim/works/greentic/greentic-bundle-extensions
cargo check --workspace 2>&1 | tail -10
```

Expected: failure specifically on the missing `schemas/standard.config.schema.json` include. Any other error means the WIT path is wrong — fix the symlink.

- [ ] **Step 6: Commit**

```bash
git add reference-extensions/bundle-standard/
git commit -m "feat(bundle-standard): package skeleton with WIT bindings

Minimal implementation of manifest, lifecycle, recipes, and bundling
WIT exports. The render export returns an error because bundle-standard
declares execution.kind=builtin and the host routes render elsewhere."
```

---

### Task 19: `bundle-standard` describe.json + schemas + i18n + examples

**Goal:** All the artifacts that accompany the WASM component inside the `.gtxpack` archive.

**Files:**
- Create: `reference-extensions/bundle-standard/describe.json`
- Create: `reference-extensions/bundle-standard/schemas/standard.config.schema.json`
- Create: `reference-extensions/bundle-standard/i18n/en.json`
- Create: `reference-extensions/bundle-standard/i18n/id.json`
- Create: `reference-extensions/bundle-standard/examples/minimal.json`

- [ ] **Step 1: Write `describe.json`**

Identical to the shape shown in §5 of the spec. Paste verbatim:

```json
{
  "apiVersion": "greentic.ai/v1",
  "kind": "BundleExtension",
  "metadata": {
    "id": "greentic.bundle-standard",
    "name": "Standard Bundle Recipe",
    "version": "0.1.0",
    "summary": "Package designer session into a Greentic pack (.gtpack ZIP)",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": {
    "greenticDesigner": ">=0.6.0",
    "extRuntime": "^0.1.0"
  },
  "capabilities": {
    "offered": [{ "id": "greentic:bundle/standard", "version": "0.1.0" }],
    "required": []
  },
  "runtime": {
    "component": "extension.wasm",
    "memoryLimitMB": 128,
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "execution": {
    "kind": "builtin",
    "builtinId": "standard"
  },
  "contributions": {
    "recipes": [
      {
        "id": "standard",
        "displayName": "Standard Greentic Pack",
        "description": "Package designer session into a .gtpack archive",
        "configSchema": "schemas/standard.config.schema.json",
        "supportedCapabilities": [
          "greentic:adaptive-cards/*",
          "greentic:flows/*",
          "greentic:channels/*"
        ]
      }
    ]
  }
}
```

- [ ] **Step 2: Write `schemas/standard.config.schema.json`**

Identical to §4 of the spec. Paste verbatim:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["metadata", "channels"],
  "additionalProperties": false,
  "properties": {
    "metadata": {
      "type": "object",
      "required": ["name", "version"],
      "properties": {
        "name":    { "type": "string", "pattern": "^[a-z][a-z0-9-]{1,62}$" },
        "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" },
        "author":  { "type": "string" }
      }
    },
    "channels": {
      "type": "array",
      "items": { "enum": ["slack","teams","webchat","telegram","whatsapp","webex","email"] },
      "uniqueItems": true,
      "minItems": 1
    },
    "embed_ui": { "enum": ["none","webchat"], "default": "none" },
    "i18n": {
      "type": "object",
      "properties": {
        "source":  { "type": "string", "default": "en" },
        "targets": { "type": "array", "items": { "type": "string" }, "default": [] }
      }
    },
    "format": { "enum": ["gtpack-legacy"], "default": "gtpack-legacy" }
  }
}
```

- [ ] **Step 3: Write `i18n/en.json`**

```json
{
  "bundle-standard.recipe.standard.display": "Standard Greentic Pack",
  "bundle-standard.recipe.standard.description": "Package a designer session into a .gtpack archive",
  "bundle-standard.error.unknown_recipe": "Unknown recipe: {recipe}",
  "bundle-standard.error.invalid_format": "Format '{format}' is not supported (only 'gtpack-legacy' is available in Phase A)"
}
```

- [ ] **Step 4: Write `i18n/id.json`**

```json
{
  "bundle-standard.recipe.standard.display": "Paket Greentic Standar",
  "bundle-standard.recipe.standard.description": "Kemas sesi designer menjadi arsip .gtpack",
  "bundle-standard.error.unknown_recipe": "Recipe tidak dikenal: {recipe}",
  "bundle-standard.error.invalid_format": "Format '{format}' tidak didukung (hanya 'gtpack-legacy' yang tersedia di Phase A)"
}
```

- [ ] **Step 5: Write `examples/minimal.json`**

```json
{
  "metadata": { "name": "demo-bundle", "version": "0.1.0" },
  "channels": ["webchat"],
  "format": "gtpack-legacy"
}
```

- [ ] **Step 6: Re-run `cargo check` now that the schema include resolves**

```bash
cargo check --workspace 2>&1 | tail -5
```

Expected: `Finished`.

- [ ] **Step 7: Commit**

```bash
git add reference-extensions/bundle-standard/
git commit -m "feat(bundle-standard): describe, schemas, i18n, and minimal example

Ships the contribution manifest, config JSON schema, English + Indonesian
message catalogs, and a minimal valid config example. Enables cargo check
to succeed now that include_str! for the schema resolves."
```

---

### Task 20: `bundle-standard` `build.sh`

**Goal:** Wrap `cargo component build` and stage into a `.gtxpack` ZIP archive.

**Files:**
- Create: `reference-extensions/bundle-standard/build.sh`

- [ ] **Step 1: Write the build script**

```bash
#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

echo "==> cargo component build --release"
cargo component build --release

# cargo-component emits into the workspace target dir.
WASM_PATH="../../target/wasm32-wasip1/release/greentic_ext_bundle_standard.wasm"
if [ ! -f "$WASM_PATH" ]; then
  echo "ERROR: wasm not found at $WASM_PATH" >&2
  exit 1
fi

STAGE="$(mktemp -d)"
trap "rm -rf $STAGE" EXIT

cp describe.json "$STAGE/"
cp "$WASM_PATH"  "$STAGE/extension.wasm"
cp -r schemas i18n "$STAGE/"

VERSION="$(python3 -c 'import json;print(json.load(open("describe.json"))["metadata"]["version"])')"
OUT="$HERE/greentic.bundle-standard-${VERSION}.gtxpack"
TMP_ZIP="$STAGE/../greentic_bundle_standard_$$.zip"
(cd "$STAGE" && zip -r "$TMP_ZIP" .) > /dev/null
mv "$TMP_ZIP" "$OUT"

echo "==> built $OUT"
echo "==> size: $(du -h "$OUT" | cut -f1)"
```

- [ ] **Step 2: Mark executable and run**

```bash
chmod +x reference-extensions/bundle-standard/build.sh
(cd reference-extensions/bundle-standard && ./build.sh)
```

Expected: outputs `greentic.bundle-standard-0.1.0.gtxpack` next to `build.sh`.

- [ ] **Step 3: Add the built artifact to `.gitignore` and commit an empty placeholder**

Already handled by `.gitignore` (via `/**/*.gtxpack.tmp` — add `*.gtxpack` if the team does NOT want built artifacts committed, or remove that rule to mirror the AC ext which commits the artifact).

Per the spec §5, the AC ext pattern commits the built artifact. Follow that:

```bash
# Remove any *.gtxpack ignore rule if present, then:
git add reference-extensions/bundle-standard/build.sh reference-extensions/bundle-standard/greentic.bundle-standard-0.1.0.gtxpack
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(bundle-standard): build.sh + first built artifact

build.sh runs cargo component build, stages describe.json + WASM +
schemas + i18n, and zips them into greentic.bundle-standard-0.1.0.gtxpack.
Artifact committed for distribution convenience (mirrors AC ext)."
```

---

### Task 21: `bundle-standard` validation smoke test

**Goal:** Prove the WIT exports load and return expected metadata (in-proc, using `wit-bindgen`'s test harness).

**Files:**
- Create: `reference-extensions/bundle-standard/tests/validate_config_smoke.rs`

- [ ] **Step 1: Write the smoke test**

Because the crate has `crate-type = ["cdylib", "rlib"]`, tests can invoke the `rlib` path. Exported WIT functions are accessible via the generated bindings.

```rust
#![cfg(not(target_family = "wasm"))]

// The rlib side exposes bindings; we exercise the schema content.

const SCHEMA: &str =
    include_str!("../schemas/standard.config.schema.json");
const MINIMAL: &str = include_str!("../examples/minimal.json");

#[test]
fn minimal_example_is_valid_against_schema() {
    let schema_json: serde_json::Value =
        serde_json::from_str(SCHEMA).expect("schema parses");
    let config_json: serde_json::Value =
        serde_json::from_str(MINIMAL).expect("example parses");
    let validator =
        jsonschema::validator_for(&schema_json).expect("schema compiles");
    assert!(validator.is_valid(&config_json));
}

#[test]
fn schema_rejects_missing_channels() {
    let schema_json: serde_json::Value =
        serde_json::from_str(SCHEMA).expect("schema parses");
    let config_json: serde_json::Value = serde_json::from_str(
        r#"{ "metadata": { "name": "x", "version": "0.1.0" } }"#,
    )
    .unwrap();
    let validator =
        jsonschema::validator_for(&schema_json).expect("schema compiles");
    assert!(!validator.is_valid(&config_json));
}
```

- [ ] **Step 2: Add `[dev-dependencies]` to the bundle-standard `Cargo.toml`**

```toml
[dev-dependencies]
jsonschema = { version = "0.18", default-features = false }
serde_json = { workspace = true }
```

- [ ] **Step 3: Run the test**

```bash
cargo test --package greentic-ext-bundle-standard
```

Expected: `test result: ok. 2 passed`.

- [ ] **Step 4: Commit**

```bash
git add reference-extensions/bundle-standard/Cargo.toml reference-extensions/bundle-standard/tests/
git commit -m "test(bundle-standard): validate minimal example against schema

Two tests: the shipped minimal.json must be valid; a config missing
channels must be rejected."
```

---

### Task 22: Round-trip install test against `greentic-bundle`

**Goal:** Prove that the built `.gtxpack` installs into `greentic-bundle`'s install directory and shows up in `ext list`.

**Files:**
- Create: `reference-extensions/bundle-standard/tests/install_roundtrip.sh` (shell test, not a cargo test)

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
set -euo pipefail

BUNDLE_REPO="${BUNDLE_REPO:-$(cd "$(dirname "$0")"/../../../greentic-bundle && pwd)}"
if [ ! -d "$BUNDLE_REPO" ]; then
  echo "ERROR: greentic-bundle repo not found at $BUNDLE_REPO" >&2
  exit 1
fi

ARTIFACT="$(cd "$(dirname "$0")"/.. && pwd)/greentic.bundle-standard-0.1.0.gtxpack"
if [ ! -f "$ARTIFACT" ]; then
  echo "ERROR: artifact not built; run ../build.sh first" >&2
  exit 1
fi

# Extract the gtxpack into a tmp install dir shaped like state/ext/<id>/.
TMP_INSTALL="$(mktemp -d)"
mkdir -p "$TMP_INSTALL/greentic.bundle-standard"
unzip -q "$ARTIFACT" -d "$TMP_INSTALL/greentic.bundle-standard"

(cd "$BUNDLE_REPO" && cargo run --features extensions -- \
  ext --extension-dir "$TMP_INSTALL" list) | tee "$TMP_INSTALL/list.out"

grep -q "greentic.bundle-standard" "$TMP_INSTALL/list.out"
grep -q "recipe=standard" "$TMP_INSTALL/list.out"
echo "==> round-trip install OK"
```

Mark executable: `chmod +x reference-extensions/bundle-standard/tests/install_roundtrip.sh`.

- [ ] **Step 2: Run the round-trip**

```bash
reference-extensions/bundle-standard/tests/install_roundtrip.sh
```

Expected: `==> round-trip install OK`.

- [ ] **Step 3: Commit**

```bash
git add reference-extensions/bundle-standard/tests/install_roundtrip.sh
git commit -m "test(bundle-standard): round-trip install smoke against greentic-bundle"
```

---

### Task 23: PR #2 handoff

**Goal:** Push the new repo, open PR #2 (or initial push + PR against empty main), and record the cross-linking commit SHAs.

- [ ] **Step 1: Create the GitHub repo**

```bash
gh repo create greenticai/greentic-bundle-extensions \
  --public \
  --description "Reference bundle extensions for Greentic Designer"
```

- [ ] **Step 2: Push**

```bash
cd /home/bimbim/works/greentic/greentic-bundle-extensions
git remote add origin git@github.com:greenticai/greentic-bundle-extensions.git
git push -u origin main
```

(If the main branch is protected and requires PR review, instead create a `feat/bundle-standard-0.1.0` branch locally, push it, and open a PR.)

- [ ] **Step 3: Smoke-test locally one more time**

```bash
bash ci/local_check.sh
```

Expected: green.

- [ ] **Step 4: Update memory**

Append to `~/.claude/projects/-home-bimbim-works-greentic/memory/bundle-extension-migration.md` under a "Phase A status" heading:

```markdown
### Phase A status (updated 2026-04-17)

- `greentic-bundle` PR #1 merged at commit <SHA>
- `greentic-bundle-extensions` repo created; `bundle-standard` 0.1.0 shipped
- `greentic-ext-runtime` git-dep pinned at rev <REV>
- All Phase A acceptance criteria pass
- Phase B not started; prerequisites (host::storage, bundle-core pure-Rust
  extraction) deferred pending real demand
```

- [ ] **Step 5: Update the greentic-docs site**

In `../greentic-docs`, add a tutorial stub at `src/content/docs/en/extensions/bundle-extensions.md` linking to the spec + the reference extension. A separate docs PR handles translations — not blocking Phase A merge.

---

## Self-Review

After completing all 23 tasks, the engineer should verify:

1. **Spec coverage:** Every section of the spec has a corresponding task.
   - §1 Context & motivation → Task 1 (bootstrap) + Task 15 (PR handoff describes scope)
   - §2 Architecture → Tasks 1–8 (every file in the architecture diagram is implemented)
   - §3 Module layout → Tasks 2–8 (each file), Task 10 (install dir convention)
   - §4 Recipe standard → Tasks 7 (handler), 10 (CLI wiring), 19 (schema shipped)
   - §5 Reference extension structure → Tasks 16–22
   - §6 WIT contract usage → Task 17 (vendor), Task 18 (implement exports)
   - §7 Testing strategy → Task 2–8 (unit), Task 13 (integration), Task 21 (bundle-standard tests), Task 22 (round-trip)
   - §8 Acceptance criteria → Task 14 (CI), Task 15 (PR #1 acceptance), Task 23 (PR #2 acceptance)
   - §9 Open items → recorded in Task 23 Step 4 memory update
   - §10 Timeline → reflected in the two-PR cadence of PR #1 (Tasks 1–15) then PR #2 (Tasks 16–23)

2. **No placeholders:** Every `TODO`, `FIXME`, or unfilled step should be absent. The only deferred items are Phase B concerns explicitly marked as such.

3. **Type consistency:** `BuiltinRecipeId::Standard`, `DesignerSession`, `StandardConfig`, `RenderedArtifact`, `ExtensionError`, `ExtensionRegistry`, `Descriptor`, `Execution`, `BundleRecipeContribution` — all used consistently across tasks 2, 3, 5, 7, 8, 10.

4. **i18n:** All new user-facing strings go through `crate::i18n::t` / `crate::i18n::tf`. Validator passes.

---

## Execution

Plan complete. Two execution options for the follow-up session:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task with two-stage review (`superpowers:subagent-driven-development`).

**2. Inline Execution** — execute tasks in-session using `superpowers:executing-plans`, batch checkpoints for review.
