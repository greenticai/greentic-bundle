use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use super::{BundleEntry, BundleEntryKind, BundleFsReader, READER_ENV};

pub struct UnsquashfsBundleFsReader;

impl BundleFsReader for UnsquashfsBundleFsReader {
    fn list_bundle(&self, bundle_file: &Path) -> Result<Vec<BundleEntry>> {
        let output = Command::new("unsquashfs")
            .args(["-ls", bundle_file.to_str().unwrap_or_default()])
            .output()
            .map_err(map_spawn_error)?;
        if !output.status.success() {
            bail!(
                "unsquashfs failed while listing {}: {}",
                bundle_file.display(),
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
            .map(|path| BundleEntry {
                path,
                kind: BundleEntryKind::Other,
            })
            .collect())
    }

    fn extract_bundle(&self, bundle_file: &Path, output_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(output_dir)?;
        let output = Command::new("unsquashfs")
            .args([
                "-no-progress",
                "-quiet",
                "-dest",
                output_dir.to_str().unwrap_or_default(),
                bundle_file.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(map_spawn_error)?;
        if !output.status.success() {
            bail!(
                "unsquashfs failed while extracting {} into {}: {}",
                bundle_file.display(),
                output_dir.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
}

pub fn read_bundle_file_with_unsquashfs(bundle_file: &Path, inner_path: &str) -> Result<Vec<u8>> {
    let output = Command::new("unsquashfs")
        .args(["-cat", bundle_file.to_str().unwrap_or_default(), inner_path])
        .output()
        .map_err(map_spawn_error)?;
    if !output.status.success() {
        bail!(
            "unsquashfs failed for {}:{}: {}",
            bundle_file.display(),
            inner_path,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn map_spawn_error(error: std::io::Error) -> anyhow::Error {
    match error.kind() {
        ErrorKind::NotFound => anyhow::anyhow!(
            "{READER_ENV}=unsquashfs was requested, but unsquashfs was not found. Install squashfs-tools or unset {READER_ENV} to use the Rust-native reader."
        ),
        _ => anyhow::Error::new(error).context("spawn unsquashfs"),
    }
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
    }
}
