use std::fmt;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use backhand::{FilesystemReader, InnerNode};
use serde::{Deserialize, Serialize};

pub const BUNDLE_FORMAT_VERSION: &str = "gtbundle-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: String,
    pub bundle_id: String,
    pub bundle_name: String,
    pub requested_mode: String,
    pub locale: String,
    pub artifact_extension: String,
    #[serde(default)]
    pub generated_resolved_files: Vec<String>,
    #[serde(default)]
    pub generated_setup_files: Vec<String>,
    #[serde(default)]
    pub app_packs: Vec<String>,
    #[serde(default)]
    pub extension_providers: Vec<String>,
    #[serde(default)]
    pub catalogs: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resolved_targets: Vec<BundleResolvedTargetView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleLock {
    pub schema_version: u32,
    pub bundle_id: String,
    pub requested_mode: String,
    pub execution: String,
    pub cache_policy: String,
    pub tool_version: String,
    pub build_format_version: String,
    pub workspace_root: String,
    pub lock_file: String,
    pub catalogs: Vec<CatalogLockEntry>,
    pub app_packs: Vec<DependencyLock>,
    pub extension_providers: Vec<DependencyLock>,
    pub setup_state_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogLockEntry {
    pub requested_ref: String,
    pub resolved_ref: String,
    pub digest: String,
    pub source: String,
    pub item_count: usize,
    pub item_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLock {
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleSourceKind {
    Artifact,
    BuildDir,
}

impl BundleSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::BuildDir => "build_dir",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleRuntimeSurface {
    pub format_version: String,
    pub bundle_id: String,
    pub bundle_name: String,
    pub requested_mode: String,
    pub locale: String,
    pub execution: String,
    pub cache_policy: String,
    pub workspace_root: String,
    pub lock_file: String,
    pub app_packs: Vec<BundleDependencyView>,
    pub extension_providers: Vec<BundleDependencyView>,
    pub catalogs: Vec<BundleCatalogView>,
    pub hooks: Vec<String>,
    pub subscriptions: Vec<String>,
    pub capabilities: Vec<String>,
    pub resolved_targets: Vec<BundleResolvedTargetView>,
    pub generated_resolved_files: Vec<BundleFileView>,
    pub generated_setup_files: Vec<BundleFileView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleDependencyView {
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleCatalogView {
    pub requested_ref: String,
    pub resolved_ref: String,
    pub digest: String,
    pub source: String,
    pub item_count: usize,
    pub item_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFileView {
    pub path: String,
    pub kind: BundleFileKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleResolvedTargetView {
    pub path: String,
    pub tenant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    pub default_policy: String,
    pub tenant_gmap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_gmap: Option<String>,
    #[serde(default)]
    pub app_pack_policies: Vec<BundleResolvedReferencePolicyView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleResolvedReferencePolicyView {
    pub reference: String,
    pub policy: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleFileKind {
    Resolved,
    SetupState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedBundle {
    pub source_kind: BundleSourceKind,
    pub source_path: String,
    pub format_version: String,
    pub manifest: BundleManifest,
    pub lock: BundleLock,
}

impl OpenedBundle {
    pub fn from_parts(
        source_kind: BundleSourceKind,
        source_path: impl Into<String>,
        manifest: BundleManifest,
        lock: BundleLock,
    ) -> Result<Self, BundleReadError> {
        let opened = Self {
            source_kind,
            source_path: source_path.into(),
            format_version: manifest.format_version.clone(),
            manifest,
            lock,
        };
        opened.validate_basic_structure()?;
        Ok(opened)
    }

    pub fn runtime_surface(&self) -> BundleRuntimeSurface {
        BundleRuntimeSurface {
            format_version: self.manifest.format_version.clone(),
            bundle_id: self.manifest.bundle_id.clone(),
            bundle_name: self.manifest.bundle_name.clone(),
            requested_mode: self.manifest.requested_mode.clone(),
            locale: self.manifest.locale.clone(),
            execution: self.lock.execution.clone(),
            cache_policy: self.lock.cache_policy.clone(),
            workspace_root: self.lock.workspace_root.clone(),
            lock_file: self.lock.lock_file.clone(),
            app_packs: self
                .lock
                .app_packs
                .iter()
                .map(|entry| BundleDependencyView {
                    reference: entry.reference.clone(),
                    digest: entry.digest.clone(),
                })
                .collect(),
            extension_providers: self
                .lock
                .extension_providers
                .iter()
                .map(|entry| BundleDependencyView {
                    reference: entry.reference.clone(),
                    digest: entry.digest.clone(),
                })
                .collect(),
            catalogs: self
                .lock
                .catalogs
                .iter()
                .map(|entry| BundleCatalogView {
                    requested_ref: entry.requested_ref.clone(),
                    resolved_ref: entry.resolved_ref.clone(),
                    digest: entry.digest.clone(),
                    source: entry.source.clone(),
                    item_count: entry.item_count,
                    item_ids: entry.item_ids.clone(),
                    cache_path: entry.cache_path.clone(),
                })
                .collect(),
            hooks: self.manifest.hooks.clone(),
            subscriptions: self.manifest.subscriptions.clone(),
            capabilities: self.manifest.capabilities.clone(),
            resolved_targets: self.manifest.resolved_targets.clone(),
            generated_resolved_files: self
                .manifest
                .generated_resolved_files
                .iter()
                .map(|path| BundleFileView {
                    path: path.clone(),
                    kind: BundleFileKind::Resolved,
                })
                .collect(),
            generated_setup_files: self
                .manifest
                .generated_setup_files
                .iter()
                .map(|path| BundleFileView {
                    path: path.clone(),
                    kind: BundleFileKind::SetupState,
                })
                .collect(),
        }
    }

    pub fn validate_basic_structure(&self) -> Result<(), BundleReadError> {
        if self.manifest.format_version != BUNDLE_FORMAT_VERSION {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                format!(
                    "unsupported bundle format version: {}",
                    self.manifest.format_version
                ),
            ));
        }
        if self.manifest.bundle_id.trim().is_empty() {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                "bundle manifest is missing bundle_id".to_string(),
            ));
        }
        if self.lock.bundle_id.trim().is_empty() {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                "bundle lock is missing bundle_id".to_string(),
            ));
        }
        if self.manifest.bundle_id != self.lock.bundle_id {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                "bundle manifest and lock bundle_id do not match".to_string(),
            ));
        }
        if self.manifest.requested_mode != self.lock.requested_mode {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                "bundle manifest and lock requested_mode do not match".to_string(),
            ));
        }
        if self.manifest.artifact_extension != ".gtbundle" {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                format!(
                    "unsupported artifact extension: {}",
                    self.manifest.artifact_extension
                ),
            ));
        }
        if self.lock.workspace_root != "bundle.yaml" {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                format!("unexpected workspace_root: {}", self.lock.workspace_root),
            ));
        }
        if self.lock.lock_file != "bundle.lock.json" {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                format!("unexpected lock_file: {}", self.lock.lock_file),
            ));
        }
        if self.lock.setup_state_files != self.manifest.generated_setup_files {
            return Err(BundleReadError::invalid(
                self.source_kind,
                &self.source_path,
                "bundle manifest and lock setup state files do not match".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleReadError {
    pub kind: BundleReadErrorKind,
    pub source_kind: BundleSourceKind,
    pub source_path: String,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleReadErrorKind {
    Io,
    Invalid,
    Tool,
}

impl BundleReadError {
    fn io(source_kind: BundleSourceKind, source_path: &Path, details: String) -> Self {
        Self {
            kind: BundleReadErrorKind::Io,
            source_kind,
            source_path: source_path.display().to_string(),
            details,
        }
    }

    fn invalid(source_kind: BundleSourceKind, source_path: &str, details: String) -> Self {
        Self {
            kind: BundleReadErrorKind::Invalid,
            source_kind,
            source_path: source_path.to_string(),
            details,
        }
    }

    fn tool(source_kind: BundleSourceKind, source_path: &Path, details: String) -> Self {
        Self {
            kind: BundleReadErrorKind::Tool,
            source_kind,
            source_path: source_path.display().to_string(),
            details,
        }
    }
}

impl fmt::Display for BundleReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} read failed for {} ({}): {}",
            self.source_kind.as_str(),
            self.source_path,
            match self.kind {
                BundleReadErrorKind::Io => "io",
                BundleReadErrorKind::Invalid => "invalid",
                BundleReadErrorKind::Tool => "tool",
            },
            self.details
        )
    }
}

