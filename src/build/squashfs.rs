use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

pub fn build_artifact(source_dir: &Path, artifact: &Path) -> Result<()> {
    if let Some(parent) = artifact.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if artifact.exists() {
        std::fs::remove_file(artifact)?;
    }
    let output = Command::new("mksquashfs")
        .arg(source_dir)
        .arg(artifact)
        .args([
            "-noappend",
            "-all-root",
            "-no-progress",
            "-quiet",
            "-processors",
            "1",
            "-fstime",
            "0",
            "-mkfs-time",
            "0",
            "-all-time",
            "0",
            "-sort",
            "/dev/null",
        ])
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `mksquashfs` was not found on PATH; install SquashFS tools to build `.gtbundle` artifacts"
            ),
            _ => anyhow::Error::new(error).context("spawn mksquashfs"),
        })?;
    if !output.status.success() {
        bail!(
            "mksquashfs failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn read_artifact_file(artifact: &Path, inner_path: &str) -> Result<String> {
    let output = Command::new("unsquashfs")
        .args(["-cat", artifact.to_str().unwrap_or_default(), inner_path])
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `unsquashfs` was not found on PATH; install SquashFS tools to read `.gtbundle` artifacts"
            ),
            _ => anyhow::Error::new(error).context("spawn unsquashfs"),
        })?;
    if !output.status.success() {
        bail!(
            "unsquashfs failed for {}:{}: {}",
            artifact.display(),
            inner_path,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}
