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
/// - OCI refs: `oci://ghcr.io/org/packs/messaging/foo:stable` → `foo`
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
    use super::*;

    #[test]
    fn slug_strips_oci_scheme_and_tag() {
        assert_eq!(
            slug_for_reference(
                "oci://ghcr.io/greenticai/packs/messaging/messaging-webchat-gui:stable"
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
}