impl std::error::Error for BundleReadError {}

pub fn open_artifact(path: &Path) -> Result<OpenedBundle, BundleReadError> {
    let manifest_raw = read_artifact_file(path, "bundle-manifest.json")?;
    let lock_raw = read_artifact_file(path, "bundle-lock.json")?;
    let manifest = parse_manifest(BundleSourceKind::Artifact, path, &manifest_raw)?;
    let lock = parse_lock(BundleSourceKind::Artifact, path, &lock_raw)?;
    let opened = OpenedBundle::from_parts(
        BundleSourceKind::Artifact,
        path.display().to_string(),
        manifest,
        lock,
    )?;
    validate_artifact_contents(path, &opened)?;
    Ok(opened)
}

pub fn open_build_dir(path: &Path) -> Result<OpenedBundle, BundleReadError> {
    open_build_dir_with_source(path, path.display().to_string())
}

pub fn open_build_dir_with_source(
    path: &Path,
    source_path: impl Into<String>,
) -> Result<OpenedBundle, BundleReadError> {
    let manifest_raw = read_build_file(path, "bundle-manifest.json")?;
    let lock_raw = read_build_file(path, "bundle-lock.json")?;
    let manifest = parse_manifest(BundleSourceKind::BuildDir, path, &manifest_raw)?;
    let lock = parse_lock(BundleSourceKind::BuildDir, path, &lock_raw)?;
    let opened = OpenedBundle::from_parts(BundleSourceKind::BuildDir, source_path, manifest, lock)?;
    validate_build_dir_contents(path, &opened)?;
    Ok(opened)
}

