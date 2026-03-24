pub mod export;
pub mod lock;
pub mod manifest;
pub mod plan;
pub mod squashfs;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use greentic_bundle_reader::{
    BundleLock as ReaderBundleLock, BundleManifest as ReaderBundleManifest, OpenedBundle,
};
use serde::Serialize;
use tempfile::TempDir;

pub const FUTURE_ARTIFACT_EXTENSION: &str = ".gtbundle";
pub const BUILD_STATE_DIR: &str = "state/build";
pub const BUILD_FORMAT_VERSION: &str = "gtbundle-v1";

#[derive(Debug, Clone, Serialize)]
pub struct BuildResult {
    pub artifact_path: String,
    pub build_dir: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub target: String,
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectReport {
    pub target: String,
    pub kind: String,
    pub manifest: ReaderBundleManifest,
    pub lock: ReaderBundleLock,
    pub runtime_surface: greentic_bundle_reader::BundleRuntimeSurface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnbundleResult {
    pub artifact_path: String,
    pub output_dir: String,
}

pub fn build_workspace(root: &Path, output: Option<&Path>, dry_run: bool) -> Result<BuildResult> {
    let state = plan::build_state(root)?;
    let artifact = output
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| default_artifact_path(root, &state.manifest.bundle_id));
    let export_plan = export::export_plan(&state, &artifact);
    if dry_run {
        return Ok(BuildResult {
            artifact_path: export_plan.artifact_path,
            build_dir: export_plan.build_dir,
            manifest_path: export_plan.manifest_path,
        });
    }
    export::write_build_outputs(&state, &artifact)
}

pub fn export_build_dir(build_dir: &Path, output: &Path, dry_run: bool) -> Result<BuildResult> {
    let state = plan::load_build_state(build_dir)?;
    let export_plan = export::export_plan(&state, output);
    if dry_run {
        return Ok(BuildResult {
            artifact_path: export_plan.artifact_path,
            build_dir: export_plan.build_dir,
            manifest_path: export_plan.manifest_path,
        });
    }
    export::write_build_outputs(&state, output)
}

pub fn inspect_target(root: Option<&Path>, artifact: Option<&Path>) -> Result<InspectReport> {
    match (root, artifact) {
        (Some(root), None) => {
            let opened = open_workspace_build_dir(root)?;
            Ok(InspectReport {
                target: root.display().to_string(),
                kind: "workspace".to_string(),
                manifest: opened.manifest.clone(),
                lock: opened.lock.clone(),
                runtime_surface: opened.runtime_surface(),
                contents: None,
            })
        }
        (None, Some(artifact)) => inspect_artifact(artifact),
        _ => bail!("inspect requires exactly one of workspace root or artifact path"),
    }
}

pub fn doctor_target(root: Option<&Path>, artifact: Option<&Path>) -> Result<DoctorReport> {
    match (root, artifact) {
        (Some(root), None) => doctor_workspace(root),
        (None, Some(artifact)) => doctor_artifact(artifact),
        _ => bail!("doctor requires exactly one of workspace root or artifact path"),
    }
}

fn doctor_workspace(root: &Path) -> Result<DoctorReport> {
    let state = plan::build_state(root)?;
    let drift_ok = lock::lock_matches_manifest(&state.lock, &state.manifest);
    let reader_validation = open_workspace_build_dir(root);
    let reader_ok = reader_validation.is_ok();
    let checks = vec![
        DoctorCheck {
            name: "bundle.yaml".to_string(),
            ok: root.join(crate::project::WORKSPACE_ROOT_FILE).exists(),
            details: None,
        },
        DoctorCheck {
            name: "bundle.lock.json".to_string(),
            ok: root.join(crate::project::LOCK_FILE).exists(),
            details: None,
        },
        DoctorCheck {
            name: "lock drift".to_string(),
            ok: drift_ok,
            details: (!drift_ok).then_some(
                "bundle.lock.json does not match current workspace manifest inputs".to_string(),
            ),
        },
        DoctorCheck {
            name: "reader validation".to_string(),
            ok: reader_ok,
            details: if reader_ok {
                Some("workspace manifest/lock satisfy reader contract".to_string())
            } else {
                Some(
                    reader_validation
                        .err()
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| {
                            "workspace manifest/lock do not satisfy reader contract".to_string()
                        }),
                )
            },
        },
    ];
    Ok(DoctorReport {
        target: root.display().to_string(),
        ok: checks.iter().all(|check| check.ok),
        checks,
    })
}

fn doctor_artifact(artifact: &Path) -> Result<DoctorReport> {
    let opened = greentic_bundle_reader::open_artifact(artifact)
        .with_context(|| format!("open artifact {}", artifact.display()))?;
    let checks = vec![
        DoctorCheck {
            name: "artifact exists".to_string(),
            ok: artifact.exists(),
            details: None,
        },
        DoctorCheck {
            name: "manifest embedded".to_string(),
            ok: !opened.manifest.bundle_id.is_empty(),
            details: None,
        },
        DoctorCheck {
            name: "lock embedded".to_string(),
            ok: !opened.lock.bundle_id.is_empty(),
            details: None,
        },
        DoctorCheck {
            name: "reader validation".to_string(),
            ok: true,
            details: Some(format!(
                "{} opened by {} reader",
                opened.format_version,
                opened.source_kind.as_str()
            )),
        },
    ];
    Ok(DoctorReport {
        target: artifact.display().to_string(),
        ok: checks.iter().all(|check| check.ok),
        checks,
    })
}

fn inspect_artifact(artifact: &Path) -> Result<InspectReport> {
    let opened = greentic_bundle_reader::open_artifact(artifact)
        .with_context(|| format!("open artifact {}", artifact.display()))?;
    Ok(InspectReport {
        target: artifact.display().to_string(),
        kind: "artifact".to_string(),
        manifest: opened.manifest.clone(),
        lock: opened.lock.clone(),
        runtime_surface: opened.runtime_surface(),
        contents: Some(squashfs::list_artifact_contents(artifact)?),
    })
}

pub fn unbundle_artifact(artifact: &Path, output_dir: &Path) -> Result<UnbundleResult> {
    squashfs::unpack_artifact(artifact, output_dir)?;
    Ok(UnbundleResult {
        artifact_path: artifact.display().to_string(),
        output_dir: output_dir.display().to_string(),
    })
}

pub fn default_artifact_path(root: &Path, bundle_id: &str) -> PathBuf {
    root.join("dist")
        .join(format!("{bundle_id}{FUTURE_ARTIFACT_EXTENSION}"))
}

fn open_workspace_build_dir(root: &Path) -> Result<OpenedBundle> {
    let state = plan::build_state(root)?;
    let staging_dir = temp_build_dir(&state)?;
    greentic_bundle_reader::open_build_dir_with_source(
        staging_dir.path(),
        root.display().to_string(),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn temp_build_dir(state: &plan::BuildState) -> Result<TempDir> {
    let staging_dir = tempfile::tempdir().context("create temporary normalized build dir")?;
    export::write_normalized_build_dir(state, staging_dir.path())?;
    Ok(staging_dir)
}
