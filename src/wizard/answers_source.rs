//! Resolves the `--answers` value into raw document bytes.
//!
//! A local path is read from disk (the historical behaviour). A reference that
//! starts with `oci://` or `ghcr://` is pulled from an OCI registry as a raw
//! JSON artifact — the same artifact the greentic-demo publish pipeline pushes
//! via `oras push --artifact-type application/vnd.greentic.answers.<kind>.v1+json`.
//!
//! The network boundary is the [`AnswersArtifactClient`] trait so the resolution
//! logic can be unit-tested without a registry.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use greentic_distributor_client::{OciPackFetcher, PackFetchOptions};
use tokio::runtime::Runtime;

/// Layer media type for a `create` answer document.
const ANSWERS_CREATE_MEDIA_TYPE: &str = "application/vnd.greentic.answers.create.v1+json";
/// Layer media type for a `setup` answer document.
const ANSWERS_SETUP_MEDIA_TYPE: &str = "application/vnd.greentic.answers.setup.v1+json";

/// GHCR namespace the `ghcr://` shortcut expands into, mirroring the catalog
/// client's `ghcr://` handling.
const GHCR_DEFAULT_NAMESPACE: &str = "ghcr.io/greenticai";

/// Where the `--answers` value came from, plus the bytes to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswersSource {
    /// A local filesystem path (read by the caller with `fs::read_to_string`).
    Local(PathBuf),
    /// A document pulled from an OCI registry. `reference` is the original
    /// user-supplied value, kept for error messages.
    Remote { reference: String, bytes: Vec<u8> },
}

/// Fetches a raw answer-document artifact from an OCI registry.
///
/// Abstracted as a trait so [`resolve_answers_source`] can be tested with a
/// fake that never touches the network.
pub trait AnswersArtifactClient {
    /// Pull the JSON layer bytes for a fully-resolved OCI reference
    /// (scheme already stripped, e.g. `ghcr.io/greenticai/answers/x/create:latest`).
    fn fetch_json(&self, oci_reference: &str) -> Result<Vec<u8>>;
}

/// Production client backed by `greentic_distributor_client`.
pub struct DistributorAnswersClient;

impl AnswersArtifactClient for DistributorAnswersClient {
    fn fetch_json(&self, oci_reference: &str) -> Result<Vec<u8>> {
        let answers_media_types = vec![
            ANSWERS_CREATE_MEDIA_TYPE.to_string(),
            ANSWERS_SETUP_MEDIA_TYPE.to_string(),
        ];
        let fetcher: OciPackFetcher = OciPackFetcher::new(PackFetchOptions {
            allow_tags: true,
            offline: crate::runtime::offline(),
            cache_dir: answers_cache_dir(),
            accepted_layer_media_types: answers_media_types.clone(),
            preferred_layer_media_types: answers_media_types,
            ..PackFetchOptions::default()
        });
        let runtime = Runtime::new().context("create tokio runtime for answers pull")?;
        let bytes = runtime
            .block_on(fetcher.fetch_pack(oci_reference))
            .with_context(|| format!("failed to pull answers document from {oci_reference}"))?;
        Ok(bytes)
    }
}

/// Cache directory for pulled answer artifacts.
fn answers_cache_dir() -> PathBuf {
    std::env::temp_dir()
        .join("greentic-bundle")
        .join("answers-cache")
}

/// True when `raw` names a remote OCI reference rather than a local path.
pub fn is_remote_reference(raw: &str) -> bool {
    raw.starts_with("oci://") || raw.starts_with("ghcr://")
}

/// Resolve a `--answers` value into its bytes-or-path.
///
/// Local paths are returned as [`AnswersSource::Local`] without any I/O. Remote
/// references are pulled via `client` and returned as [`AnswersSource::Remote`].
pub fn resolve_answers_source(
    raw: &str,
    client: &dyn AnswersArtifactClient,
) -> Result<AnswersSource> {
    if !is_remote_reference(raw) {
        return Ok(AnswersSource::Local(PathBuf::from(raw)));
    }
    if crate::runtime::offline() {
        bail!("cannot pull answers document from {raw}: offline mode is enabled");
    }
    let oci_reference = map_answers_reference(raw)?;
    let bytes = client.fetch_json(&oci_reference)?;
    Ok(AnswersSource::Remote {
        reference: raw.to_string(),
        bytes,
    })
}