fn read_build_file(root: &Path, name: &str) -> Result<String, BundleReadError> {
    fs::read_to_string(root.join(name)).map_err(|error| {
        BundleReadError::io(
            BundleSourceKind::BuildDir,
            root,
            format!("read {}: {error}", root.join(name).display()),
        )
    })
}

fn read_artifact_file(path: &Path, inner_path: &str) -> Result<String, BundleReadError> {
    let bytes = read_artifact_bytes(path, inner_path)?;
    String::from_utf8(bytes).map_err(|error| {
        BundleReadError::invalid(
            BundleSourceKind::Artifact,
            &path.display().to_string(),
            format!("artifact entry {inner_path} is not valid utf-8: {error}"),
        )
    })
}

fn read_artifact_bytes(path: &Path, inner_path: &str) -> Result<Vec<u8>, BundleReadError> {
    let filesystem = open_artifact_filesystem(path)?;
    let normalized_inner = normalize_artifact_path(inner_path).map_err(|error| {
        BundleReadError::invalid(
            BundleSourceKind::Artifact,
            &path.display().to_string(),
            format!("invalid artifact path {inner_path}: {error}"),
        )
    })?;
    for node in filesystem.files() {
        let Some(node_path) = normalize_node_path(&node.fullpath).map_err(|error| {
            BundleReadError::tool(
                BundleSourceKind::Artifact,
                path,
                format!("read SquashFS path {}: {error}", node.fullpath.display()),
            )
        })?
        else {
            continue;
        };
        if node_path != normalized_inner {
            continue;
        }
        let InnerNode::File(file) = &node.inner else {
            return Err(BundleReadError::invalid(
                BundleSourceKind::Artifact,
                &path.display().to_string(),
                format!("artifact entry {inner_path} is not a file"),
            ));
        };
        let mut reader = filesystem.file(file).reader();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(|error| {
            BundleReadError::tool(
                BundleSourceKind::Artifact,
                path,
                format!("read artifact entry {inner_path}: {error}"),
            )
        })?;
        return Ok(bytes);
    }
    Err(BundleReadError::tool(
        BundleSourceKind::Artifact,
        path,
        format!("artifact entry {inner_path} not found"),
    ))
}

