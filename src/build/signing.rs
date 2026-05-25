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

use anyhow::{Context, Result, anyhow, bail};
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use greentic_distributor_client::signing::{
    InTotoStatement, SlsaProvenance, TrustRoot, TrustedKey, key_id_for_public_key_pem,
    sign_statement, verify_artifact_dsse,
};
use sha2::{Digest, Sha256};

/// Configuration for the bundle signer.
#[derive(Debug, Clone)]
pub struct SigningConfig {
    /// Path to the Ed25519 PKCS#8 PEM private key.
    pub signing_key_path: PathBuf,
    /// Optional explicit DSSE `keyid`. When set, it must match the canonical
    /// key id derived from the private key — a mismatch is rejected before any
    /// build output is written.
    pub key_id_override: Option<String>,
    /// SLSA `builder.id`. Defaults to `greentic-bundle:<package version>`.
    pub builder_id: Option<String>,
    /// Override of the output signature path. Default: `<artifact>.sig`.
    /// Rejected if it resolves to the artifact path.
    pub signature_path_override: Option<PathBuf>,
}

/// SLSA `build_type` discriminator for a `.gtbundle` artifact.
pub const BUNDLE_BUILD_TYPE: &str = "gtbundle";

const SIGNATURE_SUFFIX: &str = ".sig";
const PUBLIC_KEY_SUFFIX: &str = ".pub";
const STAGING_SUFFIX: &str = ".partial";

/// A validated signing context: a parsed private key and a resolved sidecar
/// path that has been proven distinct from the artifact. Constructed once via
/// [`PreparedSigner::prepare`] before any build output is written so that
/// signing-configuration errors abort the build *before* the `.gtbundle` lands
/// on disk (closes Codex finding #3).
#[derive(Debug)]
pub struct PreparedSigner {
    pub sig_path: PathBuf,
    /// PKCS#8 PEM (kept in memory for the duration of the build).
    private_pem: String,
    /// `key_id` derived from the private key's verifying key — the only id we
    /// ever write into the envelope (closes Codex finding #2).
    canonical_key_id: String,
    /// SPKI PEM of the public half, used for the self-verify step.
    canonical_public_pem: String,
    builder_id: String,
}

impl PreparedSigner {
    /// Validate the signing config against the intended artifact path. Performs:
    /// 1. Read + parse the PKCS#8 PEM, derive the canonical `(key_id, public_pem)`.
    /// 2. If `--key-id` override is given, require it to match the canonical id
    ///    (case-insensitive hex).
    /// 3. If a `<key>.pub` sibling exists, require its derived id to match the
    ///    canonical id — a stale sibling after key rotation is a hard error.
    /// 4. Resolve the signature output path and reject collisions with the
    ///    artifact (lexical, absolute, and canonical-symlink comparisons).
    pub fn prepare(artifact: &Path, cfg: &SigningConfig) -> Result<Self> {
        let private_pem = fs::read_to_string(&cfg.signing_key_path)
            .with_context(|| format!("read signing key: {}", cfg.signing_key_path.display()))?;
        let signing_key = SigningKey::from_pkcs8_pem(&private_pem).with_context(|| {
            format!(
                "parse PKCS#8 PEM private key: {}",
                cfg.signing_key_path.display()
            )
        })?;
        let public_pem = signing_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .context("encode SPKI public key PEM")?;
        let canonical_key_id = key_id_for_public_key_pem(&public_pem)
            .map_err(|e| anyhow!("derive canonical key id: {e}"))?;

        if let Some(override_) = cfg.key_id_override.as_deref() {
            if override_.is_empty() {
                bail!("--key-id may not be empty");
            }
            if !override_.eq_ignore_ascii_case(&canonical_key_id) {
                bail!(
                    "--key-id {override_} does not match the private key (canonical id: {canonical_key_id})"
                );
            }
        }

        let pub_sibling = append_suffix(&cfg.signing_key_path, PUBLIC_KEY_SUFFIX);
        if pub_sibling.exists() {
            let sibling_pem = fs::read_to_string(&pub_sibling)
                .with_context(|| format!("read public key sibling: {}", pub_sibling.display()))?;
            let sibling_id = key_id_for_public_key_pem(&sibling_pem)
                .map_err(|e| anyhow!("derive id from {}: {e}", pub_sibling.display()))?;
            if !sibling_id.eq_ignore_ascii_case(&canonical_key_id) {
                bail!(
                    "public key sibling {} does not match the private key (pub id={sibling_id} priv id={canonical_key_id}); a stale .pub after key rotation will silently break verification",
                    pub_sibling.display()
                );
            }
        }

        let sig_path = cfg
            .signature_path_override
            .clone()
            .unwrap_or_else(|| default_signature_path(artifact));
        reject_signature_artifact_collision(&sig_path, artifact)?;

        Ok(Self {
            sig_path,
            private_pem,
            canonical_key_id,
            canonical_public_pem: public_pem,
            builder_id: cfg.builder_id.clone().unwrap_or_else(default_builder_id),
        })
    }

