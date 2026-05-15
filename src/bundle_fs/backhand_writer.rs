use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use backhand::{FilesystemReader, FilesystemWriter, InnerNode, NodeHeader};
use walkdir::WalkDir;

use super::{BundleEntry, BundleEntryKind, BundleFsReader, BundleFsWriter};

const ROOT_PERMISSIONS: u16 = 0o755;
const DIR_PERMISSIONS: u16 = 0o755;
const FILE_PERMISSIONS: u16 = 0o644;
const SYMLINK_PERMISSIONS: u16 = 0o777;
const NORMALIZED_TIME: u32 = 0;

pub struct BackhandBundleFsWriter;

pub struct BackhandBundleFsReader;

impl BundleFsWriter for BackhandBundleFsWriter {
    fn write_bundle(&self, input_dir: &Path, output_file: &Path) -> Result<()> {
        write_bundle_with_backhand(input_dir, output_file).with_context(|| {
            format!(
                "Failed to create .gtbundle using Rust-native SquashFS writer from {} to {}",
                input_dir.display(),
                output_file.display()
            )
        })
    }
}

impl BundleFsReader for BackhandBundleFsReader {
    fn list_bundle(&self, bundle_file: &Path) -> Result<Vec<BundleEntry>> {
        let filesystem = open_backhand_filesystem(bundle_file)?;
        let mut entries = Vec::new();
        for node in filesystem.files() {
            let Some(path) = normalized_node_path(&node.fullpath)? else {
                continue;
            };
            let kind = match &node.inner {
                InnerNode::File(_) => BundleEntryKind::File,
                InnerNode::Dir(_) => BundleEntryKind::Directory,
                InnerNode::Symlink(_) => BundleEntryKind::Symlink,
                InnerNode::CharacterDevice(_)
                | InnerNode::BlockDevice(_)
                | InnerNode::NamedPipe
                | InnerNode::Socket => BundleEntryKind::Other,
            };
            entries.push(BundleEntry { path, kind });
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn extract_bundle(&self, bundle_file: &Path, output_dir: &Path) -> Result<()> {
        let filesystem = open_backhand_filesystem(bundle_file)?;
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("create extraction directory {}", output_dir.display()))?;
        for node in filesystem.files() {
            let Some(path) = normalized_node_path(&node.fullpath)? else {
                continue;
            };
            let destination = safe_output_path(output_dir, &path)?;
            match &node.inner {
                InnerNode::Dir(_) => {
                    std::fs::create_dir_all(&destination)
                        .with_context(|| format!("create directory {}", destination.display()))?;
                }
                InnerNode::File(file) => {
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent).with_context(|| {
                            format!("create parent directory {}", parent.display())
                        })?;
                    }
                    let mut source = filesystem.file(file).reader();
                    let mut target = File::create(&destination)
                        .with_context(|| format!("create file {}", destination.display()))?;
                    std::io::copy(&mut source, &mut target)
                        .with_context(|| format!("extract file {path}"))?;
                }
                InnerNode::Symlink(symlink) => {
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent).with_context(|| {
                            format!("create parent directory {}", parent.display())
                        })?;
                    }
                    create_symlink(&symlink.link, &destination)
                        .with_context(|| format!("extract symlink {path}"))?;
                }
                InnerNode::CharacterDevice(_)
                | InnerNode::BlockDevice(_)
                | InnerNode::NamedPipe
                | InnerNode::Socket => {
                    bail!("unsupported SquashFS entry type while extracting {path}");
                }
            }
        }
        Ok(())
    }
}

pub fn read_bundle_file_with_backhand(bundle_file: &Path, inner_path: &str) -> Result<Vec<u8>> {
    let filesystem = open_backhand_filesystem(bundle_file)?;
    let normalized_inner = normalize_inner_path(inner_path)?;
    for node in filesystem.files() {
        let Some(path) = normalized_node_path(&node.fullpath)? else {
            continue;
        };
        if path != normalized_inner {
            continue;
        }
        let InnerNode::File(file) = &node.inner else {
            bail!("{inner_path} is not a file in {}", bundle_file.display());
        };
        let mut reader = filesystem.file(file).reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .with_context(|| format!("read {inner_path} from {}", bundle_file.display()))?;
        return Ok(bytes);
    }
    bail!(
        "bundle entry {inner_path} not found in {}",
        bundle_file.display()
    )
}

