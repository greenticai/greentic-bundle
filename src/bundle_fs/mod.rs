mod backhand_writer;
mod native_mksquashfs_writer;
mod native_unsquashfs_reader;

use std::path::Path;

use anyhow::{Result, bail};

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
    use super::{BundleFsReaderKind, BundleFsWriterKind, READER_ENV, WRITER_ENV};

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
