use std::fs;
use std::path::Path;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ExportPlan {
    pub artifact_path: String,
    pub build_dir: String,
    pub manifest_path: String,
}

pub fn export_plan(state: &crate::build::plan::BuildState, artifact: &Path) -> ExportPlan {
    ExportPlan {
        artifact_path: artifact.display().to_string(),
        build_dir: state.build_dir.display().to_string(),
        manifest_path: state
            .build_dir
            .join("bundle-manifest.json")
            .display()
            .to_string(),
    }
}

pub fn write_build_outputs(
    state: &crate::build::plan::BuildState,
    artifact: &Path,
) -> Result<crate::build::BuildResult> {
    write_normalized_build_dir(state, &state.build_dir)?;
    crate::build::squashfs::build_artifact(&state.build_dir, artifact)?;
    Ok(crate::build::BuildResult {
        artifact_path: artifact.display().to_string(),
        build_dir: state.build_dir.display().to_string(),
        manifest_path: state
            .build_dir
            .join("bundle-manifest.json")
            .display()
            .to_string(),
    })
}

pub fn write_normalized_build_dir(
    state: &crate::build::plan::BuildState,
    build_dir: &Path,
) -> Result<()> {
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)?;
    }
    fs::create_dir_all(build_dir)?;
    fs::write(
        build_dir.join("bundle-manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&state.manifest)?),
    )?;
    fs::write(
        build_dir.join("bundle-lock.json"),
        format!("{}\n", serde_json::to_string_pretty(&state.lock)?),
    )?;
    fs::write(build_dir.join("bundle.yaml"), &state.bundle_yaml)?;
    for (name, contents) in &state.resolved_files {
        let path = build_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
    }
    for (name, contents) in &state.setup_files {
        let path = build_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
    }
    Ok(())
}
