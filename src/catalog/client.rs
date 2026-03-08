use std::path::Path;

use anyhow::{Result, bail};
use greentic_distributor_client::oci_packs::DefaultRegistryClient;
use greentic_distributor_client::{OciPackFetcher, PackFetchOptions};
use tokio::runtime::Runtime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedCatalog {
    pub resolved_ref: String,
    pub digest: String,
    pub bytes: Vec<u8>,
}

pub trait CatalogArtifactClient {
    fn fetch_catalog(&self, root: &Path, reference: &str) -> Result<FetchedCatalog>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CacheOnlyCatalogClient;

impl CatalogArtifactClient for CacheOnlyCatalogClient {
    fn fetch_catalog(&self, _root: &Path, reference: &str) -> Result<FetchedCatalog> {
        bail!(
            "catalog {reference} is not cached locally and this build does not yet provide a remote fetch backend; seed the workspace-local cache first or rerun with a local file:// catalog"
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DistributorCatalogClient;

impl CatalogArtifactClient for DistributorCatalogClient {
    fn fetch_catalog(&self, root: &Path, reference: &str) -> Result<FetchedCatalog> {
        let mapped = map_remote_catalog_reference(reference)?;
        let fetcher: OciPackFetcher<DefaultRegistryClient> =
            OciPackFetcher::new(PackFetchOptions {
                allow_tags: true,
                offline: false,
                cache_dir: root
                    .join(crate::catalog::CACHE_ROOT_DIR)
                    .join("distributor"),
                ..PackFetchOptions::default()
            });
        let runtime = Runtime::new()?;
        let resolved = runtime.block_on(fetcher.fetch_pack_to_cache(&mapped.oci_reference))?;
        let bytes = std::fs::read(&resolved.path)?;
        Ok(FetchedCatalog {
            resolved_ref: format!("oci://{}", mapped.oci_reference),
            digest: resolved.resolved_digest,
            bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCatalogRef {
    pub oci_reference: String,
    pub source_kind: RemoteCatalogSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCatalogSourceKind {
    Oci,
    GhcrWellKnown,
}

pub fn map_remote_catalog_reference(reference: &str) -> Result<RemoteCatalogRef> {
    if let Some(raw) = reference.strip_prefix("oci://") {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("catalog reference {reference} is missing an OCI path");
        }
        return Ok(RemoteCatalogRef {
            oci_reference: trimmed.to_string(),
            source_kind: RemoteCatalogSourceKind::Oci,
        });
    }

    if let Some(raw) = reference.strip_prefix("ghcr://") {
        let trimmed = raw.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            bail!("catalog reference {reference} is missing a GHCR path");
        }
        return Ok(RemoteCatalogRef {
            oci_reference: format!("ghcr.io/greenticai/{}", default_ghcr_tag(trimmed)),
            source_kind: RemoteCatalogSourceKind::GhcrWellKnown,
        });
    }

    bail!(
        "catalog {reference} is not a supported remote catalog ref; use file://, oci://<registry>/<repo>[:tag|@sha256:...], or ghcr://<path>[:tag|@sha256:...] for ghcr.io/greenticai"
    )
}

fn default_ghcr_tag(reference: &str) -> String {
    if reference.contains('@')
        || reference
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .contains(':')
    {
        reference.to_string()
    } else {
        format!("{reference}:latest")
    }
}

#[cfg(test)]
mod tests {
    use super::{RemoteCatalogSourceKind, map_remote_catalog_reference};

    #[test]
    fn maps_ghcr_shortcut_to_greenticai_namespace() {
        let mapped =
            map_remote_catalog_reference("ghcr://packs/well-known-packs.json").expect("mapped");
        assert_eq!(
            mapped.oci_reference,
            "ghcr.io/greenticai/packs/well-known-packs.json:latest"
        );
        assert_eq!(mapped.source_kind, RemoteCatalogSourceKind::GhcrWellKnown);
    }

    #[test]
    fn preserves_explicit_digest_for_ghcr_shortcut() {
        let mapped =
            map_remote_catalog_reference("ghcr://packs/well-known-packs.json@sha256:abc123")
                .expect("mapped");
        assert_eq!(
            mapped.oci_reference,
            "ghcr.io/greenticai/packs/well-known-packs.json@sha256:abc123"
        );
    }

    #[test]
    fn preserves_raw_oci_reference() {
        let mapped =
            map_remote_catalog_reference("oci://ghcr.io/example/catalogs/demo:1").expect("mapped");
        assert_eq!(mapped.oci_reference, "ghcr.io/example/catalogs/demo:1");
        assert_eq!(mapped.source_kind, RemoteCatalogSourceKind::Oci);
    }
}