fn write_bundle_with_backhand(input_dir: &Path, output_file: &Path) -> Result<()> {
    if !input_dir.is_dir() {
        bail!(
            "bundle input directory does not exist: {}",
            input_dir.display()
        );
    }
    super::assert_no_dev_secret_paths(input_dir)?;
    if let Some(parent) = output_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create artifact parent {}", parent.display()))?;
    }
    if output_file.exists() {
        std::fs::remove_file(output_file)
            .with_context(|| format!("remove existing artifact {}", output_file.display()))?;
    }

    let mut writer = FilesystemWriter::default();
    writer.set_time(NORMALIZED_TIME);
    writer.set_root_uid(0);
    writer.set_root_gid(0);
    writer.set_root_mode(ROOT_PERMISSIONS);
    writer.set_only_root_id();
    writer.set_no_padding();
    // mksquashfs wrote compressor options by default. We leave backhand's default
    // XZ compressor/options intact and normalize everything else we control.
    writer.set_emit_compression_options(true);

    for entry in sorted_entries(input_dir)? {
        let relative_path = normalized_relative_path(input_dir, &entry)?;
        let metadata = std::fs::symlink_metadata(&entry)
            .with_context(|| format!("read metadata for {}", entry.display()))?;
        let file_type = metadata.file_type();

        if file_type.is_dir() {
            writer
                .push_dir(&relative_path, header(DIR_PERMISSIONS))
                .with_context(|| format!("add directory {relative_path} to SquashFS"))?;
        } else if file_type.is_file() {
            let file = File::open(&entry)
                .with_context(|| format!("open staged file {}", entry.display()))?;
            writer
                .push_file(file, &relative_path, header(FILE_PERMISSIONS))
                .with_context(|| format!("add file {relative_path} to SquashFS"))?;
        } else if file_type.is_symlink() {
            let target = std::fs::read_link(&entry)
                .with_context(|| format!("read symlink target {}", entry.display()))?;
            let target = normalized_link_target(&target)?;
            writer
                .push_symlink(target, &relative_path, header(SYMLINK_PERMISSIONS))
                .with_context(|| format!("add symlink {relative_path} to SquashFS"))?;
        } else {
            bail!(
                "unsupported staged bundle entry type at {}; only files, directories, and symlinks can be bundled",
                entry.display()
            );
        }
    }

    let mut output = File::create(output_file)
        .with_context(|| format!("create artifact {}", output_file.display()))?;
    writer
        .write(&mut output)
        .with_context(|| format!("write SquashFS artifact {}", output_file.display()))?;
    Ok(())
}

fn open_backhand_filesystem(bundle_file: &Path) -> Result<FilesystemReader<'static>> {
    let file = File::open(bundle_file)
        .with_context(|| format!("open bundle {}", bundle_file.display()))?;
    FilesystemReader::from_reader(BufReader::new(file))
        .with_context(|| format!("read SquashFS bundle {}", bundle_file.display()))
}

fn header(permissions: u16) -> NodeHeader {
    NodeHeader {
        permissions,
        uid: 0,
        gid: 0,
        mtime: NORMALIZED_TIME,
    }
}

fn sorted_entries(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(input_dir)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.with_context(|| format!("walk staged bundle {}", input_dir.display()))?;
        entries.push(entry.into_path());
    }
    entries.sort_by_key(|path| normalized_relative_path(input_dir, path).unwrap_or_default());
    Ok(entries)
}

fn normalized_relative_path(input_dir: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(input_dir).with_context(|| {
        format!(
            "make {} relative to {}",
            path.display(),
            input_dir.display()
        )
    })?;
    normalized_path(relative)
}

fn normalized_link_target(path: &Path) -> Result<String> {
    normalized_path(path)
}

fn normalized_node_path(path: &Path) -> Result<Option<String>> {
    if path == Path::new("/") {
        return Ok(None);
    }
    let stripped = path.strip_prefix("/").unwrap_or(path);
    Ok(Some(normalized_path(stripped)?))
}

fn normalize_inner_path(path: &str) -> Result<String> {
    normalized_path(Path::new(path.trim_matches('/')))
}

fn normalized_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or_else(|| {
                    anyhow!("bundle paths must be valid UTF-8: {}", path.display())
                })?;
                if part.is_empty() {
                    bail!(
                        "bundle path contains an empty component: {}",
                        path.display()
                    );
                }
                parts.push(part.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir => parts.push("..".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                bail!("bundle paths must be relative: {}", path.display());
            }
        }
    }
    if parts.is_empty() {
        bail!("bundle path cannot be empty");
    }
    Ok(parts.join("/"))
}

fn safe_output_path(output_dir: &Path, inner_path: &str) -> Result<PathBuf> {
    let mut out = output_dir.to_path_buf();
    for component in Path::new(inner_path).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("refusing to extract unsafe bundle path: {inner_path}");
            }
        }
    }
    Ok(out)
}

#[cfg(unix)]
fn create_symlink(target: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, destination)
}

#[cfg(windows)]
fn create_symlink(target: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, destination)
}

#[cfg(test)]
mod tests {
    use super::normalized_path;
    use std::path::Path;

    #[test]
    fn normalizes_paths_with_forward_slashes() {
        assert_eq!(
            normalized_path(Path::new("assets/example.txt")).unwrap(),
            "assets/example.txt"
        );
    }
}
