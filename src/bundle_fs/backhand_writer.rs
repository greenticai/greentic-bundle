use std::collections::HashSet;
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
        // Phase 0 P0.4: prevent duplicate normalized inner paths from silently
        // overwriting each other. An attacker could otherwise stage a benign
        // entry first to pass any scanner and then overwrite it with a
        // malicious payload before the consumer ever reads the file.
        let mut seen_paths: HashSet<String> = HashSet::new();
        for node in filesystem.files() {
            let Some(path) = normalized_node_path(&node.fullpath)? else {
                continue;
            };
            if !seen_paths.insert(path.clone()) {
                bail!("duplicate bundle entry rejected: {path}");
            }
            let destination = safe_output_path(output_dir, &path)?;
            match &node.inner {
                InnerNode::Dir(_) => {
                    safe_create_dir_all(output_dir, &destination)
                        .with_context(|| format!("create directory {}", destination.display()))?;
                }
                InnerNode::File(file) => {
                    if let Some(parent) = destination.parent() {
                        safe_create_dir_all(output_dir, parent).with_context(|| {
                            format!("create parent directory {}", parent.display())
                        })?;
                    }
                    assert_no_existing_symlink(&destination)
                        .with_context(|| format!("validate file destination {path}"))?;
                    let mut source = filesystem.file(file).reader();
                    let mut target = File::create(&destination)
                        .with_context(|| format!("create file {}", destination.display()))?;
                    std::io::copy(&mut source, &mut target)
                        .with_context(|| format!("extract file {path}"))?;
                }
                InnerNode::Symlink(symlink) => {
                    if let Some(parent) = destination.parent() {
                        safe_create_dir_all(output_dir, parent).with_context(|| {
                            format!("create parent directory {}", parent.display())
                        })?;
                    }
                    assert_no_existing_symlink(&destination)
                        .with_context(|| format!("validate symlink destination {path}"))?;
                    // Phase 0 P0.4: validate the symlink's target string before
                    // we materialize it. The writer accepts symlinks for
                    // legitimate bundle authoring, so we cannot refuse them
                    // outright on the reader — but we must refuse targets that
                    // resolve outside the extract root. `link.link` is the raw
                    // bytes the archive stored; we treat it as a Path and
                    // reject absolute paths, drive-letter prefixes, and
                    // relative paths whose `..` count outruns the depth of
                    // the symlink's own parent.
                    assert_symlink_target_within_root(&path, &symlink.link)
                        .with_context(|| format!("validate symlink target for {path}"))?;
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

// Phase 0 P0.4: walk each ancestor of `target` from the extract root down,
// verifying with no-follow `symlink_metadata` that no existing component is a
// symlink before descending. This is the categorization fix called out in
// the plan: the previous `std::fs::create_dir_all` follows symlinks, so an
// archive could plant `etc-link -> /etc` and then write through it on the
// next iteration. Creates only missing directories under `target`; preserves
// any pre-existing real directory.
fn safe_create_dir_all(extract_root: &Path, target: &Path) -> Result<()> {
    if !target.starts_with(extract_root) {
        bail!(
            "refusing to descend outside extract root: {} not under {}",
            target.display(),
            extract_root.display()
        );
    }
    let relative = target.strip_prefix(extract_root).map_err(|err| {
        anyhow!(
            "make {} relative to extract root {}: {err}",
            target.display(),
            extract_root.display()
        )
    })?;
    let mut current = extract_root.to_path_buf();
    for component in relative.components() {
        let part = match component {
            Component::Normal(part) => part,
            Component::CurDir => continue,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "refusing to traverse unsafe component during mkdir: {}",
                    target.display()
                );
            }
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    bail!(
                        "refusing to descend through symlink at {}",
                        current.display()
                    );
                }
                if !meta.file_type().is_dir() {
                    bail!(
                        "refusing to descend through non-directory at {}",
                        current.display()
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)
                    .with_context(|| format!("create directory {}", current.display()))?;
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("stat {} during safe mkdir", current.display()));
            }
        }
    }
    Ok(())
}

// Phase 0 P0.4: the final write into `destination` would also follow a
// symlink at `destination` itself (not just its ancestors), so refuse if the
// destination already exists as a symlink. Missing path is fine.
fn assert_no_existing_symlink(destination: &Path) -> Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!(
                "refusing to write through existing symlink at {}",
                destination.display()
            );
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

