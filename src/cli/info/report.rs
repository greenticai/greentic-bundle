use anyhow::{Context, Result};
use greentic_bundle_reader::{
    BundleLock, BundleManifest, BundleResolvedTargetView, BundleSourceKind, DependencyLock,
    OpenedBundle,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::pack_probe::{self, PackMetaSlim};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoReport {
    pub info_schema_version: u32,
    pub bundle_id: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub mode: String,
    pub locale: String,
    pub app_packs: Vec<PackRef>,
    pub extension_providers: Vec<PackRef>,
    pub catalogs: Vec<CatalogRef>,
    pub access: AccessSummary,
    pub capabilities: Vec<String>,
    pub hooks: Vec<String>,
    pub subscriptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRef {
    pub reference: String,
    pub version: Option<String>,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogRef {
    pub name: String,
    pub item_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessSummary {
    pub tenants: u32,
    pub teams: u32,
    pub targets: Vec<AccessTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTarget {
    pub tenant: String,
    pub team_count: u32,
    pub default_policy: String,
}

impl InfoReport {
    /// Project a `.gtbundle` artifact's opened bundle into the info report shape.
    ///
    /// When the bundle was opened from an artifact (`.gtbundle` SquashFS), we
    /// additionally probe each inlined `.gtpack` file for its `manifest.cbor`
    /// and populate the per-pack `version` column. Probing is best-effort:
    /// missing or unreadable pack manifests leave `version = None` rather than
    /// failing the whole command.
    pub fn from_opened_bundle(opened: &OpenedBundle) -> Self {
        let meta = pack_metadata_for(opened);
        project(&opened.manifest, &opened.lock, &meta)
    }

    /// Read a bundle workspace directory and produce the same `InfoReport`.
    ///
    /// Workspace inputs don't have inlined packs, so `version` stays None for
    /// every pack. Users see versions once they build to a `.gtbundle`.
    pub fn from_workspace(path: &Path) -> Result<Self> {
        let report = crate::build::inspect_target(Some(path), None)
            .with_context(|| format!("reading bundle workspace at {}", path.display()))?;
        Ok(project(&report.manifest, &report.lock, &BTreeMap::new()))
    }
}

/// Gather `{ reference -> PackMetaSlim }` for packs referenced by an opened
/// artifact bundle. Returns empty for non-artifact sources.
fn pack_metadata_for(opened: &OpenedBundle) -> BTreeMap<String, PackMetaSlim> {
    if !matches!(opened.source_kind, BundleSourceKind::Artifact) {
        return BTreeMap::new();
    }
    let artifact_path = PathBuf::from(&opened.source_path);
    let mut refs: Vec<&str> = Vec::new();
    for lock in &opened.lock.app_packs {
        refs.push(lock.reference.as_str());
    }
    for lock in &opened.lock.extension_providers {
        refs.push(lock.reference.as_str());
    }
    pack_probe::probe_inlined_packs(&artifact_path, &refs)
}

fn project(
    manifest: &BundleManifest,
    lock: &BundleLock,
    pack_meta: &BTreeMap<String, PackMetaSlim>,
) -> InfoReport {
    InfoReport {
        info_schema_version: 1,
        bundle_id: manifest.bundle_id.clone(),
        name: manifest.bundle_name.clone(),
        version: None,
        description: None,
        mode: manifest.requested_mode.clone(),
        locale: manifest.locale.clone(),
        app_packs: project_packs(&manifest.app_packs, &lock.app_packs, pack_meta),
        extension_providers: project_packs(
            &manifest.extension_providers,
            &lock.extension_providers,
            pack_meta,
        ),
        catalogs: lock
            .catalogs
            .iter()
            .map(|c| CatalogRef {
                name: catalog_display_name(&c.resolved_ref, &c.requested_ref),
                item_count: c.item_count as u32,
            })
            .collect(),
        access: access_summary(&manifest.resolved_targets),
        capabilities: manifest.capabilities.clone(),
        hooks: manifest.hooks.clone(),
        subscriptions: manifest.subscriptions.clone(),
    }
}

fn project_packs(
    names: &[String],
    locks: &[DependencyLock],
    pack_meta: &BTreeMap<String, PackMetaSlim>,
) -> Vec<PackRef> {
    names
        .iter()
        .map(|name| {
            let digest = locks
                .iter()
                .find(|l| l.reference == *name)
                .and_then(|l| l.digest.clone());
            // Look up probed metadata by the manifest-declared reference
            // (matches the key used in `pack_metadata_for`).
            let version = pack_meta.get(name).and_then(|m| m.version.clone());
            PackRef {
                reference: name.clone(),
                version,
                digest,
            }
        })
        .collect()
}

fn catalog_display_name(resolved: &str, requested: &str) -> String {
    if !resolved.is_empty() {
        resolved.to_string()
    } else {
        requested.to_string()
    }
}

fn access_summary(targets: &[BundleResolvedTargetView]) -> AccessSummary {
    use std::collections::BTreeMap;
    let mut per_tenant: BTreeMap<String, (u32, Option<String>)> = BTreeMap::new();
    for t in targets {
        let entry = per_tenant.entry(t.tenant.clone()).or_insert((0, None));
        if t.team.is_some() {
            entry.0 += 1;
        }
        if entry.1.is_none() {
            entry.1 = Some(t.default_policy.clone());
        }
    }
    let teams: u32 = per_tenant.values().map(|(c, _)| *c).sum();
    AccessSummary {
        tenants: per_tenant.len() as u32,
        teams,
        targets: per_tenant
            .into_iter()
            .map(|(tenant, (team_count, pol))| AccessTarget {
                tenant,
                team_count,
                default_policy: pol.unwrap_or_else(|| "public".into()),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_schema_version_one() {
        let report = InfoReport {
            info_schema_version: 1,
            bundle_id: "b".into(),
            name: "b".into(),
            version: None,
            description: None,
            mode: "production".into(),
            locale: "en".into(),
            app_packs: vec![],
            extension_providers: vec![],
            catalogs: vec![],
            access: AccessSummary {
                tenants: 0,
                teams: 0,
                targets: vec![],
            },
            capabilities: vec![],
            hooks: vec![],
            subscriptions: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(v["info_schema_version"], 1);
        assert_eq!(v["mode"], "production");
        assert_eq!(v["locale"], "en");
        assert_eq!(v["access"]["tenants"], 0);
    }

    #[test]
    fn from_opened_bundle_projects_all_fields() {
        use greentic_bundle_reader::{
            BundleLock, BundleManifest, BundleResolvedTargetView, BundleSourceKind,
            CatalogLockEntry, DependencyLock, OpenedBundle,
        };

        let manifest = BundleManifest {
            format_version: "1".into(),
            bundle_id: "acme".into(),
            bundle_name: "acme-demo".into(),
            requested_mode: "production".into(),
            locale: "en".into(),
            artifact_extension: "gtbundle".into(),
            generated_resolved_files: vec![],
            generated_setup_files: vec![],
            app_packs: vec!["hello-bot".into(), "support-bot".into()],
            extension_providers: vec!["slack-provider".into()],
            catalogs: vec![],
            hooks: vec!["on_install".into()],
            subscriptions: vec!["user.created".into()],
            capabilities: vec!["state.kv".into()],
            resolved_targets: vec![
                BundleResolvedTargetView {
                    path: "tenants/default/default.yaml".into(),
                    tenant: "default".into(),
                    team: Some("engineering".into()),
                    default_policy: "public".into(),
                    tenant_gmap: "tenants/default/tenant.gmap".into(),
                    team_gmap: Some("tenants/default/engineering.gmap".into()),
                    app_pack_policies: vec![],
                },
                BundleResolvedTargetView {
                    path: "tenants/default/marketing.yaml".into(),
                    tenant: "default".into(),
                    team: Some("marketing".into()),
                    default_policy: "public".into(),
                    tenant_gmap: "tenants/default/tenant.gmap".into(),
                    team_gmap: Some("tenants/default/marketing.gmap".into()),
                    app_pack_policies: vec![],
                },
                BundleResolvedTargetView {
                    path: "tenants/acme/tenant.yaml".into(),
                    tenant: "acme".into(),
                    team: None,
                    default_policy: "forbidden".into(),
                    tenant_gmap: "tenants/acme/tenant.gmap".into(),
                    team_gmap: None,
                    app_pack_policies: vec![],
                },
            ],
        };

        let lock = BundleLock {
            schema_version: 1,
            bundle_id: "acme".into(),
            requested_mode: "production".into(),
            execution: "default".into(),
            cache_policy: "default".into(),
            tool_version: "0.0.0".into(),
            build_format_version: "1".into(),
            workspace_root: "".into(),
            lock_file: "".into(),
            catalogs: vec![CatalogLockEntry {
                requested_ref: "file://catalog.json".into(),
                resolved_ref: "catalog.json".into(),
                digest: "sha256:abc".into(),
                source: "file".into(),
                item_count: 12,
                item_ids: vec!["hello-bot".into()],
                cache_path: None,
            }],
            app_packs: vec![
                DependencyLock {
                    reference: "hello-bot".into(),
                    digest: Some("sha256:aaa".into()),
                },
                DependencyLock {
                    reference: "support-bot".into(),
                    digest: None,
                },
            ],
            extension_providers: vec![DependencyLock {
                reference: "slack-provider".into(),
                digest: Some("sha256:bbb".into()),
            }],
            setup_state_files: vec![],
        };

        let opened = OpenedBundle {
            source_kind: BundleSourceKind::Artifact,
            source_path: "/tmp/demo.gtbundle".into(),
            format_version: "1".into(),
            manifest,
            lock,
        };

        let r = InfoReport::from_opened_bundle(&opened);

        assert_eq!(r.info_schema_version, 1);
        assert_eq!(r.bundle_id, "acme");
        assert_eq!(r.name, "acme-demo");
        assert_eq!(r.version, None);
        assert_eq!(r.description, None);
        assert_eq!(r.mode, "production");
        assert_eq!(r.locale, "en");

        assert_eq!(r.app_packs.len(), 2);
        assert_eq!(r.app_packs[0].reference, "hello-bot");
        assert_eq!(r.app_packs[0].digest.as_deref(), Some("sha256:aaa"));
        assert_eq!(r.app_packs[1].reference, "support-bot");
        assert_eq!(r.app_packs[1].digest, None);

        assert_eq!(r.extension_providers.len(), 1);
        assert_eq!(r.extension_providers[0].reference, "slack-provider");
        assert_eq!(
            r.extension_providers[0].digest.as_deref(),
            Some("sha256:bbb")
        );

        assert_eq!(r.catalogs.len(), 1);
        assert_eq!(r.catalogs[0].name, "catalog.json");
        assert_eq!(r.catalogs[0].item_count, 12);

        assert_eq!(r.access.tenants, 2);
        assert_eq!(r.access.teams, 2);
        let default_target = r
            .access
            .targets
            .iter()
            .find(|t| t.tenant == "default")
            .unwrap();
        assert_eq!(default_target.team_count, 2);
        assert_eq!(default_target.default_policy, "public");
        let acme_target = r
            .access
            .targets
            .iter()
            .find(|t| t.tenant == "acme")
            .unwrap();
        assert_eq!(acme_target.team_count, 0);
        assert_eq!(acme_target.default_policy, "forbidden");

        assert_eq!(r.capabilities, vec!["state.kv".to_string()]);
        assert_eq!(r.hooks, vec!["on_install".to_string()]);
        assert_eq!(r.subscriptions, vec!["user.created".to_string()]);
    }
}
