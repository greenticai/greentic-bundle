use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml_bw::Value;

use super::manifest::{BundleManifest, ResolvedReferencePolicy, ResolvedTargetSummary};

#[derive(Debug, Clone)]
pub struct BuildState {
    pub root: PathBuf,
    pub build_dir: PathBuf,
    pub manifest: BundleManifest,
    pub lock: crate::project::BundleLock,
    pub bundle_yaml: String,
    pub resolved_files: Vec<(String, String)>,
    pub setup_files: Vec<(String, String)>,
    pub asset_files: Vec<(String, Vec<u8>)>,
}

pub fn build_state(root: &Path) -> Result<BuildState> {
    let lock = crate::project::read_bundle_lock(root)
        .with_context(|| format!("read {}", root.join(crate::project::LOCK_FILE).display()))?;
    let bundle_yaml = fs::read_to_string(root.join(crate::project::WORKSPACE_ROOT_FILE))
        .with_context(|| {
            format!(
                "read {}",
                root.join(crate::project::WORKSPACE_ROOT_FILE).display()
            )
        })?;

    let bundle_doc = parse_yaml_document(&bundle_yaml);
    let bundle_id = yaml_string(&bundle_doc, "bundle_id").unwrap_or_else(|| lock.bundle_id.clone());
    let bundle_name = yaml_string(&bundle_doc, "bundle_name").unwrap_or_else(|| bundle_id.clone());
    let requested_mode =
        yaml_string(&bundle_doc, "mode").unwrap_or_else(|| lock.requested_mode.clone());
    let locale = yaml_string(&bundle_doc, "locale").unwrap_or_else(|| "en".to_string());
    let app_packs = yaml_string_list(&bundle_doc, "app_packs");
    let extension_providers = yaml_string_list(&bundle_doc, "extension_providers");
    let catalogs = yaml_string_list(&bundle_doc, "remote_catalogs");
    let hooks = yaml_string_list(&bundle_doc, "hooks");
    let subscriptions = yaml_string_list(&bundle_doc, "subscriptions");
    let capabilities = yaml_string_list(&bundle_doc, "capabilities");
    let resolved_files = collect_files(root, &root.join("resolved"))?;
    let setup_files = collect_named_files(root, &lock.setup_state_files)?;
    let asset_files = collect_asset_files(root)?;
    let resolved_targets = resolved_files
        .iter()
        .filter_map(|(name, contents)| parse_resolved_target(name, contents))
        .collect();

    let manifest = BundleManifest {
        format_version: crate::build::BUILD_FORMAT_VERSION.to_string(),
        bundle_id,
        bundle_name,
        requested_mode,
        locale,
        artifact_extension: crate::build::FUTURE_ARTIFACT_EXTENSION.to_string(),
        generated_resolved_files: resolved_files
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
        generated_setup_files: setup_files.iter().map(|(name, _)| name.clone()).collect(),
        app_packs,
        extension_providers,
        catalogs,
        hooks,
        subscriptions,
        capabilities,
        resolved_targets,
    };

    let build_dir = root
        .join(crate::build::BUILD_STATE_DIR)
        .join(&manifest.bundle_id)
        .join("normalized");
    Ok(BuildState {
        root: root.to_path_buf(),
        build_dir,
        manifest,
        lock,
        bundle_yaml,
        resolved_files,
        setup_files,
        asset_files,
    })
}

pub fn load_build_state(build_dir: &Path) -> Result<BuildState> {
    let manifest_raw = fs::read_to_string(build_dir.join("bundle-manifest.json"))?;
    let lock_raw = fs::read_to_string(build_dir.join("bundle-lock.json"))?;
    let bundle_yaml = fs::read_to_string(build_dir.join("bundle.yaml"))?;
    let manifest = serde_json::from_str::<BundleManifest>(&manifest_raw)?;
    let lock = serde_json::from_str::<crate::project::BundleLock>(&lock_raw)?;
    let resolved_files = collect_files(build_dir, &build_dir.join("resolved"))?;
    let setup_files = collect_files(build_dir, &build_dir.join("state").join("setup"))?;
    let asset_files = collect_asset_files(build_dir)?;
    Ok(BuildState {
        root: build_dir.to_path_buf(),
        build_dir: build_dir.to_path_buf(),
        manifest,
        lock,
        bundle_yaml,
        resolved_files,
        setup_files,
        asset_files,
    })
}

