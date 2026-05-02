use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use greentic_distributor_client::{
    CachePolicy, DistClient, DistOptions, OciPackFetcher, PackFetchOptions, ResolvePolicy,
    oci_packs::DefaultRegistryClient,
};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

pub const WORKSPACE_ROOT_FILE: &str = "bundle.yaml";
pub const LOCK_FILE: &str = "bundle.lock.json";
pub const LOCK_SCHEMA_VERSION: u32 = 1;

const DEFAULT_GMAP: &str = "_ = forbidden\n";
const GREENTIC_GTPACK_TAR_MEDIA_TYPE: &str = "application/vnd.greentic.gtpack.layer.v1+tar";
const GREENTIC_GTPACK_TAR_GZIP_MEDIA_TYPE: &str =
    "application/vnd.greentic.gtpack.layer.v1.tar+gzip";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleWorkspaceDefinition {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub bundle_id: String,
    pub bundle_name: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub advanced_setup: bool,
    #[serde(default)]
    pub app_packs: Vec<String>,
    #[serde(default)]
    pub app_pack_mappings: Vec<AppPackMapping>,
    #[serde(default)]
    pub extension_providers: Vec<String>,
    #[serde(default)]
    pub remote_catalogs: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub setup_execution_intent: bool,
    #[serde(default)]
    pub export_intent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPackMapping {
    pub reference: String,
    pub scope: MappingScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingScope {
    Global,
    Tenant,
    Team,
}

#[derive(Debug, Serialize)]
struct ResolvedManifest {
    version: String,
    tenant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    team: Option<String>,
    project_root: String,
    bundle: BundleSummary,
    policy: PolicySection,
    catalogs: Vec<String>,
    app_packs: Vec<ResolvedReferencePolicy>,
    extension_providers: Vec<String>,
    hooks: Vec<String>,
    subscriptions: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BundleSummary {
    bundle_id: String,
    bundle_name: String,
    locale: String,
    mode: String,
    advanced_setup: bool,
    setup_execution_intent: bool,
    export_intent: bool,
}

#[derive(Debug, Serialize)]
struct PolicySection {
    source: PolicySource,
    default: String,
}

#[derive(Debug, Serialize)]
struct PolicySource {
    tenant_gmap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_gmap: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResolvedReferencePolicy {
    reference: String,
    policy: String,
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
    pub catalogs: Vec<crate::catalog::resolve::CatalogLockEntry>,
    pub app_packs: Vec<DependencyLock>,
    pub extension_providers: Vec<DependencyLock>,
    pub setup_state_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLock {
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceField {
    AppPack,
    ExtensionProvider,
}

impl BundleWorkspaceDefinition {
    pub fn new(bundle_name: String, bundle_id: String, locale: String, mode: String) -> Self {
        Self {
            schema_version: default_schema_version(),
            bundle_id,
            bundle_name,
            locale,
            mode,
            advanced_setup: false,
            app_packs: Vec::new(),
            app_pack_mappings: Vec::new(),
            extension_providers: Vec::new(),
            remote_catalogs: Vec::new(),
            hooks: Vec::new(),
            subscriptions: Vec::new(),
            capabilities: Vec::new(),
            setup_execution_intent: false,
            export_intent: false,
        }
    }

    pub fn canonicalize(&mut self) {
        canonicalize_mappings(&mut self.app_pack_mappings);
        self.app_packs.extend(
            self.app_pack_mappings
                .iter()
                .map(|entry| entry.reference.clone()),
        );
        sort_unique(&mut self.app_packs);
        sort_unique(&mut self.extension_providers);
        sort_unique(&mut self.remote_catalogs);
        sort_unique(&mut self.hooks);
        sort_unique(&mut self.subscriptions);
        sort_unique(&mut self.capabilities);
    }

    pub fn references(&self, field: ReferenceField) -> &[String] {
        match field {
            ReferenceField::AppPack => &self.app_packs,
            ReferenceField::ExtensionProvider => &self.extension_providers,
        }
    }

    pub fn references_mut(&mut self, field: ReferenceField) -> &mut Vec<String> {
        match field {
            ReferenceField::AppPack => &mut self.app_packs,
            ReferenceField::ExtensionProvider => &mut self.extension_providers,
        }
    }
}

pub fn ensure_layout(root: &Path) -> Result<()> {
    ensure_dir(&root.join("tenants"))?;
    ensure_dir(&root.join("tenants").join("default"))?;
    ensure_dir(&root.join("tenants").join("default").join("teams"))?;
    ensure_dir(&root.join("resolved"))?;
    ensure_dir(&root.join("state").join("resolved"))?;
    write_if_missing(&root.join(WORKSPACE_ROOT_FILE), "schema_version: 1\n")?;
    write_if_missing(
        &root.join("tenants").join("default").join("tenant.gmap"),
        DEFAULT_GMAP,
    )?;
    Ok(())
}

/// Bundle-level capability for read-only asset access by packs.
pub const CAP_BUNDLE_ASSETS_READ_V1: &str = "greentic.cap.bundle_assets.read.v1";

/// Creates the `assets/` directory at the bundle root for bundle-level shared assets.
pub fn ensure_assets_dir(root: &Path) -> Result<()> {
    ensure_dir(&root.join("assets"))
}

pub fn read_bundle_workspace(root: &Path) -> Result<BundleWorkspaceDefinition> {
    let raw = std::fs::read_to_string(root.join(WORKSPACE_ROOT_FILE))?;
    let mut definition = serde_yaml_bw::from_str::<BundleWorkspaceDefinition>(&raw)?;
    definition.canonicalize();
    Ok(definition)
}

pub fn write_bundle_workspace(root: &Path, workspace: &BundleWorkspaceDefinition) -> Result<()> {
    let mut workspace = workspace.clone();
    workspace.canonicalize();
    let path = root.join(WORKSPACE_ROOT_FILE);
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(path, render_bundle_workspace(&workspace))?;
    Ok(())
}

pub fn init_bundle_workspace(
    root: &Path,
    workspace: &BundleWorkspaceDefinition,
) -> Result<Vec<PathBuf>> {
    ensure_layout(root)?;
    let has_bundle_assets = workspace
        .capabilities
        .iter()
        .any(|c| c == CAP_BUNDLE_ASSETS_READ_V1);
    if has_bundle_assets {
        ensure_assets_dir(root)?;
    }
    write_bundle_workspace(root, workspace)?;
    let lock = empty_bundle_lock(workspace);
    write_bundle_lock(root, &lock)?;
    sync_project(root)?;
    let mut files = vec![
        root.join(WORKSPACE_ROOT_FILE),
        root.join(LOCK_FILE),
        root.join("tenants/default/tenant.gmap"),
        root.join("resolved/default.yaml"),
        root.join("state/resolved/default.yaml"),
    ];
    if has_bundle_assets {
        files.push(root.join("assets"));
    }
    Ok(files)
}

pub fn sync_lock_with_workspace(root: &Path, workspace: &BundleWorkspaceDefinition) -> Result<()> {
    let mut lock = if root.join(LOCK_FILE).exists() {
        read_bundle_lock(root)?
    } else {
        empty_bundle_lock(workspace)
    };
    lock.bundle_id = workspace.bundle_id.clone();
    lock.requested_mode = workspace.mode.clone();
    lock.workspace_root = WORKSPACE_ROOT_FILE.to_string();
    lock.lock_file = LOCK_FILE.to_string();
    lock.app_packs = workspace
        .app_packs
        .iter()
        .map(|reference| DependencyLock {
            reference: reference.clone(),
            digest: None,
        })
        .collect();
    lock.extension_providers = workspace
        .extension_providers
        .iter()
        .map(|reference| DependencyLock {
            reference: reference.clone(),
            digest: None,
        })
        .collect();
    write_bundle_lock(root, &lock)
}

pub fn ensure_tenant(root: &Path, tenant: &str) -> Result<()> {
    let tenant_dir = root.join("tenants").join(tenant);
    ensure_dir(&tenant_dir.join("teams"))?;
    write_if_missing(&tenant_dir.join("tenant.gmap"), DEFAULT_GMAP)?;
    Ok(())
}

pub fn ensure_team(root: &Path, tenant: &str, team: &str) -> Result<()> {
    ensure_tenant(root, tenant)?;
    let team_dir = root.join("tenants").join(tenant).join("teams").join(team);
    ensure_dir(&team_dir)?;
    write_if_missing(&team_dir.join("team.gmap"), DEFAULT_GMAP)?;
    Ok(())
}

pub fn gmap_path(root: &Path, target: &crate::access::GmapTarget) -> PathBuf {
    if let Some(team) = &target.team {
        root.join("tenants")
            .join(&target.tenant)
            .join("teams")
            .join(team)
            .join("team.gmap")
    } else {
        root.join("tenants")
            .join(&target.tenant)
            .join("tenant.gmap")
    }
}

pub fn resolved_output_paths(root: &Path, tenant: &str, team: Option<&str>) -> Vec<PathBuf> {
    let filename = match team {
        Some(team) => format!("{tenant}.{team}.yaml"),
        None => format!("{tenant}.yaml"),
    };
    vec![
        root.join("resolved").join(&filename),
        root.join("state").join("resolved").join(filename),
    ]
}

pub fn sync_project(root: &Path) -> Result<()> {
    sync_project_with_reference_roots(root, &[])
}

pub fn sync_project_with_reference_roots(root: &Path, reference_roots: &[PathBuf]) -> Result<()> {
    ensure_layout(root)?;
    if let Ok(workspace) = read_bundle_workspace(root) {
        materialize_workspace_dependencies(root, &workspace, reference_roots)?;
    }
    for tenant in list_tenants(root)? {
        let teams = list_teams(root, &tenant)?;
        if teams.is_empty() {
            let manifest = build_manifest(root, &tenant, None);
            write_resolved_outputs(root, &tenant, None, &manifest)?;
        } else {
            let tenant_manifest = build_manifest(root, &tenant, None);
            write_resolved_outputs(root, &tenant, None, &tenant_manifest)?;
            for team in teams {
                let manifest = build_manifest(root, &tenant, Some(&team));
                write_resolved_outputs(root, &tenant, Some(&team), &manifest)?;
            }
        }
    }
    Ok(())
}

fn materialize_workspace_dependencies(
    root: &Path,
    workspace: &BundleWorkspaceDefinition,
    reference_roots: &[PathBuf],
) -> Result<()> {
    let app_targets = app_pack_copy_targets(workspace);
    let provider_targets: Vec<_> = workspace
        .extension_providers
        .iter()
        .filter(|p| !should_skip_extension_provider_materialization(p))
        .collect();
    let total = app_targets.len() + provider_targets.len();
    let mut current = 0usize;

    for mapping in &app_targets {
        current += 1;
        let dest = root.join(&mapping.destination);
        if dest.exists() {
            eprintln!("  [{current}/{total}] Cached: {}", mapping.reference);
        } else {
            eprintln!(
                "  [{current}/{total}] Resolving app pack: {}",
                mapping.reference
            );
        }
        materialize_reference_into(
            root,
            reference_roots,
            &mapping.reference,
            &mapping.destination,
        )?;
    }
    for provider in &provider_targets {
        current += 1;
        let destination = provider_destination_path(provider);
        let dest = root.join(&destination);
        if dest.exists() {
            eprintln!("  [{current}/{total}] Cached: {provider}");
        } else {
            eprintln!("  [{current}/{total}] Resolving provider: {provider}");
        }
        materialize_reference_into(root, reference_roots, provider, &destination)?;
    }
    if total > 0 {
        eprintln!("  [done] Resolved {total} package(s)");
    }
    Ok(())
}

fn should_skip_extension_provider_materialization(reference: &str) -> bool {
    bundled_catalog_mode()
        && (reference.starts_with("oci://")
            || reference.starts_with("repo://")
            || reference.starts_with("store://")
            || reference.starts_with("https://"))
}

fn bundled_catalog_mode() -> bool {
    std::env::var("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

struct MaterializedCopyTarget {
    reference: String,
    destination: PathBuf,
}

fn app_pack_copy_targets(workspace: &BundleWorkspaceDefinition) -> Vec<MaterializedCopyTarget> {
    if workspace.app_pack_mappings.is_empty() {
        return workspace
            .app_packs
            .iter()
            .map(|reference| MaterializedCopyTarget {
                reference: reference.clone(),
                destination: PathBuf::from("packs")
                    .join(format!("{}.gtpack", inferred_access_pack_id(reference))),
            })
            .collect();
    }

    workspace
        .app_pack_mappings
        .iter()
        .map(|mapping| {
            let filename = format!("{}.gtpack", inferred_access_pack_id(&mapping.reference));
            let destination = match mapping.scope {
                MappingScope::Global => PathBuf::from("packs").join(filename),
                MappingScope::Tenant => PathBuf::from("tenants")
                    .join(mapping.tenant.as_deref().unwrap_or("default"))
                    .join("packs")
                    .join(filename),
                MappingScope::Team => PathBuf::from("tenants")
                    .join(mapping.tenant.as_deref().unwrap_or("default"))
                    .join("teams")
                    .join(mapping.team.as_deref().unwrap_or("default"))
                    .join("packs")
                    .join(filename),
            };
            MaterializedCopyTarget {
                reference: mapping.reference.clone(),
                destination,
            }
        })
        .collect()
}

fn provider_destination_path(reference: &str) -> PathBuf {
    let provider_type = inferred_provider_type(reference);
    let provider_name = inferred_provider_filename(reference);
    PathBuf::from("providers")
        .join(provider_type)
        .join(format!("{provider_name}.gtpack"))
}

fn materialize_reference_into(
    root: &Path,
    reference_roots: &[PathBuf],
    reference: &str,
    relative_destination: &Path,
) -> Result<()> {
    let destination = root.join(relative_destination);
    if destination.exists() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        ensure_dir(parent)?;
    }

    if let Some(local_path) = parse_local_pack_reference(root, reference_roots, reference) {
        if local_path.is_dir() {
            return Ok(());
        }
        std::fs::copy(&local_path, &destination).with_context(|| {
            format!("copy {} to {}", local_path.display(), destination.display())
        })?;
        return Ok(());
    }

    if !(reference.starts_with("oci://")
        || reference.starts_with("repo://")
        || reference.starts_with("store://")
        || reference.starts_with("https://"))
    {
        return Ok(());
    }

    let path = resolve_remote_pack_path(root, reference)?;
    std::fs::copy(&path, &destination)
        .with_context(|| format!("copy {} to {}", path.display(), destination.display()))?;

    Ok(())
}

fn parse_local_pack_reference(
    root: &Path,
    reference_roots: &[PathBuf],
    reference: &str,
) -> Option<PathBuf> {
    if let Some(path) = reference.strip_prefix("file://") {
        let path = PathBuf::from(path.trim());
        if path.is_absolute() {
            return path.exists().then_some(path);
        }
        for base in reference_roots
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(root))
        {
            let candidate = base.join(&path);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        return None;
    }
    if reference.contains("://") {
        return None;
    }
    let candidate = PathBuf::from(reference);
    if candidate.is_absolute() {
        return candidate.exists().then_some(candidate);
    }
    for base in reference_roots
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(root))
    {
        let joined = base.join(&candidate);
        if joined.exists() {
            return Some(joined);
        }
    }
    None
}

fn resolve_remote_pack_path(root: &Path, reference: &str) -> Result<PathBuf> {
    if let Some(oci_reference) = reference.strip_prefix("oci://") {
        let mut options = PackFetchOptions {
            allow_tags: true,
            offline: crate::runtime::offline(),
            cache_dir: root.join(crate::catalog::CACHE_ROOT_DIR).join("artifacts"),
            ..PackFetchOptions::default()
        };
        options.accepted_layer_media_types.extend([
            GREENTIC_GTPACK_TAR_MEDIA_TYPE.to_string(),
            GREENTIC_GTPACK_TAR_GZIP_MEDIA_TYPE.to_string(),
        ]);
        options.preferred_layer_media_types.splice(
            0..0,
            [
                GREENTIC_GTPACK_TAR_MEDIA_TYPE.to_string(),
                GREENTIC_GTPACK_TAR_GZIP_MEDIA_TYPE.to_string(),
            ],
        );
        let fetcher: OciPackFetcher<DefaultRegistryClient> = OciPackFetcher::new(options);
        let runtime = Runtime::new().context("create OCI pack resolver runtime")?;
        let resolved = runtime
            .block_on(fetcher.fetch_pack_to_cache(oci_reference))
            .with_context(|| format!("resolve OCI pack ref {reference}"))?;
        return Ok(resolved.path);
    }

    let options = DistOptions {
        allow_tags: true,
        offline: crate::runtime::offline(),
        cache_dir: root.join(crate::catalog::CACHE_ROOT_DIR).join("artifacts"),
        ..DistOptions::default()
    };
    let client = DistClient::new(options);
    let runtime = Runtime::new().context("create artifact resolver runtime")?;
    let source = client
        .parse_source(reference)
        .with_context(|| format!("parse artifact ref {reference}"))?;
    let descriptor = runtime
        .block_on(client.resolve(source, ResolvePolicy))
        .with_context(|| format!("resolve artifact ref {reference}"))?;
    let resolved = runtime
        .block_on(client.fetch(&descriptor, CachePolicy))
        .with_context(|| format!("fetch artifact ref {reference}"))?;
    if let Some(path) = resolved.wasm_path {
        return Ok(path);
    }
    if let Some(bytes) = resolved.wasm_bytes {
        let digest = resolved.resolved_digest.trim_start_matches("sha256:");
        let temp_path = root
            .join(crate::catalog::CACHE_ROOT_DIR)
            .join("artifacts")
            .join("inline")
            .join(format!("{digest}.gtpack"));
        if let Some(parent) = temp_path.parent() {
            ensure_dir(parent)?;
        }
        std::fs::write(&temp_path, bytes)
            .with_context(|| format!("write cached inline artifact {}", temp_path.display()))?;
        return Ok(temp_path);
    }
    anyhow::bail!("artifact ref {reference} resolved without file payload");
}

pub fn list_tenants(root: &Path) -> Result<Vec<String>> {
    let tenants_dir = root.join("tenants");
    let mut tenants = Vec::new();
    if !tenants_dir.exists() {
        return Ok(tenants);
    }
    for entry in std::fs::read_dir(tenants_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            tenants.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    tenants.sort();
    Ok(tenants)
}

pub fn list_teams(root: &Path, tenant: &str) -> Result<Vec<String>> {
    let teams_dir = root.join("tenants").join(tenant).join("teams");
    let mut teams = Vec::new();
    if !teams_dir.exists() {
        return Ok(teams);
    }
    for entry in std::fs::read_dir(teams_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            teams.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    teams.sort();
    Ok(teams)
}

pub fn write_bundle_lock(root: &Path, lock: &BundleLock) -> Result<()> {
    let path = root.join(LOCK_FILE);
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(lock)?))?;
    Ok(())
}

pub fn read_bundle_lock(root: &Path) -> Result<BundleLock> {
    let path = root.join(LOCK_FILE);
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn build_manifest(root: &Path, tenant: &str, team: Option<&str>) -> ResolvedManifest {
    let workspace = read_workspace_or_default(root);
    let tenant_gmap = relative_path(root, &root.join("tenants").join(tenant).join("tenant.gmap"));
    let team_gmap = team.map(|team| {
        relative_path(
            root,
            &root
                .join("tenants")
                .join(tenant)
                .join("teams")
                .join(team)
                .join("team.gmap"),
        )
    });

    let app_packs = evaluate_app_pack_policies(root, tenant, team, &workspace.app_packs);

    ResolvedManifest {
        version: "1".to_string(),
        tenant: tenant.to_string(),
        team: team.map(ToOwned::to_owned),
        project_root: root.display().to_string(),
        bundle: BundleSummary {
            bundle_id: workspace.bundle_id,
            bundle_name: workspace.bundle_name,
            locale: workspace.locale,
            mode: workspace.mode,
            advanced_setup: workspace.advanced_setup,
            setup_execution_intent: workspace.setup_execution_intent,
            export_intent: workspace.export_intent,
        },
        policy: PolicySection {
            source: PolicySource {
                tenant_gmap,
                team_gmap,
            },
            default: "forbidden".to_string(),
        },
        catalogs: workspace.remote_catalogs,
        app_packs,
        extension_providers: workspace.extension_providers,
        hooks: workspace.hooks,
        subscriptions: workspace.subscriptions,
        capabilities: workspace.capabilities,
    }
}

fn render_bundle_workspace(workspace: &BundleWorkspaceDefinition) -> String {
    format!(
        concat!(
            "schema_version: {}\n",
            "bundle_id: {}\n",
            "bundle_name: {}\n",
            "locale: {}\n",
            "mode: {}\n",
            "advanced_setup: {}\n",
            "app_packs:{}\n",
            "app_pack_mappings:{}\n",
            "extension_providers:{}\n",
            "remote_catalogs:{}\n",
            "hooks:{}\n",
            "subscriptions:{}\n",
            "capabilities:{}\n",
            "setup_execution_intent: {}\n",
            "export_intent: {}\n"
        ),
        workspace.schema_version,
        workspace.bundle_id,
        workspace.bundle_name,
        workspace.locale,
        workspace.mode,
        workspace.advanced_setup,
        yaml_list(&workspace.app_packs),
        yaml_mapping_list(&workspace.app_pack_mappings),
        yaml_list(&workspace.extension_providers),
        yaml_list(&workspace.remote_catalogs),
        yaml_list(&workspace.hooks),
        yaml_list(&workspace.subscriptions),
        yaml_list(&workspace.capabilities),
        workspace.setup_execution_intent,
        workspace.export_intent
    )
}

fn yaml_mapping_list(values: &[AppPackMapping]) -> String {
    if values.is_empty() {
        " []".to_string()
    } else {
        values
            .iter()
            .map(|value| {
                let mut out = format!(
                    "\n  - reference: {}\n    scope: {}",
                    value.reference,
                    match value.scope {
                        MappingScope::Global => "global",
                        MappingScope::Tenant => "tenant",
                        MappingScope::Team => "team",
                    }
                );
                if let Some(tenant) = &value.tenant {
                    out.push_str(&format!("\n    tenant: {tenant}"));
                }
                if let Some(team) = &value.team {
                    out.push_str(&format!("\n    team: {team}"));
                }
                out
            })
            .collect::<String>()
    }
}

fn empty_bundle_lock(workspace: &BundleWorkspaceDefinition) -> BundleLock {
    BundleLock {
        schema_version: LOCK_SCHEMA_VERSION,
        bundle_id: workspace.bundle_id.clone(),
        requested_mode: workspace.mode.clone(),
        execution: "execute".to_string(),
        cache_policy: "workspace-local".to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        build_format_version: "bundle-lock-v1".to_string(),
        workspace_root: WORKSPACE_ROOT_FILE.to_string(),
        lock_file: LOCK_FILE.to_string(),
        catalogs: Vec::new(),
        app_packs: workspace
            .app_packs
            .iter()
            .map(|reference| DependencyLock {
                reference: reference.clone(),
                digest: None,
            })
            .collect(),
        extension_providers: workspace
            .extension_providers
            .iter()
            .map(|reference| DependencyLock {
                reference: reference.clone(),
                digest: None,
            })
            .collect(),
        setup_state_files: Vec::new(),
    }
}

fn yaml_list(values: &[String]) -> String {
    if values.is_empty() {
        " []".to_string()
    } else {
        values
            .iter()
            .map(|value| format!("\n  - {value}"))
            .collect::<String>()
    }
}

fn sort_unique(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
}

fn canonicalize_mappings(values: &mut Vec<AppPackMapping>) {
    values.retain(|value| !value.reference.trim().is_empty());
    for value in values.iter_mut() {
        if value
            .tenant
            .as_deref()
            .is_some_and(|tenant| tenant.trim().is_empty())
        {
            value.tenant = None;
        }
        if value
            .team
            .as_deref()
            .is_some_and(|team| team.trim().is_empty())
        {
            value.team = None;
        }
        if matches!(value.scope, MappingScope::Global) {
            value.tenant = None;
            value.team = None;
        } else if matches!(value.scope, MappingScope::Tenant) {
            value.team = None;
        }
    }
    values.sort_by(|left, right| {
        left.reference
            .cmp(&right.reference)
            .then(left.scope.cmp(&right.scope))
            .then(left.tenant.cmp(&right.tenant))
            .then(left.team.cmp(&right.team))
    });
    values.dedup_by(|left, right| {
        left.reference == right.reference
            && left.scope == right.scope
            && left.tenant == right.tenant
            && left.team == right.team
    });
}

fn default_schema_version() -> u32 {
    1
}

fn default_locale() -> String {
    "en".to_string()
}

fn default_mode() -> String {
    "create".to_string()
}

fn write_resolved_outputs(
    root: &Path,
    tenant: &str,
    team: Option<&str>,
    manifest: &ResolvedManifest,
) -> Result<()> {
    let yaml = render_manifest_yaml(manifest);
    for output in resolved_output_paths(root, tenant, team) {
        if let Some(parent) = output.parent() {
            ensure_dir(parent)?;
        }
        std::fs::write(output, &yaml)?;
    }
    Ok(())
}

fn render_manifest_yaml(manifest: &ResolvedManifest) -> String {
    let mut lines = vec![
        format!("version: {}", manifest.version),
        format!("tenant: {}", manifest.tenant),
    ];
    if let Some(team) = &manifest.team {
        lines.push(format!("team: {}", team));
    }
    lines.extend([
        format!("project_root: {}", manifest.project_root),
        "bundle:".to_string(),
        format!("  bundle_id: {}", manifest.bundle.bundle_id),
        format!("  bundle_name: {}", manifest.bundle.bundle_name),
        format!("  locale: {}", manifest.bundle.locale),
        format!("  mode: {}", manifest.bundle.mode),
        format!("  advanced_setup: {}", manifest.bundle.advanced_setup),
        format!(
            "  setup_execution_intent: {}",
            manifest.bundle.setup_execution_intent
        ),
        format!("  export_intent: {}", manifest.bundle.export_intent),
        "policy:".to_string(),
        "  source:".to_string(),
        format!("    tenant_gmap: {}", manifest.policy.source.tenant_gmap),
    ]);
    if let Some(team_gmap) = &manifest.policy.source.team_gmap {
        lines.push(format!("    team_gmap: {}", team_gmap));
    }
    lines.push(format!("  default: {}", manifest.policy.default));
    lines.push("catalogs:".to_string());
    lines.extend(render_yaml_list("  ", &manifest.catalogs));
    lines.push("app_packs:".to_string());
    if manifest.app_packs.is_empty() {
        lines.push("  []".to_string());
    } else {
        for entry in &manifest.app_packs {
            lines.push(format!("  - reference: {}", entry.reference));
            lines.push(format!("    policy: {}", entry.policy));
        }
    }
    lines.push("extension_providers:".to_string());
    lines.extend(render_yaml_list("  ", &manifest.extension_providers));
    lines.push("hooks:".to_string());
    lines.extend(render_yaml_list("  ", &manifest.hooks));
    lines.push("subscriptions:".to_string());
    lines.extend(render_yaml_list("  ", &manifest.subscriptions));
    lines.push("capabilities:".to_string());
    lines.extend(render_yaml_list("  ", &manifest.capabilities));
    format!("{}\n", lines.join("\n"))
}

fn read_workspace_or_default(root: &Path) -> BundleWorkspaceDefinition {
    read_bundle_workspace(root).unwrap_or_else(|_| {
        let bundle_id = root
            .file_name()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "bundle".to_string());
        BundleWorkspaceDefinition::new(
            bundle_id.clone(),
            bundle_id,
            default_locale(),
            default_mode(),
        )
    })
}

fn evaluate_app_pack_policies(
    root: &Path,
    tenant: &str,
    team: Option<&str>,
    app_packs: &[String],
) -> Vec<ResolvedReferencePolicy> {
    let tenant_rules =
        crate::access::parse_file(&root.join("tenants").join(tenant).join("tenant.gmap"))
            .unwrap_or_default();
    let team_rules = team
        .and_then(|team_name| {
            crate::access::parse_file(
                &root
                    .join("tenants")
                    .join(tenant)
                    .join("teams")
                    .join(team_name)
                    .join("team.gmap"),
            )
            .ok()
        })
        .unwrap_or_default();

    let mut entries = app_packs
        .iter()
        .map(|reference| {
            let target = crate::access::GmapPath {
                pack: Some(inferred_access_pack_id(reference)),
                flow: None,
                node: None,
            };
            let policy = if team.is_some() {
                crate::access::eval_with_overlay(&tenant_rules, &team_rules, &target)
            } else {
                crate::access::eval_policy(&tenant_rules, &target)
            };
            ResolvedReferencePolicy {
                reference: reference.clone(),
                policy: policy
                    .map(|decision| decision.policy.to_string())
                    .unwrap_or_else(|| "unset".to_string()),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.reference.cmp(&right.reference));
    entries
}

fn inferred_access_pack_id(reference: &str) -> String {
    let cleaned = reference
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(reference)
        .split('@')
        .next()
        .unwrap_or(reference)
        .split(':')
        .next()
        .unwrap_or(reference)
        .trim_end_matches(".json")
        .trim_end_matches(".gtpack")
        .trim_end_matches(".yaml")
        .trim_end_matches(".yml");
    let mut normalized = String::with_capacity(cleaned.len());
    let mut last_dash = false;
    for ch in cleaned.chars() {
        let out = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            ch.to_ascii_lowercase()
        } else if last_dash {
            continue;
        } else {
            last_dash = true;
            '-'
        };
        normalized.push(out);
    }
    normalized.trim_matches('-').to_string()
}

fn inferred_provider_type(reference: &str) -> String {
    let raw = reference.trim();
    for marker in ["/providers/", "/packs/"] {
        if let Some((_, rest)) = raw.split_once(marker)
            && let Some(segment) = rest.split('/').next()
            && !segment.is_empty()
        {
            return segment.to_string();
        }
    }

    let inferred = inferred_access_pack_id(reference);
    let mut parts = inferred.split('-');
    match (parts.next(), parts.next()) {
        (Some("greentic"), Some(domain)) if !domain.is_empty() => domain.to_string(),
        (Some(domain), Some(_)) if !domain.is_empty() => domain.to_string(),
        (Some(_domain), None) => "other".to_string(),
        _ => "other".to_string(),
    }
}

fn inferred_provider_filename(reference: &str) -> String {
    let cleaned = reference
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(reference)
        .split('@')
        .next()
        .unwrap_or(reference)
        .split(':')
        .next()
        .unwrap_or(reference)
        .trim_end_matches(".gtpack");
    if let Some(deployer_target) = cleaned.strip_prefix("greentic.deploy.")
        && !deployer_target.trim().is_empty()
    {
        return deployer_target.trim().to_string();
    }
    if cleaned.is_empty() {
        inferred_access_pack_id(reference)
    } else {
        cleaned.to_string()
    }
}

fn render_yaml_list(indent: &str, values: &[String]) -> Vec<String> {
    if values.is_empty() {
        vec![format!("{indent}[]")]
    } else {
        values
            .iter()
            .map(|value| format!("{indent}- {value}"))
            .collect()
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

/// Extracts `assets/webchat-gui/` entries from all provider `.gtpack` files into
/// the bundle root so users can see and directly modify skins, config, and other
/// webchat-gui assets. Other internal pack assets (fixtures, schemas,
/// secret-requirements, etc.) are intentionally excluded. Existing files are
/// never overwritten — user customizations are preserved.
pub fn scaffold_assets_from_packs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    let providers_dir = root.join("providers");
    if !providers_dir.is_dir() {
        return Ok(written);
    }
    for dir_entry in collect_gtpack_files(&providers_dir)? {
        match extract_pack_assets(root, &dir_entry) {
            Ok(paths) => written.extend(paths),
            Err(err) => {
                eprintln!(
                    "Warning: could not scaffold assets from {}: {err}",
                    dir_entry.display()
                );
            }
        }
    }
    Ok(written)
}

fn collect_gtpack_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_gtpack_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "gtpack") {
            files.push(path);
        }
    }
    Ok(files)
}

fn extract_pack_assets(root: &Path, pack_path: &Path) -> Result<Vec<PathBuf>> {
    let file =
        std::fs::File::open(pack_path).with_context(|| format!("open {}", pack_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("read zip {}", pack_path.display()))?;
    let mut written = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !name.starts_with("assets/webchat-gui/") || entry.is_dir() {
            continue;
        }
        let target = root.join(&name);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut out)?;
        written.push(target);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{provider_destination_path, should_skip_extension_provider_materialization};

    #[test]
    fn bundled_catalog_mode_skips_https_provider_materialization() {
        unsafe {
            std::env::set_var("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG", "1");
        }
        assert!(should_skip_extension_provider_materialization(
            "https://example.com/providers/events-webhook.gtpack"
        ));
        unsafe {
            std::env::remove_var("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG");
        }
    }

    #[test]
    fn deployer_provider_destination_uses_canonical_filename() {
        assert_eq!(
            provider_destination_path(
                "oci://ghcr.io/greenticai/packs/deployer/greentic.deploy.aws.gtpack:latest"
            ),
            PathBuf::from("providers/deployer/aws.gtpack")
        );
    }
}
