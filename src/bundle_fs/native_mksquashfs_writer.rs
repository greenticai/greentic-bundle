use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use super::{BundleFsWriter, WRITER_ENV};

pub struct MksquashfsBundleFsWriter;

impl BundleFsWriter for MksquashfsBundleFsWriter {
    fn write_bundle(&self, input_dir: &Path, output_file: &Path) -> Result<()> {
        if let Some(parent) = output_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if output_file.exists() {
            std::fs::remove_file(output_file)?;
        }
        super::assert_no_dev_secret_paths(input_dir)?;
        let output = Command::new("mksquashfs")
            .arg(input_dir)
            .arg(output_file)
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
                    "{WRITER_ENV}=mksquashfs was requested, but mksquashfs was not found. Install squashfs-tools or unset {WRITER_ENV} to use the Rust-native writer."
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
}