    fn build_envelope_json(&self, artifact_bytes: &[u8], artifact_name: &str) -> Result<Vec<u8>> {
        let digest_hex = hex::encode(Sha256::digest(artifact_bytes));
        let predicate = SlsaProvenance {
            builder_id: self.builder_id.clone(),
            build_type: BUNDLE_BUILD_TYPE.to_string(),
            built_at: None,
            tlog_entry_id: None,
        };
        let statement = InTotoStatement::provenance(artifact_name, &digest_hex, predicate);
        let envelope = sign_statement(&statement, &self.private_pem, &self.canonical_key_id)
            .map_err(|e| anyhow!("sign in-toto statement: {e}"))?;
        let envelope_json =
            serde_json::to_vec_pretty(&envelope).context("serialize DSSE envelope")?;

        // Self-verify before publishing — defense in depth against a
        // PKCS#8/key-id binding bug (closes Codex finding #2 recommendation).
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: self.canonical_key_id.clone(),
            public_key_pem: self.canonical_public_pem.clone(),
        }]);
        verify_artifact_dsse(&envelope_json, &digest_hex, &trust)
            .map_err(|e| anyhow!("self-verify of emitted envelope failed: {e}"))?;

        Ok(envelope_json)
    }
}

/// Atomically write a signed bundle: stage the artifact + sidecar adjacent to
/// their final paths, self-verify, then rename both into place. On any error,
/// best-effort remove both staged files so a release job collecting dist
/// outputs after a failed step never sees an unsigned `.gtbundle` masquerading
/// as a candidate (closes Codex finding #3).
///
/// `write_artifact` is called with the *staged* artifact path; callers pass
/// `bundle_fs::write_bundle(build_dir, ...)` here.
pub fn stage_sign_and_publish<F>(
    artifact: &Path,
    signer: &PreparedSigner,
    write_artifact: F,
) -> Result<PathBuf>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let staged_artifact = append_suffix(artifact, STAGING_SUFFIX);
    let staged_sig = append_suffix(&signer.sig_path, STAGING_SUFFIX);

    // Pre-clean any prior staging cruft so a partial leftover from an earlier
    // crash doesn't masquerade as the current run's output.
    let _ = fs::remove_file(&staged_artifact);
    let _ = fs::remove_file(&staged_sig);

    let outcome = (|| -> Result<()> {
        if let Some(parent) = signer.sig_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("create signature parent dir: {}", parent.display()))?;
        }
        write_artifact(&staged_artifact)?;
        let bytes = fs::read(&staged_artifact)
            .with_context(|| format!("read staged artifact: {}", staged_artifact.display()))?;
        let artifact_name = artifact
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| artifact.display().to_string());
        let envelope_json = signer.build_envelope_json(&bytes, &artifact_name)?;
        fs::write(&staged_sig, &envelope_json)
            .with_context(|| format!("write staged sidecar: {}", staged_sig.display()))?;
        Ok(())
    })();

    if let Err(e) = outcome {
        let _ = fs::remove_file(&staged_artifact);
        let _ = fs::remove_file(&staged_sig);
        return Err(e);
    }

    // Same filesystem, sibling paths → POSIX rename is atomic.
    fs::rename(&staged_artifact, artifact).with_context(|| {
        format!(
            "rename staged artifact {} -> {}",
            staged_artifact.display(),
            artifact.display()
        )
    })?;
    if let Err(e) = fs::rename(&staged_sig, &signer.sig_path) {
        // Artifact already in place but sidecar move failed — remove the
        // artifact so the caller never sees an unsigned `.gtbundle`.
        let _ = fs::remove_file(artifact);
        let _ = fs::remove_file(&staged_sig);
        return Err(anyhow::Error::new(e).context(format!(
            "rename staged sidecar -> {}",
            signer.sig_path.display()
        )));
    }
    Ok(signer.sig_path.clone())
}

