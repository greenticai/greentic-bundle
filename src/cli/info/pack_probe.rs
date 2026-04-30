//! Best-effort probe of pack metadata for inlined `.gtpack` files inside a
//! `.gtbundle` SquashFS artifact.
//!
//! Given a bundle artifact path and a list of pack references (as they appear
//! in `bundle-manifest.json` / `bundle-lock.json`), this module:
//!
//! 1. Lists the `.gtpack` files inlined in the SquashFS via `unsquashfs -l`.
//! 2. Matches each reference to one of those files by pack slug.
//! 3. Extracts `manifest.cbor` from the matched pack with `unsquashfs -cat`.
//! 4. Decodes the manifest and returns `{ name, version }`.
//!
//! Every step is best-effort: on any failure we log a `tracing::warn!` and the
//! caller keeps `version = None`. This keeps `greentic-bundle info` useful even
//! when `unsquashfs` is unavailable or a pack reference doesn't match any
//! inlined file (e.g. the bundle was built by an older builder, or the pack
//! was skipped).
//!
//! The mapping is intentionally conservative: if multiple inlined packs could
//! match one reference, we return `None` instead of guessing. Users get "no
//! version shown" rather than a wrong version.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use tracing::warn;

/// Minimal projection of `greentic-pack`'s `PackMeta` — only the `version`
/// field is surfaced in `info`. Keeping this local avoids pulling
/// `greentic-pack` (and its full WASM toolchain dep graph) into
/// `greentic-bundle` just to read one string. `#[serde(default)]` +
/// `Option` means extra / missing fields never fail decoding — if a future
/// pack format drops `version` entirely, the probe just returns `None`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PackMetaSlim {
    #[serde(default)]
    pub version: Option<String>,
}

/// Extract metadata for a set of pack references from an artifact bundle.
///
/// Returns a map keyed by the original reference string. Absent keys mean
/// either the reference had no matching inlined `.gtpack` file, or decoding
/// failed. The caller should treat every lookup as optional.
pub(crate) fn probe_inlined_packs(
    artifact: &Path,
    references: &[&str],
) -> BTreeMap<String, PackMetaSlim> {
    let mut out = BTreeMap::new();
    if references.is_empty() {
        return out;
    }

    let files = match list_gtpack_files(artifact) {
        Ok(files) => files,
        Err(err) => {
            warn!(
                bundle = %artifact.display(),
                error = %err,
                "could not list .gtpack files in bundle; pack versions will be unavailable",
            );
            return out;
        }
    };

    for reference in references {
        let slug = slug_for_reference(reference);
        let Some(inner_path) = match_pack_file(&files, &slug) else {
            // No matching inlined pack — expected for purely external refs.
            continue;
        };
        match extract_pack_metadata(artifact, inner_path) {
            Ok(Some(meta)) => {
                out.insert((*reference).to_string(), meta);
            }
            Ok(None) => {
                // manifest.cbor not present in the pack — odd but not fatal.
                warn!(
                    bundle = %artifact.display(),
                    pack = %inner_path,
                    "pack has no manifest.cbor entry",
                );
            }
            Err(err) => {
                warn!(
                    bundle = %artifact.display(),
                    pack = %inner_path,
                    error = %err,
                    "could not read pack manifest; version unavailable",
                );
            }
        }
    }

    out
}

/// Extract `manifest.cbor` from an inlined `.gtpack` at `pack_inner_path`
/// inside `artifact` and decode it.
///
/// Returns `Ok(None)` if the pack has no `manifest.cbor` entry. Returns
/// `Err` for I/O / decode failures.
pub(crate) fn extract_pack_metadata(
    artifact: &Path,
    pack_inner_path: &str,
) -> anyhow::Result<Option<PackMetaSlim>> {
    let zip_bytes = unsquashfs_cat_bytes(artifact, pack_inner_path)?;
    let reader = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| anyhow::anyhow!("open .gtpack zip: {e}"))?;
    let mut cbor_bytes = Vec::new();
    {
        let mut entry = match archive.by_name("manifest.cbor") {
            Ok(e) => e,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(anyhow::anyhow!("open manifest.cbor: {e}")),
        };
        entry
            .read_to_end(&mut cbor_bytes)
            .map_err(|e| anyhow::anyhow!("read manifest.cbor: {e}"))?;
    }
    let meta: PackMetaSlim = ciborium::de::from_reader(cbor_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode manifest.cbor: {e}"))?;
    Ok(Some(meta))
}

