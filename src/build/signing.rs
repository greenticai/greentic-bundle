//! C2 — DSSE+Ed25519 artifact signer for `.gtbundle` outputs.
//!
//! Reuses the cryptographic primitive in
//! [`greentic_distributor_client::signing`] and emits a single-signature DSSE
//! envelope alongside the artifact as `<artifact>.sig`. The envelope wraps an
//! in-toto Statement v1 whose subject pins the artifact's SHA-256 digest and
//! whose predicate is a minimal SLSA-provenance v1 document.
//!
//! Phase B scope: signature *authenticity* only. KMS-backed keys, Rekor
//! transparency-log submission, and full provenance materials belong to the
//! Trust plan.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use greentic_distributor_client::signing::{
    InTotoStatement, SlsaProvenance, key_id_for_public_key_pem, sign_statement,
};
use sha2::{Digest, Sha256};

/// Configuration for the bundle signer.
#[derive(Debug, Clone)]
pub struct SigningConfig {
    /// Path to the Ed25519 PKCS#8 PEM private key.
    pub signing_key_path: PathBuf,
    /// Explicit `keyid` written into the DSSE signature. When absent, the id
    /// is derived from a sibling `<key>.pub` SPKI PEM (lowercase hex of the
    /// first 16 bytes of `SHA-256(raw 32-byte public key)`).
    pub key_id_override: Option<String>,
    /// SLSA `builder.id`. Defaults to `greentic-bundle:<package version>`.
    pub builder_id: Option<String>,
    /// Override of the output signature path. Default: `<artifact>.sig`.
    pub signature_path_override: Option<PathBuf>,
}

/// SLSA `build_type` discriminator for a `.gtbundle` artifact.
pub const BUNDLE_BUILD_TYPE: &str = "gtbundle";

const SIGNATURE_SUFFIX: &str = ".sig";
const PUBLIC_KEY_SUFFIX: &str = ".pub";

/// Sign the artifact at `artifact`, writing a DSSE envelope sidecar at the
/// resolved signature path. Returns the path that was written.
pub fn sign_artifact(artifact: &Path, cfg: &SigningConfig) -> Result<PathBuf> {
    let bytes = fs::read(artifact)
        .with_context(|| format!("read artifact for signing: {}", artifact.display()))?;
    let digest_hex = hex::encode(Sha256::digest(&bytes));

    let private_pem = fs::read_to_string(&cfg.signing_key_path)
        .with_context(|| format!("read signing key: {}", cfg.signing_key_path.display()))?;
    let key_id = resolve_key_id(&cfg.signing_key_path, cfg.key_id_override.as_deref())?;

    let artifact_name = artifact
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| artifact.display().to_string());

    let predicate = SlsaProvenance {
        builder_id: cfg.builder_id.clone().unwrap_or_else(default_builder_id),
        build_type: BUNDLE_BUILD_TYPE.to_string(),
        built_at: None,
        tlog_entry_id: None,
    };
    let statement = InTotoStatement::provenance(artifact_name, &digest_hex, predicate);
    let envelope = sign_statement(&statement, &private_pem, &key_id)
        .with_context(|| format!("sign artifact {}", artifact.display()))?;

    let sig_path = cfg
        .signature_path_override
        .clone()
        .unwrap_or_else(|| default_signature_path(artifact));
    if let Some(parent) = sig_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create signature parent dir: {}", parent.display()))?;
    }
    let envelope_json = serde_json::to_vec_pretty(&envelope).context("serialize DSSE envelope")?;
    fs::write(&sig_path, envelope_json)
        .with_context(|| format!("write signature sidecar: {}", sig_path.display()))?;
    Ok(sig_path)
}

/// Default sidecar path: append `.sig` to the artifact path.
pub fn default_signature_path(artifact: &Path) -> PathBuf {
    append_suffix(artifact, SIGNATURE_SUFFIX)
}

fn resolve_key_id(private_path: &Path, override_: Option<&str>) -> Result<String> {
    if let Some(id) = override_ {
        if id.is_empty() {
            bail!("--key-id may not be empty");
        }
        return Ok(id.to_string());
    }
    let pub_path = append_suffix(private_path, PUBLIC_KEY_SUFFIX);
    if !pub_path.exists() {
        bail!(
            "cannot derive key id: pass --key-id explicitly or place the SPKI public PEM at {}",
            pub_path.display()
        );
    }
    let pub_pem = fs::read_to_string(&pub_path)
        .with_context(|| format!("read public key sibling: {}", pub_path.display()))?;
    key_id_for_public_key_pem(&pub_pem)
        .with_context(|| format!("derive key id from {}", pub_path.display()))
}