/// Default sidecar path: append `.sig` to the artifact path.
pub fn default_signature_path(artifact: &Path) -> PathBuf {
    append_suffix(artifact, SIGNATURE_SUFFIX)
}

/// Reject a signature output path that resolves to the artifact path. Checks
/// three layers: lexical equality, absolute-path equality, and
/// canonicalize-on-both (catches symlink collisions). Closes Codex finding #1.
fn reject_signature_artifact_collision(sig_path: &Path, artifact: &Path) -> Result<()> {
    if sig_path == artifact {
        bail!(
            "signature output path equals the artifact path ({}); refusing to overwrite the bundle with its own envelope",
            artifact.display()
        );
    }
    let sig_abs = std::path::absolute(sig_path).unwrap_or_else(|_| sig_path.to_path_buf());
    let art_abs = std::path::absolute(artifact).unwrap_or_else(|_| artifact.to_path_buf());
    if sig_abs == art_abs {
        bail!(
            "signature output path {} resolves to the artifact path {}; refusing to overwrite the bundle",
            sig_path.display(),
            artifact.display()
        );
    }
    if sig_path.exists() && artifact.exists() {
        let sig_can = sig_path.canonicalize().ok();
        let art_can = artifact.canonicalize().ok();
        if let (Some(s), Some(a)) = (sig_can, art_can)
            && s == a
        {
            bail!(
                "signature output path {} canonicalizes to the artifact path {} (symlink collision); refusing to overwrite the bundle",
                sig_path.display(),
                artifact.display()
            );
        }
    }
    Ok(())
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
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use tempfile::tempdir;

    fn ephemeral_keypair(seed: u8) -> (String, String) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let vk = sk.verifying_key();
        let priv_pem = sk.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
        let pub_pem = vk.to_public_key_pem(LineEnding::LF).unwrap();
        (priv_pem, pub_pem)
    }

    fn write_key(dir: &Path, name: &str, pem: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, pem).unwrap();
        p
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
    fn prepare_derives_key_id_from_private_pem_without_pub_sibling() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(31);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();
        assert_eq!(
            signer.canonical_key_id,
            key_id_for_public_key_pem(&pub_pem).unwrap()
        );
        assert_eq!(signer.sig_path, default_signature_path(&artifact));
    }

    #[test]
    fn prepare_rejects_key_id_override_that_doesnt_match_private_key() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(32);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                // 32-char hex but for a different key
                key_id_override: Some("deadbeefdeadbeefdeadbeefdeadbeef".into()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("mismatched --key-id must be rejected");
        assert!(
            format!("{err:#}").contains("does not match"),
            "got: {err:#}"
        );
    }

    #[test]
    fn prepare_rejects_stale_pub_sibling_after_key_rotation() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(33);
        let (_priv_other, pub_other) = ephemeral_keypair(34);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        // Sibling .pub belongs to a DIFFERENT key — simulates a stale file
        // left over from rotation.
        fs::write(append_suffix(&key_path, PUBLIC_KEY_SUFFIX), &pub_other).unwrap();
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("stale .pub must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("stale .pub") || msg.contains("does not match"),
            "got: {msg}"
        );
    }

    #[test]
    fn prepare_rejects_empty_key_id_override() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(35);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(String::new()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("empty override must be rejected");
        assert!(format!("{err:#}").contains("--key-id"));
    }

    #[test]
    fn prepare_rejects_signature_path_equal_to_artifact() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(36);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(artifact.clone()),
            },
        )
        .expect_err("identical paths must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("refusing to overwrite"), "got: {msg}");
    }

    #[test]
    fn prepare_rejects_signature_path_resolving_to_artifact_via_relative() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(37);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        // Same absolute path, expressed via a redundant ./ prefix.
        let alias = dir.path().join(".").join("a.gtbundle");
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(alias),
            },
        )
        .expect_err("path alias to artifact must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    #[cfg(unix)]
    #[test]
    fn prepare_rejects_signature_path_via_symlink_to_artifact() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"existing-bundle").unwrap();
        let (priv_pem, _pub_pem) = ephemeral_keypair(38);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let link = dir.path().join("alias.sig");
        std::os::unix::fs::symlink(&artifact, &link).unwrap();
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(link),
            },
        )
        .expect_err("symlink to artifact must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    #[test]
    fn stage_sign_and_publish_emits_verifiable_sidecar() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(39);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        fs::write(append_suffix(&key_path, PUBLIC_KEY_SUFFIX), &pub_pem).unwrap();
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: Some("greentic-bundle:test".into()),
                signature_path_override: None,
            },
        )
        .unwrap();

        let sig_path = stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"squashfs-bytes")?;
            Ok(())
        })
        .expect("stage+sign");
        assert_eq!(sig_path, default_signature_path(&artifact));
        assert!(artifact.exists());
        assert!(sig_path.exists());
        // Staged files must be gone.
        assert!(!append_suffix(&artifact, STAGING_SUFFIX).exists());
        assert!(!append_suffix(&sig_path, STAGING_SUFFIX).exists());

        let envelope_bytes = fs::read(&sig_path).unwrap();
        let key_id = key_id_for_public_key_pem(&pub_pem).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: key_id.clone(),
            public_key_pem: pub_pem,
        }]);
        let artifact_bytes = fs::read(&artifact).unwrap();
        let expected_digest = hex::encode(Sha256::digest(&artifact_bytes));
        let verified = verify_artifact_dsse(&envelope_bytes, &expected_digest, &trust).unwrap();
        assert_eq!(verified.verified_key_ids, vec![key_id]);
    }

    #[test]
    fn stage_sign_and_publish_leaves_no_artifact_when_write_step_fails() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(40);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();

        let err = stage_sign_and_publish(&artifact, &signer, |_staged| {
            anyhow::bail!("simulated write_bundle failure")
        })
        .expect_err("must propagate write failure");
        assert!(format!("{err:#}").contains("simulated"));
        // Crucial: no .gtbundle, no .sig, no .partial siblings.
        assert!(!artifact.exists());
        assert!(!signer.sig_path.exists());
        assert!(!append_suffix(&artifact, STAGING_SUFFIX).exists());
        assert!(!append_suffix(&signer.sig_path, STAGING_SUFFIX).exists());
    }

    #[test]
    fn stage_sign_and_publish_cleans_up_prior_partial_leftovers() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(41);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();
        // Pre-seed both staging paths with stale bytes from a "crashed" prior run.
        fs::write(append_suffix(&artifact, STAGING_SUFFIX), b"stale-art").unwrap();
        fs::write(
            append_suffix(&signer.sig_path, STAGING_SUFFIX),
            b"stale-sig",
        )
        .unwrap();
        stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"fresh-bytes")?;
            Ok(())
        })
        .expect("stage+sign");
        // Final artifact carries the fresh bytes, not the stale .partial cruft.
        assert_eq!(fs::read(&artifact).unwrap(), b"fresh-bytes");
    }
}