/// Enumerate `.gtpack` files inside the SquashFS via `unsquashfs -l`.
///
/// Returns paths as they appear in the archive (e.g.
/// `providers/messaging/messaging-webchat-gui.gtpack`), stripping the leading
/// `squashfs-root/` that `unsquashfs -l` prepends.
fn list_gtpack_files(artifact: &Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("unsquashfs")
        .args(["-l", artifact.to_str().unwrap_or_default()])
        .output()
        .map_err(|e| anyhow::anyhow!("spawn unsquashfs -l: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "unsquashfs -l failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut files = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if !trimmed.ends_with(".gtpack") {
            continue;
        }
        // `unsquashfs -l` lines look like: `squashfs-root/providers/messaging/x.gtpack`
        let stripped = trimmed
            .strip_prefix("squashfs-root/")
            .unwrap_or(trimmed)
            .to_string();
        files.push(stripped);
    }
    Ok(files)
}

/// Run `unsquashfs -cat <artifact> <inner>` and return raw stdout bytes.
fn unsquashfs_cat_bytes(artifact: &Path, inner_path: &str) -> anyhow::Result<Vec<u8>> {
    let output = Command::new("unsquashfs")
        .args(["-cat", artifact.to_str().unwrap_or_default(), inner_path])
        .output()
        .map_err(|e| anyhow::anyhow!("spawn unsquashfs -cat: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "unsquashfs -cat {} failed: {}",
            inner_path,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Compute a "pack slug" from a dependency reference so we can match it to an
/// inlined SquashFS filename.
///
/// Handles:
/// - OCI refs: `oci://ghcr.io/org/packs/messaging/foo:latest` → `foo`
/// - HTTP(S) URLs: `https://.../foo.gtpack` → `foo`
/// - Bare names: `foo` → `foo`
/// - Path-like refs: `./foo.gtpack` → `foo`
fn slug_for_reference(reference: &str) -> String {
    // Drop any scheme prefix (oci://, https://, file://, ...).
    let without_scheme = match reference.find("://") {
        Some(idx) => &reference[idx + 3..],
        None => reference,
    };
    // Take last path segment.
    let last_segment = without_scheme
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(without_scheme);
    // Strip `:tag` (OCI tag) — only the last `:`, to avoid breaking digests
    // which are preceded by `@` not `:`.
    let without_tag = match last_segment.rsplit_once(':') {
        Some((head, _tag)) => head,
        None => last_segment,
    };
    // Strip `.gtpack` extension if present.
    without_tag
        .strip_suffix(".gtpack")
        .unwrap_or(without_tag)
        .to_string()
}

/// Find the inlined pack file matching `slug`.
///
/// Strategy: the file's basename (without `.gtpack`) must equal `slug`. If
/// exactly one file matches, return it. If zero or multiple files match,
/// return `None` (conservative — better null than wrong).
fn match_pack_file<'a>(files: &'a [String], slug: &str) -> Option<&'a str> {
    if slug.is_empty() {
        return None;
    }
    let mut best: Option<&str> = None;
    for f in files {
        let basename = f.rsplit('/').next().unwrap_or(f.as_str());
        let stem = basename.strip_suffix(".gtpack").unwrap_or(basename);
        if stem == slug {
            if best.is_some() {
                // Ambiguous — multiple files with same stem; bail out.
                return None;
            }
            best = Some(f.as_str());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    use super::*;

    static PATH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct PathGuard(Option<OsString>);

    impl Drop for PathGuard {
        fn drop(&mut self) {
            if let Some(path) = self.0.take() {
                unsafe {
                    std::env::set_var("PATH", path);
                }
            } else {
                unsafe {
                    std::env::remove_var("PATH");
                }
            }
        }
    }

    fn prepend_path(dir: &Path) -> PathGuard {
        let original = std::env::var_os("PATH");
        let joined = match &original {
            Some(path) => std::env::join_paths(
                std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(path)),
            )
            .expect("join PATH entries"),
            None => OsString::from(dir.as_os_str()),
        };
        unsafe {
            std::env::set_var("PATH", joined);
        }
        PathGuard(original)
    }

    fn install_fake_unsquashfs(tempdir: &Path, script: &str) -> PathBuf {
        let path = tempdir.join("unsquashfs");
        fs::write(&path, script).expect("write fake unsquashfs");
        let mut permissions = fs::metadata(&path)
            .expect("stat fake unsquashfs")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod fake unsquashfs");
        path
    }

    fn make_gtpack_with_manifest(version: Option<&str>) -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct Manifest<'a> {
            version: Option<&'a str>,
        }

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("manifest.cbor", zip::write::FileOptions::<()>::default())
            .expect("create manifest entry");
        let mut cbor = Vec::new();
        ciborium::ser::into_writer(&Manifest { version }, &mut cbor).expect("encode manifest");
        writer.write_all(&cbor).expect("write manifest bytes");
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn slug_strips_oci_scheme_and_tag() {
        assert_eq!(
            slug_for_reference(
                "oci://ghcr.io/greenticai/packs/messaging/messaging-webchat-gui:latest"
            ),
            "messaging-webchat-gui"
        );
    }

    #[test]
    fn slug_strips_https_and_gtpack_extension() {
        assert_eq!(
            slug_for_reference(
                "https://github.com/greenticai/greentic-demo/releases/latest/download/hr-onboarding.gtpack"
            ),
            "hr-onboarding"
        );
    }

    #[test]
    fn slug_handles_bare_name() {
        assert_eq!(slug_for_reference("foo"), "foo");
        assert_eq!(slug_for_reference("foo:1.2.3"), "foo");
    }

    #[test]
    fn slug_handles_path_reference() {
        assert_eq!(slug_for_reference("./local/bar.gtpack"), "bar");
        assert_eq!(slug_for_reference("packs/baz.gtpack"), "baz");
    }

    #[test]
    fn match_pack_file_finds_unique_match() {
        let files = vec![
            "packs/hr-onboarding.gtpack".to_string(),
            "providers/messaging/messaging-webchat-gui.gtpack".to_string(),
            "providers/state/state-memory.gtpack".to_string(),
        ];
        assert_eq!(
            match_pack_file(&files, "messaging-webchat-gui"),
            Some("providers/messaging/messaging-webchat-gui.gtpack")
        );
        assert_eq!(
            match_pack_file(&files, "hr-onboarding"),
            Some("packs/hr-onboarding.gtpack")
        );
    }

    #[test]
    fn match_pack_file_returns_none_for_miss() {
        let files = vec!["packs/foo.gtpack".to_string()];
        assert_eq!(match_pack_file(&files, "missing"), None);
    }

    #[test]
    fn match_pack_file_returns_none_for_ambiguous() {
        let files = vec!["a/foo.gtpack".to_string(), "b/foo.gtpack".to_string()];
        assert_eq!(match_pack_file(&files, "foo"), None);
    }

    #[test]
    fn match_pack_file_returns_none_for_empty_slug() {
        let files = vec!["packs/foo.gtpack".to_string()];
        assert_eq!(match_pack_file(&files, ""), None);
    }

    #[test]
    fn probe_inlined_packs_returns_empty_when_no_references_are_requested() {
        assert!(probe_inlined_packs(Path::new("ignored.gtbundle"), &[]).is_empty());
    }

    #[test]
    fn list_gtpack_files_filters_non_gtpacks_and_strips_squashfs_prefix() {
        let _lock = PATH_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock PATH");
        let tempdir = tempfile::tempdir().expect("create temp dir");
        install_fake_unsquashfs(
            tempdir.path(),
            r#"#!/bin/sh
if [ "$1" = "-l" ]; then
  cat <<'EOF'
squashfs-root/providers/messaging/messaging-webchat-gui.gtpack
squashfs-root/providers/state/state-memory.gtpack
squashfs-root/notes.txt
EOF
else
  echo "unexpected args: $@" >&2
  exit 1
fi
"#,
        );
        let _path_guard = prepend_path(tempdir.path());

        let files = list_gtpack_files(Path::new("demo.gtbundle")).expect("list .gtpack files");

        assert_eq!(
            files,
            vec![
                "providers/messaging/messaging-webchat-gui.gtpack".to_string(),
                "providers/state/state-memory.gtpack".to_string(),
            ]
        );
    }

    #[test]
    fn probe_inlined_packs_reads_manifest_versions_via_unsquashfs() {
        let _lock = PATH_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock PATH");
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let pack_path = tempdir.path().join("messaging-webchat-gui.gtpack");
        fs::write(&pack_path, make_gtpack_with_manifest(Some("1.2.3"))).expect("write .gtpack");
        install_fake_unsquashfs(
            tempdir.path(),
            &format!(
                r#"#!/bin/sh
if [ "$1" = "-l" ]; then
  cat <<'EOF'
squashfs-root/providers/messaging/messaging-webchat-gui.gtpack
EOF
elif [ "$1" = "-cat" ]; then
  cat "{pack_path}"
else
  echo "unexpected args: $@" >&2
  exit 1
fi
"#,
                pack_path = pack_path.display()
            ),
        );
        let _path_guard = prepend_path(tempdir.path());

        let refs = ["oci://ghcr.io/greenticai/packs/messaging/messaging-webchat-gui:latest"];
        let meta = probe_inlined_packs(Path::new("demo.gtbundle"), &refs);

        assert_eq!(meta.len(), 1);
        assert_eq!(
            meta.get(refs[0]).and_then(|entry| entry.version.as_deref()),
            Some("1.2.3")
        );
    }

    #[test]
    fn extract_pack_metadata_returns_none_when_manifest_is_missing() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("readme.txt", zip::write::FileOptions::<()>::default())
            .expect("create readme entry");
        writer.write_all(b"hello").expect("write readme");
        let bytes = writer.finish().expect("finish zip").into_inner();

        let _lock = PATH_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock PATH");
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let pack_path = tempdir.path().join("missing-manifest.gtpack");
        fs::write(&pack_path, bytes).expect("write .gtpack");
        install_fake_unsquashfs(
            tempdir.path(),
            &format!(
                r#"#!/bin/sh
if [ "$1" = "-cat" ]; then
  cat "{pack_path}"
else
  echo "unexpected args: $@" >&2
  exit 1
fi
"#,
                pack_path = pack_path.display()
            ),
        );
        let _path_guard = prepend_path(tempdir.path());

        assert!(
            extract_pack_metadata(Path::new("demo.gtbundle"), "packs/missing-manifest.gtpack")
                .expect("missing manifest is non-fatal")
                .is_none()
        );
    }

    #[test]
    fn extract_pack_metadata_reports_decode_errors() {
        let _lock = PATH_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock PATH");
        let tempdir = tempfile::tempdir().expect("create temp dir");
        let pack_path = tempdir.path().join("broken.gtpack");
        fs::write(&pack_path, b"not-a-zip").expect("write invalid .gtpack");
        install_fake_unsquashfs(
            tempdir.path(),
            &format!(
                r#"#!/bin/sh
if [ "$1" = "-cat" ]; then
  cat "{pack_path}"
else
  echo "unexpected args: $@" >&2
  exit 1
fi
"#,
                pack_path = pack_path.display()
            ),
        );
        let _path_guard = prepend_path(tempdir.path());

        let err = extract_pack_metadata(Path::new("demo.gtbundle"), "packs/broken.gtpack")
            .expect_err("invalid zip should fail");
        assert!(err.to_string().contains("open .gtpack zip"));
    }
}
