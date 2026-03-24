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

pub fn list_artifact_contents(artifact: &Path) -> Result<Vec<String>> {
    let output = Command::new("unsquashfs")
        .args(["-ls", artifact.to_str().unwrap_or_default()])
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `unsquashfs` was not found on PATH; install SquashFS tools to read `.gtbundle` artifacts"
            ),
            _ => anyhow::Error::new(error).context("spawn unsquashfs"),
        })?;
    if !output.status.success() {
        bail!(
            "unsquashfs failed while listing {}: {}",
            artifact.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_unsquashfs_list_entry)
        .filter(|entry| !entry.is_empty() && entry != ".")
        .collect())
}

pub fn unpack_artifact(artifact: &Path, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let output = Command::new("unsquashfs")
        .args([
            "-no-progress",
            "-quiet",
            "-dest",
            output_dir.to_str().unwrap_or_default(),
            artifact.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `unsquashfs` was not found on PATH; install SquashFS tools to read `.gtbundle` artifacts"
            ),
            _ => anyhow::Error::new(error).context("spawn unsquashfs"),
        })?;
    if !output.status.success() {
        bail!(
            "unsquashfs failed while extracting {} into {}: {}",
            artifact.display(),
            output_dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn normalize_unsquashfs_list_entry(line: &str) -> String {
    let trimmed = line.trim_matches('/');
    let Some(rest) = trimmed.strip_prefix("squashfs-root") else {
        return trimmed.to_string();
    };
    rest.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_unsquashfs_list_entry;

    #[test]
    fn normalizes_unsquashfs_root_entries() {
        assert_eq!(normalize_unsquashfs_list_entry("squashfs-root"), "");
        assert_eq!(
            normalize_unsquashfs_list_entry("squashfs-root/bundle.yaml"),
            "bundle.yaml"
        );
        assert_eq!(
            normalize_unsquashfs_list_entry("squashfs-root/resolved/default.yaml"),
            "resolved/default.yaml"
        );
    }
}