fn collect_asset_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for relative_root in ["packs", "providers", "tenants"] {
        let dir = root.join(relative_root);
        if !dir.exists() {
            continue;
        }
        for entry in walk(&dir)? {
            if entry.extension().and_then(|value| value.to_str()) != Some("gtpack") {
                continue;
            }
            let rel = entry
                .strip_prefix(root)
                .unwrap_or(&entry)
                .display()
                .to_string();
            files.push((rel, fs::read(&entry)?));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|left, right| left.0 == right.0);
    Ok(files)
}

fn collect_files(root: &Path, dir: &Path) -> Result<Vec<(String, String)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in walk(dir)? {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .display()
            .to_string();
        let contents = fs::read_to_string(&entry)?;
        files.push((rel, contents));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn collect_named_files(root: &Path, names: &[String]) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    for name in names {
        let path = root.join(name);
        if path.exists() {
            files.push((name.clone(), fs::read_to_string(path)?));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn walk(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn parse_yaml_document(raw: &str) -> Option<Value> {
    serde_yaml_bw::from_str(raw).ok()
}

fn yaml_string(doc: &Option<Value>, key: &str) -> Option<String> {
    doc.as_ref()?.get(key)?.as_str().map(ToOwned::to_owned)
}

fn yaml_string_list(doc: &Option<Value>, key: &str) -> Vec<String> {
    match doc.as_ref().and_then(|value| value.get(key)) {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(Value::Null(_)) | None => Vec::new(),
        Some(Value::String(value, _)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn parse_resolved_target(path: &str, raw: &str) -> Option<ResolvedTargetSummary> {
    let doc = parse_yaml_document(raw)?;
    let tenant = doc.get("tenant")?.as_str()?.to_string();
    let default_policy = doc
        .get("policy")
        .and_then(|value| value.get("default"))
        .and_then(Value::as_str)
        .unwrap_or("forbidden")
        .to_string();
    let tenant_gmap = doc
        .get("policy")
        .and_then(|value| value.get("source"))
        .and_then(|value| value.get("tenant_gmap"))
        .and_then(Value::as_str)?
        .to_string();
    let team_gmap = doc
        .get("policy")
        .and_then(|value| value.get("source"))
        .and_then(|value| value.get("team_gmap"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let team = doc
        .get("team")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Some(ResolvedTargetSummary {
        path: path.to_string(),
        tenant,
        team,
        default_policy,
        tenant_gmap,
        team_gmap,
        app_pack_policies: parse_reference_policies(raw),
    })
}

fn parse_reference_policies(raw: &str) -> Vec<ResolvedReferencePolicy> {
    let mut lines = raw.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((left, _)) = line.split_once(':') else {
            continue;
        };
        if left.trim() != "app_packs" {
            continue;
        }

        let mut entries = Vec::new();
        while let Some(next) = lines.peek().copied() {
            let trimmed = next.trim();
            if trimmed == "[]" {
                lines.next();
                break;
            }
            let Some(reference) = trimmed.strip_prefix("- reference:") else {
                break;
            };
            let reference = reference.trim().to_string();
            lines.next();

            let mut policy = "unset".to_string();
            if let Some(policy_line) = lines.peek().copied()
                && let Some(value) = policy_line.trim().strip_prefix("policy:")
            {
                policy = value.trim().to_string();
                lines.next();
            }
            entries.push(ResolvedReferencePolicy { reference, policy });
        }
        return entries;
    }
    Vec::new()
}