fn default_builder_id() -> String {
    format!("greentic-bundle:{}", env!("CARGO_PKG_VERSION"))
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
    use ed25519_dalek::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use greentic_distributor_client::signing::{
        TrustRoot, TrustedKey, key_id_for_public_key_pem, verify_artifact_dsse,
    };
    use tempfile::tempdir;

    fn ephemeral_keypair(seed: u8) -> (String, String) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let vk = sk.verifying_key();
        let priv_pem = sk.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
        let pub_pem = vk.to_public_key_pem(LineEnding::LF).unwrap();
        (priv_pem, pub_pem)
    }

    #[test]
    fn default_signature_path_appends_sig() {
        let p = Path::new("/tmp/dist/example.gtbundle");
        assert_eq!(
            default_signature_path(p),
            PathBuf::from("/tmp/dist/example.gtbundle.sig")
        );
    }

    #[test]
    fn sign_and_verify_roundtrip_with_pub_sibling() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        fs::write(&artifact, b"squashfs-bytes").unwrap();

        let (priv_pem, pub_pem) = ephemeral_keypair(11);
        let key_path = dir.path().join("signing-key.pem");
        fs::write(&key_path, &priv_pem).unwrap();
        fs::write(append_suffix(&key_path, PUBLIC_KEY_SUFFIX), &pub_pem).unwrap();

        let sig_path = sign_artifact(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: Some("greentic-bundle:test".into()),
                signature_path_override: None,
            },
        )
        .expect("sign");

        assert_eq!(sig_path, append_suffix(&artifact, SIGNATURE_SUFFIX));
        let envelope_bytes = fs::read(&sig_path).unwrap();
        let key_id = key_id_for_public_key_pem(&pub_pem).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: key_id.clone(),
            public_key_pem: pub_pem,
        }]);
        let artifact_bytes = fs::read(&artifact).unwrap();
        let expected_digest = hex::encode(Sha256::digest(&artifact_bytes));
        let verified =
            verify_artifact_dsse(&envelope_bytes, &expected_digest, &trust).expect("verify");
        assert_eq!(verified.verified_key_ids, vec![key_id]);
    }

    #[test]
    fn key_id_override_used_verbatim() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"x").unwrap();

        let (priv_pem, pub_pem) = ephemeral_keypair(12);
        let key_path = dir.path().join("k.pem");
        fs::write(&key_path, &priv_pem).unwrap();

        // No `.pub` sibling on disk; --key-id must be honored.
        let derived = key_id_for_public_key_pem(&pub_pem).unwrap();
        let sig_path = sign_artifact(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(derived.clone()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect("sign");
        let envelope_bytes = fs::read(&sig_path).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: derived,
            public_key_pem: pub_pem,
        }]);
        let expected = hex::encode(Sha256::digest(b"x"));
        verify_artifact_dsse(&envelope_bytes, &expected, &trust).expect("verify");
    }

    #[test]
    fn missing_pub_sibling_and_no_override_errors() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"x").unwrap();
        let (priv_pem, _pub_pem) = ephemeral_keypair(13);
        let key_path = dir.path().join("k.pem");
        fs::write(&key_path, &priv_pem).unwrap();
        let err = sign_artifact(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("--key-id") || msg.contains("public PEM"),
            "got: {msg}"
        );
    }

    #[test]
    fn empty_key_id_override_rejected() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"x").unwrap();
        let (priv_pem, _pub_pem) = ephemeral_keypair(14);
        let key_path = dir.path().join("k.pem");
        fs::write(&key_path, &priv_pem).unwrap();
        let err = sign_artifact(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(String::new()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("empty key id must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("--key-id"), "got: {msg}");
    }

    #[test]
    fn signature_path_override_is_used() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"x").unwrap();
        let (priv_pem, pub_pem) = ephemeral_keypair(15);
        let key_path = dir.path().join("k.pem");
        fs::write(&key_path, &priv_pem).unwrap();
        let override_path = dir.path().join("sigs").join("custom.json");
        let derived = key_id_for_public_key_pem(&pub_pem).unwrap();
        let sig_path = sign_artifact(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(derived),
                builder_id: None,
                signature_path_override: Some(override_path.clone()),
            },
        )
        .expect("sign");
        assert_eq!(sig_path, override_path);
        assert!(sig_path.exists());
    }
}