// Phase 0 P0.4: refuse symlink targets that escape the extract root. The
// link target is interpreted relative to the symlink's *parent directory*
// inside the archive — that's how the kernel will resolve it once extracted.
// Reject absolute paths and Windows-style prefixes outright. For relative
// targets, fold `..` against the symlink's depth: if the accumulated depth
// ever goes negative, the symlink can escape.
//
// Example: a symlink at `packs/inner/link` with target `../../../etc/passwd`
// starts at depth 2 (`packs`, `inner`), consumes 3 `..` -> depth -1 -> bail.
fn assert_symlink_target_within_root(symlink_inner_path: &str, target: &Path) -> Result<()> {
    let parent_depth = Path::new(symlink_inner_path)
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter(|component| matches!(component, Component::Normal(_)))
                .count()
        })
        .unwrap_or(0);
    let mut depth: i64 = parent_depth as i64;
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    bail!(
                        "refusing symlink target {} from {}: escapes extract root",
                        target.display(),
                        symlink_inner_path
                    );
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "refusing absolute symlink target {} from {}",
                    target.display(),
                    symlink_inner_path
                );
            }
        }
    }
    Ok(())
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
    use super::{
        assert_no_existing_symlink, assert_symlink_target_within_root, normalized_path,
        safe_create_dir_all,
    };
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn normalizes_paths_with_forward_slashes() {
        assert_eq!(
            normalized_path(Path::new("assets/example.txt")).unwrap(),
            "assets/example.txt"
        );
    }

    // Phase 0 P0.4 — safe_create_dir_all.

    #[test]
    fn safe_create_dir_all_creates_missing_dirs() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        let target = root.join("a/b/c");
        safe_create_dir_all(root, &target).expect("mkdir");
        assert!(target.is_dir());
    }

    #[test]
    fn safe_create_dir_all_accepts_existing_real_dirs() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("a/b")).expect("seed");
        safe_create_dir_all(root, &root.join("a/b/c")).expect("mkdir");
        assert!(root.join("a/b/c").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn safe_create_dir_all_rejects_traversal_through_symlink() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        // Plant a symlink at <root>/escape -> <outside>. A naive
        // create_dir_all(<root>/escape/inner) would resolve through the link.
        std::os::unix::fs::symlink(&outside, root.join("escape")).expect("symlink");
        let err = safe_create_dir_all(root, &root.join("escape/inner"))
            .expect_err("must reject symlink ancestor");
        assert!(
            format!("{err:#}").contains("descend through symlink"),
            "unexpected error: {err:#}"
        );
        // The outside dir must remain empty.
        assert!(!outside.join("inner").exists());
    }

    #[test]
    fn safe_create_dir_all_rejects_target_outside_root() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).expect("root");
        let outside = temp.path().join("outside");
        let err =
            safe_create_dir_all(&root, &outside).expect_err("must reject target outside root");
        assert!(format!("{err:#}").contains("outside extract root"));
    }

    // Phase 0 P0.4 — assert_no_existing_symlink.

    #[cfg(unix)]
    #[test]
    fn assert_no_existing_symlink_rejects_symlink_at_destination() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        std::os::unix::fs::symlink("/tmp/nope", root.join("link")).expect("symlink");
        let err = assert_no_existing_symlink(&root.join("link"))
            .expect_err("must reject existing symlink");
        assert!(format!("{err:#}").contains("write through existing symlink"));
    }

    #[test]
    fn assert_no_existing_symlink_accepts_missing_destination() {
        let temp = TempDir::new().expect("tempdir");
        assert_no_existing_symlink(&temp.path().join("missing")).expect("missing is fine");
    }

    #[test]
    fn assert_no_existing_symlink_accepts_existing_file() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("real.txt");
        std::fs::write(&path, "x").expect("write");
        assert_no_existing_symlink(&path).expect("real file is fine");
    }

    // Phase 0 P0.4 — assert_symlink_target_within_root.

    #[test]
    fn symlink_target_within_root_accepts_sibling() {
        assert_symlink_target_within_root("packs/a/link", Path::new("../b/file"))
            .expect("sibling resolves under root");
    }

    #[test]
    fn symlink_target_within_root_rejects_absolute_target() {
        let err = assert_symlink_target_within_root("packs/link", Path::new("/etc/passwd"))
            .expect_err("must reject absolute");
        assert!(format!("{err:#}").contains("absolute symlink target"));
    }

    #[test]
    fn symlink_target_within_root_rejects_escaping_target() {
        // Symlink at depth 1 (parent = "packs"). `../../etc` consumes 2 `..`
        // → depth -1 → escape.
        let err = assert_symlink_target_within_root("packs/link", Path::new("../../etc"))
            .expect_err("must reject escape");
        assert!(format!("{err:#}").contains("escapes extract root"));
    }

    #[test]
    fn symlink_target_within_root_accepts_walk_back_to_root() {
        // Symlink at depth 2; walking back to depth 0 stays at root and is
        // therefore fine — anything inside the root after `../..` is allowed.
        assert_symlink_target_within_root("packs/inner/link", Path::new("../../allowed/file"))
            .expect("walk back to root is within bounds");
    }
}
