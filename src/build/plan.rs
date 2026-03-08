use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

    let bundle_id =
        find_yaml_scalar(&bundle_yaml, "bundle_id").unwrap_or_else(|| lock.bundle_id.clone());
    let bundle_name =
        find_yaml_scalar(&bundle_yaml, "bundle_name").unwrap_or_else(|| bundle_id.clone());
    let requested_mode =
        find_yaml_scalar(&bundle_yaml, "mode").unwrap_or_else(|| lock.requested_mode.clone());
    let locale = find_yaml_scalar(&bundle_yaml, "locale").unwrap_or_else(|| "en".to_string());
    let app_packs = find_yaml_list(&bundle_yaml, "app_packs");
    let extension_providers = find_yaml_list(&bundle_yaml, "extension_providers");
    let catalogs = find_yaml_list(&bundle_yaml, "remote_catalogs");
    let hooks = find_yaml_list(&bundle_yaml, "hooks");
    let subscriptions = find_yaml_list(&bundle_yaml, "subscriptions");
    let capabilities = find_yaml_list(&bundle_yaml, "capabilities");
    let resolved_files = collect_files(root, &root.join("resolved"))?;
    let setup_files = collect_named_files(root, &lock.setup_state_files)?;
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
    Ok(BuildState {
        root: build_dir.to_path_buf(),
        build_dir: build_dir.to_path_buf(),
        manifest,
        lock,
        bundle_yaml,
        resolved_files,
        setup_files,
    })
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

fn find_yaml_scalar(raw: &str, key: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        (left.trim() == key).then(|| right.trim().to_string())
    })
}

fn find_yaml_list(raw: &str, key: &str) -> Vec<String> {
    let mut lines = raw.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let inline = right.trim();
        if inline == "[]" || inline == "[ ]" {
            return Vec::new();
        }
        if !inline.is_empty() {
            return vec![inline.to_string()];
        }
        let mut items = Vec::new();
        while let Some(next) = lines.peek().copied() {
            let trimmed = next.trim();
            if let Some(value) = trimmed.strip_prefix("- ") {
                items.push(value.trim().to_string());
                lines.next();
            } else {
                break;
            }
        }
        return items;
    }
    Vec::new()
}

fn parse_resolved_target(path: &str, raw: &str) -> Option<ResolvedTargetSummary> {
    let tenant = find_yaml_scalar(raw, "tenant")?;
    let default_policy =
        find_yaml_scalar(raw, "default").unwrap_or_else(|| "forbidden".to_string());
    let tenant_gmap = find_yaml_scalar(raw, "tenant_gmap")?;
    let team_gmap = find_yaml_scalar(raw, "team_gmap");
    let team = find_yaml_scalar(raw, "team");
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