fn open_artifact_filesystem(path: &Path) -> Result<FilesystemReader<'static>, BundleReadError> {
    let file = fs::File::open(path).map_err(|error| {
        BundleReadError::io(
            BundleSourceKind::Artifact,
            path,
            format!("open artifact {}: {error}", path.display()),
        )
    })?;
    FilesystemReader::from_reader(BufReader::new(file)).map_err(|error| {
        BundleReadError::tool(
            BundleSourceKind::Artifact,
            path,
            format!("read SquashFS artifact with Rust-native reader: {error}"),
        )
    })
}

fn normalize_node_path(path: &Path) -> Result<Option<String>, String> {
    if path == Path::new("/") {
        return Ok(None);
    }
    let stripped = path.strip_prefix("/").unwrap_or(path);
    normalize_path(stripped).map(Some)
}

fn normalize_artifact_path(path: &str) -> Result<String, String> {
    normalize_path(Path::new(path.trim_matches('/')))
}

fn normalize_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| format!("path must be valid UTF-8: {}", path.display()))?;
                parts.push(part.to_string());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(format!("path must be relative: {}", path.display()));
            }
        }
    }
    if parts.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    Ok(parts.join("/"))
}

fn parse_manifest(
    source_kind: BundleSourceKind,
    source_path: &Path,
    raw: &str,
) -> Result<BundleManifest, BundleReadError> {
    serde_json::from_str(raw).map_err(|error| {
        BundleReadError::invalid(
            source_kind,
            &source_path.display().to_string(),
            format!("parse bundle-manifest.json: {error}"),
        )
    })
}

fn parse_lock(
    source_kind: BundleSourceKind,
    source_path: &Path,
    raw: &str,
) -> Result<BundleLock, BundleReadError> {
    serde_json::from_str(raw).map_err(|error| {
        BundleReadError::invalid(
            source_kind,
            &source_path.display().to_string(),
            format!("parse bundle-lock.json: {error}"),
        )
    })
}

fn validate_build_dir_contents(path: &Path, opened: &OpenedBundle) -> Result<(), BundleReadError> {
    ensure_path_exists(
        BundleSourceKind::BuildDir,
        path,
        &path.join("bundle.yaml"),
        "bundle.yaml",
    )?;
    for rel_path in &opened.manifest.generated_resolved_files {
        ensure_path_exists(
            BundleSourceKind::BuildDir,
            path,
            &path.join(rel_path),
            rel_path,
        )?;
    }
    for rel_path in &opened.manifest.generated_setup_files {
        ensure_path_exists(
            BundleSourceKind::BuildDir,
            path,
            &path.join(rel_path),
            rel_path,
        )?;
    }
    Ok(())
}

fn validate_artifact_contents(path: &Path, opened: &OpenedBundle) -> Result<(), BundleReadError> {
    read_artifact_file(path, "bundle.yaml")?;
    for rel_path in &opened.manifest.generated_resolved_files {
        read_artifact_file(path, rel_path)?;
    }
    for rel_path in &opened.manifest.generated_setup_files {
        read_artifact_file(path, rel_path)?;
    }
    Ok(())
}

fn ensure_path_exists(
    source_kind: BundleSourceKind,
    source_path: &Path,
    full_path: &Path,
    display_path: &str,
) -> Result<(), BundleReadError> {
    if full_path.exists() {
        return Ok(());
    }
    Err(BundleReadError::invalid(
        source_kind,
        &source_path.display().to_string(),
        format!("missing required bundle file: {display_path}"),
    ))
}