/// Map a scheme-prefixed reference to a bare OCI reference.
///
/// * `oci://X` -> `X` (verbatim).
/// * `ghcr://path[:tag|@sha256:...]` -> `ghcr.io/greenticai/path[:tag|@sha256:...]`,
///   defaulting to `:latest` when neither a tag nor a digest is present.
fn map_answers_reference(raw: &str) -> Result<String> {
    if let Some(rest) = raw.strip_prefix("oci://") {
        if rest.is_empty() {
            bail!("answers reference {raw} is missing an OCI path");
        }
        return Ok(rest.to_string());
    }
    if let Some(rest) = raw.strip_prefix("ghcr://") {
        let trimmed = rest.trim_start_matches('/');
        if trimmed.is_empty() {
            bail!("answers reference {raw} is missing a GHCR path");
        }
        return Ok(ensure_tag_or_digest(&format!(
            "{GHCR_DEFAULT_NAMESPACE}/{trimmed}"
        )));
    }
    bail!("answers reference {raw} is not a supported remote reference; use oci:// or ghcr://")
}

/// Append `:latest` when a reference carries neither an explicit tag nor a digest.
fn ensure_tag_or_digest(reference: &str) -> String {
    if reference.contains("@sha256:") {
        return reference.to_string();
    }
    let last_segment = reference.rsplit('/').next().unwrap_or(reference);
    if last_segment.contains(':') {
        return reference.to_string();
    }
    format!("{reference}:latest")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Fake client that records the reference it was asked to fetch and returns
    /// canned bytes.
    struct FakeClient {
        bytes: Vec<u8>,
        seen: RefCell<Vec<String>>,
    }

    impl FakeClient {
        fn new(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl AnswersArtifactClient for FakeClient {
        fn fetch_json(&self, oci_reference: &str) -> Result<Vec<u8>> {
            self.seen.borrow_mut().push(oci_reference.to_string());
            Ok(self.bytes.clone())
        }
    }

    #[test]
    fn detects_remote_and_local_references() {
        assert!(is_remote_reference(
            "oci://ghcr.io/greenticai/answers/x/create:latest"
        ));
        assert!(is_remote_reference("ghcr://answers/x/create"));
        assert!(!is_remote_reference("demos/x-create-answers.json"));
        assert!(!is_remote_reference("/abs/path/create.json"));
        assert!(!is_remote_reference("./relative.json"));
    }

    #[test]
    fn oci_reference_passes_through_verbatim() {
        assert_eq!(
            map_answers_reference("oci://ghcr.io/greenticai/answers/quickstart/create:latest")
                .unwrap(),
            "ghcr.io/greenticai/answers/quickstart/create:latest"
        );
    }

    #[test]
    fn ghcr_shortcut_expands_namespace_and_defaults_latest() {
        assert_eq!(
            map_answers_reference("ghcr://answers/quickstart/create").unwrap(),
            "ghcr.io/greenticai/answers/quickstart/create:latest"
        );
    }

    #[test]
    fn ghcr_shortcut_preserves_explicit_tag_and_digest() {
        assert_eq!(
            map_answers_reference("ghcr://answers/quickstart/create:1.2.3").unwrap(),
            "ghcr.io/greenticai/answers/quickstart/create:1.2.3"
        );
        assert_eq!(
            map_answers_reference("ghcr://answers/quickstart/create@sha256:abc123").unwrap(),
            "ghcr.io/greenticai/answers/quickstart/create@sha256:abc123"
        );
    }

    #[test]
    fn rejects_unsupported_and_empty_references() {
        assert!(map_answers_reference("https://example.com/x.json").is_err());
        assert!(map_answers_reference("oci://").is_err());
        assert!(map_answers_reference("ghcr://").is_err());
    }

    #[test]
    fn local_path_resolves_without_touching_client() {
        let client = FakeClient::new(b"unused");
        let source = resolve_answers_source("demos/x-create-answers.json", &client).unwrap();
        assert_eq!(
            source,
            AnswersSource::Local(PathBuf::from("demos/x-create-answers.json"))
        );
        assert!(client.seen.borrow().is_empty());
    }

    #[test]
    fn remote_reference_pulls_bytes_via_mapped_reference() {
        let client = FakeClient::new(br#"{"answers":{}}"#);
        let source = resolve_answers_source("ghcr://answers/quickstart/create", &client).unwrap();
        assert_eq!(
            source,
            AnswersSource::Remote {
                reference: "ghcr://answers/quickstart/create".to_string(),
                bytes: br#"{"answers":{}}"#.to_vec(),
            }
        );
        // The client was called with the fully-resolved OCI reference.
        assert_eq!(
            client.seen.borrow().as_slice(),
            ["ghcr.io/greenticai/answers/quickstart/create:latest"]
        );
    }
}
