use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const WORKSPACE_ROOT_FILE: &str = "bundle.yaml";
pub const LOCK_FILE: &str = "bundle.lock.json";
pub const LOCK_SCHEMA_VERSION: u32 = 1;

const DEFAULT_GMAP: &str = "_ = forbidden\n";

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
    write_bundle_workspace(root, workspace)?;
    let lock = empty_bundle_lock(workspace);
    write_bundle_lock(root, &lock)?;
    sync_project(root)?;
    Ok(vec![
        root.join(WORKSPACE_ROOT_FILE),
        root.join(LOCK_FILE),
        root.join("tenants/default/tenant.gmap"),
        root.join("resolved/default.yaml"),
        root.join("state/resolved/default.yaml"),
    ])
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
    ensure_layout(root)?;
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
                pack: Some(reference.clone()),
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
