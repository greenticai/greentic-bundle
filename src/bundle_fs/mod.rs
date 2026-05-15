mod backhand_writer;
mod native_mksquashfs_writer;
mod native_unsquashfs_reader;

use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

pub use backhand_writer::{BackhandBundleFsReader, BackhandBundleFsWriter};
pub use native_mksquashfs_writer::MksquashfsBundleFsWriter;
pub use native_unsquashfs_reader::UnsquashfsBundleFsReader;

pub const WRITER_ENV: &str = "GREENTIC_BUNDLE_SQUASHFS_WRITER";
pub const READER_ENV: &str = "GREENTIC_BUNDLE_SQUASHFS_READER";

pub trait BundleFsWriter {
    fn write_bundle(&self, input_dir: &Path, output_file: &Path) -> Result<()>;
}

pub trait BundleFsReader {
    fn list_bundle(&self, bundle_file: &Path) -> Result<Vec<BundleEntry>>;
    fn extract_bundle(&self, bundle_file: &Path, output_dir: &Path) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    pub path: String,
    pub kind: BundleEntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleFsWriterKind {
    Backhand,
    Mksquashfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleFsReaderKind {
    Backhand,
    Unsquashfs,
}

impl BundleFsWriterKind {
    pub fn from_env_value(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None => Ok(Self::Backhand),
            Some("backhand") => Ok(Self::Backhand),
            Some("mksquashfs") => Ok(Self::Mksquashfs),
            Some(value) => bail!(
                "{WRITER_ENV}={value} is not supported. Accepted values: backhand, mksquashfs"
            ),
        }
    }
}

impl BundleFsReaderKind {
    pub fn from_env_value(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None => Ok(Self::Backhand),
            Some("backhand") => Ok(Self::Backhand),
            Some("unsquashfs") => Ok(Self::Unsquashfs),
            Some(value) => bail!(
                "{READER_ENV}={value} is not supported. Accepted values: backhand, unsquashfs"
            ),
        }
    }
}

pub fn selected_writer_kind() -> Result<BundleFsWriterKind> {
    BundleFsWriterKind::from_env_value(std::env::var(WRITER_ENV).ok().as_deref())
}

pub fn selected_reader_kind() -> Result<BundleFsReaderKind> {
    BundleFsReaderKind::from_env_value(std::env::var(READER_ENV).ok().as_deref())
}

pub fn write_bundle(input_dir: &Path, output_file: &Path) -> Result<()> {
    match selected_writer_kind()? {
        BundleFsWriterKind::Backhand => BackhandBundleFsWriter.write_bundle(input_dir, output_file),
        BundleFsWriterKind::Mksquashfs => {
            MksquashfsBundleFsWriter.write_bundle(input_dir, output_file)
        }
    }
}

// Phase 0 secret-leak hotfix: defense in depth at the archive boundary.
// build_dir is supposed to be controlled by `write_normalized_build_dir`, but
// any drift that lands a dev-store file or its directory tree inside build_dir
// must not silently ship inside the .gtbundle. We bail loud so the upstream
// pipeline gets fixed instead of leaking. See plans/next-gen-deployment.md P0.1.
pub(crate) fn assert_no_dev_secret_paths(input_dir: &Path) -> Result<()> {
    for entry in WalkDir::new(input_dir).min_depth(1).follow_links(false) {
        let entry = entry.with_context(|| {
            format!(
                "walk staged bundle for dev-secret denylist: {}",
                input_dir.display()
            )
        })?;
        let relative = entry.path().strip_prefix(input_dir).unwrap_or(entry.path());
        if let Some(reason) = dev_secret_match(relative) {
            bail!(
                "refusing to archive dev-secret path {} ({reason}); fix the bundle pipeline rather than shipping it",
                relative.display()
            );
        }
    }
    Ok(())
}

fn dev_secret_match(relative: &Path) -> Option<&'static str> {
    let parts: Vec<&str> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    for window in parts.windows(2) {
        if window[0] == ".greentic" && window[1] == "dev" {
            return Some(".greentic/dev/ tree");
        }
    }
    for window in parts.windows(3) {
        if window[0] == ".greentic" && window[1] == "state" && window[2] == "dev" {
            return Some(".greentic/state/dev/ tree");
        }
    }
    if parts.last().copied() == Some(".dev.secrets.env") {
        return Some(".dev.secrets.env file");
    }
    None
}

pub fn list_bundle(bundle_file: &Path) -> Result<Vec<BundleEntry>> {
    match selected_reader_kind()? {
        BundleFsReaderKind::Backhand => BackhandBundleFsReader.list_bundle(bundle_file),
        BundleFsReaderKind::Unsquashfs => UnsquashfsBundleFsReader.list_bundle(bundle_file),
    }
}

pub fn extract_bundle(bundle_file: &Path, output_dir: &Path) -> Result<()> {
    match selected_reader_kind()? {
        BundleFsReaderKind::Backhand => {
            BackhandBundleFsReader.extract_bundle(bundle_file, output_dir)
        }
        BundleFsReaderKind::Unsquashfs => {
            UnsquashfsBundleFsReader.extract_bundle(bundle_file, output_dir)
        }
    }
}

