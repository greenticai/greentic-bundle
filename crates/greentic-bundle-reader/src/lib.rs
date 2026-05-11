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