pub fn build_dir_from_artifact_source(root: &Path, bundle_id: &str) -> PathBuf {
    root.join("state")
        .join("build")
        .join(bundle_id)
        .join("normalized")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> BundleManifest {
        BundleManifest {
            format_version: BUNDLE_FORMAT_VERSION.to_string(),
            bundle_id: "demo".to_string(),
            bundle_name: "Demo".to_string(),
            requested_mode: "auto".to_string(),
            locale: "en".to_string(),
            artifact_extension: ".gtbundle".to_string(),
            generated_resolved_files: Vec::new(),
            generated_setup_files: Vec::new(),
            app_packs: Vec::new(),
            extension_providers: Vec::new(),
            catalogs: Vec::new(),
            hooks: Vec::new(),
            subscriptions: Vec::new(),
            capabilities: Vec::new(),
            resolved_targets: Vec::new(),
        }
    }

    fn valid_lock() -> BundleLock {
        BundleLock {
            schema_version: 1,
            bundle_id: "demo".to_string(),
            requested_mode: "auto".to_string(),
            execution: "exec".to_string(),
            cache_policy: "policy".to_string(),
            tool_version: "0.0.0".to_string(),
            build_format_version: BUNDLE_FORMAT_VERSION.to_string(),
            workspace_root: "bundle.yaml".to_string(),
            lock_file: "bundle.lock.json".to_string(),
            catalogs: Vec::new(),
            app_packs: Vec::new(),
            extension_providers: Vec::new(),
            setup_state_files: Vec::new(),
        }
    }

    fn opened_with(
        manifest: BundleManifest,
        lock: BundleLock,
    ) -> Result<OpenedBundle, BundleReadError> {
        OpenedBundle::from_parts(BundleSourceKind::Artifact, "demo.gtbundle", manifest, lock)
    }

    #[test]
    fn source_kind_as_str_returns_canonical_labels() {
        assert_eq!(BundleSourceKind::Artifact.as_str(), "artifact");
        assert_eq!(BundleSourceKind::BuildDir.as_str(), "build_dir");
    }

    #[test]
    fn display_renders_kind_and_details() {
        let io = BundleReadError {
            kind: BundleReadErrorKind::Io,
            source_kind: BundleSourceKind::Artifact,
            source_path: "demo.gtbundle".to_string(),
            details: "boom".to_string(),
        };
        assert_eq!(
            io.to_string(),
            "artifact read failed for demo.gtbundle (io): boom"
        );

        let invalid = BundleReadError {
            kind: BundleReadErrorKind::Invalid,
            source_kind: BundleSourceKind::BuildDir,
            source_path: "build/".to_string(),
            details: "bad".to_string(),
        };
        assert_eq!(
            invalid.to_string(),
            "build_dir read failed for build/ (invalid): bad"
        );

        let tool = BundleReadError {
            kind: BundleReadErrorKind::Tool,
            source_kind: BundleSourceKind::Artifact,
            source_path: "x".to_string(),
            details: "y".to_string(),
        };
        assert_eq!(tool.to_string(), "artifact read failed for x (tool): y");
    }

    #[test]
    fn from_parts_succeeds_for_consistent_manifest_and_lock() {
        let opened = opened_with(valid_manifest(), valid_lock()).expect("valid bundle");
        assert_eq!(opened.source_kind, BundleSourceKind::Artifact);
        assert_eq!(opened.format_version, BUNDLE_FORMAT_VERSION);
        let surface = opened.runtime_surface();
        assert_eq!(surface.bundle_id, "demo");
        assert_eq!(surface.workspace_root, "bundle.yaml");
        assert!(surface.generated_resolved_files.is_empty());
        assert!(surface.generated_setup_files.is_empty());
    }

    #[test]
    fn validate_rejects_unsupported_format_version() {
        let mut manifest = valid_manifest();
        manifest.format_version = "gtbundle-v999".to_string();
        let err = opened_with(manifest, valid_lock()).expect_err("bad version");
        assert_eq!(err.kind, BundleReadErrorKind::Invalid);
        assert!(err.details.contains("unsupported bundle format version"));
    }

    #[test]
    fn validate_rejects_empty_manifest_bundle_id() {
        let mut manifest = valid_manifest();
        manifest.bundle_id = "   ".to_string();
        let err = opened_with(manifest, valid_lock()).expect_err("empty manifest id");
        assert!(err.details.contains("manifest is missing bundle_id"));
    }

    #[test]
    fn validate_rejects_empty_lock_bundle_id() {
        let mut lock = valid_lock();
        lock.bundle_id = " ".to_string();
        let err = opened_with(valid_manifest(), lock).expect_err("empty lock id");
        assert!(err.details.contains("lock is missing bundle_id"));
    }

    #[test]
    fn validate_rejects_mismatched_bundle_id() {
        let mut lock = valid_lock();
        lock.bundle_id = "other".to_string();
        let err = opened_with(valid_manifest(), lock).expect_err("mismatched id");
        assert!(err.details.contains("bundle_id do not match"));
    }

    #[test]
    fn validate_rejects_mismatched_requested_mode() {
        let mut lock = valid_lock();
        lock.requested_mode = "different".to_string();
        let err = opened_with(valid_manifest(), lock).expect_err("mismatched mode");
        assert!(err.details.contains("requested_mode do not match"));
    }

    #[test]
    fn validate_rejects_unexpected_artifact_extension() {
        let mut manifest = valid_manifest();
        manifest.artifact_extension = ".zip".to_string();
        let err = opened_with(manifest, valid_lock()).expect_err("bad ext");
        assert!(err.details.contains("unsupported artifact extension"));
    }

    #[test]
    fn validate_rejects_unexpected_workspace_root() {
        let mut lock = valid_lock();
        lock.workspace_root = "other.yaml".to_string();
        let err = opened_with(valid_manifest(), lock).expect_err("bad workspace");
        assert!(err.details.contains("unexpected workspace_root"));
    }

    #[test]
    fn validate_rejects_unexpected_lock_file_name() {
        let mut lock = valid_lock();
        lock.lock_file = "other.json".to_string();
        let err = opened_with(valid_manifest(), lock).expect_err("bad lock file");
        assert!(err.details.contains("unexpected lock_file"));
    }

    #[test]
    fn validate_rejects_mismatched_setup_state_files() {
        let mut manifest = valid_manifest();
        manifest.generated_setup_files = vec!["state/a.json".to_string()];
        let err = opened_with(manifest, valid_lock()).expect_err("setup mismatch");
        assert!(err.details.contains("setup state files do not match"));
    }

    #[test]
    fn normalize_artifact_path_strips_leading_and_trailing_slashes() {
        assert_eq!(
            normalize_artifact_path("/bundle-manifest.json").expect("normalize"),
            "bundle-manifest.json"
        );
        assert_eq!(
            normalize_artifact_path("setup/state.json/").expect("normalize"),
            "setup/state.json"
        );
    }

    #[test]
    fn normalize_artifact_path_rejects_empty_input() {
        let err = normalize_artifact_path("").expect_err("empty path");
        assert!(err.contains("path cannot be empty"));
    }

    #[test]
    fn normalize_artifact_path_rejects_parent_segments() {
        let err = normalize_artifact_path("a/../b").expect_err("parent dir");
        assert!(err.contains("path must be relative"));
    }

    #[test]
    fn normalize_node_path_treats_root_as_none() {
        assert_eq!(normalize_node_path(Path::new("/")).expect("root"), None);
        assert_eq!(
            normalize_node_path(Path::new("/bundle-manifest.json")).expect("file"),
            Some("bundle-manifest.json".to_string())
        );
        assert_eq!(
            normalize_node_path(Path::new("/nested/dir/file.bin")).expect("nested"),
            Some("nested/dir/file.bin".to_string())
        );
    }

    #[test]
    fn build_dir_from_artifact_source_uses_state_build_normalized_layout() {
        let root = Path::new("/tmp/root");
        let path = build_dir_from_artifact_source(root, "demo");
        assert_eq!(path, Path::new("/tmp/root/state/build/demo/normalized"));
    }
}