pub fn read_bundle_file(bundle_file: &Path, inner_path: &str) -> Result<Vec<u8>> {
    match selected_reader_kind()? {
        BundleFsReaderKind::Backhand => {
            backhand_writer::read_bundle_file_with_backhand(bundle_file, inner_path)
        }
        BundleFsReaderKind::Unsquashfs => {
            native_unsquashfs_reader::read_bundle_file_with_unsquashfs(bundle_file, inner_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::{
        BundleFsReaderKind, BundleFsWriterKind, READER_ENV, WRITER_ENV, assert_no_dev_secret_paths,
        dev_secret_match,
    };

    #[test]
    fn dev_secret_match_detects_dev_directory() {
        assert_eq!(
            dev_secret_match(Path::new(".greentic/dev/whatever.bin")),
            Some(".greentic/dev/ tree")
        );
    }

    #[test]
    fn dev_secret_match_detects_state_dev_directory() {
        assert_eq!(
            dev_secret_match(Path::new(".greentic/state/dev/anything")),
            Some(".greentic/state/dev/ tree")
        );
    }

    #[test]
    fn dev_secret_match_detects_dev_secrets_env_file() {
        assert_eq!(
            dev_secret_match(Path::new("nested/path/.dev.secrets.env")),
            Some(".dev.secrets.env file")
        );
    }

    #[test]
    fn dev_secret_match_passes_through_safe_paths() {
        assert_eq!(dev_secret_match(Path::new("packs/pack-a.gtpack")), None);
        assert_eq!(
            dev_secret_match(Path::new("state/setup/provider-a.json")),
            None
        );
    }

    #[test]
    fn assert_denylist_bails_on_dev_store_file() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        let dev_dir = root.join(".greentic/dev");
        std::fs::create_dir_all(&dev_dir).expect("dev dir");
        std::fs::write(dev_dir.join(".dev.secrets.env"), "GTC_SECRET=leaked").expect("seed");
        let err = assert_no_dev_secret_paths(root).expect_err("must bail");
        let msg = format!("{err:#}");
        assert!(msg.contains("refusing to archive"));
        assert!(msg.contains(".greentic/dev"));
    }

    #[test]
    fn assert_denylist_bails_on_stray_dev_secrets_env() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("packs")).expect("dir");
        std::fs::write(root.join("packs/.dev.secrets.env"), "TOKEN=leaked").expect("seed");
        let err = assert_no_dev_secret_paths(root).expect_err("must bail");
        let msg = format!("{err:#}");
        assert!(msg.contains(".dev.secrets.env"));
    }

    #[test]
    fn assert_denylist_passes_on_clean_tree() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("state/setup")).expect("dir");
        std::fs::write(root.join("state/setup/provider-a.json"), "{}").expect("seed");
        assert_no_dev_secret_paths(root).expect("clean tree passes");
    }

    #[test]
    fn writer_selection_defaults_to_backhand() {
        assert_eq!(
            BundleFsWriterKind::from_env_value(None).expect("writer kind"),
            BundleFsWriterKind::Backhand
        );
    }

    #[test]
    fn writer_selection_accepts_backhand() {
        assert_eq!(
            BundleFsWriterKind::from_env_value(Some("backhand")).expect("writer kind"),
            BundleFsWriterKind::Backhand
        );
    }

    #[test]
    fn writer_selection_accepts_mksquashfs() {
        assert_eq!(
            BundleFsWriterKind::from_env_value(Some("mksquashfs")).expect("writer kind"),
            BundleFsWriterKind::Mksquashfs
        );
    }

    #[test]
    fn writer_selection_rejects_unknown_values() {
        let error = BundleFsWriterKind::from_env_value(Some("external")).expect_err("error");
        let message = error.to_string();
        assert!(message.contains(WRITER_ENV));
        assert!(message.contains("backhand, mksquashfs"));
    }

    #[test]
    fn reader_selection_defaults_to_backhand() {
        assert_eq!(
            BundleFsReaderKind::from_env_value(None).expect("reader kind"),
            BundleFsReaderKind::Backhand
        );
    }

    #[test]
    fn reader_selection_accepts_backhand() {
        assert_eq!(
            BundleFsReaderKind::from_env_value(Some("backhand")).expect("reader kind"),
            BundleFsReaderKind::Backhand
        );
    }

    #[test]
    fn reader_selection_accepts_unsquashfs() {
        assert_eq!(
            BundleFsReaderKind::from_env_value(Some("unsquashfs")).expect("reader kind"),
            BundleFsReaderKind::Unsquashfs
        );
    }

    #[test]
    fn reader_selection_rejects_unknown_values() {
        let error = BundleFsReaderKind::from_env_value(Some("external")).expect_err("error");
        let message = error.to_string();
        assert!(message.contains(READER_ENV));
        assert!(message.contains("backhand, unsquashfs"));
    }
}
